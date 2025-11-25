use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::dependency_tracker::DependencyTracker;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_version::RubyVersion;
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
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

// ============================================================================
// Processing Options
// ============================================================================

/// Options for processing definitions
#[derive(Default, Clone)]
pub struct DefinitionOptions {
    pub dependency_tracker: Option<Arc<Mutex<DependencyTracker>>>,
}

impl DefinitionOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dependency_tracker(mut self, tracker: Arc<Mutex<DependencyTracker>>) -> Self {
        self.dependency_tracker = Some(tracker);
        self
    }
}

/// Options for processing references
#[derive(Clone, Copy)]
pub struct ReferenceOptions {
    pub include_local_vars: bool,
    pub track_unresolved: bool,
}

impl Default for ReferenceOptions {
    fn default() -> Self {
        Self {
            include_local_vars: true,
            track_unresolved: false,
        }
    }
}

impl ReferenceOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_local_vars(mut self, include: bool) -> Self {
        self.include_local_vars = include;
        self
    }

    pub fn with_unresolved_tracking(mut self, track: bool) -> Self {
        self.track_unresolved = track;
        self
    }
}

// ============================================================================
// Workspace Initialization
// ============================================================================

/// Initialize workspace and run complete indexing
pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> Result<()> {
    let workspace_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert folder URI to file path"))?;

    info!("Initializing workspace: {:?}", workspace_path);

    let mut coordinator = IndexingCoordinator::new(workspace_path, server.config.lock().clone());
    coordinator.run_complete_indexing(server).await?;

    Ok(())
}

// ============================================================================
// Core Processing Functions (Content-based)
// ============================================================================

/// Result of processing definitions, including affected URIs for diagnostic propagation
pub struct ProcessDefinitionsResult {
    /// URIs of files that reference definitions that were removed
    /// These files need their diagnostics updated
    pub affected_uris: std::collections::HashSet<Url>,
}

/// Process content for definitions (in-memory content)
/// Returns information about affected files for cross-file diagnostic propagation
pub fn process_definitions(
    server: &RubyLanguageServer,
    uri: Url,
    content: &str,
    options: DefinitionOptions,
) -> Result<ProcessDefinitionsResult, String> {
    let parse_result = parse(content.as_bytes());
    if parse_result.errors().count() > 0 {
        debug!(
            "Parse errors in content for URI {:?}: {} errors",
            uri,
            parse_result.errors().count()
        );
        return Ok(ProcessDefinitionsResult {
            affected_uris: std::collections::HashSet::new(),
        });
    }

    // Remove existing entries for this URI before adding new ones
    // This returns FQNs that were completely removed (no definitions left in other files)
    let removed_fqns = server.index().lock().remove_entries_for_uri(&uri);
    let removed_fqn_set: std::collections::HashSet<_> = removed_fqns.into_iter().collect();

    let mut visitor = IndexVisitor::new(server, uri.clone());
    if let Some(tracker) = options.dependency_tracker {
        visitor = visitor.with_dependency_tracker(tracker);
    }
    visitor.visit(&parse_result.node());

    // Resolve mixins only for entries in this file (not the entire index)
    server.index().lock().resolve_mixins_for_uri(&uri);

    // Get the FQNs of entries that were just added to this file
    let added_fqns: Vec<_> = {
        let index = server.index();
        let index_guard = index.lock();
        index_guard
            .file_entries
            .get(&uri)
            .map(|entries| entries.iter().map(|e| e.fqn.clone()).collect())
            .unwrap_or_default()
    };
    let added_fqn_set: std::collections::HashSet<_> = added_fqns.iter().cloned().collect();

    let mut affected_uris = std::collections::HashSet::new();

    // Only mark as unresolved FQNs that were TRULY removed (removed but not re-added)
    // This prevents adding unresolved entries for FQNs that are just being re-indexed
    let truly_removed: Vec<_> = removed_fqn_set
        .difference(&added_fqn_set)
        .cloned()
        .collect();

    if !truly_removed.is_empty() {
        let removed_affected = server
            .index()
            .lock()
            .mark_references_as_unresolved(&truly_removed);
        affected_uris.extend(removed_affected);
    }

    // Clear unresolved entries for ALL added FQNs (new or re-added)
    // because they might resolve previously unresolved references
    if !added_fqns.is_empty() {
        let resolved_affected = server.index().lock().clear_resolved_entries(&added_fqns);
        affected_uris.extend(resolved_affected);
    }

    // Update the document in the server's cache with the visitor's modified document
    if let Some(doc_arc) = server.docs.lock().get(&uri) {
        *doc_arc.write() = visitor.document;
    }

    Ok(ProcessDefinitionsResult { affected_uris })
}

/// Process content for references (in-memory content)
pub fn process_references(
    server: &RubyLanguageServer,
    uri: Url,
    content: &str,
    options: ReferenceOptions,
) -> Result<(), String> {
    let parse_result = parse(content.as_bytes());
    if parse_result.errors().count() > 0 {
        debug!(
            "Parse errors in content for URI {:?}: {} errors",
            uri,
            parse_result.errors().count()
        );
        return Ok(());
    }

    // Remove existing references and optionally unresolved entries
    {
        let index_arc = server.index();
        let mut index = index_arc.lock();
        index.remove_references_for_uri(&uri);
        if options.track_unresolved {
            index.remove_unresolved_entries_for_uri(&uri);
        }
    }

    let mut visitor = if options.track_unresolved {
        ReferenceVisitor::with_unresolved_tracking(server, uri.clone(), options.include_local_vars)
    } else if options.include_local_vars {
        ReferenceVisitor::new(server, uri.clone())
    } else {
        ReferenceVisitor::with_options(server, uri.clone(), false)
    };
    visitor.visit(&parse_result.node());

    // Update the document in the server's cache
    if let Some(doc_arc) = server.docs.lock().get(&uri) {
        *doc_arc.write() = visitor.document;
    }

    Ok(())
}

// ============================================================================
// File-based Processing Functions
// ============================================================================

/// Process a file for definitions (reads from disk)
pub fn process_file_definitions(
    server: &RubyLanguageServer,
    uri: Url,
    options: DefinitionOptions,
) -> Result<(), String> {
    let file_path = uri
        .to_file_path()
        .map_err(|_| format!("Failed to convert URI to file path: {}", uri))?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;

    let parse_result = parse(content.as_bytes());
    if parse_result.errors().count() > 0 {
        debug!(
            "Parse errors in file {:?}: {} errors",
            file_path,
            parse_result.errors().count()
        );
        return Ok(());
    }

    // Remove existing entries for this URI before adding new ones
    server.index().lock().remove_entries_for_uri(&uri);

    // Create document and add to cache
    let document = RubyDocument::new(uri.clone(), content.clone(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    let mut visitor = IndexVisitor::new(server, uri.clone());
    if let Some(tracker) = options.dependency_tracker {
        visitor = visitor.with_dependency_tracker(tracker);
    }
    visitor.visit(&parse_result.node());

    Ok(())
}

/// Process a file for references (reads from disk)
pub fn process_file_references(
    server: &RubyLanguageServer,
    uri: Url,
    options: ReferenceOptions,
) -> Result<(), String> {
    let file_path = uri
        .to_file_path()
        .map_err(|_| format!("Failed to convert URI to file path: {}", uri))?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file {:?}: {}", file_path, e))?;

    let parse_result = parse(content.as_bytes());
    if parse_result.errors().count() > 0 {
        debug!(
            "Parse errors in file {:?}: {} errors",
            file_path,
            parse_result.errors().count()
        );
        return Ok(());
    }

    // Remove existing references and optionally unresolved entries
    {
        let index_arc = server.index();
        let mut index = index_arc.lock();
        index.remove_references_for_uri(&uri);
        if options.track_unresolved {
            index.remove_unresolved_entries_for_uri(&uri);
        }
    }

    // Create document and add to cache
    let document = RubyDocument::new(uri.clone(), content.clone(), 0);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    let mut visitor = if options.track_unresolved {
        ReferenceVisitor::with_unresolved_tracking(server, uri.clone(), options.include_local_vars)
    } else if options.include_local_vars {
        ReferenceVisitor::new(server, uri.clone())
    } else {
        ReferenceVisitor::with_options(server, uri.clone(), false)
    };
    visitor.visit(&parse_result.node());

    Ok(())
}

// ============================================================================
// Parallel Processing
// ============================================================================

/// Processing mode for parallel file processing
#[derive(Clone)]
pub enum ProcessingMode {
    Definitions(DefinitionOptions),
    References(ReferenceOptions),
}

/// Process multiple files in parallel
pub async fn process_files_parallel(
    server: &RubyLanguageServer,
    files: Vec<PathBuf>,
    mode: ProcessingMode,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let total_files = files.len();
    let start_time = Instant::now();

    let mode_str = match &mode {
        ProcessingMode::Definitions(_) => {
            debug!("Processing {} files for definitions", total_files);
            "definitions"
        }
        ProcessingMode::References(opts) => {
            debug!(
                "Processing {} files for references (include_local_vars: {}, track_unresolved: {})",
                total_files, opts.include_local_vars, opts.track_unresolved
            );
            "references"
        }
    };

    // Determine optimal concurrency based on system capabilities
    let max_concurrent = thread::available_parallelism()
        .unwrap_or(NonZeroUsize::new(4).unwrap())
        .get()
        .min(32);

    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut join_set = JoinSet::new();

    for file_path in files {
        let server_clone = server.clone();
        let semaphore_clone = semaphore.clone();
        let mode_clone = mode.clone();

        join_set.spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();

            let file_uri = Url::from_file_path(&file_path)
                .map_err(|_| format!("Failed to convert file path to URI: {:?}", file_path))?;

            match mode_clone {
                ProcessingMode::Definitions(opts) => {
                    process_file_definitions(&server_clone, file_uri, opts)
                }
                ProcessingMode::References(opts) => {
                    process_file_references(&server_clone, file_uri, opts)
                }
            }
        });
    }

    // Collect results
    let mut successful = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => successful += 1,
            Ok(Err(e)) => {
                debug!("File processing error: {}", e);
                failed += 1;
            }
            Err(e) => {
                error!("Task join error: {}", e);
                failed += 1;
            }
        }
    }

    let duration = start_time.elapsed();
    info!(
        "Completed {} indexing: {}/{} files successful in {:.2}s (avg: {:.2}ms/file)",
        mode_str,
        successful,
        total_files,
        duration.as_secs_f64(),
        duration.as_millis() as f64 / total_files as f64
    );

    if failed > 0 {
        warn!(
            "{} files failed to process during {} indexing",
            failed, mode_str
        );
    }

    Ok(())
}

// ============================================================================
// Diagnostic Helpers
// ============================================================================

/// Get diagnostics for unresolved entries (constants and methods) from the index
pub fn get_unresolved_diagnostics(server: &RubyLanguageServer, uri: &Url) -> Vec<Diagnostic> {
    use crate::indexer::index::UnresolvedEntry;

    let index_arc = server.index();
    let index = index_arc.lock();
    let unresolved_list = index.get_unresolved_entries(uri);

    unresolved_list
        .iter()
        .map(|entry| match entry {
            UnresolvedEntry::Constant { name, location, .. } => Diagnostic {
                range: location.range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unresolved-constant".to_string())),
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: format!("Unresolved constant `{}`", name),
                related_information: None,
                tags: None,
                data: None,
            },
            UnresolvedEntry::Method {
                name,
                receiver,
                location,
            } => {
                let message = match receiver {
                    Some(recv) => format!("Unresolved method `{}` on `{}`", name, recv),
                    None => format!("Unresolved method `{}`", name),
                };

                Diagnostic {
                    range: location.range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unresolved-method".to_string())),
                    code_description: None,
                    source: Some("ruby-fast-lsp".to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                }
            }
        })
        .collect()
}

// ============================================================================
// Ruby Version Detection
// ============================================================================

/// Detect system Ruby version without workspace context
pub fn detect_system_ruby_version() -> Option<(u8, u8)> {
    let output = std::process::Command::new("ruby")
        .args(["--version"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    // Parse output like "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
    let version_part = version_output.split_whitespace().nth(1)?;
    debug!("System ruby version output: {}", version_part);
    let version = RubyVersion::from_full_version(version_part)?;
    Some((version.major, version.minor))
}

// ============================================================================
// File Discovery
// ============================================================================

/// Find all Ruby files in a directory (recursive)
pub fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut ruby_files = Vec::new();
    collect_ruby_files(dir, &mut ruby_files)?;
    Ok(ruby_files)
}

fn collect_ruby_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_ruby_files(&path, files)?;
        } else if path.extension().map_or(false, |ext| ext == "rb") {
            files.push(path);
        }
    }
    Ok(())
}
