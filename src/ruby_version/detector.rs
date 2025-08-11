use log::{debug, info, warn};
use std::fs;
use std::path::PathBuf;
use tower_lsp::lsp_types::Url;

use crate::version::MinorVersion;

/// Detects Ruby version from various workspace sources
pub struct RubyVersionDetector {
    workspace_root: PathBuf,
}

impl RubyVersionDetector {
    pub fn new(workspace_uri: &Url) -> Option<Self> {
        let workspace_root = workspace_uri.to_file_path().ok()?;
        Some(Self { workspace_root })
    }

    /// Detect Ruby version from workspace, trying multiple sources in priority order
    pub fn detect_version(&self) -> Option<MinorVersion> {
        // Priority order for version detection
        let detection_methods: Vec<(&str, fn(&Self) -> Option<MinorVersion>)> = vec![
            ("ruby-version file", Self::detect_from_ruby_version),
            ("Gemfile", Self::detect_from_gemfile),
            ("rbenv version", Self::detect_from_rbenv),
            ("rvm current", Self::detect_from_rvm),
            ("system ruby", Self::detect_from_system),
        ];

        for (method_name, detect_fn) in detection_methods {
            if let Some(version) = detect_fn(self) {
                info!("Detected Ruby version {} from {}", version, method_name);
                return Some(version);
            } else {
                debug!("No Ruby version found from {}", method_name);
            }
        }

        warn!("Could not detect Ruby version from workspace");
        None
    }

    /// Detect version from .ruby-version file
    fn detect_from_ruby_version(&self) -> Option<MinorVersion> {
        let ruby_version_file = self.workspace_root.join(".ruby-version");
        if !ruby_version_file.exists() {
            return None;
        }

        let content = fs::read_to_string(&ruby_version_file).ok()?;
        let version_str = content.trim();

        debug!("Found .ruby-version file with content: {}", version_str);
        MinorVersion::from_full_version(version_str)
    }

    /// Detect version from Gemfile ruby directive
    fn detect_from_gemfile(&self) -> Option<MinorVersion> {
        let gemfile_path = self.workspace_root.join("Gemfile");
        if !gemfile_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&gemfile_path).ok()?;

        // Look for ruby version specification in Gemfile
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("ruby ") {
                // Extract version from patterns like:
                // ruby "3.0.0"
                // ruby '2.7.4'
                // ruby "~> 3.0"
                if let Some(version_part) = line.split_whitespace().nth(1) {
                    let version_str = version_part.trim_matches(|c| {
                        c == '"' || c == '\'' || c == '~' || c == '>' || c == ' '
                    });
                    debug!("Found Gemfile ruby directive: {}", version_str);
                    if let Some(version) = MinorVersion::from_full_version(version_str) {
                        return Some(version);
                    }
                }
            }
        }

        None
    }

    /// Detect version from rbenv
    fn detect_from_rbenv(&self) -> Option<MinorVersion> {
        // Check for .rbenv-version file
        let rbenv_version_file = self.workspace_root.join(".rbenv-version");
        if rbenv_version_file.exists() {
            if let Ok(content) = fs::read_to_string(&rbenv_version_file) {
                let version_str = content.trim();
                debug!("Found .rbenv-version file with content: {}", version_str);
                if let Some(version) = MinorVersion::from_full_version(version_str) {
                    return Some(version);
                }
            }
        }

        // Try rbenv version command
        if let Ok(output) = std::process::Command::new("rbenv")
            .args(&["version"])
            .current_dir(&self.workspace_root)
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                // Parse output like "3.0.0 (set by /path/to/.ruby-version)"
                if let Some(version_str) = version_output.split_whitespace().next() {
                    debug!("rbenv version output: {}", version_str);
                    if let Some(version) = MinorVersion::from_full_version(version_str) {
                        return Some(version);
                    }
                }
            }
        }

        None
    }

    /// Detect version from rvm
    fn detect_from_rvm(&self) -> Option<MinorVersion> {
        // Check for .rvmrc file
        let rvmrc_file = self.workspace_root.join(".rvmrc");
        if rvmrc_file.exists() {
            if let Ok(content) = fs::read_to_string(&rvmrc_file) {
                // Look for ruby version in .rvmrc
                for line in content.lines() {
                    if line.contains("ruby-") {
                        // Extract version from patterns like "rvm use ruby-3.0.0"
                        if let Some(ruby_part) = line.split("ruby-").nth(1) {
                            let version_str = ruby_part.split_whitespace().next().unwrap_or("");
                            debug!("Found .rvmrc ruby version: {}", version_str);
                            if let Some(version) = MinorVersion::from_full_version(version_str) {
                                return Some(version);
                            }
                        }
                    }
                }
            }
        }

        // Try rvm current command
        if let Ok(output) = std::process::Command::new("rvm")
            .args(&["current"])
            .current_dir(&self.workspace_root)
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                // Parse output like "ruby-3.0.0"
                if let Some(version_str) = version_output.strip_prefix("ruby-") {
                    let version_str = version_str.trim();
                    debug!("rvm current output: {}", version_str);
                    if let Some(version) = MinorVersion::from_full_version(version_str) {
                        return Some(version);
                    }
                }
            }
        }

        None
    }

    /// Detect version from system ruby
    fn detect_from_system(&self) -> Option<MinorVersion> {
        if let Ok(output) = std::process::Command::new("ruby")
            .args(&["--version"])
            .current_dir(&self.workspace_root)
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                // Parse output like "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
                if let Some(version_part) = version_output.split_whitespace().nth(1) {
                    debug!("System ruby version output: {}", version_part);
                    if let Some(version) = MinorVersion::from_full_version(version_part) {
                        return Some(version);
                    }
                }
            }
        }

        None
    }

    /// Get all available Ruby versions from version managers
    pub fn get_available_versions(&self) -> Vec<MinorVersion> {
        let mut versions = Vec::new();

        // Get versions from rbenv
        if let Ok(output) = std::process::Command::new("rbenv")
            .args(&["versions", "--bare"])
            .output()
        {
            if output.status.success() {
                let versions_output = String::from_utf8_lossy(&output.stdout);
                for line in versions_output.lines() {
                    let version_str = line.trim();
                    if let Some(version) = MinorVersion::from_full_version(version_str) {
                        if !versions.contains(&version) {
                            versions.push(version);
                        }
                    }
                }
            }
        }

        // Get versions from rvm
        if let Ok(output) = std::process::Command::new("rvm")
            .args(&["list", "strings"])
            .output()
        {
            if output.status.success() {
                let versions_output = String::from_utf8_lossy(&output.stdout);
                for line in versions_output.lines() {
                    let line = line.trim();
                    if let Some(version_str) = line.strip_prefix("ruby-") {
                        if let Some(version) = MinorVersion::from_full_version(version_str) {
                            if !versions.contains(&version) {
                                versions.push(version);
                            }
                        }
                    }
                }
            }
        }

        versions.sort();
        versions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workspace() -> (TempDir, RubyVersionDetector) {
        let temp_dir = TempDir::new().unwrap();
        let workspace_uri = Url::from_file_path(temp_dir.path()).unwrap();
        let detector = RubyVersionDetector::new(&workspace_uri).unwrap();
        (temp_dir, detector)
    }

    #[test]
    fn test_detect_from_ruby_version_file() {
        let (temp_dir, detector) = create_test_workspace();

        // Create .ruby-version file
        let ruby_version_path = temp_dir.path().join(".ruby-version");
        fs::write(&ruby_version_path, "3.0.4").unwrap();

        let version = detector.detect_from_ruby_version();
        assert_eq!(version, Some(MinorVersion::new(3, 0)));
    }

    #[test]
    fn test_detect_from_gemfile() {
        let (temp_dir, detector) = create_test_workspace();

        // Create Gemfile with ruby directive
        let gemfile_path = temp_dir.path().join("Gemfile");
        fs::write(
            &gemfile_path,
            r#"
source 'https://rubygems.org'

ruby "2.7.6"

gem 'rails', '~> 7.0'
"#,
        )
        .unwrap();

        let version = detector.detect_from_gemfile();
        assert_eq!(version, Some(MinorVersion::new(2, 7)));
    }

    #[test]
    fn test_detect_from_rbenv_version_file() {
        let (temp_dir, detector) = create_test_workspace();

        // Create .rbenv-version file
        let rbenv_version_path = temp_dir.path().join(".rbenv-version");
        fs::write(&rbenv_version_path, "3.1.0").unwrap();

        let version = detector.detect_from_rbenv();
        assert_eq!(version, Some(MinorVersion::new(3, 1)));
    }
}
