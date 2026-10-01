//! Maven coordinates parsed from `Jars.lock` and `Jarfile` declarations.

use std::path::{Path, PathBuf};

use super::ClasspathError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MavenCoordinate {
    pub(super) group: String,
    pub(super) artifact: String,
    pub(super) classifier: Option<String>,
    pub(super) version: String,
}

impl MavenCoordinate {
    pub(super) fn repository_path(&self, repository: &Path) -> PathBuf {
        let mut filename = format!("{}-{}", self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            filename.push('-');
            filename.push_str(classifier);
        }
        filename.push_str(".jar");
        repository
            .join(self.group.replace('.', "/"))
            .join(&self.artifact)
            .join(&self.version)
            .join(filename)
    }

    pub(super) fn display(&self) -> String {
        match &self.classifier {
            Some(classifier) => format!(
                "{}:{}:{}:{}",
                self.group, self.artifact, classifier, self.version
            ),
            None => format!("{}:{}:{}", self.group, self.artifact, self.version),
        }
    }
}

pub(super) fn parse_lock_coordinate(line: &str) -> Result<Option<MavenCoordinate>, ClasspathError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || !line.contains(':') {
        return Ok(None);
    }
    let normalized = line.replace(":jar:", ":");
    let parts = normalized.split(':').collect::<Vec<_>>();
    if parts.len() < 5 {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    }
    let (classifier, version_index) = if parts.len() == 5 {
        (None, 2)
    } else {
        (Some(parts[2].to_string()), 3)
    };
    let coordinate = MavenCoordinate {
        group: parts[0].to_string(),
        artifact: parts[1].to_string(),
        classifier,
        version: parts[version_index].to_string(),
    };
    validate_coordinate(&coordinate, line)?;
    Ok(Some(coordinate))
}

pub(super) fn parse_jarfile_coordinate(
    line: &str,
) -> Result<Option<MavenCoordinate>, ClasspathError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let Some(arguments) = line.strip_prefix("jar ") else {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    };
    let literals = quoted_literals(arguments)?;
    if literals.len() < 2 {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    }
    let Some((group, artifact)) = literals[0].split_once(':') else {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    };
    let coordinate = MavenCoordinate {
        group: group.to_string(),
        artifact: artifact.to_string(),
        classifier: None,
        version: literals[1].clone(),
    };
    validate_coordinate(&coordinate, line)?;
    Ok(Some(coordinate))
}

fn quoted_literals(source: &str) -> Result<Vec<String>, ClasspathError> {
    let mut values = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, character)) = chars.next() {
        if character != '\'' && character != '"' {
            continue;
        }
        let quote = character;
        let mut value = String::new();
        let mut closed = false;
        for (_, character) in chars.by_ref() {
            if character == quote {
                closed = true;
                break;
            }
            value.push(character);
        }
        if !closed {
            return Err(ClasspathError::InvalidLockEntry(source.to_string()));
        }
        values.push(value);
    }
    Ok(values)
}

fn validate_coordinate(coordinate: &MavenCoordinate, source: &str) -> Result<(), ClasspathError> {
    let values = [
        coordinate.group.as_str(),
        coordinate.artifact.as_str(),
        coordinate.version.as_str(),
    ];
    if values
        .iter()
        .any(|value| value.is_empty() || value.contains('/') || value.contains('\\'))
        || coordinate
            .classifier
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.contains(['/', '\\']))
    {
        return Err(ClasspathError::InvalidLockEntry(source.to_string()));
    }
    Ok(())
}
