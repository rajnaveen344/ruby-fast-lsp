use crate::types::ruby_version::RubyVersion;
use log::debug;

// ============================================================================
// Ruby Version Detection
// ============================================================================

/// Detect system Ruby version without workspace context
pub fn detect_system_ruby_version() -> Option<(u8, u8)> {
    let output = std::process::Command::new("ruby")
        .args(["--version"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    // Parse output like "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
    let version_part = version_output.split_whitespace().nth(1)?;
    debug!("System ruby version output: {}", version_part);
    let version = RubyVersion::from_full_version(version_part)?;
    Some((version.major, version.minor))
}
