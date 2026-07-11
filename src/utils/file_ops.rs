//! File System Utilities
//!
//! Common utility functions for:
//! - File collection and filtering
//! - Ruby file detection
//! - Path utilities for distinguishing project vs external files

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;
use walkdir::{DirEntry, WalkDir};

use crate::config::IndexingConfig;

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

/// Collect project files using workspace-relative glob configuration.
///
/// Standard Ruby files are included by default. Included patterns may add
/// nonstandard files such as `bin/console`. Excluded patterns are applied last
/// and therefore always win. `.git` is never traversed.
pub fn collect_project_files(dir: &Path, config: &IndexingConfig) -> Result<Vec<PathBuf>> {
    let included = build_glob_set("includedPatterns", &config.included_patterns)?;
    let excluded = build_glob_set("excludedPatterns", &config.excluded_patterns)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(is_not_git_directory)
    {
        let entry = entry.map_err(|error| {
            anyhow::anyhow!(
                "Failed to walk project directory {}: {}",
                dir.display(),
                error
            )
        })?;
        if !entry.file_type().is_file() {
            continue;
        }

        let relative = entry.path().strip_prefix(dir).map_err(|error| {
            anyhow::anyhow!(
                "INVARIANT VIOLATED: walked path {} is outside workspace {}: {}. This is a bug because project globs must be workspace-relative. Fix: keep WalkDir rooted at the workspace.",
                entry.path().display(),
                dir.display(),
                error
            )
        })?;
        let is_included = should_index_file(entry.path()) || included.is_match(relative);
        if is_included && !excluded.is_match(relative) {
            files.push(entry.into_path());
        }
    }

    files.sort();
    Ok(files)
}

fn build_glob_set(setting: &str, patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            anyhow::anyhow!(
                "Invalid rubyFastLsp.indexing.{} glob {:?}: {}",
                setting,
                pattern,
                error
            )
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        anyhow::anyhow!(
            "Failed to compile rubyFastLsp.indexing.{} globs: {}",
            setting,
            error
        )
    })
}

fn is_not_git_directory(entry: &DirEntry) -> bool {
    !(entry.file_type().is_dir() && entry.file_name() == ".git")
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
// Path Classification
// ============================================================================

/// Check if a URI belongs to a project file (not stdlib, gem, or stubs)
///
/// Uses path-based heuristics; callers with analysis-engine source metadata
/// should prefer that instead.
pub fn is_project_file(uri: &Url) -> bool {
    if let Ok(file_path) = uri.to_file_path() {
        let path_str = file_path.to_string_lossy();

        // Check for rubystubs (bundled with extension or system)
        if path_str.contains("/rubystubs") {
            return false;
        }

        // Check if the file is in common stdlib or gem paths
        let is_stdlib_or_gem = path_str.contains("/ruby/")
            && (path_str.contains("/lib/ruby/")
                || path_str.contains("/gems/")
                || path_str.contains("/site_ruby/")
                || path_str.contains("/vendor_ruby/"));

        !is_stdlib_or_gem
    } else {
        true
    }
}

// ============================================================================
// File Processing Helpers
// ============================================================================

/// Convert a file path to a URI
pub fn path_to_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("Failed to convert path to URI: {:?}", path))
}

/// Read file content asynchronously
pub async fn read_file_async(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", path, e))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
        assert_eq!(filenames, RUBY_FILENAMES);
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
        let gem_uri =
            Url::parse("file:///usr/lib/ruby/gems/3.0.0/gems/rails-7.0.0/lib/rails.rb").unwrap();
        assert!(!is_project_file(&gem_uri));
    }

    #[test]
    fn test_collect_project_files_applies_patterns_with_exclusions_winning() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("vendor/generated")).unwrap();
        std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
        std::fs::write(root.join("app/user.rb"), "class User; end").unwrap();
        std::fs::write(root.join("app/show.html.erb"), "<%= User %>").unwrap();
        std::fs::write(root.join("bin/console"), "puts :console").unwrap();
        std::fs::write(root.join("vendor/generated/model.rb"), "class Model; end").unwrap();
        std::fs::write(root.join("vendor/generated/keep.rb"), "class Keep; end").unwrap();
        std::fs::write(root.join(".git/hooks/pre-commit"), "puts :hidden").unwrap();

        let config = IndexingConfig {
            included_patterns: vec!["bin/*".to_string(), "vendor/generated/keep.rb".to_string()],
            excluded_patterns: vec!["vendor/**/*".to_string()],
            ..IndexingConfig::default()
        };

        let files = collect_project_files(root, &config).unwrap();
        let relative = files
            .iter()
            .map(|path| path.strip_prefix(root).unwrap().to_path_buf())
            .collect::<Vec<_>>();

        assert_eq!(
            relative,
            vec![
                PathBuf::from("app/show.html.erb"),
                PathBuf::from("app/user.rb"),
                PathBuf::from("bin/console")
            ]
        );
    }

    #[test]
    fn test_collect_project_files_rejects_invalid_glob() {
        let workspace = TempDir::new().unwrap();
        let config = IndexingConfig {
            excluded_patterns: vec!["[invalid".to_string()],
            ..IndexingConfig::default()
        };

        let error = collect_project_files(workspace.path(), &config).unwrap_err();

        assert!(error.to_string().contains("[invalid"));
    }
}
