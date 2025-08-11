use std::path::{Path, PathBuf};
use std::process::Command;
use log::{debug, info, warn};

/// Automatically discovers Ruby-related paths for indexing
pub struct PathDiscovery {
    workspace_root: PathBuf,
}

impl PathDiscovery {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Get all paths that should be indexed for Ruby development
    pub fn discover_index_paths(&self, ruby_version: (u8, u8)) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 1. Add core stubs from VSIX package
        if let Some(stubs_path) = self.get_core_stubs_path(ruby_version) {
            paths.push(stubs_path);
            info!("Added core stubs path for Ruby {}.{}", ruby_version.0, ruby_version.1);
        }

        // 2. Add Ruby standard library paths
        if let Some(stdlib_paths) = self.discover_ruby_stdlib_paths() {
            paths.extend(stdlib_paths);
            info!("Added Ruby standard library paths");
        }

        // 3. Add gem paths from current project
        if let Some(gem_paths) = self.discover_gem_paths() {
            paths.extend(gem_paths);
            info!("Added gem paths from project dependencies");
        }

        // 4. Add workspace source paths
        paths.extend(self.discover_workspace_ruby_paths());

        debug!("Discovered {} index paths total", paths.len());
        paths
    }

    /// Get core stubs path from VSIX package
    fn get_core_stubs_path(&self, ruby_version: (u8, u8)) -> Option<PathBuf> {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // In VSIX package: executable is in bin/<platform>/, stubs are in stubs/
                let vsix_stubs = exe_dir.parent()?.parent()?.join("stubs");
                if vsix_stubs.exists() {
                    // Try exact version first
                    let version_dir = format!("rubystubs{}{}", ruby_version.0, ruby_version.1);
                    let version_path = vsix_stubs.join(&version_dir);
                    if version_path.exists() {
                        debug!("Found exact core stubs for Ruby {}.{}", ruby_version.0, ruby_version.1);
                        return Some(version_path);
                    }
                    
                    // If exact version not found, try to find the closest available version
                    if let Some(fallback_path) = self.find_closest_stub_version(&vsix_stubs, ruby_version) {
                        return Some(fallback_path);
                    }
                    
                    warn!("No compatible core stubs found for Ruby {}.{} in {:?}", 
                          ruby_version.0, ruby_version.1, vsix_stubs);
                }
            }
        }
        None
    }
    
    /// Find the closest available stub version to the requested Ruby version
    fn find_closest_stub_version(&self, stubs_dir: &Path, target_version: (u8, u8)) -> Option<PathBuf> {
        let mut available_versions = Vec::new();
        
        // Scan for available stub directories
        if let Ok(entries) = std::fs::read_dir(stubs_dir) {
            for entry in entries.flatten() {
                if let Some(dir_name) = entry.file_name().to_str() {
                    if dir_name.starts_with("rubystubs") && entry.file_type().map_or(false, |ft| ft.is_dir()) {
                        // Extract version from directory name like "rubystubs30" -> (3, 0)
                        let version_part = &dir_name[9..]; // Skip "rubystubs"
                        if version_part.len() >= 2 {
                            if let (Ok(major), Ok(minor)) = (
                                version_part[0..1].parse::<u8>(),
                                version_part[1..2].parse::<u8>()
                            ) {
                                available_versions.push(((major, minor), entry.path()));
                            }
                        }
                    }
                }
            }
        }
        
        if available_versions.is_empty() {
            return None;
        }
        
        // Sort by version (newest first) and find the best match
        available_versions.sort_by(|a, b| b.0.cmp(&a.0));
        
        // Prefer the same major version, or the latest available if no same major version
        let same_major = available_versions.iter()
            .find(|((major, _), _)| *major == target_version.0);
            
        if let Some(((major, minor), path)) = same_major {
            warn!("Using Ruby {}.{} stubs as fallback for Ruby {}.{}", 
                  major, minor, target_version.0, target_version.1);
            Some(path.clone())
        } else if let Some(((major, minor), path)) = available_versions.first() {
            warn!("Using Ruby {}.{} stubs as fallback for Ruby {}.{} (no same major version available)", 
                  major, minor, target_version.0, target_version.1);
            Some(path.clone())
        } else {
            None
        }
    }

    /// Discover Ruby standard library paths
    pub fn discover_ruby_stdlib_paths(&self) -> Option<Vec<PathBuf>> {
        let output = Command::new("ruby")
            .args(&["-e", "puts $LOAD_PATH"])
            .output()
            .ok()?;

        if !output.status.success() {
            warn!("Failed to get Ruby load paths");
            return None;
        }

        let load_paths = String::from_utf8_lossy(&output.stdout);
        let mut main_ruby_lib_paths = std::collections::HashSet::new();

        for path_str in load_paths.lines() {
            let path = PathBuf::from(path_str.trim());
            if path.exists() && self.is_ruby_stdlib_path(&path) {
                // Find the main Ruby lib directory instead of adding all subdirectories
                if let Some(main_lib_path) = self.find_main_ruby_lib_path(&path) {
                    main_ruby_lib_paths.insert(main_lib_path);
                }
            }
        }

        if main_ruby_lib_paths.is_empty() {
            None
        } else {
            Some(main_ruby_lib_paths.into_iter().collect())
        }
    }

    /// Find the main Ruby lib directory from a stdlib path
    fn find_main_ruby_lib_path(&self, path: &Path) -> Option<PathBuf> {
        let path_str = path.to_string_lossy();
        
        // Look for patterns like /path/to/ruby/lib/ruby/3.3.0 or /path/to/ruby/lib/ruby/site_ruby/3.3.0
        // and extract the main /path/to/ruby/lib/ruby part
        if let Some(lib_ruby_pos) = path_str.find("/lib/ruby/") {
            let main_path = &path_str[..lib_ruby_pos + "/lib/ruby".len()];
            let main_lib_path = PathBuf::from(main_path);
            if main_lib_path.exists() {
                return Some(main_lib_path);
            }
        }
        
        // Fallback: if we can't find the pattern, return the path as-is
        Some(path.to_path_buf())
    }

    /// Check if a path looks like Ruby standard library
    fn is_ruby_stdlib_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();
        
        // Common patterns for Ruby stdlib paths
        path_str.contains("/lib/ruby/") || 
        path_str.contains("/ruby/") && (
            path_str.contains("/site_ruby/") ||
            path_str.contains("/vendor_ruby/") ||
            path_str.ends_with("/lib")
        )
    }

    /// Discover gem paths from Bundler and RubyGems
    pub fn discover_gem_paths(&self) -> Option<Vec<PathBuf>> {
        let mut gem_paths = Vec::new();

        // Try Bundler first (for projects with Gemfile)
        if let Some(bundler_paths) = self.discover_bundler_gem_paths() {
            gem_paths.extend(bundler_paths);
        }

        // Fallback to system gem paths
        if gem_paths.is_empty() {
            if let Some(system_gem_paths) = self.discover_system_gem_paths() {
                gem_paths.extend(system_gem_paths);
            }
        }

        if gem_paths.is_empty() {
            None
        } else {
            Some(gem_paths)
        }
    }

    /// Discover gem paths using Bundler
    fn discover_bundler_gem_paths(&self) -> Option<Vec<PathBuf>> {
        // Check if we have a Gemfile
        let gemfile = self.workspace_root.join("Gemfile");
        if !gemfile.exists() {
            return None;
        }

        // Get gem paths from bundle show --paths
        let output = Command::new("bundle")
            .args(&["show", "--paths"])
            .current_dir(&self.workspace_root)
            .output()
            .ok()?;

        if !output.status.success() {
            debug!("Bundle show --paths failed, trying alternative approach");
            return self.discover_bundler_gem_paths_alternative();
        }

        let gem_paths_output = String::from_utf8_lossy(&output.stdout);
        let mut gem_paths = Vec::new();

        for path_str in gem_paths_output.lines() {
            let path = PathBuf::from(path_str.trim());
            if path.exists() {
                // Add the lib directory of each gem
                let lib_path = path.join("lib");
                if lib_path.exists() {
                    gem_paths.push(lib_path);
                }
            }
        }

        if gem_paths.is_empty() {
            None
        } else {
            Some(gem_paths)
        }
    }

    /// Alternative method to discover Bundler gem paths
    fn discover_bundler_gem_paths_alternative(&self) -> Option<Vec<PathBuf>> {
        // Try to get the bundle path
        let output = Command::new("bundle")
            .args(&["config", "get", "path"])
            .current_dir(&self.workspace_root)
            .output()
            .ok()?;

        if output.status.success() {
            let bundle_path_output = String::from_utf8_lossy(&output.stdout);
            if let Some(path_line) = bundle_path_output.lines().find(|line| !line.trim().is_empty()) {
                let bundle_path = PathBuf::from(path_line.trim());
                if bundle_path.exists() {
                    return Some(vec![bundle_path.join("gems")]);
                }
            }
        }

        None
    }

    /// Discover system gem paths
    fn discover_system_gem_paths(&self) -> Option<Vec<PathBuf>> {
        let output = Command::new("gem")
            .args(&["environment", "gempath"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let gem_paths_output = String::from_utf8_lossy(&output.stdout);
        let mut gem_paths = Vec::new();

        for path_str in gem_paths_output.split(':') {
            let gems_dir = PathBuf::from(path_str.trim()).join("gems");
            if gems_dir.exists() {
                gem_paths.push(gems_dir);
            }
        }

        if gem_paths.is_empty() {
            None
        } else {
            Some(gem_paths)
        }
    }

    /// Discover Ruby source files in the workspace
    pub fn discover_workspace_ruby_paths(&self) -> Vec<PathBuf> {
        // Just return the workspace root - find_ruby_files will recursively
        // search all subdirectories (app/, lib/, config/, spec/, test/, etc.)
        vec![self.workspace_root.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_path_discovery_creation() {
        let workspace = PathBuf::from("/tmp");
        let discovery = PathDiscovery::new(workspace.clone());
        assert_eq!(discovery.workspace_root, workspace);
    }

    #[test]
    fn test_is_ruby_stdlib_path() {
        let discovery = PathDiscovery::new(PathBuf::from("/tmp"));
        
        assert!(discovery.is_ruby_stdlib_path(&PathBuf::from("/usr/lib/ruby/3.0.0")));
        assert!(discovery.is_ruby_stdlib_path(&PathBuf::from("/opt/ruby/lib/ruby/site_ruby")));
        assert!(!discovery.is_ruby_stdlib_path(&PathBuf::from("/home/user/project")));
    }

    #[test]
    fn test_workspace_ruby_paths() {
        let temp_dir = env::temp_dir().join("ruby_test_workspace");
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        // Create some common Ruby directories
        std::fs::create_dir_all(temp_dir.join("app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("lib")).unwrap();
        
        let discovery = PathDiscovery::new(temp_dir.clone());
        let paths = discovery.discover_workspace_ruby_paths();
        
        // Should only return the workspace root since find_ruby_files recursively searches
        assert_eq!(paths.len(), 1);
        assert!(paths.contains(&temp_dir));
        
        // Cleanup
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}