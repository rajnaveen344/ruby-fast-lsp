//! File System Utilities
//!
//! Common utility functions for:
//! - File collection and filtering
//! - Ruby file detection
//! - Path utilities for distinguishing project vs external files

use anyhow::Result;
use std::path::{Path, PathBuf};

// ============================================================================
// File Detection
// ============================================================================

// Keep these tables synchronized with the canonical editor policy. The test
// below fails if server discovery and the packaged client list drift.
const RUBY_EXTENSIONS: &[&str] = &[
    "rb",
    "builder",
    "eye",
    "fcgi",
    "gemspec",
    "god",
    "irbrc",
    "jbuilder",
    "mspec",
    "pluginspec",
    "podspec",
    "prawn",
    "pryrc",
    "rabl",
    "rake",
    "rbi",
    "rbuild",
    "rbw",
    "rbx",
    "ru",
    "ruby",
    "spec",
    "thor",
    "watchr",
];

const ERB_EXTENSIONS: &[&str] = &["erb", "rhtml", "rhtm"];

const RUBY_FILENAMES: &[&str] = &[
    ".irbrc",
    ".pryrc",
    ".simplecov",
    "Appraisals",
    "Berksfile",
    "Brewfile",
    "Buildfile",
    "Capfile",
    "Dangerfile",
    "Deliverfile",
    "Fastfile",
    "Gemfile",
    "Guardfile",
    "Jarfile",
    "Mavenfile",
    "Podfile",
    "Puppetfile",
    "Rakefile",
    "Snapfile",
    "Steepfile",
    "Thorfile",
    "Vagrantfile",
];

/// Check if a file should be indexed based on its extension and name.
///
/// Returns true for common Ruby/ERB extensions and conventional Ruby DSL
/// filenames.
pub fn should_index_file(path: &Path) -> bool {
    if let Some(extension) = path.extension() {
        extension.to_str().is_some_and(|extension| {
            RUBY_EXTENSIONS.contains(&extension) || ERB_EXTENSIONS.contains(&extension)
        })
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| RUBY_FILENAMES.contains(&name))
    }
}

// ============================================================================
// File Collection
// ============================================================================

/// Find all Ruby files in a directory (recursive)
/// Wrapper around collect_ruby_files for compatibility
pub fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(collect_ruby_files(dir))
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
///
/// Only skips `.git` directory. All other directories are traversed.
/// File source (Project/Gem/Stdlib) is determined by the indexers based on
/// discovered paths from tools (bundler, rubygems, ruby), not by exclusion patterns.
fn collect_ruby_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Only skip .git - everything else is traversed
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if dir_name != ".git" {
                        collect_ruby_files_recursive(&path, files);
                    }
                }
            } else if should_index_file(&path) {
                files.push(path);
            }
        }
    }
}

// ============================================================================
// File Processing Helpers
// ============================================================================

// ============================================================================
// Tests
// ============================================================================

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
        assert!(should_index_file(&PathBuf::from("show.html.erb")));
        assert!(should_index_file(&PathBuf::from("config.ru")));
        assert!(should_index_file(&PathBuf::from("tasks.thor")));
        assert!(should_index_file(&PathBuf::from("show.json.jbuilder")));
        assert!(should_index_file(&PathBuf::from("types.rbi")));
        assert!(should_index_file(&PathBuf::from("plugin.podspec")));

        // Test special Ruby files
        assert!(should_index_file(&PathBuf::from("Rakefile")));
        assert!(should_index_file(&PathBuf::from("Gemfile")));
        assert!(should_index_file(&PathBuf::from("Guardfile")));
        assert!(should_index_file(&PathBuf::from("Thorfile")));
        assert!(should_index_file(&PathBuf::from("Fastfile")));
        assert!(should_index_file(&PathBuf::from(".simplecov")));

        // Test non-Ruby files
        assert!(!should_index_file(&PathBuf::from("test.txt")));
        assert!(!should_index_file(&PathBuf::from("test.js")));
        assert!(!should_index_file(&PathBuf::from("README.md")));
    }

    #[test]
    fn editor_file_kind_policy_matches_server_discovery() {
        let policy: serde_json::Value = serde_json::from_str(include_str!(
            "../../editors/vscode/vsix/ruby_file_kinds.json"
        ))
        .expect("canonical Ruby file-kind policy must be valid JSON");
        let extensions = |key: &str| {
            policy[key]
                .as_array()
                .expect("file-kind extensions must be an array")
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .expect("file-kind extension must be a string")
                        .trim_start_matches('.')
                })
                .collect::<Vec<_>>()
        };
        let filenames = policy["rubyFilenames"]
            .as_array()
            .expect("Ruby filenames must be an array")
            .iter()
            .map(|value| value.as_str().expect("Ruby filename must be a string"))
            .collect::<Vec<_>>();

        assert_eq!(extensions("rubyExtensions"), RUBY_EXTENSIONS);
        assert_eq!(extensions("erbExtensions"), ERB_EXTENSIONS);
        assert_eq!(extensions("signatureExtensions"), ["rbs"]);
        assert_eq!(filenames, RUBY_FILENAMES);
    }
}
