//! Project-confined pattern matching and canonical path reads for classpath inputs.

use crate::invariant::ExpectInvariant;
use globset::Glob;
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

use super::ClasspathError;

pub(super) fn project_pattern_matches(
    project_root: &Path,
    pattern: &str,
    max_entries: usize,
) -> Result<Vec<PathBuf>, ClasspathError> {
    validate_project_pattern(pattern)?;
    let matcher = Glob::new(pattern)
        .map_err(|_| ClasspathError::InvalidProjectPattern(pattern.to_string()))?
        .compile_matcher();
    let mut matches = Vec::new();
    let mut visited = 0usize;
    let walker = WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git");
    for entry in walker {
        let entry = entry.map_err(|error| ClasspathError::Io {
            path: project_root.to_path_buf(),
            message: error.to_string(),
        })?;
        visited += 1;
        if visited > max_entries {
            return Err(ClasspathError::LimitExceeded("project pattern entries"));
        }
        let path = entry.path();
        let relative = path.strip_prefix(project_root).expect_invariant(
            "project walker entry is outside its root",
            "the walker starts at the project root",
            "walk from project_root only",
        );
        if matcher.is_match(relative) {
            let canonical = fs::canonicalize(path).map_err(|error| ClasspathError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
            if !canonical.starts_with(project_root) {
                return Err(ClasspathError::PathEscapesProject(canonical));
            }
            matches.push(canonical);
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn validate_project_pattern(pattern: &str) -> Result<(), ClasspathError> {
    let path = Path::new(pattern);
    if pattern.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ClasspathError::InvalidProjectPattern(pattern.to_string()));
    }
    Ok(())
}

pub(super) fn canonical_directory(path: &Path) -> std::io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path is not a directory",
        ));
    }
    Ok(path)
}

pub(super) fn canonical_file(path: &Path) -> std::io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path is not a file",
        ));
    }
    Ok(path)
}

pub(super) fn read_text(path: &Path, max_bytes: u64) -> Result<String, ClasspathError> {
    let metadata = fs::metadata(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > max_bytes {
        return Err(ClasspathError::LimitExceeded("classpath metadata bytes"));
    }
    fs::read_to_string(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}
