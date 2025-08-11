use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::ruby_library::PathDiscovery;


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RubyFastLspConfig {
    #[serde(rename = "rubyVersion")]
    pub ruby_version: String,
    
    #[serde(rename = "enableCoreStubs")]
    pub enable_core_stubs: bool,
    
    #[serde(rename = "versionDetection")]
    pub version_detection: VersionDetectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VersionDetectionConfig {
    #[serde(rename = "enableRbenv")]
    pub enable_rbenv: bool,
    
    #[serde(rename = "enableRvm")]
    pub enable_rvm: bool,
    
    #[serde(rename = "enableChruby")]
    pub enable_chruby: bool,
    
    #[serde(rename = "enableSystemRuby")]
    pub enable_system_ruby: bool,
}

impl Default for RubyFastLspConfig {
    fn default() -> Self {
        Self {
            ruby_version: "auto".to_string(),
            enable_core_stubs: true,
            version_detection: VersionDetectionConfig::default(),
        }
    }
}

impl Default for VersionDetectionConfig {
    fn default() -> Self {
        Self {
            enable_rbenv: true,
            enable_rvm: true,
            enable_chruby: true,
            enable_system_ruby: true,
        }
    }
}

impl RubyFastLspConfig {
    /// Parse Ruby version from configuration
    pub fn get_ruby_version(&self) -> Option<(u8, u8)> {
        if self.ruby_version == "auto" {
            None // Will trigger auto-detection
        } else {
            // Parse version like "3.0" -> (3, 0)
            let parts: Vec<&str> = self.ruby_version.split('.').collect();
            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
                    return Some((major, minor));
                }
            }
            None
        }
    }
    
    /// Get automatically discovered index paths based on Ruby version and workspace
    pub fn get_index_paths(&self, ruby_version: (u8, u8), workspace_root: PathBuf) -> Vec<PathBuf> {
        // Use path discovery to automatically find all relevant paths
        let discovery = PathDiscovery::new(workspace_root);
        let discovered_paths = discovery.discover_all_paths();
        
        let mut paths = Vec::new();
        
        // Add gem paths from current project
        paths.extend(discovered_paths.gem_paths);
        
        // Add load paths (includes Ruby stdlib and project paths)
        paths.extend(discovered_paths.load_paths);
        
        // Add core stubs if enabled
        if self.enable_core_stubs {
            if let Some(core_stubs_path) = self.get_core_stubs_path_internal(ruby_version) {
                paths.push(PathBuf::from(core_stubs_path));
            }
        }
        
        paths
    }

    /// Get the core stubs path for the detected Ruby version
    pub fn get_core_stubs_path_for_version(&self, ruby_version: (u8, u8)) -> Option<PathBuf> {
        if !self.enable_core_stubs {
            return None;
        }
        
        self.get_core_stubs_path_internal(ruby_version).map(PathBuf::from)
    }

    /// Internal method to get core stubs path
    fn get_core_stubs_path_internal(&self, ruby_version: (u8, u8)) -> Option<String> {
        if !self.enable_core_stubs {
            return None;
        }
        
        // Find the executable path and construct stubs path
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // In VSIX package: executable is in bin/<platform>/, stubs are in stubs/
                // So we need to go up two levels: bin/<platform>/ -> bin/ -> root/ -> stubs/
                let vsix_stubs = exe_dir.parent()?.parent()?.join("stubs");
                if vsix_stubs.exists() {
                    let version_dir = format!("rubystubs{}{}", ruby_version.0, ruby_version.1);
                    let version_path = vsix_stubs.join(version_dir);
                    if version_path.exists() {
                        return Some(version_path.to_string_lossy().to_string());
                    }
                }
            }
        }
        None
    }
    
    /// Get the core stubs path for the detected Ruby version (deprecated - use get_index_paths instead)
    #[deprecated(note = "Use get_index_paths instead for automatic path discovery")]
    pub fn get_core_stubs_path(&self, ruby_version: (u8, u8)) -> Option<String> {
        if !self.enable_core_stubs {
            return None;
        }
        
        // Find the executable path and construct stubs path
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // In VSIX package: executable is in bin/<platform>/, stubs are in stubs/
                // So we need to go up two levels: bin/<platform>/ -> bin/ -> root/ -> stubs/
                let vsix_stubs = exe_dir.parent()?.parent()?.join("stubs");
                if vsix_stubs.exists() {
                    let version_dir = format!("rubystubs{}{}", ruby_version.0, ruby_version.1);
                    let version_path = vsix_stubs.join(version_dir);
                    if version_path.exists() {
                        return Some(version_path.to_string_lossy().to_string());
                    }
                }
            }
        }
        None
    }
}