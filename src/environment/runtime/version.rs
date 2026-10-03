//! Ruby compatibility version identity. This is the one parser for a
//! `major.minor[.patch]` family string, used by configuration validation,
//! legacy compatibility selection, and core stub selection. Runtime discovery
//! and `.ruby-version`/`.tool-versions` markers live in `catalog`.

use super::catalog::RuntimeImplementation;
use std::cmp::Ordering;

/// Parse the `major.minor` family of a version string. Components after the
/// minor version are ignored; anything that is not numeric is rejected.
pub fn parse_ruby_family(source: &str) -> Option<(u16, u16)> {
    let mut components = source.split('.');
    let major = components.next()?.parse().ok()?;
    let minor = components.next()?.parse().ok()?;
    Some((major, minor))
}

/// A Ruby compatibility minor version (e.g. 3.0, 3.1) and the implementation
/// that provides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RubyVersion {
    pub major: u8,
    pub minor: u8,
    pub implementation: RuntimeImplementation,
}

impl RubyVersion {
    pub fn new(major: u8, minor: u8) -> Self {
        Self::new_with_implementation(major, minor, RuntimeImplementation::Mri)
    }

    pub fn new_with_implementation(
        major: u8,
        minor: u8,
        implementation: RuntimeImplementation,
    ) -> Self {
        Self {
            major,
            minor,
            implementation,
        }
    }

    /// The version for a parsed family. A family outside the bundled stub
    /// range has no version, so callers keep the bundled fallback rather
    /// than a truncated one.
    pub fn from_family(
        (major, minor): (u16, u16),
        implementation: RuntimeImplementation,
    ) -> Option<Self> {
        Some(Self::new_with_implementation(
            u8::try_from(major).ok()?,
            u8::try_from(minor).ok()?,
            implementation,
        ))
    }

    /// Parse a family string such as `3.1` or `3.1.4`.
    pub fn parse(source: &str, implementation: RuntimeImplementation) -> Option<Self> {
        Self::from_family(parse_ruby_family(source)?, implementation)
    }

    pub fn to_tuple(self) -> (u8, u8) {
        (self.major, self.minor)
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
    fn version_comparison() {
        let v30 = RubyVersion::new(3, 0);
        let v31 = RubyVersion::new(3, 1);
        let v27 = RubyVersion::new(2, 7);

        assert!(v31 > v30);
        assert!(v30 > v27);
        assert!(v27 < v30);
    }

    #[test]
    fn version_display() {
        assert_eq!(RubyVersion::new(3, 1).to_string(), "3.1");
    }

    #[test]
    fn family_parsing() {
        let cases = [
            ("3.0", Some((3, 0))),
            ("3.1", Some((3, 1))),
            ("2.7", Some((2, 7))),
            ("1.9", Some((1, 9))),
            ("auto", None),
            ("invalid", None),
            ("3", None),
            ("3.0.1", Some((3, 0))),
            ("300.0", Some((300, 0))),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_ruby_family(input), expected, "input: {input}");
        }
    }

    #[test]
    fn families_outside_the_stub_range_have_no_version() {
        assert_eq!(
            RubyVersion::parse("3.3.1", RuntimeImplementation::Jruby),
            Some(RubyVersion::new_with_implementation(
                3,
                3,
                RuntimeImplementation::Jruby
            ))
        );
        assert_eq!(
            RubyVersion::parse("300.0", RuntimeImplementation::Mri),
            None
        );
    }
}
