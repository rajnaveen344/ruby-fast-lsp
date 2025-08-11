use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::analyzer_prism::visitors::reference_visitor::ReferenceVisitor;
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use anyhow::Result;
use log::{debug, error, info, warn};
use tower_lsp::lsp_types::*;
use parking_lot::RwLock;
use ruby_prism::{parse, Visit};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Detect Ruby version from workspace context
fn detect_workspace_ruby_version(workspace_path: &Path) -> Option<(u8, u8)> {
    // Check for .ruby-version file
    let ruby_version_file = workspace_path.join(".ruby-version");
    if ruby_version_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&ruby_version_file) {
            let version_str = content.trim();
            if let Some((major, minor)) = parse_ruby_version(version_str) {
                info!("Detected Ruby version {} from .ruby-version file", version_str);
                return Some((major, minor));
            }
        }
    }
    
    // Check for Gemfile
    let gemfile = workspace_path.join("Gemfile");
    if gemfile.exists() {
        if let Ok(content) = std::fs::read_to_string(&gemfile) {
            for line in content.lines() {
                if line.trim().starts_with("ruby") {
                    // Extract version from lines like: ruby "3.1.0" or ruby '3.1'
                    if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                        let quote_char = line.chars().nth(start).unwrap();
                        if let Some(end) = line[start + 1..].find(quote_char) {
                            let version_str = &line[start + 1..start + 1 + end];
                            if let Some((major, minor)) = parse_ruby_version(version_str) {
                                info!("Detected Ruby version {} from Gemfile", version_str);
                                return Some((major, minor));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Fallback: detect system Ruby version
    detect_system_ruby_version()
}

/// Detect the system Ruby version by running `ruby --version`
fn detect_system_ruby_version() -> Option<(u8, u8)> {
    use std::process::Command;
    
    let output = Command::new("ruby")
        .args(&["--version"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    // Ruby version output format: "ruby 3.3.5 (2024-09-03 revision ef084cc8f4) [arm64-darwin23]"
    for word in version_output.split_whitespace() {
        if let Some((major, minor)) = parse_ruby_version(word) {
            info!("Detected system Ruby version {}.{}", major, minor);
            return Some((major, minor));
        }
    }
    
    None
}

/// Parse Ruby version string into (major, minor) tuple
fn parse_ruby_version(version_str: &str) -> Option<(u8, u8)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() >= 2 {
        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
            return Some((major, minor));
        }
    }
    None
}

pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> Result<()> {
    let start_time = Instant::now();
    info!("Indexing workspace folder: {}", folder_uri);

    // Convert URI to filesystem path
    let folder_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert URI to file path: {}", folder_uri))?;

    // Get all index paths using the new path discovery system
    let config = server.config.lock().clone();
    
    // Determine Ruby version for path discovery
    let ruby_version = if let Some(version) = config.get_ruby_version() {
        info!("Using configured Ruby version {}.{}", version.0, version.1);
        version
    } else {
        // Auto-detect Ruby version in workspace context
        detect_workspace_ruby_version(&folder_path)
            .unwrap_or_else(|| {
                warn!("Could not detect Ruby version, falling back to Ruby 3.0");
                (3, 0)
            })
    };
    
    let all_index_paths = config.get_index_paths(ruby_version, folder_path.clone());
    
    info!("Discovered {} paths to index:", all_index_paths.len());
    for path in &all_index_paths {
        info!("  - {}", path.display());
    }

    // Find all Ruby files across all discovered paths
    let file_search_start = Instant::now();
    let mut all_files = Vec::new();
    
    for path in all_index_paths {
        if path.exists() {
            let files = find_ruby_files(&path).await?;
            info!("Found {} Ruby files in {}", files.len(), path.display());
            all_files.extend(files);
        } else {
            debug!("Skipping non-existent path: {}", path.display());
        }
    }
    
    let file_search_duration = file_search_start.elapsed();
    info!(
        "Found {} total Ruby files to index in {:?}",
        all_files.len(),
        file_search_duration
    );

    // Process files in parallel for indexing
    let definitions_index_start = Instant::now();
    process_files_parallel(server, all_files.clone(), ProcessingMode::Definitions).await?;
    let definitions_index_duration = definitions_index_start.elapsed();
    info!(
        "Definitions indexing completed in {:?}",
        definitions_index_duration
    );

    // Process files in parallel for references after indexing is complete
    let references_index_start = Instant::now();
    process_files_parallel(
        server,
        all_files,
        ProcessingMode::References {
            include_local_vars: false, // Skip local variable references during workspace init as they are file-scoped
        },
    )
    .await?;
    let references_index_duration = references_index_start.elapsed();
    info!(
        "References indexing completed in {:?}",
        references_index_duration
    );

    let total_duration = start_time.elapsed();
    info!(
        "Workspace initialization completed in {:?} (file search: {:?}, definitions indexing: {:?}, references indexing: {:?})",
        total_duration, file_search_duration, definitions_index_duration, references_index_duration
    );
    Ok(())
}

/// Enum to specify which processing mode to use
#[derive(Clone, Copy, Debug)]
pub enum ProcessingMode {
    /// Process files for definitions (first pass)
    Definitions,
    /// Process files for references (second pass)
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
    if files.is_empty() {
        return Ok(());
    }

    let setup_start = Instant::now();

    let num_cores = thread::available_parallelism()
        .unwrap_or(NonZeroUsize::new(4).unwrap())
        .get();

    // Create a semaphore to limit concurrent tasks
    // Use 75% of available cores to avoid overwhelming the system
    let max_concurrent = std::cmp::max(1, num_cores * 3 / 4);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    let server_clone = server.clone();
    let setup_duration = setup_start.elapsed();

    let mut tasks = JoinSet::new();

    let spawn_start = Instant::now();
    let files_count = files.len();
    for file_path in files {
        let semaphore_clone = semaphore.clone();
        let file_path_clone = file_path.clone();

        let server_task = server_clone.clone();

        tasks.spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();

            let content = match fs::read_to_string(&file_path_clone).await {
                Ok(content) => content,
                Err(e) => {
                    error!("Error reading file {}: {}", file_path_clone.display(), e);
                    return Err(anyhow::anyhow!("Failed to read file: {}", e));
                }
            };

            let uri = match Url::from_file_path(&file_path_clone) {
                Ok(uri) => uri,
                Err(_) => {
                    return Err(anyhow::anyhow!("Failed to convert file path to URI"));
                }
            };

            let document = RubyDocument::new(uri.clone(), content, 0);
            server_task
                .docs
                .lock()
                .insert(uri.clone(), Arc::new(RwLock::new(document)));

            let result = match mode {
                ProcessingMode::Definitions => {
                    process_file_for_definitions(&server_task, uri.clone()).map_err(|e| {
                        anyhow::anyhow!("Failed to index file {}: {}", file_path_clone.display(), e)
                    })
                }
                ProcessingMode::References { include_local_vars } => {
                    process_file_for_references(&server_task, uri.clone(), include_local_vars)
                        .map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to process references in file {}: {}",
                                file_path_clone.display(),
                                e
                            )
                        })
                }
            };

            server_task.docs.lock().remove(&uri);

            result
        });
    }
    let spawn_duration = spawn_start.elapsed();
    debug!(
        "Spawned {} indexing tasks in {:?}",
        files_count, spawn_duration
    );

    // Wait for all tasks to complete
    let wait_start = Instant::now();
    let mut completed_tasks = 0;
    let mut failed_tasks = 0;

    while let Some(result) = tasks.join_next().await {
        completed_tasks += 1;
        if let Err(e) = result {
            failed_tasks += 1;
            error!("Task panicked: {}", e);
        } else if let Err(e) = result.unwrap() {
            failed_tasks += 1;
            error!("{}", e);
        }

        // Log progress periodically
        if completed_tasks % 100 == 0 || completed_tasks == files_count {
            debug!(
                "Indexed {}/{} files ({} failed)",
                completed_tasks, files_count, failed_tasks
            );
        }
    }

    let wait_duration = wait_start.elapsed();
    let total_duration = setup_start.elapsed();

    info!(
        "Parallel {:?} completed in {:?} (setup: {:?}, spawn: {:?}, processing: {:?})",
        mode, total_duration, setup_duration, spawn_duration, wait_duration
    );

    info!(
        "Successfully completed {:?} for {} files ({} failed)",
        mode,
        files_count - failed_tasks,
        failed_tasks
    );

    Ok(())
}

pub async fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut ruby_files = Vec::new();
    let mut dirs_to_process = vec![dir.to_path_buf()];

    while let Some(current_dir) = dirs_to_process.pop() {
        let mut entries = match fs::read_dir(&current_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                debug!("Error reading directory {}: {}", current_dir.display(), e);
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_dir() {
                dirs_to_process.push(path);
            } else if let Some(ext) = path.extension() {
                if ext == "rb" {
                    ruby_files.push(path);
                }
            }
        }
    }

    Ok(ruby_files)
}

pub fn process_file_for_definitions(server: &RubyLanguageServer, uri: Url) -> Result<(), String> {
    // Remove any existing entries for this URI
    server.index().lock().remove_entries_for_uri(&uri);

    // Get the document from the server's docs HashMap
    let document = match server.docs.lock().get(&uri) {
        Some(doc) => doc.clone(),
        None => {
            return Err(format!("Document not found for URI: {}", uri));
        }
    };

    // Parse the file
    let doc_guard = document.read();
    let parse_result = parse(doc_guard.content.as_bytes());
    let node = parse_result.node();

    // Create a visitor and process the AST
    let mut visitor = IndexVisitor::new(server, uri.clone());
    visitor.visit(&node);

    // Persist mutations (like local variable indexes) back to the server's document store
    // TODO: This is a temporary fix. We should be able to mutate the document in place
    //       using docs: Arc<Mutex<HashMap<Url, Arc<Mutex<RubyDocument>>>>> @server.rs
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(visitor.document.clone())));

    debug!("Indexed file: {}", uri);
    Ok(())
}

/// Process a file for references after indexing is complete
pub fn process_file_for_references(
    server: &RubyLanguageServer,
    uri: Url,
    include_local_vars: bool,
) -> Result<(), String> {
    // Remove any existing references for this URI
    server.index().lock().remove_references_for_uri(&uri);

    // Get the document from the server's docs HashMap
    let document = match server.docs.lock().get(&uri) {
        Some(doc) => doc.clone(),
        None => {
            return Err(format!("Document not found for URI: {}", uri));
        }
    };

    // Parse the file
    let doc_guard = document.read();
    let parse_result = parse(doc_guard.content.as_bytes());
    let node = parse_result.node();

    // Create a reference visitor and process the AST
    let mut visitor = ReferenceVisitor::with_options(server, uri.clone(), include_local_vars);
    visitor.visit(&node);

    debug!("Processed references in file: {}", uri);
    Ok(())
}
