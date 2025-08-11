use std::collections::HashMap;
use log::{debug, warn};

use crate::version::MinorVersion;

/// Represents different Ruby version managers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionManager {
    Rbenv,
    Rvm,
    Chruby,
    System,
}

impl VersionManager {
    /// Detect which version managers are available on the system
    pub fn detect_available() -> Vec<VersionManager> {
        let mut managers = Vec::new();

        // Check for rbenv
        if std::process::Command::new("rbenv")
            .args(&["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            managers.push(VersionManager::Rbenv);
        }

        // Check for rvm
        if std::process::Command::new("rvm")
            .args(&["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            managers.push(VersionManager::Rvm);
        }

        // Check for chruby
        if std::process::Command::new("chruby")
            .args(&["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            managers.push(VersionManager::Chruby);
        }

        // System ruby is always available if ruby command exists
        if std::process::Command::new("ruby")
            .args(&["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            managers.push(VersionManager::System);
        }

        debug!("Detected version managers: {:?}", managers);
        managers
    }

    /// Get all installed Ruby versions for this manager
    pub fn get_installed_versions(&self) -> Vec<MinorVersion> {
        match self {
            VersionManager::Rbenv => self.get_rbenv_versions(),
            VersionManager::Rvm => self.get_rvm_versions(),
            VersionManager::Chruby => self.get_chruby_versions(),
            VersionManager::System => self.get_system_version().into_iter().collect(),
        }
    }

    /// Get the currently active version for this manager
    pub fn get_current_version(&self) -> Option<MinorVersion> {
        match self {
            VersionManager::Rbenv => self.get_rbenv_current(),
            VersionManager::Rvm => self.get_rvm_current(),
            VersionManager::Chruby => self.get_chruby_current(),
            VersionManager::System => self.get_system_version(),
        }
    }

    fn get_rbenv_versions(&self) -> Vec<MinorVersion> {
        let mut versions = Vec::new();
        
        if let Ok(output) = std::process::Command::new("rbenv")
            .args(&["versions", "--bare"])
            .output()
        {
            if output.status.success() {
                let versions_output = String::from_utf8_lossy(&output.stdout);
                for line in versions_output.lines() {
                    let version_str = line.trim();
                    if let Some(version) = MinorVersion::from_full_version(version_str) {
                        versions.push(version);
                    }
                }
            }
        }

        versions.sort();
        versions.dedup();
        versions
    }

    fn get_rbenv_current(&self) -> Option<MinorVersion> {
        if let Ok(output) = std::process::Command::new("rbenv")
            .args(&["version"])
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                if let Some(version_str) = version_output.split_whitespace().next() {
                    return MinorVersion::from_full_version(version_str);
                }
            }
        }
        None
    }

    fn get_rvm_versions(&self) -> Vec<MinorVersion> {
        let mut versions = Vec::new();
        
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
                            versions.push(version);
                        }
                    }
                }
            }
        }

        versions.sort();
        versions.dedup();
        versions
    }

    fn get_rvm_current(&self) -> Option<MinorVersion> {
        if let Ok(output) = std::process::Command::new("rvm")
            .args(&["current"])
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                if let Some(version_str) = version_output.strip_prefix("ruby-") {
                    let version_str = version_str.trim();
                    return MinorVersion::from_full_version(version_str);
                }
            }
        }
        None
    }

    fn get_chruby_versions(&self) -> Vec<MinorVersion> {
        let mut versions = Vec::new();
        
        if let Ok(output) = std::process::Command::new("chruby")
            .output()
        {
            if output.status.success() {
                let versions_output = String::from_utf8_lossy(&output.stdout);
                for line in versions_output.lines() {
                    let line = line.trim();
                    // chruby output format: "   ruby-3.0.0"
                    if let Some(version_str) = line.strip_prefix("ruby-") {
                        let version_str = version_str.trim_start_matches('*').trim();
                        if let Some(version) = MinorVersion::from_full_version(version_str) {
                            versions.push(version);
                        }
                    }
                }
            }
        }

        versions.sort();
        versions.dedup();
        versions
    }

    fn get_chruby_current(&self) -> Option<MinorVersion> {
        if let Ok(output) = std::process::Command::new("chruby")
            .output()
        {
            if output.status.success() {
                let versions_output = String::from_utf8_lossy(&output.stdout);
                for line in versions_output.lines() {
                    let line = line.trim();
                    // Look for the line with * indicating current version
                    if line.starts_with('*') {
                        if let Some(version_str) = line.strip_prefix("* ruby-") {
                            let version_str = version_str.trim();
                            return MinorVersion::from_full_version(version_str);
                        }
                    }
                }
            }
        }
        None
    }

    fn get_system_version(&self) -> Option<MinorVersion> {
        if let Ok(output) = std::process::Command::new("ruby")
            .args(&["--version"])
            .output()
        {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                if let Some(version_part) = version_output.split_whitespace().nth(1) {
                    return MinorVersion::from_full_version(version_part);
                }
            }
        }
        None
    }
}

/// Aggregates information from all available version managers
pub struct VersionManagerRegistry {
    managers: Vec<VersionManager>,
}

impl VersionManagerRegistry {
    pub fn new() -> Self {
        let managers = VersionManager::detect_available();
        Self { managers }
    }

    /// Get all unique Ruby versions across all managers
    pub fn get_all_versions(&self) -> Vec<MinorVersion> {
        let mut all_versions = Vec::new();
        
        for manager in &self.managers {
            let versions = manager.get_installed_versions();
            for version in versions {
                if !all_versions.contains(&version) {
                    all_versions.push(version);
                }
            }
        }

        all_versions.sort();
        all_versions
    }

    /// Get version information grouped by manager
    pub fn get_versions_by_manager(&self) -> HashMap<VersionManager, Vec<MinorVersion>> {
        let mut versions_map = HashMap::new();
        
        for manager in &self.managers {
            let versions = manager.get_installed_versions();
            versions_map.insert(manager.clone(), versions);
        }

        versions_map
    }

    /// Get the currently active version from the highest priority manager
    pub fn get_current_version(&self) -> Option<MinorVersion> {
        // Priority order: rbenv > rvm > chruby > system
        let priority_order = [
            VersionManager::Rbenv,
            VersionManager::Rvm,
            VersionManager::Chruby,
            VersionManager::System,
        ];

        for preferred_manager in &priority_order {
            if self.managers.contains(preferred_manager) {
                if let Some(version) = preferred_manager.get_current_version() {
                    debug!("Current Ruby version {} from {:?}", version, preferred_manager);
                    return Some(version);
                }
            }
        }

        warn!("Could not determine current Ruby version from any manager");
        None
    }

    pub fn get_available_managers(&self) -> &[VersionManager] {
        &self.managers
    }
}

impl Default for VersionManagerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_manager_registry() {
        let registry = VersionManagerRegistry::new();
        
        // Should detect at least system ruby if available
        let managers = registry.get_available_managers();
        
        // Test that we can call methods without panicking
        let _all_versions = registry.get_all_versions();
        let _versions_by_manager = registry.get_versions_by_manager();
        let _current = registry.get_current_version();
        
        // Basic sanity check
        assert!(!managers.is_empty() || std::process::Command::new("ruby").output().is_err());
    }
}