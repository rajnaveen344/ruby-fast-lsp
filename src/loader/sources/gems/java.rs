//! Exact discovery of locked Java-platform gem roots for JRuby projects.

use super::lockfile::{parse_locked_gems, select_locked_identity_for_engine};
use super::ActiveRubyEngine;
use super::LockedGemIdentity;
use super::LockedGemSource;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const MAX_LOCKFILE_BYTES: u64 = 16 * 1024 * 1024;

const MAX_JAVA_GEM_SEARCH_ENTRIES: usize = 100_000;

pub fn discover_locked_java_gem_roots(
    project_root: &Path,
    jruby_executable: &Path,
    compatibility_version: &str,
) -> Result<Vec<PathBuf>> {
    let project_root = project_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize owning project root {}",
            project_root.display()
        )
    })?;
    if !project_root.is_dir() {
        return Err(anyhow!(
            "owning project root is not a directory: {}",
            project_root.display()
        ));
    }
    let jruby_executable = jruby_executable.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize selected JRuby executable {}",
            jruby_executable.display()
        )
    })?;
    if !jruby_executable.is_file() {
        return Err(anyhow!(
            "selected JRuby executable is not a file: {}",
            jruby_executable.display()
        ));
    }
    if compatibility_version.is_empty()
        || !compatibility_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return Err(anyhow!(
            "selected JRuby compatibility version `{compatibility_version}` is invalid"
        ));
    }

    let lockfile_path = project_root.join("Gemfile.lock");
    let metadata = match std::fs::metadata(&lockfile_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect owning project lockfile {}",
                    lockfile_path.display()
                )
            });
        }
    };
    if metadata.len() > MAX_LOCKFILE_BYTES {
        return Err(anyhow!(
            "owning project lockfile {} is {} bytes, exceeding the {}-byte limit",
            lockfile_path.display(),
            metadata.len(),
            MAX_LOCKFILE_BYTES
        ));
    }
    let lockfile = std::fs::read_to_string(&lockfile_path).with_context(|| {
        format!(
            "failed to read owning project lockfile {}",
            lockfile_path.display()
        )
    })?;

    let mut identities_by_name = HashMap::<String, Vec<LockedGemIdentity>>::new();
    for identity in parse_locked_gems(&lockfile)? {
        identities_by_name
            .entry(identity.name.clone())
            .or_default()
            .push(identity);
    }
    let mut selected = identities_by_name
        .into_iter()
        .map(|(name, identities)| {
            select_locked_identity_for_engine(&name, &identities, ActiveRubyEngine::JRuby)
        })
        .collect::<Result<Vec<_>>>()?;
    selected.retain(|identity| {
        identity.source == LockedGemSource::Registry && identity.locked_version.ends_with("-java")
    });
    selected.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.locked_version.cmp(&right.locked_version))
    });

    let jruby_home = jruby_executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            anyhow!(
                "selected JRuby executable {} has no runtime home",
                jruby_executable.display()
            )
        })?;
    let runtime_name = jruby_home
        .file_name()
        .ok_or_else(|| anyhow!("selected JRuby runtime home has no directory name"))?;
    let rvm_repository = jruby_home
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "rubies"))
        .and_then(Path::parent)
        .map(|rvm| rvm.join("gems").join(runtime_name).join("gems"));
    let runtime_repositories = [
        jruby_home.join("lib/ruby/gems/shared/gems"),
        jruby_home.join(format!("lib/ruby/gems/{compatibility_version}.0/gems")),
    ];

    let mut roots = Vec::new();
    for identity in selected {
        let exact_directory = format!("{}-{}", identity.name, identity.locked_version);
        let project_matches = find_project_java_gem_matches(&project_root, &exact_directory)?;
        if let Some(root) =
            select_unique_java_gem_match(&identity, "project vendor/bundle", project_matches)?
        {
            roots.push(root);
            continue;
        }

        let rvm_matches = rvm_repository
            .iter()
            .map(|repository| exact_java_gem_directory(repository, &exact_directory))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        if let Some(root) =
            select_unique_java_gem_match(&identity, "selected RVM runtime", rvm_matches)?
        {
            roots.push(root);
            continue;
        }

        let runtime_matches = runtime_repositories
            .iter()
            .map(|repository| exact_java_gem_directory(repository, &exact_directory))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        if let Some(root) =
            select_unique_java_gem_match(&identity, "selected JRuby runtime", runtime_matches)?
        {
            roots.push(root);
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn find_project_java_gem_matches(
    project_root: &Path,
    exact_directory: &str,
) -> Result<Vec<PathBuf>> {
    let vendor_bundle = project_root.join("vendor/bundle");
    if !vendor_bundle.is_dir() {
        return Ok(Vec::new());
    }
    let mut matches = Vec::new();
    let mut entries = 0usize;
    for entry in WalkDir::new(&vendor_bundle)
        .follow_links(false)
        .max_depth(8)
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect project-local gem repository {}",
                vendor_bundle.display()
            )
        })?;
        entries += 1;
        if entries > MAX_JAVA_GEM_SEARCH_ENTRIES {
            return Err(anyhow!(
                "project-local gem repository {} exceeds the {}-entry search limit",
                vendor_bundle.display(),
                MAX_JAVA_GEM_SEARCH_ENTRIES
            ));
        }
        if !entry.file_type().is_dir()
            || entry.file_name() != exact_directory
            || entry
                .path()
                .parent()
                .and_then(Path::file_name)
                .is_none_or(|name| name != "gems")
        {
            continue;
        }
        let canonical = entry.path().canonicalize().with_context(|| {
            format!(
                "failed to canonicalize project-local Java gem {}",
                entry.path().display()
            )
        })?;
        if !canonical.starts_with(project_root) {
            return Err(anyhow!(
                "project-local Java gem {} escapes owning project {}",
                canonical.display(),
                project_root.display()
            ));
        }
        matches.push(canonical);
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn exact_java_gem_directory(repository: &Path, exact_directory: &str) -> Result<Option<PathBuf>> {
    let candidate = repository.join(exact_directory);
    if !candidate.is_dir() {
        return Ok(None);
    }
    candidate
        .canonicalize()
        .map(Some)
        .with_context(|| format!("failed to canonicalize Java gem {}", candidate.display()))
}

fn select_unique_java_gem_match(
    identity: &LockedGemIdentity,
    tier: &str,
    mut matches: Vec<PathBuf>,
) -> Result<Option<PathBuf>> {
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(anyhow!(
            "locked Java gem `{}-{}` has ambiguous installations in {tier}: {matches:?}",
            identity.name,
            identity.locked_version
        )),
    }
}
