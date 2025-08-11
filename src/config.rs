use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RubyFastLspConfig {
    #[serde(rename = "rubyVersion")]
    pub ruby_version: String,
    
    #[serde(rename = "enableCoreStubs")]
    pub enable_core_stubs: bool,
    
    #[serde(rename = "stubsPath")]
    pub stubs_path: String,
    
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
            stubs_path: String::new(),
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
    pub fn get_ruby_version(&self) -> Option<crate::stubs::version::MinorVersion> {
        if self.ruby_version == "auto" {
            None // Will trigger auto-detection
        } else {
            // Parse version like "3.0" -> MinorVersion(3, 0)
            let parts: Vec<&str> = self.ruby_version.split('.').collect();
            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
                return Some(crate::stubs::version::MinorVersion::new(major, minor));
            }
            }
            None
        }
    }
}