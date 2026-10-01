use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Represents different Ruby implementations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RubyImplementation {
    /// MRI (Matz's Ruby Interpreter) - the reference implementation
    Mri,
    /// JRuby - Ruby on the JVM
    JRuby,
    /// TruffleRuby - Ruby on GraalVM
    TruffleRuby,
}

/// Represents a Ruby minor version (e.g., 3.0, 3.1, 3.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RubyVersion {
    pub major: u8,
    pub minor: u8,
    pub implementation: RubyImplementation,
}

impl RubyVersion {
    pub fn new(major: u8, minor: u8) -> Self {
        Self {
            major,
            minor,
            implementation: RubyImplementation::Mri,
        }
    }

    pub fn new_with_implementation(
        major: u8,
        minor: u8,
        implementation: RubyImplementation,
    ) -> Self {
        Self {
            major,
            minor,
            implementation,
        }
    }

    /// Convert to tuple for easier handling
    pub fn to_tuple(self) -> (u8, u8) {
        (self.major, self.minor)
    }

    /// Convert from tuple
    pub fn from_tuple(tuple: (u8, u8)) -> Self {
        Self::new(tuple.0, tuple.1)
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
