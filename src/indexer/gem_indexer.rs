use crate::types::ruby_version::RubyVersion;
use anyhow::{anyhow, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
        debug!("Starting gem discovery process");

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

    /// Discover all installed gems using efficient bulk approach
    fn discover_installed_gems(&mut self) -> Result<()> {
        // Use Ruby script to get all gem information in one command
        let ruby_script = r#"
            require 'rubygems'
            require 'json'
            
            gems = []
            Gem::Specification.each do |spec|
              next if spec.name.nil? || spec.version.nil?
              
              gem_info = {
                name: spec.name,
                version: spec.version.to_s,
                gem_dir: spec.gem_dir,
                lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                dependencies: spec.dependencies.map(&:name),
                default_gem: spec.default_gem?
              }
              gems << gem_info
            end
            
            puts JSON.generate(gems)
        "#;

        let output = Command::new("ruby")
            .args(["-e", ruby_script])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby gem discovery script: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby gem discovery script failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let json_output = String::from_utf8_lossy(&output.stdout);
        let gem_data: Vec<serde_json::Value> = serde_json::from_str(&json_output)
            .map_err(|e| anyhow!("Failed to parse gem JSON data: {}", e))?;

        // Apply gem limit if set
        let max_gems = std::env::var("RUBY_LSP_MAX_GEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX); // No limit by default

        let total_gems = gem_data.len();
        let processed_gems = std::cmp::min(total_gems, max_gems);
        
        if max_gems < total_gems {
            info!(
                "Processing {} gems out of {} discovered (limited by RUBY_LSP_MAX_GEMS)",
                processed_gems,
                total_gems
            );
        }

        // Process gem data
        for (i, gem_json) in gem_data.iter().enumerate() {
            if i >= max_gems {
                break;
            }

            if let Ok(gem_info) = self.parse_gem_json(gem_json) {
                debug!(
                    "Discovered gem: {} v{} at {:?} (default: {}, lib_paths: {})",
                    gem_info.name,
                    gem_info.version,
                    gem_info.path,
                    gem_info.is_default,
                    gem_info.lib_paths.len()
                );
                self.discovered_gems
                    .entry(gem_info.name.clone())
                    .or_insert_with(Vec::new)
                    .push(gem_info);
            }
        }

        Ok(())
    }

    /// Parse gem information from JSON data
    fn parse_gem_json(&self, gem_json: &serde_json::Value) -> Result<GemInfo> {
        let name = gem_json["name"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing gem name"))?
            .to_string();
        
        let version = gem_json["version"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing gem version"))?
            .to_string();
        
        let gem_dir = gem_json["gem_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_default();
        
        let lib_paths = gem_json["lib_dirs"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(PathBuf::from)
                    .filter(|p| p.exists())
                    .collect()
            })
            .unwrap_or_default();
        
        let dependencies = gem_json["dependencies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        
        let is_default = gem_json["default_gem"]
            .as_bool()
            .unwrap_or(false);
        
        Ok(GemInfo {
            name,
            version,
            path: gem_dir,
            lib_paths,
            dependencies,
            is_default,
        })
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


}
