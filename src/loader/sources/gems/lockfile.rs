//! Gemfile and Gemfile.lock parsing and exact locked version selection.

use super::vendor_cache::validate_archive_identity_component;
use super::ActiveRubyEngine;
use super::GemInfo;
use super::GemSource;
use super::IndexerGem;
use super::LockedGemIdentity;
use super::LockedGemSource;
use anyhow::{anyhow, Context, Result};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

/// Compare two gem version strings
pub(super) fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };

    let parts_a = parse(a);
    let parts_b = parse(b);

    for (x, y) in parts_a.iter().zip(parts_b.iter()) {
        match x.cmp(y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

pub(super) fn select_locked_identity_for_engine(
    name: &str,
    identities: &[LockedGemIdentity],
    engine: ActiveRubyEngine,
) -> Result<LockedGemIdentity> {
    invariant!(
        !identities.is_empty(),
        what = "lock identity selection received no candidates for `{name}`",
        why = "grouping creates an entry only after parsing an identity",
        fix = "never call selection with an empty lockfile group",
        name = name,
    );
    invariant!(
        identities.iter().all(|identity| identity.name == name),
        what = "lock identity group for `{name}` contains another gem name",
        why = "lock identities are grouped by exact gem name",
        fix = "insert each parsed identity into its own name bucket",
        name = name,
    );

    let source = identities[0].source;
    if identities.iter().any(|identity| identity.source != source) {
        return Err(anyhow!(
            "Gemfile.lock contains conflicting source identities for `{name}`: {identities:?}"
        ));
    }

    if identities.len() == 1 {
        return Ok(identities[0].clone());
    }
    if source != LockedGemSource::Registry {
        return Err(anyhow!(
            "Gemfile.lock contains multiple non-registry identities for `{name}`: {identities:?}"
        ));
    }

    let (java, non_java): (Vec<_>, Vec<_>) = identities
        .iter()
        .partition(|identity| identity.locked_version.ends_with("-java"));
    let preferred = match engine {
        ActiveRubyEngine::JRuby if java.len() == 1 => java[0],
        ActiveRubyEngine::JRuby if java.is_empty() && non_java.len() == 1 => non_java[0],
        ActiveRubyEngine::Other if non_java.len() == 1 => non_java[0],
        ActiveRubyEngine::Other if non_java.is_empty() && java.len() == 1 => java[0],
        ActiveRubyEngine::JRuby | ActiveRubyEngine::Other => {
            return Err(anyhow!(
                "Gemfile.lock contains ambiguous platform identities for `{name}` and active engine {engine:?}: {identities:?}"
            ));
        }
    };
    Ok(preferred.clone())
}

pub(super) fn parse_locked_gems(content: &str) -> Result<Vec<LockedGemIdentity>> {
    let mut source = None;
    let mut in_specs = false;
    let mut gems = Vec::<LockedGemIdentity>::new();

    for line in content.lines() {
        if !line.starts_with(' ') {
            source = match line {
                "GEM" => Some(LockedGemSource::Registry),
                "GIT" => Some(LockedGemSource::Git),
                "PATH" => Some(LockedGemSource::Path),
                _ => None,
            };
            in_specs = false;
            continue;
        }
        let Some(source) = source else {
            continue;
        };
        if line == "  specs:" {
            in_specs = true;
            continue;
        }
        if !in_specs {
            continue;
        }

        if let Some(dependency) = line.strip_prefix("      ") {
            let Some(current) = gems.last_mut() else {
                return Err(anyhow!(
                    "Gemfile.lock contains a dependency before its owning specification"
                ));
            };
            if current.source != source {
                return Err(anyhow!(
                    "Gemfile.lock dependency source changed before its owning specification"
                ));
            }
            let dependency = dependency
                .trim()
                .split([' ', '('])
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("Gemfile.lock contains an empty dependency name"))?;
            validate_archive_identity_component("dependency name", dependency)?;
            current.dependencies.push(dependency.to_string());
            continue;
        }

        let Some(spec) = line.strip_prefix("    ") else {
            continue;
        };
        let (name, locked_version) = spec
            .trim()
            .split_once(" (")
            .and_then(|(name, version)| version.strip_suffix(')').map(|version| (name, version)))
            .ok_or_else(|| anyhow!("Gemfile.lock contains an invalid gem specification: {spec}"))?;
        validate_archive_identity_component("gem name", name)?;
        validate_archive_identity_component("gem version", locked_version)?;
        gems.push(LockedGemIdentity {
            name: name.to_string(),
            locked_version: locked_version.to_string(),
            source,
            dependencies: Vec::new(),
        });
    }

    Ok(gems)
}

pub(super) fn locked_version_for(version: &str, platform: &str) -> String {
    if platform == "ruby" {
        version.to_string()
    } else {
        format!("{version}-{platform}")
    }
}

pub(super) fn parse_gemfile_gem_statement(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || !trimmed.starts_with("gem ") {
        return None;
    }
    let start = trimmed.find('"').or_else(|| trimmed.find('\''))?;
    let quote = trimmed.as_bytes().get(start).copied()?;
    let end = trimmed[start + 1..]
        .as_bytes()
        .iter()
        .position(|byte| *byte == quote)?;
    let name = &trimmed[start + 1..start + 1 + end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

impl IndexerGem {
    pub(super) fn load_locked_gems(&mut self) -> Result<()> {
        self.locked_gems.clear();
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let lockfile_path = root.join("Gemfile.lock");
        let content = match std::fs::read_to_string(&lockfile_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read owning project lockfile {}",
                        lockfile_path.display()
                    )
                });
            }
        };

        let mut identities_by_name = HashMap::<String, Vec<LockedGemIdentity>>::new();
        for identity in parse_locked_gems(&content)? {
            identities_by_name
                .entry(identity.name.clone())
                .or_default()
                .push(identity);
        }
        for (name, identities) in identities_by_name {
            let selected =
                select_locked_identity_for_engine(&name, &identities, self.active_ruby_engine)?;
            self.locked_gems.insert(name, selected);
        }
        Ok(())
    }

    /// Read direct `gem` roots from the owning project's Gemfile.
    ///
    /// Bundler projects declare their indexing roots in `Gemfile` before any
    /// project source pass completes. Parsing that file during discovery lets
    /// dependency product loading overlap exhaustive project fact collection
    /// without waiting for every project file to contribute the same roots.
    pub(crate) fn gemfile_required_roots_blocking(&self) -> Result<Vec<String>> {
        let Some(root) = &self.workspace_root else {
            return Ok(Vec::new());
        };
        let gemfile_path = root.join("Gemfile");
        let content = match std::fs::read_to_string(&gemfile_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read owning project Gemfile {}",
                        gemfile_path.display()
                    )
                });
            }
        };
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        for line in content.lines() {
            let Some(name) = parse_gemfile_gem_statement(line.trim()) else {
                continue;
            };
            if seen.insert(name.clone()) {
                roots.push(name);
            }
        }
        roots.sort();
        Ok(roots)
    }

    /// Find Gemfile in workspace hierarchy
    pub(super) fn find_gemfile(&self) -> Result<PathBuf> {
        if let Some(root) = &self.workspace_root {
            // Check workspace root
            let gemfile = root.join("Gemfile");
            if gemfile.exists() {
                return Ok(gemfile);
            }

            return Err(anyhow!(
                "No Gemfile found at Ruby project root {}",
                root.display()
            ));
        }

        // Fallback to current directory
        let current = std::env::current_dir()?.join("Gemfile");
        if current.exists() {
            return Ok(current);
        }

        Err(anyhow!("No Gemfile found in workspace hierarchy"))
    }

    /// Select the preferred version of a gem from multiple available versions
    pub(super) fn select_preferred_version<'a>(
        &self,
        versions: &'a [GemInfo],
    ) -> Option<&'a GemInfo> {
        let name = versions.first()?.name.as_str();
        invariant!(
            versions.iter().all(|candidate| candidate.name == name),
            what = "gem candidate bucket contains multiple names",
            why = "discovered_gems is keyed by gem name",
            fix = "insert every candidate under its own exact name",
        );

        if let Some(locked) = self.locked_gems.get(name) {
            let exact = |source| {
                versions.iter().find(|candidate| {
                    candidate.source == source
                        && candidate.locked_version == locked.locked_version
                        && self.gem_platform_matches_active_engine(&candidate.platform)
                        && !candidate.lib_paths.is_empty()
                })
            };

            if let Some(installed) = exact(GemSource::BundlerInstalled) {
                return Some(installed);
            }

            return match locked.source {
                LockedGemSource::Registry => {
                    exact(GemSource::GlobalInstalled).or_else(|| exact(GemSource::VendorArchive))
                }
                LockedGemSource::Git => exact(GemSource::VendorGit),
                LockedGemSource::Path => None,
            };
        }

        if let Some(installed) = versions
            .iter()
            .find(|candidate| candidate.source == GemSource::BundlerInstalled)
        {
            return Some(installed);
        }

        if self.explicitly_included_gems.contains(name) {
            return versions
                .iter()
                .filter(|candidate| candidate.source == GemSource::GlobalInstalled)
                .max_by(|a, b| compare_versions(&a.version, &b.version));
        }

        None
    }

    fn gem_platform_matches_active_engine(&self, platform: &str) -> bool {
        match self.active_ruby_engine {
            ActiveRubyEngine::JRuby => platform == "ruby" || platform == "java",
            ActiveRubyEngine::Other => platform != "java",
        }
    }

    pub(super) fn required_gems_with_dependencies(&self) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        let mut roots = self.required_gems.iter().cloned().collect::<Vec<_>>();
        roots.sort();
        let mut queue = roots.into_iter().collect::<VecDeque<String>>();

        while let Some(name) = queue.pop_front() {
            if self.excluded_gems.contains(&name) {
                continue;
            }
            if !seen.insert(name.clone()) {
                continue;
            }
            ordered.push(name.clone());

            let Some(gem_versions) = self.discovered_gems.get(&name) else {
                continue;
            };
            let Some(gem_info) = self.select_preferred_version(gem_versions) else {
                continue;
            };

            for dependency in &gem_info.dependencies {
                if !seen.contains(dependency) {
                    queue.push_back(dependency.clone());
                }
            }
        }

        ordered
    }
}
