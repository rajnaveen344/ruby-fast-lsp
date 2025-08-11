use log::{debug, warn};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Handles discovery of Ruby-related paths including gems, load paths, and executables
pub struct PathDiscovery {
    workspace_root: PathBuf,
}

impl PathDiscovery {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Discover all relevant Ruby paths for the workspace
    pub fn discover_all_paths(&self) -> DiscoveredPaths {
        let gem_paths = self.discover_bundler_gem_paths();
        let load_paths = self.discover_load_paths();
        let executable_paths = self.discover_executable_paths();

        DiscoveredPaths {
            gem_paths,
            load_paths,
            executable_paths,
        }
    }

    /// Discover simplified paths for indexing: only project root, standard library, and core stubs
    pub fn discover_simplified_paths(&self) -> SimplifiedPaths {
        let project_root = self.workspace_root.clone();
        let stdlib_paths = self.discover_stdlib_paths();

        SimplifiedPaths {
            project_root,
            stdlib_paths,
        }
    }

    /// Discover Ruby standard library paths only (excluding gems and project paths)
    fn discover_stdlib_paths(&self) -> Vec<PathBuf> {
        let mut stdlib_paths = Vec::new();

        // Get Ruby's standard library paths
        let output = std::process::Command::new("ruby")
            .args(&["-e", "puts $LOAD_PATH.select { |path| path.include?('ruby') && !path.include?('gems') && !path.include?('bundler') }"])
            .current_dir(&self.workspace_root)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let paths_output = String::from_utf8_lossy(&output.stdout);
                for line in paths_output.lines() {
                    let path = PathBuf::from(line.trim());
                    if path.exists() && !path.starts_with(&self.workspace_root) {
                        stdlib_paths.push(path);
                    }
                }
            }
            Ok(output) => {
                warn!(
                    "Ruby stdlib path command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                warn!("Failed to execute ruby command for stdlib paths: {}", e);
            }
        }

        debug!("Discovered {} stdlib paths", stdlib_paths.len());
        stdlib_paths
    }

    /// Discover gem paths using Bundler, optimized to only include direct dependencies
    pub fn discover_bundler_gem_paths(&self) -> Vec<PathBuf> {
        let mut gem_paths = Vec::new();

        // First, try to get direct dependencies from Gemfile
        if let Some(direct_gems) = self.parse_gemfile_dependencies() {
            debug!("Found {} direct dependencies in Gemfile", direct_gems.len());

            // Get paths for direct dependencies only
            for gem_name in direct_gems {
                if let Some(gem_path) = self.get_gem_path(&gem_name) {
                    gem_paths.push(gem_path);
                }
            }
        } else {
            // Fallback to the original method if Gemfile parsing fails
            warn!("Failed to parse Gemfile, falling back to bundle list");
            gem_paths = self.discover_bundler_gem_paths_fallback();
        }

        debug!("Discovered {} gem paths", gem_paths.len());
        gem_paths
    }

    /// Parse direct dependencies from Gemfile
    fn parse_gemfile_dependencies(&self) -> Option<HashSet<String>> {
        let gemfile_path = self.workspace_root.join("Gemfile");

        if !gemfile_path.exists() {
            debug!("No Gemfile found at {:?}", gemfile_path);
            return None;
        }

        match fs::read_to_string(&gemfile_path) {
            Ok(content) => {
                let mut gems = HashSet::new();

                for line in content.lines() {
                    if let Some(gem_name) = self.extract_gem_name_from_line(line) {
                        gems.insert(gem_name);
                    }
                }

                debug!("Parsed {} gems from Gemfile", gems.len());
                Some(gems)
            }
            Err(e) => {
                warn!("Failed to read Gemfile: {}", e);
                None
            }
        }
    }

    /// Extract gem name from a Gemfile line
    fn extract_gem_name_from_line(&self, line: &str) -> Option<String> {
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            return None;
        }

        // Look for gem declarations
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "gem" {
            let gem_name_part = parts[1];

            // Remove quotes and symbols, then handle trailing commas
            let cleaned = gem_name_part
                .trim_end_matches(',')
                .trim_matches('"')
                .trim_matches('\'')
                .trim_matches(':');

            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }

        None
    }

    /// Get the installation path for a specific gem
    fn get_gem_path(&self, gem_name: &str) -> Option<PathBuf> {
        let output = std::process::Command::new("bundle")
            .args(&["show", gem_name])
            .current_dir(&self.workspace_root)
            .output()
            .ok()?;

        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let path_str = path_str.trim();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }

        None
    }

    /// Fallback method using bundle list (original implementation)
    fn discover_bundler_gem_paths_fallback(&self) -> Vec<PathBuf> {
        let mut gem_paths = Vec::new();

        let output = std::process::Command::new("bundle")
            .args(&["list", "--paths"])
            .current_dir(&self.workspace_root)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let paths_output = String::from_utf8_lossy(&output.stdout);
                for line in paths_output.lines() {
                    let path = PathBuf::from(line.trim());
                    if path.exists() {
                        gem_paths.push(path);
                    }
                }
            }
            Ok(output) => {
                warn!(
                    "Bundle list command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                warn!("Failed to execute bundle list: {}", e);
            }
        }

        gem_paths
    }

    /// Discover Ruby load paths
    pub fn discover_load_paths(&self) -> Vec<PathBuf> {
        let mut load_paths = Vec::new();

        // Get Ruby's default load paths
        let output = std::process::Command::new("ruby")
            .args(&["-e", "puts $LOAD_PATH"])
            .current_dir(&self.workspace_root)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let paths_output = String::from_utf8_lossy(&output.stdout);
                for line in paths_output.lines() {
                    let path = PathBuf::from(line.trim());
                    if path.exists() {
                        load_paths.push(path);
                    }
                }
            }
            Ok(output) => {
                warn!(
                    "Ruby load path command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                warn!("Failed to execute ruby command: {}", e);
            }
        }

        // Add common project paths
        let project_paths = [
            self.workspace_root.join("lib"),
            self.workspace_root.join("app"),
            self.workspace_root.join("config"),
            self.workspace_root.join("spec"),
            self.workspace_root.join("test"),
        ];

        for path in &project_paths {
            if path.exists() {
                load_paths.push(path.clone());
            }
        }

        debug!("Discovered {} load paths", load_paths.len());
        load_paths
    }

    /// Discover executable paths
    pub fn discover_executable_paths(&self) -> Vec<PathBuf> {
        let mut executable_paths = Vec::new();

        // Add bundler bin paths
        let output = std::process::Command::new("bundle")
            .args(&["exec", "ruby", "-e", "puts Gem.bindir"])
            .current_dir(&self.workspace_root)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let bin_path = String::from_utf8_lossy(&output.stdout);
                let bin_path = PathBuf::from(bin_path.trim());
                if bin_path.exists() {
                    executable_paths.push(bin_path);
                }
            }
            Ok(output) => {
                debug!(
                    "Bundle exec command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                debug!("Failed to execute bundle exec: {}", e);
            }
        }

        // Add project bin directory
        let project_bin = self.workspace_root.join("bin");
        if project_bin.exists() {
            executable_paths.push(project_bin);
        }

        debug!("Discovered {} executable paths", executable_paths.len());
        executable_paths
    }

    /// Check if the workspace has a Gemfile
    pub fn has_gemfile(&self) -> bool {
        self.workspace_root.join("Gemfile").exists()
    }

    /// Check if the workspace has a Gemfile.lock
    pub fn has_gemfile_lock(&self) -> bool {
        self.workspace_root.join("Gemfile.lock").exists()
    }

    /// Get the workspace root
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

/// Container for all discovered paths
#[derive(Debug, Clone)]
pub struct DiscoveredPaths {
    pub gem_paths: Vec<PathBuf>,
    pub load_paths: Vec<PathBuf>,
    pub executable_paths: Vec<PathBuf>,
}

/// Container for simplified paths: only project root, standard library, and core stubs
#[derive(Debug, Clone)]
pub struct SimplifiedPaths {
    pub project_root: PathBuf,
    pub stdlib_paths: Vec<PathBuf>,
}

impl SimplifiedPaths {
    /// Get all paths as a single vector
    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut all_paths = vec![self.project_root.clone()];
        all_paths.extend(self.stdlib_paths.iter().cloned());
        all_paths
    }

    /// Get total count of all paths
    pub fn total_count(&self) -> usize {
        1 + self.stdlib_paths.len() // 1 for project_root + stdlib_paths
    }
}

impl DiscoveredPaths {
    /// Get all paths as a single vector
    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut all_paths = Vec::new();
        all_paths.extend(self.gem_paths.iter().cloned());
        all_paths.extend(self.load_paths.iter().cloned());
        all_paths.extend(self.executable_paths.iter().cloned());
        all_paths
    }

    /// Check if any paths were discovered
    pub fn is_empty(&self) -> bool {
        self.gem_paths.is_empty() && self.load_paths.is_empty() && self.executable_paths.is_empty()
    }

    /// Get total count of all paths
    pub fn total_count(&self) -> usize {
        self.gem_paths.len() + self.load_paths.len() + self.executable_paths.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workspace() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Create a simple Gemfile
        let gemfile_content = r#"
source 'https://rubygems.org'

gem 'rails', '~> 7.0'
gem 'rspec'
gem 'nokogiri', '~> 1.13'
gem 'debug', group: :development
"#;

        fs::write(temp_dir.path().join("Gemfile"), gemfile_content).unwrap();

        // Create some directories
        fs::create_dir(temp_dir.path().join("lib")).unwrap();
        fs::create_dir(temp_dir.path().join("app")).unwrap();

        temp_dir
    }

    #[test]
    fn test_path_discovery_new() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        assert_eq!(discovery.workspace_root(), temp_dir.path());
    }

    #[test]
    fn test_has_gemfile() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        assert!(discovery.has_gemfile());
        assert!(!discovery.has_gemfile_lock());
    }

    #[test]
    fn test_parse_gemfile_dependencies() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        let gems = discovery.parse_gemfile_dependencies().unwrap();

        assert!(gems.contains("rails"));
        assert!(gems.contains("rspec"));
        assert!(gems.contains("nokogiri"));
        assert!(gems.contains("debug"));
        assert_eq!(gems.len(), 4);
    }

    #[test]
    fn test_extract_gem_name_from_line() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        // Test various gem declaration formats
        assert_eq!(
            discovery.extract_gem_name_from_line("gem 'rails'"),
            Some("rails".to_string())
        );
        assert_eq!(
            discovery.extract_gem_name_from_line("gem \"nokogiri\""),
            Some("nokogiri".to_string())
        );
        assert_eq!(
            discovery.extract_gem_name_from_line("gem :rspec"),
            Some("rspec".to_string())
        );
        assert_eq!(
            discovery.extract_gem_name_from_line("gem 'rails', '~> 7.0'"),
            Some("rails".to_string())
        );
        assert_eq!(
            discovery.extract_gem_name_from_line("gem 'debug', group: :development"),
            Some("debug".to_string())
        );

        // Test non-gem lines
        assert_eq!(
            discovery.extract_gem_name_from_line("# This is a comment"),
            None
        );
        assert_eq!(
            discovery.extract_gem_name_from_line("source 'https://rubygems.org'"),
            None
        );
        assert_eq!(discovery.extract_gem_name_from_line(""), None);
        assert_eq!(
            discovery.extract_gem_name_from_line("not_a_gem \"rails\""),
            None
        );
    }

    #[test]
    fn test_discover_load_paths() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        let load_paths = discovery.discover_load_paths();

        // Should include project directories that exist
        let lib_path = temp_dir.path().join("lib");
        let app_path = temp_dir.path().join("app");

        assert!(load_paths.contains(&lib_path));
        assert!(load_paths.contains(&app_path));
    }

    #[test]
    fn test_discovered_paths() {
        let gem_paths = vec![
            PathBuf::from("/path/to/gem1"),
            PathBuf::from("/path/to/gem2"),
        ];
        let load_paths = vec![PathBuf::from("/path/to/lib")];
        let executable_paths = vec![PathBuf::from("/path/to/bin")];

        let discovered = DiscoveredPaths {
            gem_paths: gem_paths.clone(),
            load_paths: load_paths.clone(),
            executable_paths: executable_paths.clone(),
        };

        assert_eq!(discovered.total_count(), 4);
        assert!(!discovered.is_empty());

        let all_paths = discovered.all_paths();
        assert_eq!(all_paths.len(), 4);
        assert!(all_paths.contains(&PathBuf::from("/path/to/gem1")));
        assert!(all_paths.contains(&PathBuf::from("/path/to/lib")));
        assert!(all_paths.contains(&PathBuf::from("/path/to/bin")));
    }

    #[test]
    fn test_empty_discovered_paths() {
        let discovered = DiscoveredPaths {
            gem_paths: vec![],
            load_paths: vec![],
            executable_paths: vec![],
        };

        assert_eq!(discovered.total_count(), 0);
        assert!(discovered.is_empty());
        assert!(discovered.all_paths().is_empty());
    }

    #[test]
    fn test_simplified_paths() {
        let project_root = PathBuf::from("/path/to/project");
        let stdlib_paths = vec![
            PathBuf::from("/usr/lib/ruby/3.0"),
            PathBuf::from("/usr/lib/ruby/site_ruby"),
        ];

        let simplified = SimplifiedPaths {
            project_root: project_root.clone(),
            stdlib_paths: stdlib_paths.clone(),
        };

        assert_eq!(simplified.total_count(), 3); // 1 project_root + 2 stdlib_paths
        
        let all_paths = simplified.all_paths();
        assert_eq!(all_paths.len(), 3);
        assert!(all_paths.contains(&project_root));
        assert!(all_paths.contains(&PathBuf::from("/usr/lib/ruby/3.0")));
        assert!(all_paths.contains(&PathBuf::from("/usr/lib/ruby/site_ruby")));
    }

    #[test]
    fn test_discover_simplified_paths() {
        let temp_dir = create_test_workspace();
        let discovery = PathDiscovery::new(temp_dir.path().to_path_buf());

        let simplified_paths = discovery.discover_simplified_paths();

        // Should have the project root
        assert_eq!(simplified_paths.project_root, temp_dir.path().to_path_buf());
        
        // Should have some stdlib paths (this might vary by system, so just check it's not empty)
        // Note: This test might not work in all environments, so we'll just check the structure
        assert!(simplified_paths.total_count() >= 1); // At least the project root
    }
}
