use crate::types::ruby_version::RubyVersion;
use anyhow::{anyhow, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

/// Information about an installed gem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub lib_paths: Vec<PathBuf>,
    pub dependencies: Vec<String>,
    pub is_default: bool,
}

/// Manages gem discovery and indexing
#[derive(Clone, Debug)]
pub struct GemIndexer {
    ruby_version: Option<RubyVersion>,
    discovered_gems: HashMap<String, Vec<GemInfo>>,
    gem_paths: Vec<PathBuf>,
}

impl GemIndexer {
    pub fn new(ruby_version: Option<RubyVersion>) -> Self {
        Self {
            ruby_version,
            discovered_gems: HashMap::new(),
            gem_paths: Vec::new(),
        }
    }

    /// Discover all installed gems and their paths
    pub fn discover_gems(&mut self) -> Result<()> {
        info!("Starting gem discovery process");

        // Clear previous discoveries
        self.discovered_gems.clear();
        self.gem_paths.clear();

        // Get gem environment information
        self.discover_gem_paths()?;
        self.discover_installed_gems()?;
        self.resolve_gem_lib_paths()?;

        info!("Discovered {} unique gems", self.discovered_gems.len());
        Ok(())
    }

    /// Get gem paths from Ruby's gem environment
    fn discover_gem_paths(&mut self) -> Result<()> {
        let output = Command::new("ruby")
            .args(["-e", "require 'rubygems'; puts Gem.path.join('\n')"])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby command: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let gem_paths = String::from_utf8_lossy(&output.stdout);
        for path_str in gem_paths.lines() {
            let path = PathBuf::from(path_str.trim());
            if path.exists() && path.is_dir() {
                self.gem_paths.push(path.clone());
                debug!("Found gem path: {:?}", path);
            }
        }

        Ok(())
    }

    /// Discover all installed gems using gem list
    fn discover_installed_gems(&mut self) -> Result<()> {
        // Get list of all installed gems with versions
        let output = Command::new("gem")
            .args(["list", "--local", "--no-versions"])
            .output()
            .map_err(|e| anyhow!("Failed to execute gem list command: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Gem list command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let gem_list = String::from_utf8_lossy(&output.stdout);
        let mut gem_names = HashSet::new();

        for line in gem_list.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with("***") {
                // Extract gem name (before any version info)
                if let Some(name) = line.split_whitespace().next() {
                    gem_names.insert(name.to_string());
                }
            }
        }

        // Limit the number of gems to process to avoid timeouts
        let max_gems = std::env::var("RUBY_LSP_MAX_GEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50); // Default to 50 gems

        info!(
            "Processing up to {} gems out of {} discovered",
            max_gems,
            gem_names.len()
        );

        // Get detailed information for each gem (limited)
        let mut processed = 0;
        for gem_name in gem_names {
            if processed >= max_gems {
                debug!(
                    "Reached maximum gem limit ({}), stopping discovery",
                    max_gems
                );
                break;
            }

            if let Ok(gem_info) = self.get_gem_info(&gem_name) {
                self.discovered_gems
                    .entry(gem_name.clone())
                    .or_insert_with(Vec::new)
                    .extend(gem_info);
            }
            processed += 1;
        }

        Ok(())
    }

    /// Get detailed information about a specific gem
    fn get_gem_info(&self, gem_name: &str) -> Result<Vec<GemInfo>> {
        // Use a timeout to prevent hanging on problematic gems
        let mut cmd = Command::new("gem");
        cmd.args(["specification", gem_name, "--all", "--ruby"]);

        // Set a timeout for the command
        let output = std::process::Command::new("timeout")
            .args(["10s"]) // 10 second timeout
            .arg("gem")
            .args(["specification", gem_name, "--all", "--ruby"])
            .output()
            .or_else(|_| {
                // Fallback to direct command if timeout is not available
                cmd.output()
            })
            .map_err(|e| anyhow!("Failed to get gem specification for {}: {}", gem_name, e))?;

        if !output.status.success() {
            debug!(
                "Failed to get gem specification for {}: {}",
                gem_name,
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(Vec::new());
        }

        let spec_output = String::from_utf8_lossy(&output.stdout);
        self.parse_gem_specifications(&spec_output)
    }

    /// Parse gem specifications from gem command output
    fn parse_gem_specifications(&self, spec_output: &str) -> Result<Vec<GemInfo>> {
        let mut gems = Vec::new();
        let mut current_gem: Option<GemInfo> = None;
        let mut in_dependencies = false;

        for line in spec_output.lines() {
            let line = line.trim();

            if line.starts_with("Gem::Specification.new") {
                // Start of a new gem specification
                if let Some(gem) = current_gem.take() {
                    gems.push(gem);
                }
                current_gem = Some(GemInfo {
                    name: String::new(),
                    version: String::new(),
                    path: PathBuf::new(),
                    lib_paths: Vec::new(),
                    dependencies: Vec::new(),
                    is_default: false,
                });
                in_dependencies = false;
            } else if let Some(ref mut gem) = current_gem {
                if line.starts_with("s.name = ") {
                    gem.name = self.extract_quoted_value(line);
                } else if line.starts_with("s.version = ") {
                    gem.version = self.extract_quoted_value(line);
                } else if line.starts_with("s.installed_by_version = ") {
                    // This indicates it's a default gem
                    gem.is_default = true;
                } else if line.starts_with("s.add_runtime_dependency") {
                    in_dependencies = true;
                    if let Some(dep_name) = self.extract_dependency_name(line) {
                        gem.dependencies.push(dep_name);
                    }
                } else if in_dependencies && line.starts_with("s.add_runtime_dependency") {
                    if let Some(dep_name) = self.extract_dependency_name(line) {
                        gem.dependencies.push(dep_name);
                    }
                }
            }
        }

        // Add the last gem if any
        if let Some(gem) = current_gem {
            gems.push(gem);
        }

        Ok(gems)
    }

    /// Extract quoted value from gem specification line
    fn extract_quoted_value(&self, line: &str) -> String {
        if let Some(start) = line.find('"') {
            if let Some(end) = line.rfind('"') {
                if start < end {
                    return line[start + 1..end].to_string();
                }
            }
        }
        String::new()
    }

    /// Extract dependency name from add_runtime_dependency line
    fn extract_dependency_name(&self, line: &str) -> Option<String> {
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                return Some(line[start + 1..start + 1 + end].to_string());
            }
        }
        None
    }

    /// Resolve library paths for discovered gems
    fn resolve_gem_lib_paths(&mut self) -> Result<()> {
        // Collect gem info to avoid borrowing conflicts
        let mut updates = Vec::new();

        for gem_versions in self.discovered_gems.values() {
            for gem in gem_versions.iter() {
                let lib_paths = self.find_gem_lib_paths(&gem.name, &gem.version);
                let path = if let Some(first_lib) = lib_paths.first() {
                    first_lib
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| gem.path.clone())
                } else {
                    gem.path.clone()
                };
                updates.push((gem.name.clone(), gem.version.clone(), lib_paths, path));
            }
        }

        // Apply updates
        for (name, version, lib_paths, path) in updates {
            if let Some(gem_versions) = self.discovered_gems.get_mut(&name) {
                for gem in gem_versions.iter_mut() {
                    if gem.version == version {
                        gem.lib_paths = lib_paths.clone();
                        gem.path = path.clone();
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Find library paths for a specific gem
    fn find_gem_lib_paths(&self, gem_name: &str, version: &str) -> Vec<PathBuf> {
        let mut lib_paths = Vec::new();

        for gem_path in &self.gem_paths {
            let gems_dir = gem_path.join("gems");
            if gems_dir.exists() {
                // Look for gem directory with version
                let gem_dir = gems_dir.join(format!("{}-{}", gem_name, version));
                if gem_dir.exists() {
                    let lib_dir = gem_dir.join("lib");
                    if lib_dir.exists() && lib_dir.is_dir() {
                        lib_paths.push(lib_dir);
                    }
                }
            }
        }

        lib_paths
    }

    /// Get all gem library paths for indexing
    pub fn get_gem_lib_paths(&self) -> Vec<PathBuf> {
        let mut all_paths = Vec::new();

        for gem_versions in self.discovered_gems.values() {
            // For each gem, use the latest version or the default one
            if let Some(gem) = self.select_preferred_gem_version(gem_versions) {
                all_paths.extend(gem.lib_paths.clone());
            }
        }

        all_paths
    }

    /// Select the preferred version of a gem (active gems take priority)
    fn select_preferred_gem_version<'a>(&self, versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        if versions.is_empty() {
            return None;
        }

        // First, try to find the active gem version
        if let Some(active_gem) = self.find_active_gem_version(versions) {
            return Some(active_gem);
        }

        // Prefer default gems
        if let Some(default_gem) = versions.iter().find(|gem| gem.is_default) {
            return Some(default_gem);
        }

        // Otherwise, select the latest version using semantic versioning
        versions
            .iter()
            .max_by(|a, b| self.compare_gem_versions(&a.version, &b.version))
    }

    /// Find the active gem version by checking which one is currently loaded
    fn find_active_gem_version<'a>(&self, versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        // Try to determine which gem version is currently active
        // by checking the gem environment
        if let Ok(output) = Command::new("ruby")
            .args([
                "-e",
                "require 'rubygems'; puts Gem.loaded_specs.keys.join('\n')",
            ])
            .output()
        {
            if output.status.success() {
                let loaded_gems = String::from_utf8_lossy(&output.stdout);
                let gem_name = &versions[0].name; // All versions have the same name

                if loaded_gems.lines().any(|line| line.trim() == gem_name) {
                    // If the gem is loaded, try to get its version
                    if let Ok(version_output) = Command::new("ruby")
                        .args([
                            "-e",
                            &format!(
                                "require 'rubygems'; puts Gem.loaded_specs['{}'].version rescue ''",
                                gem_name
                            ),
                        ])
                        .output()
                    {
                        if version_output.status.success() {
                            let active_version = String::from_utf8_lossy(&version_output.stdout)
                                .trim()
                                .to_string();
                            return versions.iter().find(|gem| gem.version == active_version);
                        }
                    }
                }
            }
        }

        None
    }

    /// Compare gem versions using semantic versioning principles
    fn compare_gem_versions(&self, version_a: &str, version_b: &str) -> std::cmp::Ordering {
        let parts_a: Vec<u32> = version_a
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let parts_b: Vec<u32> = version_b
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        let max_len = parts_a.len().max(parts_b.len());

        for i in 0..max_len {
            let a = parts_a.get(i).unwrap_or(&0);
            let b = parts_b.get(i).unwrap_or(&0);

            match a.cmp(b) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }

        std::cmp::Ordering::Equal
    }

    /// Get information about a specific gem
    pub fn get_gem(&self, name: &str) -> Option<&GemInfo> {
        self.discovered_gems
            .get(name)
            .and_then(|versions| self.select_preferred_gem_version(versions))
    }

    /// Get all discovered gems
    pub fn get_all_gems(&self) -> Vec<&GemInfo> {
        self.discovered_gems
            .values()
            .filter_map(|versions| self.select_preferred_gem_version(versions))
            .collect()
    }

    /// Check if a gem is available
    pub fn has_gem(&self, name: &str) -> bool {
        self.discovered_gems.contains_key(name)
    }

    /// Get gem count
    pub fn gem_count(&self) -> usize {
        self.discovered_gems.len()
    }

    /// Get gem paths
    pub fn get_gem_paths(&self) -> &[PathBuf] {
        &self.gem_paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gem_indexer_creation() {
        let indexer = GemIndexer::new(Some(RubyVersion::new(3, 0)));
        assert_eq!(indexer.gem_count(), 0);
    }

    #[test]
    fn test_extract_quoted_value() {
        let indexer = GemIndexer::new(None);
        let line = r#"s.name = "test_gem""#;
        assert_eq!(indexer.extract_quoted_value(line), "test_gem");
    }

    #[test]
    fn test_extract_dependency_name() {
        let indexer = GemIndexer::new(None);
        let line = r#"s.add_runtime_dependency "rails", ">= 6.0""#;
        assert_eq!(
            indexer.extract_dependency_name(line),
            Some("rails".to_string())
        );
    }
}
