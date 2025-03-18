use anyhow::Result;
use log::{error, info};
use lsp_types::Url;
use std::path::{Path, PathBuf};
use tokio::fs;

use super::traverser::RubyIndexer;

pub async fn init_workspace(indexer: &mut RubyIndexer, folder_uri: Url) -> Result<()> {
    info!("Indexing workspace folder: {}", folder_uri);

    // Convert URI to filesystem path
    let folder_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert URI to file path: {}", folder_uri))?;

    // Find all Ruby files in the workspace
    let files = find_ruby_files(&folder_path).await?;
    info!("Found {} Ruby files to index", files.len());

    // Index each file
    for file_path in files {
        if let Err(e) = index_workspace_file(indexer, &file_path).await {
            error!("Error indexing file {}: {:?}", file_path.display(), e);
        }
    }

    info!("Workspace indexing completed");
    Ok(())
}

// Handler for didOpen notification
pub fn file_opened(indexer: &mut RubyIndexer, uri: Url, content: &str) -> Result<(), String> {
    // Remove any existing entries for this file (in case it was previously indexed)
    indexer.index_mut().remove_entries_for_uri(&uri);

    // Index the file
    indexer.index_file_with_uri(uri, content)
}

// Handler for didChange/didClose notification
pub fn file_changed(indexer: &mut RubyIndexer, uri: Url, content: &str) -> Result<(), String> {
    // Remove existing entries
    indexer.index_mut().remove_entries_for_uri(&uri);

    // Re-index with new content
    indexer.index_file_with_uri(uri, content)
}

pub fn file_created(_indexer: &mut RubyIndexer, _uri: Url) -> Result<(), String> {
    todo!()
}

pub fn file_renamed(
    _indexer: &mut RubyIndexer,
    _old_uri: Url,
    _new_uri: Url,
) -> Result<(), String> {
    todo!()
}

pub fn file_deleted(_indexer: &mut RubyIndexer, _uri: Url) -> Result<(), String> {
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
                info!("Error reading directory {}: {}", current_dir.display(), e);
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

// Helper to index a single workspace file
async fn index_workspace_file(indexer: &mut RubyIndexer, file_path: &Path) -> Result<()> {
    // Read the file content
    let content = fs::read_to_string(file_path).await?;

    // Create a file URI
    let uri = Url::from_file_path(file_path)
        .map_err(|_| anyhow::anyhow!("Failed to convert file path to URI"))?;

    // Index the file
    indexer
        .index_file_with_uri(uri, &content)
        .map_err(|e| anyhow::anyhow!("Failed to index file: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
