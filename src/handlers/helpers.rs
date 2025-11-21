use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::dependency_tracker::DependencyTracker;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, error, info, warn};
use parking_lot::{Mutex, RwLock};
use ruby_prism::{parse, Visit};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tower_lsp::lsp_types::*;

/// Initialize workspace
pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> Result<()> {
    let workspace_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert folder URI to file path"))?;

    info!("Initializing workspace: {:?}", workspace_path);

    let mut coordinator = IndexingCoordinator::new(workspace_path, server.config.lock().clone());
    coordinator.run_complete_indexing(server).await?;

    Ok(())
}

// Helper function to process content for definitions without reading from filesystem
pub fn process_content_for_definitions(
    server: &RubyLanguageServer,
    uri: Url,
    content: &str,
) -> Result<(), String> {
    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        debug!(
            "Parse errors in content for URI {:?}: {} errors",
            uri, errors_count
        );
        return Ok(()); // Continue processing despite parse errors
    }

    // Remove existing entries for this URI before adding new ones
    server.index().lock().remove_entries_for_uri(&uri);

    let mut visitor = IndexVisitor::new(server, uri.clone());
    visitor.visit(&parse_result.node());

    // Resolve mixins for entries that were just added
    server.index().lock().resolve_all_mixins();

    // Update the document in the server's cache with the visitor's modified document
    if let Some(doc_arc) = server.docs.lock().get(&uri) {
        *doc_arc.write() = visitor.document;
    }

    Ok(())
}

// Helper function to process content for references without reading from filesystem
pub fn process_content_for_references(
    server: &RubyLanguageServer,
    uri: Url,
    content: &str,
    include_local_vars: bool,
) -> Result<(), String> {
    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        debug!(
            "Parse errors in content for URI {:?}: {} errors",
            uri, errors_count
        );
        return Ok(()); // Continue processing despite parse errors
    }

    // Remove existing references for this URI before adding new ones
    server.index().lock().remove_references_for_uri(&uri);

    let mut visitor = if include_local_vars {
        ReferenceVisitor::new(server, uri.clone())
    } else {
        ReferenceVisitor::with_options(server, uri.clone(), false)
    };
    visitor.visit(&parse_result.node());

    // Update the document in the server's cache with the visitor's modified document
    if let Some(doc_arc) = server.docs.lock().get(&uri) {
        *doc_arc.write() = visitor.document;
    }

    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum ProcessingMode {
    /// Index only definitions (classes, methods, constants, etc.)
    Definitions,
    /// Index references to symbols
    References { include_local_vars: bool },
}

impl ProcessingMode {
    pub fn include_local_vars(&self) -> bool {
        match self {
            ProcessingMode::Definitions => false,
            ProcessingMode::References { include_local_vars } => *include_local_vars,
        }
    }
}

pub async fn process_files_parallel(
    server: &RubyLanguageServer,
    files: Vec<PathBuf>,
    mode: ProcessingMode,
) -> Result<()> {
    process_files_parallel_with_tracker(server, files, mode, None).await
}

pub async fn process_files_parallel_with_tracker(
    server: &RubyLanguageServer,
    files: Vec<PathBuf>,
    mode: ProcessingMode,
    dependency_tracker: Option<Arc<Mutex<DependencyTracker>>>,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let total_files = files.len();
    let start_time = Instant::now();

    match mode {
        ProcessingMode::Definitions => {
            debug!("Processing {} files for definitions", total_files);
        }
        ProcessingMode::References { include_local_vars } => {
            debug!(
                "Processing {} files for references (include_local_vars: {})",
                total_files, include_local_vars
            );
        }
    }

    // Determine optimal concurrency based on system capabilities
    let max_concurrent_files = thread::available_parallelism()
        .unwrap_or(NonZeroUsize::new(4).unwrap())
        .get()
        .min(32); // Cap at 32 to avoid overwhelming the system

    let semaphore = Arc::new(Semaphore::new(max_concurrent_files));
    let mut join_set = JoinSet::new();

    for file_path in files {
        let server_clone = server.clone();
        let semaphore_clone = semaphore.clone();
        let dependency_tracker_clone = dependency_tracker.clone();

        join_set.spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();

            let file_uri = match Url::from_file_path(&file_path) {
                Ok(uri) => uri,
                Err(_) => {
                    error!("Failed to convert file path to URI: {:?}", file_path);
                    return Err(format!(
                        "Failed to convert file path to URI: {:?}",
                        file_path
                    ));
                }
            };

            match mode {
                ProcessingMode::Definitions => process_file_for_definitions_with_tracker(
                    &server_clone,
                    file_uri,
                    dependency_tracker_clone,
                ),
                ProcessingMode::References { include_local_vars } => {
                    process_file_for_references(&server_clone, file_uri, include_local_vars)
                }
            }
        });
    }

    // Collect results and handle errors
    let mut successful_files = 0;
    let mut failed_files = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => {
                successful_files += 1;
            }
            Ok(Err(e)) => {
                debug!("File processing error: {}", e);
                failed_files += 1;
            }
            Err(e) => {
                error!("Task join error: {}", e);
                failed_files += 1;
            }
        }
    }

    let duration = start_time.elapsed();
    let mode_str = match mode {
        ProcessingMode::Definitions => "definitions",
        ProcessingMode::References { .. } => "references",
    };

    info!(
        "Completed {} indexing: {}/{} files successful in {:.2}s (avg: {:.2}ms/file)",
        mode_str,
        successful_files,
        total_files,
        duration.as_secs_f64(),
        duration.as_millis() as f64 / total_files as f64
    );

    if failed_files > 0 {
        warn!(
            "{} files failed to process during {} indexing",
            failed_files, mode_str
        );
    }

    Ok(())
}

pub fn find_ruby_files_sync(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut ruby_files = Vec::new();

    fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                visit_dir(&path, files)?;
            } else if let Some(ext) = path.extension() {
                if ext == "rb" {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    visit_dir(dir, &mut ruby_files)?;
    Ok(ruby_files)
}

pub async fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
    // Use the synchronous version to avoid async recursion issues
    find_ruby_files_sync(dir)
}

pub fn process_file_for_definitions(server: &RubyLanguageServer, uri: Url) -> Result<(), String> {
    process_file_for_definitions_with_tracker(server, uri, None)
}

pub fn process_file_for_definitions_with_tracker(
    server: &RubyLanguageServer,
    uri: Url,
    dependency_tracker: Option<Arc<Mutex<DependencyTracker>>>,
) -> Result<(), String> {
    let file_path = uri
        .to_file_path()
        .map_err(|_| format!("Failed to convert URI to file path: {}", uri))?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;

    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        debug!(
            "Parse errors in file {:?}: {} errors",
            file_path, errors_count
        );
        return Ok(()); // Continue processing despite parse errors
    }

    // Remove existing entries for this URI before adding new ones
    server.index().lock().remove_entries_for_uri(&uri);

    let document = RubyDocument::new(uri.clone(), content.clone(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    let mut visitor = IndexVisitor::new(server, uri.clone());
    if let Some(tracker) = dependency_tracker {
        visitor = visitor.with_dependency_tracker(tracker);
    }
    visitor.visit(&parse_result.node());

    Ok(())
}

pub fn process_file_for_references(
    server: &RubyLanguageServer,
    uri: Url,
    include_local_vars: bool,
) -> Result<(), String> {
    let file_path = uri
        .to_file_path()
        .map_err(|_| format!("Failed to convert URI to file path: {}", uri))?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;

    let parse_result = parse(content.as_bytes());
    let errors_count = parse_result.errors().count();
    if errors_count > 0 {
        debug!(
            "Parse errors in file {:?}: {} errors",
            file_path, errors_count
        );
        return Ok(()); // Continue processing despite parse errors
    }

    // Remove existing references for this URI before adding new ones
    server.index().lock().remove_references_for_uri(&uri);

    let document = RubyDocument::new(uri.clone(), content.clone(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    let mut visitor = if include_local_vars {
        ReferenceVisitor::new(server, uri.clone())
    } else {
        ReferenceVisitor::with_options(server, uri.clone(), false)
    };
    visitor.visit(&parse_result.node());

    Ok(())
}
