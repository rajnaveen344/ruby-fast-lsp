use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Supported Ruby minor versions for stub generation
/// This array drives the build-time generation process
pub const SUPPORTED_RUBY_VERSIONS: &[MinorVersion] = &[
    MinorVersion { major: 1, minor: 9 },
    MinorVersion { major: 2, minor: 0 },
    MinorVersion { major: 2, minor: 1 },
    MinorVersion { major: 2, minor: 2 },
    MinorVersion { major: 2, minor: 3 },
    MinorVersion { major: 2, minor: 4 },
    MinorVersion { major: 2, minor: 5 },
    MinorVersion { major: 2, minor: 6 },
    MinorVersion { major: 2, minor: 7 },
    MinorVersion { major: 3, minor: 0 },
    MinorVersion { major: 3, minor: 1 },
    MinorVersion { major: 3, minor: 2 },
    MinorVersion { major: 3, minor: 3 },
    MinorVersion { major: 3, minor: 4 },
];

/// Represents a Ruby minor version (e.g., 2.7, 3.0)
/// Patch versions are ignored for stub organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MinorVersion {
    pub major: u8,
    pub minor: u8,
}

impl MinorVersion {
    pub fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    /// Parse a full Ruby version string (e.g., "2.7.6") into a MinorVersion
    pub fn from_full_version(version: &str) -> Result<Self, VersionParseError> {
        let parts: Vec<&str> = version.split('.').collect();
        
        if parts.len() < 2 {
            return Err(VersionParseError::InvalidFormat(version.to_string()));
        }

        let major = parts[0].parse::<u8>()
            .map_err(|_| VersionParseError::InvalidMajor(parts[0].to_string()))?;
        
        let minor = parts[1].parse::<u8>()
            .map_err(|_| VersionParseError::InvalidMinor(parts[1].to_string()))?;

        Ok(Self { major, minor })
    }

    /// Find the closest supported version for fallback
    /// Returns the exact match if available, otherwise the closest lower version
    pub fn find_closest_supported(&self) -> Option<MinorVersion> {
        // First try exact match
        if SUPPORTED_RUBY_VERSIONS.contains(self) {
            return Some(*self);
        }

        // Find the highest supported version that's lower than the requested version
        SUPPORTED_RUBY_VERSIONS
            .iter()
            .filter(|v| **v < *self)
            .max()
            .copied()
    }

    /// Check if this version is supported
    pub fn is_supported(&self) -> bool {
        SUPPORTED_RUBY_VERSIONS.contains(self)
    }

    /// Get the directory name for this version (e.g., "rubystubs27", "rubystubs30")
    pub fn to_directory_name(&self) -> String {
        format!("rubystubs{}{}", self.major, self.minor)
    }
}

impl Display for MinorVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for MinorVersion {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_full_version(s)
    }
}

impl PartialOrd for MinorVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinorVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => self.minor.cmp(&other.minor),
            other => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VersionParseError {
    InvalidFormat(String),
    InvalidMajor(String),
    InvalidMinor(String),
}

impl Display for VersionParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            VersionParseError::InvalidFormat(version) => {
                write!(f, "Invalid version format: {}", version)
            }
            VersionParseError::InvalidMajor(major) => {
                write!(f, "Invalid major version: {}", major)
            }
            VersionParseError::InvalidMinor(minor) => {
                write!(f, "Invalid minor version: {}", minor)
            }
        }
    }
}

impl std::error::Error for VersionParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_version() {
        assert_eq!(
            MinorVersion::from_full_version("2.7.6").unwrap(),
            MinorVersion { major: 2, minor: 7 }
        );
        
        assert_eq!(
            MinorVersion::from_full_version("3.0.0").unwrap(),
            MinorVersion { major: 3, minor: 0 }
        );
        
        assert_eq!(
            MinorVersion::from_full_version("1.9.3").unwrap(),
            MinorVersion { major: 1, minor: 9 }
        );
    }

    #[test]
    fn test_parse_invalid_version() {
        assert!(MinorVersion::from_full_version("invalid").is_err());
        assert!(MinorVersion::from_full_version("2").is_err());
        assert!(MinorVersion::from_full_version("2.x.1").is_err());
    }

    #[test]
    fn test_find_closest_supported() {
        // Exact match
        let version = MinorVersion { major: 2, minor: 7 };
        assert_eq!(version.find_closest_supported(), Some(version));

        // Fallback to lower version
        let version = MinorVersion { major: 2, minor: 8 }; // Not supported
        assert_eq!(
            version.find_closest_supported(),
            Some(MinorVersion { major: 2, minor: 7 })
        );

        // No fallback available (too low)
        let version = MinorVersion { major: 1, minor: 8 };
        assert_eq!(version.find_closest_supported(), None);

        // Future version falls back to latest
        let version = MinorVersion { major: 4, minor: 0 };
        assert_eq!(
            version.find_closest_supported(),
            Some(MinorVersion { major: 3, minor: 4 })
        );
    }

    #[test]
    fn test_is_supported() {
        assert!(MinorVersion { major: 2, minor: 7 }.is_supported());
        assert!(MinorVersion { major: 3, minor: 0 }.is_supported());
        assert!(!MinorVersion { major: 2, minor: 8 }.is_supported());
        assert!(!MinorVersion { major: 4, minor: 0 }.is_supported());
    }

    #[test]
    fn test_version_ordering() {
        let v1_9 = MinorVersion { major: 1, minor: 9 };
        let v2_0 = MinorVersion { major: 2, minor: 0 };
        let v2_7 = MinorVersion { major: 2, minor: 7 };
        let v3_0 = MinorVersion { major: 3, minor: 0 };

        assert!(v1_9 < v2_0);
        assert!(v2_0 < v2_7);
        assert!(v2_7 < v3_0);
        assert!(v3_0 > v2_7);
    }

    #[test]
    fn test_to_directory_name() {
        assert_eq!(
            MinorVersion { major: 2, minor: 7 }.to_directory_name(),
            "rubystubs27"
        );
        assert_eq!(
            MinorVersion { major: 3, minor: 0 }.to_directory_name(),
            "rubystubs30"
        );
    }
}