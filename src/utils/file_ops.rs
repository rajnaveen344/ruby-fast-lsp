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

const DEFAULT_EXTERNAL_DIRECTORIES: &[&str] = &[
    ".bundle",
    ".ruby-fast-lsp",
    ".ruby-lsp",
    "coverage",
    "log",
    "node_modules",
    "tmp",
    "vendor",
];

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

pub struct ProjectFilePolicy {
    included: GlobSet,
    excluded: GlobSet,
}

impl ProjectFilePolicy {
    pub fn new(config: &IndexingConfig) -> Result<Self> {
        Ok(Self {
            included: build_glob_set("includedPatterns", &config.included_patterns)?,
            excluded: build_glob_set("excludedPatterns", &config.excluded_patterns)?,
        })
    }

    pub fn includes(&self, workspace_root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(workspace_root) else {
            return false;
        };
        if is_git_path(relative) || is_rbs_file(path) {
            return false;
        }
        let explicitly_included = self.included.is_match(relative);
        let is_owned_default = should_index_file(path) && !is_default_external_path(relative);
        (explicitly_included || is_owned_default) && !self.excluded.is_match(relative)
    }

    pub fn includes_signature(&self, workspace_root: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(workspace_root) else {
            return false;
        };
        if is_git_path(relative) || !is_rbs_file(path) {
            return false;
        }
        let conventional = relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "sig")
            && !is_default_external_path(relative);
        (conventional || self.included.is_match(relative)) && !self.excluded.is_match(relative)
    }
}

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
    let policy = ProjectFilePolicy::new(config)?;
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

        if policy.includes(dir, entry.path()) {
            files.push(entry.into_path());
        }
    }

    files.sort();
    Ok(files)
}

pub fn collect_project_signature_files(
    dir: &Path,
    config: &IndexingConfig,
) -> Result<Vec<PathBuf>> {
    let policy = ProjectFilePolicy::new(config)?;
    let mut files = Vec::new();
    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(is_not_git_directory)
    {
        let entry = entry.map_err(|error| {
            anyhow::anyhow!(
                "Failed to walk project signature directory {}: {}",
                dir.display(),
                error
            )
        })?;
        if entry.file_type().is_file() && policy.includes_signature(dir, entry.path()) {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn is_rbs_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rbs")
}

fn is_git_path(relative: &Path) -> bool {
    relative
        .components()
        .any(|component| component.as_os_str() == ".git")
}

fn is_default_external_path(relative: &Path) -> bool {
    relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| DEFAULT_EXTERNAL_DIRECTORIES.contains(&name))
    })
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
        assert_eq!(extensions("signatureExtensions"), ["rbs"]);
        assert_eq!(filenames, RUBY_FILENAMES);
    }

    #[test]
    fn project_signature_files_use_conventional_sig_and_pattern_precedence() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        for directory in ["sig", "types", "vendor/sig", ".git/sig"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in [
            "sig/native.rbs",
            "sig/excluded.rbs",
            "types/generated.rbs",
            "vendor/sig/hidden.rbs",
            ".git/sig/hidden.rbs",
        ] {
            std::fs::write(root.join(file), "class Native\nend\n").unwrap();
        }
        let config = IndexingConfig {
            included_patterns: vec!["types/*.rbs".to_string(), "vendor/sig/*.rbs".to_string()],
            excluded_patterns: vec!["sig/excluded.rbs".to_string()],
            ..IndexingConfig::default()
        };

        let files = collect_project_signature_files(root, &config).unwrap();
        let relative = files
            .iter()
            .map(|path| path.strip_prefix(root).unwrap().to_path_buf())
            .collect::<Vec<_>>();
        assert_eq!(
            relative,
            vec![
                PathBuf::from("sig/native.rbs"),
                PathBuf::from("types/generated.rbs"),
                PathBuf::from("vendor/sig/hidden.rbs"),
            ]
        );
        assert!(collect_project_files(root, &config)
            .unwrap()
            .iter()
            .all(|path| path.extension().is_none_or(|extension| extension != "rbs")));
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
    fn test_collect_project_files_defaults_to_owned_sources_with_explicit_vendor_opt_in() {
        let workspace = TempDir::new().unwrap();
        let root = workspace.path();
        for directory in [
            "app",
            "vendor/lib",
            ".bundle/cache",
            "node_modules/gem",
            "tmp/generated",
        ] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        std::fs::write(root.join("app/user.rb"), "class User; end").unwrap();
        std::fs::write(root.join("vendor/lib/owned.rb"), "class Owned; end").unwrap();
        std::fs::write(root.join("vendor/lib/ignored.rb"), "class Ignored; end").unwrap();
        std::fs::write(root.join(".bundle/cache/cached.rb"), "class Cached; end").unwrap();
        std::fs::write(root.join("node_modules/gem/node.rb"), "class Node; end").unwrap();
        std::fs::write(root.join("tmp/generated/temp.rb"), "class Temp; end").unwrap();

        let defaults = collect_project_files(root, &IndexingConfig::default()).unwrap();
        assert_eq!(
            defaults,
            vec![root.join("app/user.rb")],
            "dependency, vendored, and temporary trees must not become editable project sources by default"
        );

        let explicitly_included = collect_project_files(
            root,
            &IndexingConfig {
                included_patterns: vec!["vendor/lib/owned.rb".to_string()],
                ..IndexingConfig::default()
            },
        )
        .unwrap();
        assert_eq!(
            explicitly_included,
            vec![root.join("app/user.rb"), root.join("vendor/lib/owned.rb")]
        );

        let exclusion_wins = collect_project_files(
            root,
            &IndexingConfig {
                included_patterns: vec!["vendor/lib/owned.rb".to_string()],
                excluded_patterns: vec!["vendor/**/*".to_string()],
                ..IndexingConfig::default()
            },
        )
        .unwrap();
        assert_eq!(exclusion_wins, vec![root.join("app/user.rb")]);
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
