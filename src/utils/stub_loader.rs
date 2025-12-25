//! Stub Loader Module
//!
//! Provides functionality to load Ruby stubs from directories.
//! 
//! In production (VSIX), stubs are shipped as zip files and extracted
//! by the VS Code extension on first activation. The LSP server then
//! reads from the extracted directories with proper file:// URIs.

use std::fs;
use std::path::{Path, PathBuf};

/// Find the stubs directory for a given Ruby version
///
/// Searches for stubs in this order:
/// 1. stubs/rubystubsXY/ - extracted stubs (production) or development stubs
///
/// Returns None if no stubs are found for the specified version.
pub fn find_stubs_directory(extension_path: &Path, ruby_version: (u8, u8)) -> Option<PathBuf> {
    let version_name = format!("rubystubs{}{}", ruby_version.0, ruby_version.1);
    let stubs_path = extension_path.join("stubs").join(&version_name);

    if stubs_path.exists() && stubs_path.is_dir() {
        log::debug!("Found stubs directory: {:?}", stubs_path);
        return Some(stubs_path);
    }

    // Try default Ruby 3.0 stubs as fallback
    if ruby_version != (3, 0) {
        log::info!(
            "Stubs for Ruby {}.{} not found, trying Ruby 3.0",
            ruby_version.0,
            ruby_version.1
        );
        return find_stubs_directory(extension_path, (3, 0));
    }

    None
}

/// Get all Ruby files in a stubs directory
pub fn get_stub_files(stubs_dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(stubs_dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let path = e.path();
                    if path.extension().map_or(false, |ext| ext == "rb") {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_stubs_directory() {
        let temp = TempDir::new().unwrap();
        let stubs_dir = temp.path().join("stubs").join("rubystubs30");
        fs::create_dir_all(&stubs_dir).unwrap();

        // Create test stub files
        fs::write(stubs_dir.join("string.rb"), "class String; end").unwrap();
        fs::write(stubs_dir.join("array.rb"), "class Array; end").unwrap();

        let found = find_stubs_directory(temp.path(), (3, 0)).unwrap();
        assert_eq!(found, stubs_dir);

        let files = get_stub_files(&found);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_fallback_to_ruby_30() {
        let temp = TempDir::new().unwrap();
        let stubs_dir = temp.path().join("stubs").join("rubystubs30");
        fs::create_dir_all(&stubs_dir).unwrap();
        fs::write(stubs_dir.join("string.rb"), "class String; end").unwrap();

        // Ask for Ruby 3.4, should fallback to 3.0
        let found = find_stubs_directory(temp.path(), (3, 4)).unwrap();
        assert_eq!(found, stubs_dir);
    }

    #[test]
    fn test_no_stubs_found() {
        let temp = TempDir::new().unwrap();
        let found = find_stubs_directory(temp.path(), (3, 0));
        assert!(found.is_none());
    }
}
