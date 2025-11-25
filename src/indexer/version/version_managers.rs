//! Version Manager Integration
//!
//! Provides interfaces to Ruby version managers (rbenv, rvm, chruby) and system Ruby.
//! Allows querying installed versions and the currently active version.

use log::{debug, warn};
use std::collections::HashMap;

use crate::types::ruby_version::RubyVersion;

// ============================================================================
// VersionManager Enum
// ============================================================================

/// Represents different Ruby version managers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionManager {
    Rbenv,
    Rvm,
    Chruby,
    System,
}

impl VersionManager {
    // ========================================================================
    // Detection
    // ========================================================================

    /// Detect which version managers are available on the system
    pub fn detect_available() -> Vec<VersionManager> {
        let mut managers = Vec::new();

        if is_command_available("rbenv", &["--version"]) {
            managers.push(VersionManager::Rbenv);
        }
        if is_command_available("rvm", &["--version"]) {
            managers.push(VersionManager::Rvm);
        }
        if is_command_available("chruby", &["--version"]) {
            managers.push(VersionManager::Chruby);
        }
        if is_command_available("ruby", &["--version"]) {
            managers.push(VersionManager::System);
        }

        debug!("Detected version managers: {:?}", managers);
        managers
    }

    // ========================================================================
    // Version Queries
    // ========================================================================

    /// Get all installed Ruby versions for this manager
    pub fn get_installed_versions(&self) -> Vec<RubyVersion> {
        let mut versions = match self {
            VersionManager::Rbenv => get_rbenv_versions(),
            VersionManager::Rvm => get_rvm_versions(),
            VersionManager::Chruby => get_chruby_versions(),
            VersionManager::System => get_system_version().into_iter().collect(),
        };
        versions.sort();
        versions.dedup();
        versions
    }

    /// Get the currently active version for this manager
    pub fn get_current_version(&self) -> Option<RubyVersion> {
        match self {
            VersionManager::Rbenv => get_rbenv_current(),
            VersionManager::Rvm => get_rvm_current(),
            VersionManager::Chruby => get_chruby_current(),
            VersionManager::System => get_system_version(),
        }
    }
}

// ============================================================================
// VersionManagerRegistry
// ============================================================================

/// Aggregates information from all available version managers
pub struct VersionManagerRegistry {
    managers: Vec<VersionManager>,
}

impl VersionManagerRegistry {
    pub fn new() -> Self {
        Self {
            managers: VersionManager::detect_available(),
        }
    }

    /// Get all available managers
    pub fn get_available_managers(&self) -> &[VersionManager] {
        &self.managers
    }

    /// Get all unique Ruby versions across all managers
    pub fn get_all_versions(&self) -> Vec<RubyVersion> {
        let mut all_versions = Vec::new();

        for manager in &self.managers {
            for version in manager.get_installed_versions() {
                if !all_versions.contains(&version) {
                    all_versions.push(version);
                }
            }
        }

        all_versions.sort();
        all_versions
    }

    /// Get version information grouped by manager
    pub fn get_versions_by_manager(&self) -> HashMap<VersionManager, Vec<RubyVersion>> {
        self.managers
            .iter()
            .map(|m| (m.clone(), m.get_installed_versions()))
            .collect()
    }

    /// Get the currently active version from the highest priority manager
    pub fn get_current_version(&self) -> Option<RubyVersion> {
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
                    debug!(
                        "Current Ruby version {} from {:?}",
                        version, preferred_manager
                    );
                    return Some(version);
                }
            }
        }

        warn!("Could not determine current Ruby version from any manager");
        None
    }
}

impl Default for VersionManagerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn is_command_available(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn get_rbenv_versions() -> Vec<RubyVersion> {
    run_command_and_parse_versions("rbenv", &["versions", "--bare"], |line| {
        RubyVersion::from_full_version(line.trim())
    })
}

fn get_rbenv_current() -> Option<RubyVersion> {
    let output = std::process::Command::new("rbenv")
        .args(["version"])
        .output()
        .ok()?;

    if output.status.success() {
        let version_output = String::from_utf8_lossy(&output.stdout);
        version_output
            .split_whitespace()
            .next()
            .and_then(RubyVersion::from_full_version)
    } else {
        None
    }
}

fn get_rvm_versions() -> Vec<RubyVersion> {
    run_command_and_parse_versions("rvm", &["list", "strings"], |line| {
        line.trim()
            .strip_prefix("ruby-")
            .and_then(RubyVersion::from_full_version)
    })
}

fn get_rvm_current() -> Option<RubyVersion> {
    let output = std::process::Command::new("rvm")
        .args(["current"])
        .output()
        .ok()?;

    if output.status.success() {
        let version_output = String::from_utf8_lossy(&output.stdout);
        version_output
            .strip_prefix("ruby-")
            .map(|s| s.trim())
            .and_then(RubyVersion::from_full_version)
    } else {
        None
    }
}

fn get_chruby_versions() -> Vec<RubyVersion> {
    run_command_and_parse_versions("chruby", &[], |line| {
        let line = line.trim();
        line.strip_prefix("ruby-")
            .map(|s| s.trim_start_matches('*').trim())
            .and_then(RubyVersion::from_full_version)
    })
}

fn get_chruby_current() -> Option<RubyVersion> {
    let output = std::process::Command::new("chruby").output().ok()?;

    if output.status.success() {
        let versions_output = String::from_utf8_lossy(&output.stdout);
        for line in versions_output.lines() {
            let line = line.trim();
            if line.starts_with('*') {
                return line
                    .strip_prefix("* ruby-")
                    .map(|s| s.trim())
                    .and_then(RubyVersion::from_full_version);
            }
        }
    }
    None
}

fn get_system_version() -> Option<RubyVersion> {
    let output = std::process::Command::new("ruby")
        .args(["--version"])
        .output()
        .ok()?;

    if output.status.success() {
        let version_output = String::from_utf8_lossy(&output.stdout);
        version_output
            .split_whitespace()
            .nth(1)
            .and_then(RubyVersion::from_full_version)
    } else {
        None
    }
}

fn run_command_and_parse_versions<F>(cmd: &str, args: &[&str], parser: F) -> Vec<RubyVersion>
where
    F: Fn(&str) -> Option<RubyVersion>,
{
    let mut versions = Vec::new();

    if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if let Some(version) = parser(line) {
                    versions.push(version);
                }
            }
        }
    }

    versions
}

// ============================================================================
// Tests
// ============================================================================

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
