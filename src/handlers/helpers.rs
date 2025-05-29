use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{debug, error, info};
use lsp_types::*;
use ruby_prism::{parse, Visit};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> Result<()> {
    let start_time = Instant::now();
    info!("Indexing workspace folder: {}", folder_uri);

    // Convert URI to filesystem path
    let folder_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert URI to file path: {}", folder_uri))?;

    // Find all Ruby files in the workspace
    let file_search_start = Instant::now();
    let files = find_ruby_files(&folder_path).await?;
    let file_search_duration = file_search_start.elapsed();
    info!(
        "Found {} Ruby files to index in {:?}",
        files.len(),
        file_search_duration
    );

    // Process files in parallel
    let indexing_start = Instant::now();
    process_files_parallel(server, files).await?;
    let indexing_duration = indexing_start.elapsed();

    let total_duration = start_time.elapsed();
    info!(
        "Workspace indexing completed in {:?} (file search: {:?}, indexing: {:?})",
        total_duration, file_search_duration, indexing_duration
    );
    Ok(())
}

pub async fn process_files_parallel(
    server: &RubyLanguageServer,
    files: Vec<PathBuf>,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let setup_start = Instant::now();

    // Get the number of logical cores for optimal parallelism
    let num_cores = std::thread::available_parallelism()
        .unwrap_or(NonZeroUsize::new(4).unwrap())
        .get();

    // Create a semaphore to limit concurrent tasks
    // Use 75% of available cores to avoid overwhelming the system
    let max_concurrent = std::cmp::max(1, num_cores * 3 / 4);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    debug!(
        "Parallelizing indexing with {} concurrent workers for {} files",
        max_concurrent,
        files.len()
    );

    // Create a shared server reference
    let index_ref = server.index();
    let setup_duration = setup_start.elapsed();
    debug!("Parallel indexing setup completed in {:?}", setup_duration);

    // Create a set to track all spawned tasks
    let mut tasks = JoinSet::new();

    // Spawn tasks for each file
    let spawn_start = Instant::now();
    let files_count = files.len();
    for file_path in files {
        let semaphore_clone = semaphore.clone();
        let index_clone = index_ref.clone();
        let file_path_clone = file_path.clone();

        // Spawn a task for each file
        tasks.spawn(async move {
            // Acquire a permit from the semaphore
            let _permit = semaphore_clone.acquire().await.unwrap();

            // Read the file content
            let content = match fs::read_to_string(&file_path_clone).await {
                Ok(content) => content,
                Err(e) => {
                    error!("Error reading file {}: {}", file_path_clone.display(), e);
                    return Err(anyhow::anyhow!("Failed to read file: {}", e));
                }
            };

            // Create a file URI
            let uri = match Url::from_file_path(&file_path_clone) {
                Ok(uri) => uri,
                Err(_) => {
                    return Err(anyhow::anyhow!("Failed to convert file path to URI"));
                }
            };

            // Process the file
            process_file_for_indexing(index_clone, uri, &content).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to process file {}: {}",
                    file_path_clone.display(),
                    e
                )
            })
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

    debug!(
        "Parallel indexing completed in {:?} (setup: {:?}, spawn: {:?}, processing: {:?})",
        total_duration, setup_duration, spawn_duration, wait_duration
    );
    info!(
        "Successfully indexed {} files ({} failed)",
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

pub fn process_file_for_indexing(
    index: Arc<std::sync::Mutex<RubyIndex>>,
    uri: Url,
    content: &str,
) -> Result<(), String> {
    // Remove any existing entries for this URI
    index.lock().unwrap().remove_entries_for_uri(&uri);

    // Parse the file
    let parse_result = parse(content.as_bytes());
    let node = parse_result.node();

    // Create a visitor and process the AST
    let mut visitor = IndexVisitor::new(index, uri.clone(), content.to_string());
    visitor.visit(&node);

    debug!("Processed file: {}", uri);
    Ok(())
}
