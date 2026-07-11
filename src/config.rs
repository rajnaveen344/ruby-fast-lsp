use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinterKind {
    #[default]
    None,
    #[serde(rename = "rubocop")]
    RuboCop,
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FormatterKind {
    #[default]
    None,
    #[serde(rename = "rubocop")]
    RuboCop,
    Standard,
}

impl FormatterKind {
    pub fn executable(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RuboCop => Some("rubocop"),
            Self::Standard => Some("standardrb"),
        }
    }

    pub fn data_name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RuboCop => Some("rubocop"),
            Self::Standard => Some("standard"),
        }
    }
}

impl LinterKind {
    pub fn executable(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RuboCop => Some("rubocop"),
            Self::Standard => Some("standardrb"),
        }
    }

    pub fn diagnostic_source(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RuboCop => Some("RuboCop"),
            Self::Standard => Some("Standard"),
        }
    }

    pub fn data_name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RuboCop => Some("rubocop"),
            Self::Standard => Some("standard"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexingConfig {
    #[serde(rename = "excludedPatterns")]
    pub excluded_patterns: Vec<String>,

    #[serde(rename = "includedPatterns")]
    pub included_patterns: Vec<String>,

    #[serde(rename = "excludedGems")]
    pub excluded_gems: Vec<String>,

    #[serde(rename = "includedGems")]
    pub included_gems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RubyFastLspConfig {
    #[serde(rename = "rubyVersion")]
    pub ruby_version: String,

    #[serde(rename = "extensionPath")]
    pub extension_path: Option<String>,

    #[serde(rename = "extensionPackages")]
    pub extension_packages: Vec<String>,

    #[serde(rename = "extensionDirs")]
    pub extension_dirs: Vec<String>,

    #[serde(rename = "extensionSettings")]
    pub extension_settings: BTreeMap<String, serde_json::Value>,

    #[serde(rename = "codeLens.modules.enabled")]
    pub code_lens_modules_enabled: Option<bool>,

    #[serde(rename = "logLevel")]
    pub log_level: String,

    /// Optional external linter. It runs on document open and save, never in
    /// the didChange typing path.
    pub linter: LinterKind,

    /// Structured command argv. Empty uses `bundle exec <linter>`.
    #[serde(rename = "linterCommand")]
    pub linter_command: Vec<String>,

    /// Optional external full-document formatter.
    pub formatter: FormatterKind,

    /// Structured formatter command argv. Empty uses `bundle exec <formatter>`.
    #[serde(rename = "formatterCommand")]
    pub formatter_command: Vec<String>,

    pub indexing: IndexingConfig,
}

impl Default for RubyFastLspConfig {
    fn default() -> Self {
        Self {
            ruby_version: "auto".to_string(),
            extension_path: None,
            extension_packages: Vec::new(),
            extension_dirs: Vec::new(),
            extension_settings: BTreeMap::new(),
            code_lens_modules_enabled: Some(true),
            log_level: "info".to_string(),
            linter: LinterKind::None,
            linter_command: Vec::new(),
            formatter: FormatterKind::None,
            formatter_command: Vec::new(),
            indexing: IndexingConfig::default(),
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
