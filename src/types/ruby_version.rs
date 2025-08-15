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

    /// Parse a full Ruby version output string and detect implementation
    pub fn parse_from_version_output(version_output: &str) -> Option<Self> {
        // Detect implementation from version output
        let implementation = if version_output.contains("jruby") {
            RubyImplementation::JRuby
        } else if version_output.contains("truffleruby") {
            RubyImplementation::TruffleRuby
        } else {
            RubyImplementation::Mri
        };

        match implementation {
            RubyImplementation::Mri => {
                // Standard MRI format: "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]"
                for word in version_output.split_whitespace() {
                    if let Some(version) = Self::parse(word) {
                        return Some(Self::new_with_implementation(
                            version.major,
                            version.minor,
                            implementation,
                        ));
                    }
                }
            }
            RubyImplementation::JRuby => {
                // JRuby format: "jruby 9.4.5.0 (3.1.4) 2023-11-02 1abae2700f OpenJDK 64-Bit Server VM 17.0.2+8 on 17.0.2+8 +jit [x86_64-darwin]"
                // The MRI compatibility version is in parentheses
                if let Some(start) = version_output.find('(') {
                    if let Some(end) = version_output[start..].find(')') {
                        let mri_version = &version_output[start + 1..start + end];
                        if let Some(version) = Self::parse(mri_version) {
                            return Some(Self::new_with_implementation(
                                version.major,
                                version.minor,
                                implementation,
                            ));
                        }
                    }
                }
                // Fallback: try to map JRuby version to MRI version
                for word in version_output.split_whitespace() {
                    if let Some(jruby_version) = Self::parse(word) {
                        if let Some(mri_version) =
                            Self::map_jruby_to_mri(jruby_version.major, jruby_version.minor)
                        {
                            return Some(Self::new_with_implementation(
                                mri_version.0,
                                mri_version.1,
                                implementation,
                            ));
                        }
                    }
                }
            }
            RubyImplementation::TruffleRuby => {
                // TruffleRuby format: "truffleruby 23.1.1, like ruby 3.2.2, GraalVM CE Native [x86_64-darwin]"
                // Look for "like ruby X.Y.Z" pattern
                if let Some(like_pos) = version_output.find("like ruby ") {
                    let after_like = &version_output[like_pos + 10..];
                    for word in after_like.split_whitespace() {
                        if let Some(version) = Self::parse(word) {
                            return Some(Self::new_with_implementation(
                                version.major,
                                version.minor,
                                implementation,
                            ));
                        }
                    }
                }
                // Fallback: try to map TruffleRuby version to MRI version
                for word in version_output.split_whitespace() {
                    if let Some(truffle_version) = Self::parse(word) {
                        if let Some(mri_version) = Self::map_truffleruby_to_mri(
                            truffle_version.major,
                            truffle_version.minor,
                        ) {
                            return Some(Self::new_with_implementation(
                                mri_version.0,
                                mri_version.1,
                                implementation,
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    /// Map JRuby version to compatible MRI version
    fn map_jruby_to_mri(jruby_major: u8, jruby_minor: u8) -> Option<(u8, u8)> {
        match (jruby_major, jruby_minor) {
            // JRuby 10.x -> Ruby 3.4
            (10, _) => Some((3, 4)),
            // JRuby 9.4.x -> Ruby 3.1
            (9, 4) => Some((3, 1)),
            // JRuby 9.3.x -> Ruby 2.6
            (9, 3) => Some((2, 6)),
            // JRuby 9.2.x -> Ruby 2.5
            (9, 2) => Some((2, 5)),
            // JRuby 9.1.x -> Ruby 2.3
            (9, 1) => Some((2, 3)),
            // JRuby 9.0.x -> Ruby 2.2
            (9, 0) => Some((2, 2)),
            // JRuby 1.7.x -> Ruby 1.9
            (1, 7) => Some((1, 9)),
            _ => None,
        }
    }

    /// Map TruffleRuby version to compatible MRI version
    fn map_truffleruby_to_mri(truffle_major: u8, _truffle_minor: u8) -> Option<(u8, u8)> {
        match truffle_major {
            // TruffleRuby 23.x -> Ruby 3.2
            23 => Some((3, 2)),
            // TruffleRuby 22.x -> Ruby 3.1
            22 => Some((3, 1)),
            // TruffleRuby 21.x -> Ruby 3.0
            21 => Some((3, 0)),
            // TruffleRuby 20.x -> Ruby 2.7
            20 => Some((2, 7)),
            // TruffleRuby 19.x -> Ruby 2.6
            19 => Some((2, 6)),
            _ => None,
        }
    }

    /// Get the MRI-compatible version for core stubs selection
    pub fn get_mri_compatible_version(&self) -> (u8, u8) {
        match self.implementation {
            RubyImplementation::Mri => (self.major, self.minor),
            RubyImplementation::JRuby | RubyImplementation::TruffleRuby => {
                // For JRuby and TruffleRuby, the version should already be mapped to MRI-compatible
                (self.major, self.minor)
            }
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
    fn test_jruby_version_parsing() {
        let jruby_output = "jruby 9.4.5.0 (3.1.4) 2023-11-02 1abae2700f OpenJDK 64-Bit Server VM 17.0.2+8 on 17.0.2+8 +jit [x86_64-darwin]";
        let version = RubyVersion::parse_from_version_output(jruby_output);
        assert_eq!(
            version,
            Some(RubyVersion::new_with_implementation(
                3,
                1,
                RubyImplementation::JRuby
            ))
        );
    }

    #[test]
    fn test_truffleruby_version_parsing() {
        let truffle_output =
            "truffleruby 23.1.1, like ruby 3.2.2, GraalVM CE Native [x86_64-darwin]";
        let version = RubyVersion::parse_from_version_output(truffle_output);
        assert_eq!(
            version,
            Some(RubyVersion::new_with_implementation(
                3,
                2,
                RubyImplementation::TruffleRuby
            ))
        );
    }

    #[test]
    fn test_mri_version_parsing() {
        let mri_output = "ruby 3.0.0p0 (2020-12-25 revision 95aff21468) [x86_64-darwin20]";
        let version = RubyVersion::parse_from_version_output(mri_output);
        assert_eq!(
            version,
            Some(RubyVersion::new_with_implementation(
                3,
                0,
                RubyImplementation::Mri
            ))
        );
    }

    #[test]
    fn test_jruby_version_mapping() {
        assert_eq!(RubyVersion::map_jruby_to_mri(10, 0), Some((3, 4)));
        assert_eq!(RubyVersion::map_jruby_to_mri(9, 4), Some((3, 1)));
        assert_eq!(RubyVersion::map_jruby_to_mri(9, 3), Some((2, 6)));
        assert_eq!(RubyVersion::map_jruby_to_mri(9, 2), Some((2, 5)));
        assert_eq!(RubyVersion::map_jruby_to_mri(9, 1), Some((2, 3)));
        assert_eq!(RubyVersion::map_jruby_to_mri(9, 0), Some((2, 2)));
        assert_eq!(RubyVersion::map_jruby_to_mri(1, 7), Some((1, 9)));
    }

    #[test]
    fn test_truffleruby_version_mapping() {
        assert_eq!(RubyVersion::map_truffleruby_to_mri(23, 1), Some((3, 2)));
        assert_eq!(RubyVersion::map_truffleruby_to_mri(22, 0), Some((3, 1)));
        assert_eq!(RubyVersion::map_truffleruby_to_mri(21, 3), Some((3, 0)));
        assert_eq!(RubyVersion::map_truffleruby_to_mri(20, 1), Some((2, 7)));
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
