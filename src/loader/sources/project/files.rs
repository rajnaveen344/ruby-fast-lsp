//! Project file discovery under the workspace indexing configuration:
//! `ProjectFilePolicy` decides which workspace files are project-owned Ruby
//! or RBS sources, and the collectors walk a project root with it.

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

use crate::environment::config::IndexingConfig;
use crate::utils::file_ops::should_index_file;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

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
