use anyhow::Result;
use log::{debug, error, info};
use lsp_types::Url;
use ruby_prism::{parse, Visit};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::server::RubyLanguageServer;

use super::index::RubyIndex;

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

// Process files in parallel using a semaphore to limit concurrency
async fn process_files_parallel(server: &RubyLanguageServer, files: Vec<PathBuf>) -> Result<()> {
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
            process_single_file(index_clone, uri, content).map_err(|e| {
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

// Process a single file with the given index, URI, and content
fn process_single_file(
    index: Arc<std::sync::Mutex<RubyIndex>>,
    uri: Url,
    content: String,
) -> Result<(), String> {
    // Remove any existing entries for this URI
    index.lock().unwrap().remove_entries_for_uri(&uri);

    // Parse the file
    let parse_result = parse(content.as_bytes());
    let node = parse_result.node();

    // Create a visitor and process the AST
    let mut visitor = IndexVisitor::new(index, uri.clone(), content.clone());
    visitor.visit(&node);

    debug!("Processed file: {}", uri);
    Ok(())
}

// Handler for didOpen notification
pub fn file_opened(server: &RubyLanguageServer, uri: Url, content: &str) -> Result<(), String> {
    // Remove any existing entries for this file (in case it was previously indexed)
    server.index.lock().unwrap().remove_entries_for_uri(&uri);

    // Index the file
    process_single_file(server.index.clone(), uri, content.to_string())
}

// Handler for didChange/didClose notification
pub fn file_changed(server: &RubyLanguageServer, uri: Url, content: &str) -> Result<(), String> {
    // Remove existing entries
    server.index.lock().unwrap().remove_entries_for_uri(&uri);

    // Index the file
    process_single_file(server.index.clone(), uri, content.to_string())
}

pub fn file_created(server: &RubyLanguageServer, _uri: Url) -> Result<(), String> {
    let _index = server.index.lock().unwrap();
    todo!()
}

pub fn file_renamed(
    server: &RubyLanguageServer,
    _old_uri: Url,
    _new_uri: Url,
) -> Result<(), String> {
    let _index = server.index.lock().unwrap();
    todo!()
}

pub fn file_deleted(server: &RubyLanguageServer, _uri: Url) -> Result<(), String> {
    let _index = server.index.lock().unwrap();
    todo!()
}

// Helper to find all Ruby files in a directory recursively
async fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
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

// This function is kept for reference but is no longer used directly
// The functionality has been moved to the parallel processing implementation
#[allow(dead_code)]
async fn index_workspace_file(server: &mut RubyLanguageServer, file_path: &Path) -> Result<()> {
    // Read the file content
    let content = fs::read_to_string(file_path).await?;

    // Create a file URI
    let uri = Url::from_file_path(file_path)
        .map_err(|_| anyhow::anyhow!("Failed to convert file path to URI"))?;

    // Index the file
    server
        .process_file(uri.clone(), &content)
        .map_err(|e| anyhow::anyhow!("Failed to index file: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_find_ruby_files() -> Result<()> {
        // Create a temporary directory structure
        let temp_dir = tempdir()?;
        let temp_path = temp_dir.path();

        // Create some nested directories
        let subdir1 = temp_path.join("subdir1");
        let subdir2 = temp_path.join("subdir2");
        let subdir3 = subdir1.join("subdir3");

        fs::create_dir(&subdir1).await?;
        fs::create_dir(&subdir2).await?;
        fs::create_dir(&subdir3).await?;

        // Create some Ruby files
        let ruby_file1 = temp_path.join("file1.rb");
        let ruby_file2 = subdir1.join("file2.rb");
        let ruby_file3 = subdir3.join("file3.rb");
        let non_ruby_file = temp_path.join("file4.txt");

        // Write some content to the files
        File::create(&ruby_file1)
            .await?
            .write_all(b"puts 'hello'")
            .await?;
        File::create(&ruby_file2)
            .await?
            .write_all(b"puts 'world'")
            .await?;
        File::create(&ruby_file3)
            .await?
            .write_all(b"puts '!'")
            .await?;
        File::create(&non_ruby_file)
            .await?
            .write_all(b"not ruby")
            .await?;

        // Find Ruby files
        let ruby_files = find_ruby_files(temp_path).await?;

        // Verify the results
        assert_eq!(ruby_files.len(), 3);
        assert!(ruby_files.contains(&ruby_file1));
        assert!(ruby_files.contains(&ruby_file2));
        assert!(ruby_files.contains(&ruby_file3));

        Ok(())
    }

    #[tokio::test]
    async fn test_parallel_processing() -> Result<()> {
        // Create a temporary directory structure with many files
        let temp_dir = tempdir()?;
        let temp_path = temp_dir.path();

        // Create 500 Ruby files to test parallel processing
        let mut file_paths = Vec::new();
        for i in 0..500 {
            let file_path = temp_path.join(format!("file{}.rb", i));
            File::create(&file_path)
                .await?
                .write_all(
                    format!(
                        "class TestClass{} \n  def method{} \n    puts 'test' \n  end \nend",
                        i, i
                    )
                    .as_bytes(),
                )
                .await?;
            file_paths.push(file_path.clone());
        }

        // Create a server for parallel processing
        let parallel_server = RubyLanguageServer::default();

        // Process files in parallel and measure time
        let start_time = Instant::now();
        process_files_parallel(&parallel_server, file_paths.clone()).await?;
        let parallel_duration = start_time.elapsed();

        println!("Parallel processing took: {:?}", parallel_duration);

        // Verify that the index contains entries for all the classes
        let index_ref = parallel_server.index();
        let index = index_ref.lock().unwrap();
        assert_eq!(index.file_entries.len(), 500);

        // Check that we have 500 class definitions
        let mut class_count = 0;
        for entries in index.definitions.values() {
            for entry in entries {
                if let crate::indexer::entry::entry_kind::EntryKind::Class { .. } = entry.kind {
                    class_count += 1;
                }
            }
        }

        assert_eq!(class_count, 500);

        // Now test sequential processing for comparison
        let mut sequential_server = RubyLanguageServer::default();

        // Process files sequentially and measure time
        let start_time = Instant::now();
        for file_path in &file_paths {
            if let Err(e) = index_workspace_file(&mut sequential_server, file_path).await {
                error!("Error indexing file {}: {:?}", file_path.display(), e);
            }
        }
        let sequential_duration = start_time.elapsed();

        println!("Sequential processing took: {:?}", sequential_duration);
        println!(
            "Speedup factor: {:.2}x",
            sequential_duration.as_secs_f64() / parallel_duration.as_secs_f64()
        );

        // Verify sequential indexing results
        let index_ref = sequential_server.index();
        let index = index_ref.lock().unwrap();
        assert_eq!(index.file_entries.len(), 500);

        Ok(())
    }
}
