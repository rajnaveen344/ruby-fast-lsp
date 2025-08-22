//! Utility functions shared across indexer modules
//!
//! This module contains common functionality used by multiple indexer components,
//! such as file collection, Ruby file detection, and path utilities.

use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;

/// Check if a file should be indexed based on its extension and name
/// 
/// Returns true for:
/// - Files with .rb, .ruby, .rake, or .gemspec extensions
/// - Special Ruby files without extensions (Rakefile, Gemfile, etc.)
pub fn should_index_file(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        matches!(extension.to_str(), Some("rb" | "ruby" | "rake" | "gemspec"))
    } else {
        // Check for files without extensions that might be Ruby
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            matches!(
                file_name,
                "Rakefile" | "Gemfile" | "Guardfile" | "Capfile" | "Vagrantfile"
            )
        } else {
            false
        }
    }
}

/// Collect Ruby files recursively from a directory
/// 
/// This function walks through a directory tree and collects all Ruby files,
/// while skipping common directories that don't contain indexable Ruby files.
pub fn collect_ruby_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_ruby_files_recursive(dir, &mut files);
    files
}

/// Recursively collect Ruby files from a directory (internal helper)
fn collect_ruby_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Skip common directories that don't contain indexable Ruby files
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !matches!(
                        dir_name,
                        ".git"
                            | ".svn"
                            | "node_modules"
                            | "tmp"
                            | "log"
                            | "coverage"
                            | ".bundle"
                            | "vendor"
                    ) {
                        collect_ruby_files_recursive(&path, files);
                    }
                }
            } else if should_index_file(&path) {
                files.push(path);
            }
        }
    }
}

/// Check if a URI belongs to a project file (not stdlib or gem)
/// 
/// This function helps distinguish between project files and external dependencies
/// by checking common stdlib and gem path patterns.
pub fn is_project_file(uri: &Url) -> bool {
    if let Ok(file_path) = uri.to_file_path() {
        let path_str = file_path.to_string_lossy();
        
        // Check if the file is in common stdlib or gem paths
        let is_stdlib_or_gem = path_str.contains("/ruby/") && 
            (path_str.contains("/lib/ruby/") || 
             path_str.contains("/gems/") ||
             path_str.contains("/rubystubs") ||
             path_str.contains("/site_ruby/") ||
             path_str.contains("/vendor_ruby/"));
        
        // If it's not in stdlib/gem paths, consider it a project file
        !is_stdlib_or_gem
    } else {
        // If we can't convert to file path, assume it's a project file
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_should_index_file() {
        // Test Ruby files
        assert!(should_index_file(&PathBuf::from("test.rb")));
        assert!(should_index_file(&PathBuf::from("test.rake")));
        assert!(should_index_file(&PathBuf::from("test.gemspec")));
        
        // Test special Ruby files
        assert!(should_index_file(&PathBuf::from("Rakefile")));
        assert!(should_index_file(&PathBuf::from("Gemfile")));
        assert!(should_index_file(&PathBuf::from("Guardfile")));
        
        // Test non-Ruby files
        assert!(!should_index_file(&PathBuf::from("test.txt")));
        assert!(!should_index_file(&PathBuf::from("test.js")));
        assert!(!should_index_file(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_is_project_file() {
        // Test project files
        let project_uri = Url::parse("file:///home/user/project/app/models/user.rb").unwrap();
        assert!(is_project_file(&project_uri));
        
        // Test stdlib files (would return false)
        let stdlib_uri = Url::parse("file:///usr/lib/ruby/3.0.0/json.rb").unwrap();
        assert!(!is_project_file(&stdlib_uri));
        
        // Test gem files (would return false)
        let gem_uri = Url::parse("file:///usr/lib/ruby/gems/3.0.0/gems/rails-7.0.0/lib/rails.rb").unwrap();
        assert!(!is_project_file(&gem_uri));
    }
}