use anyhow::Result;
use log::{info, warn};
use lsp_types::Url;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use walkdir::WalkDir;

use crate::parser::document::RubyDocument;

/// Manages workspace files and provides indexing functionality
pub struct WorkspaceManager {
    /// Root URI of the workspace
    root_uri: Option<Url>,
    /// Map of file URIs to documents (for files that aren't open but have been indexed)
    indexed_documents: Arc<RwLock<HashMap<Url, RubyDocument>>>,
    /// File extensions to consider as Ruby files
    ruby_extensions: Vec<String>,
}

impl WorkspaceManager {
    /// Create a new workspace manager
    pub fn new() -> Self {
        Self {
            root_uri: None,
            indexed_documents: Arc::new(RwLock::new(HashMap::new())),
            ruby_extensions: vec![
                ".rb".to_string(),
                ".rake".to_string(),
                ".gemspec".to_string(),
                ".ru".to_string(),
            ],
        }
    }

    /// Set the root URI of the workspace
    pub fn set_root_uri(&mut self, uri: Url) -> Result<()> {
        self.root_uri = Some(uri);
        Ok(())
    }

    /// Get the root URI of the workspace
    pub fn get_root_uri(&self) -> Option<&Url> {
        self.root_uri.as_ref()
    }

    /// Get the root path of the workspace
    fn get_root_path(&self) -> Option<PathBuf> {
        self.root_uri
            .as_ref()
            .and_then(|uri| uri.to_file_path().ok())
    }

    /// Check if a file is a Ruby file based on its extension
    fn is_ruby_file(&self, path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                return self
                    .ruby_extensions
                    .iter()
                    .any(|e| e == &format!(".{}", ext_str));
            }
        }

        // Also check if the filename ends with any of the extensions
        // This handles cases where the file might not have a proper extension
        if let Some(filename) = path.file_name() {
            if let Some(filename_str) = filename.to_str() {
                return self
                    .ruby_extensions
                    .iter()
                    .any(|e| filename_str.ends_with(e));
            }
        }

        false
    }

    /// Scan the workspace for Ruby files and index them
    pub fn scan_workspace(&self) -> Result<usize> {
        let root_path = match self.get_root_path() {
            Some(path) => path,
            None => {
                warn!("Cannot scan workspace: root URI not set");
                return Ok(0);
            }
        };

        info!("Scanning workspace for Ruby files: {:?}", root_path);

        let mut indexed_count = 0;
        let mut indexed_docs = self.indexed_documents.write().unwrap();

        for entry in WalkDir::new(&root_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check if it's a Ruby file
            if self.is_ruby_file(path) {
                // Convert path to URI
                if let Ok(uri) = Url::from_file_path(path) {
                    // Read file content
                    match fs::read_to_string(path) {
                        Ok(content) => {
                            // Create a document with version 0 (not open)
                            let document = RubyDocument::new(content, 0);
                            indexed_docs.insert(uri, document);
                            indexed_count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to read file {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        info!("Indexed {} Ruby files in workspace", indexed_count);
        Ok(indexed_count)
    }

    /// Get a document from the index by URI
    pub fn get_document(&self, uri: &Url) -> Option<RubyDocument> {
        let indexed_docs = self.indexed_documents.read().unwrap();
        indexed_docs.get(uri).cloned()
    }

    /// Check if a document is indexed
    pub fn is_document_indexed(&self, uri: &Url) -> bool {
        let indexed_docs = self.indexed_documents.read().unwrap();
        indexed_docs.contains_key(uri)
    }

    /// Get all indexed document URIs
    pub fn get_all_document_uris(&self) -> Vec<Url> {
        let indexed_docs = self.indexed_documents.read().unwrap();
        indexed_docs.keys().cloned().collect()
    }

    /// Update the index for a specific file
    pub fn update_index_for_file(&self, uri: &Url) -> Result<bool> {
        if let Ok(path) = uri.to_file_path() {
            if self.is_ruby_file(&path) {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let document = RubyDocument::new(content, 0);
                        let mut indexed_docs = self.indexed_documents.write().unwrap();
                        indexed_docs.insert(uri.clone(), document);
                        return Ok(true);
                    }
                    Err(e) => {
                        warn!("Failed to read file {}: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Remove a file from the index
    pub fn remove_from_index(&self, uri: &Url) {
        let mut indexed_docs = self.indexed_documents.write().unwrap();
        indexed_docs.remove(uri);
    }

    /// Get all indexed files with their documents
    pub fn get_indexed_files(&self) -> Vec<(Url, RubyDocument)> {
        let indexed_docs = self.indexed_documents.read().unwrap();
        indexed_docs.iter().map(|(uri, doc)| (uri.clone(), doc.clone())).collect()
    }
}
