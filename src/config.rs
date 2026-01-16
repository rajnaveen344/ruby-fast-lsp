use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RubyFastLspConfig {
    #[serde(rename = "rubyVersion")]
    pub ruby_version: String,

    #[serde(rename = "extensionPath")]
    pub extension_path: Option<String>,

    #[serde(rename = "codeLens.modules.enabled")]
    pub code_lens_modules_enabled: Option<bool>,

    #[serde(rename = "logLevel")]
    pub log_level: String,
}

impl Default for RubyFastLspConfig {
    fn default() -> Self {
        Self {
            ruby_version: "auto".to_string(),
            extension_path: None,
            code_lens_modules_enabled: Some(true),
            log_level: "info".to_string(),
        }
    }
}

impl RubyFastLspConfig {
    /// Apply log level from configuration
    pub fn apply_log_level(&self) {
        let level = match self.log_level.as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            _ => log::LevelFilter::Info,
        };
        log::set_max_level(level);
        log::info!("Log level set to: {}", self.log_level);
    }

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

    /// Get index paths based on Ruby version and workspace (simplified)
    pub fn get_index_paths(&self, ruby_version: (u8, u8), workspace_root: PathBuf) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Add workspace root
        paths.push(workspace_root);

        if let Some(core_stubs_path) = self.get_core_stubs_path_internal(ruby_version) {
            paths.push(PathBuf::from(core_stubs_path));
        }

        paths
    }

    /// Get the core stubs path for the detected Ruby version
    pub fn get_core_stubs_path_for_version(&self, ruby_version: (u8, u8)) -> Option<PathBuf> {
        self.get_core_stubs_path_internal(ruby_version)
            .map(PathBuf::from)
    }

    /// Internal method to get core stubs path
    pub fn get_core_stubs_path_internal(&self, ruby_version: (u8, u8)) -> Option<String> {
        // Use extension path if available
        if let Some(ref ext_path) = self.extension_path {
            let stubs_dir = PathBuf::from(ext_path).join("stubs");
            if stubs_dir.exists() {
                let version_dir = format!("rubystubs{}{}", ruby_version.0, ruby_version.1);
                let version_path = stubs_dir.join(version_dir);
                if version_path.exists() {
                    return Some(version_path.to_string_lossy().to_string());
                }

                // Fallback to default rubystubs30 if specific version not found
                let default_path = stubs_dir.join("rubystubs30");
                if default_path.exists() {
                    return Some(default_path.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    /// Get the core stubs path for the detected Ruby version (deprecated - use get_index_paths instead)
    #[deprecated(note = "Use get_index_paths instead for automatic path discovery")]
    pub fn get_core_stubs_path(&self, ruby_version: (u8, u8)) -> Option<String> {
        // Delegate to the internal method for consistency
        self.get_core_stubs_path_internal(ruby_version)
    }
}
