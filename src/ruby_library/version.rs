use std::cmp::Ordering;
use serde::{Deserialize, Serialize};

/// Represents a Ruby minor version (e.g., 3.0, 3.1, 3.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RubyVersion {
    pub major: u8,
    pub minor: u8,
}

impl RubyVersion {
    pub fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    /// Parse a version string like "3.0.0" or "3.1" into a RubyVersion
    pub fn parse(version_str: &str) -> Option<Self> {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() >= 2 {
            if let (Ok(major), Ok(minor)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
                return Some(Self::new(major, minor));
            }
        }
        None
    }

    /// Convert to tuple for easier handling
    pub fn to_tuple(self) -> (u8, u8) {
        (self.major, self.minor)
    }

    /// Convert from tuple
    pub fn from_tuple(tuple: (u8, u8)) -> Self {
        Self::new(tuple.0, tuple.1)
    }

    /// Parse a full version string like "3.0.0" into a RubyVersion (ignoring patch)
    pub fn from_full_version(version_str: &str) -> Option<Self> {
        Self::parse(version_str)
    }
}

impl PartialOrd for RubyVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RubyVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => self.minor.cmp(&other.minor),
            other => other,
        }
    }
}

impl std::fmt::Display for RubyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        assert_eq!(RubyVersion::parse("3.0.0"), Some(RubyVersion::new(3, 0)));
        assert_eq!(RubyVersion::parse("3.1"), Some(RubyVersion::new(3, 1)));
        assert_eq!(RubyVersion::parse("2.7.4"), Some(RubyVersion::new(2, 7)));
        assert_eq!(RubyVersion::parse("invalid"), None);
    }

    #[test]
    fn test_version_comparison() {
        let v30 = RubyVersion::new(3, 0);
        let v31 = RubyVersion::new(3, 1);
        let v27 = RubyVersion::new(2, 7);

        assert!(v31 > v30);
        assert!(v30 > v27);
        assert!(v27 < v30);
    }

    #[test]
    fn test_version_display() {
        let version = RubyVersion::new(3, 1);
        assert_eq!(format!("{}", version), "3.1");
    }
}