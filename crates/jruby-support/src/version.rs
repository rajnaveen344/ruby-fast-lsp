use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JrubyVersion {
    components: Vec<u16>,
}

impl JrubyVersion {
    pub fn parse(source: &str) -> Result<Self, VersionError> {
        let source = source.strip_prefix("jruby-").unwrap_or(source);
        let components = source
            .split('.')
            .map(|component| {
                if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(VersionError::InvalidEngineVersion(source.to_string()));
                }
                component
                    .parse::<u16>()
                    .map_err(|_| VersionError::InvalidEngineVersion(source.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if components.len() < 2 {
            return Err(VersionError::InvalidEngineVersion(source.to_string()));
        }
        Ok(Self { components })
    }

    pub fn major(&self) -> u16 {
        self.components[0]
    }

    pub fn minor(&self) -> u16 {
        self.components[1]
    }

    pub fn components(&self) -> &[u16] {
        &self.components
    }
}

impl fmt::Display for JrubyVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, component) in self.components.iter().enumerate() {
            if index > 0 {
                formatter.write_str(".")?;
            }
            write!(formatter, "{component}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RubyCompatibilityVersion {
    pub major: u16,
    pub minor: u16,
}

impl fmt::Display for RubyCompatibilityVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JrubySeries {
    V9_0,
    V9_1,
    V9_2,
    V9_3,
    V9_4,
    V10_0,
    V10_1,
}

impl JrubySeries {
    pub const SUPPORTED: [Self; 7] = [
        Self::V9_0,
        Self::V9_1,
        Self::V9_2,
        Self::V9_3,
        Self::V9_4,
        Self::V10_0,
        Self::V10_1,
    ];

    pub fn for_engine(version: &JrubyVersion) -> Result<Self, VersionError> {
        Self::for_family(version.major(), version.minor())
    }

    pub fn for_family(major: u16, minor: u16) -> Result<Self, VersionError> {
        match (major, minor) {
            (9, 0) => Ok(Self::V9_0),
            (9, 1) => Ok(Self::V9_1),
            (9, 2) => Ok(Self::V9_2),
            (9, 3) => Ok(Self::V9_3),
            (9, 4) => Ok(Self::V9_4),
            (10, 0) => Ok(Self::V10_0),
            (10, 1) => Ok(Self::V10_1),
            (major, minor) => Err(VersionError::UnsupportedSeries { major, minor }),
        }
    }

    pub fn engine_family(self) -> (u16, u16) {
        match self {
            Self::V9_0 => (9, 0),
            Self::V9_1 => (9, 1),
            Self::V9_2 => (9, 2),
            Self::V9_3 => (9, 3),
            Self::V9_4 => (9, 4),
            Self::V10_0 => (10, 0),
            Self::V10_1 => (10, 1),
        }
    }

    pub fn ruby_compatibility(self) -> RubyCompatibilityVersion {
        let (major, minor) = match self {
            Self::V9_0 => (2, 2),
            Self::V9_1 => (2, 3),
            Self::V9_2 => (2, 5),
            Self::V9_3 => (2, 6),
            Self::V9_4 => (3, 1),
            Self::V10_0 => (3, 4),
            Self::V10_1 => (4, 0),
        };
        RubyCompatibilityVersion { major, minor }
    }

    pub fn overlay_name(self) -> &'static str {
        match self {
            Self::V9_0 => "9.0",
            Self::V9_1 => "9.1",
            Self::V9_2 => "9.2",
            Self::V9_3 => "9.3",
            Self::V9_4 => "9.4",
            Self::V10_0 => "10.0",
            Self::V10_1 => "10.1",
        }
    }

    pub fn label(self) -> String {
        let (major, minor) = self.engine_family();
        format!("JRuby {major}.{minor} (Ruby {})", self.ruby_compatibility())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JrubyRuntimeIdentity {
    pub engine_version: JrubyVersion,
    pub series: JrubySeries,
    pub ruby_compatibility: RubyCompatibilityVersion,
}

impl JrubyRuntimeIdentity {
    pub fn from_identifier(identifier: &str) -> Result<Self, VersionError> {
        let engine_version = JrubyVersion::parse(identifier)?;
        let series = JrubySeries::for_engine(&engine_version)?;
        Ok(Self {
            engine_version,
            series,
            ruby_compatibility: series.ruby_compatibility(),
        })
    }

    pub fn from_version_output(output: &str) -> Result<Self, VersionError> {
        let mut words = output.split_whitespace();
        if words.next() != Some("jruby") {
            return Err(VersionError::NotJruby);
        }
        let engine = words.next().ok_or_else(|| {
            VersionError::InvalidEngineVersion("missing JRuby engine version".to_string())
        })?;
        let identity = Self::from_identifier(engine)?;
        if let Some(open) = output.find('(') {
            let close = output[open + 1..]
                .find(')')
                .map(|offset| open + 1 + offset)
                .ok_or(VersionError::InvalidCompatibilityVersion)?;
            let reported = parse_compatibility(&output[open + 1..close])?;
            if reported != identity.ruby_compatibility {
                return Err(VersionError::CompatibilityMismatch {
                    engine: identity.engine_version,
                    expected: identity.ruby_compatibility,
                    reported,
                });
            }
        }
        Ok(identity)
    }
}

fn parse_compatibility(source: &str) -> Result<RubyCompatibilityVersion, VersionError> {
    let mut components = source.split('.');
    let major = components
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(VersionError::InvalidCompatibilityVersion)?;
    let minor = components
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(VersionError::InvalidCompatibilityVersion)?;
    Ok(RubyCompatibilityVersion { major, minor })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    NotJruby,
    InvalidEngineVersion(String),
    InvalidCompatibilityVersion,
    UnsupportedSeries {
        major: u16,
        minor: u16,
    },
    CompatibilityMismatch {
        engine: JrubyVersion,
        expected: RubyCompatibilityVersion,
        reported: RubyCompatibilityVersion,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_supported_series_without_nearest_version_fallback() {
        let expected = [
            ("jruby-9.0.5.0", JrubySeries::V9_0, (2, 2)),
            ("jruby-9.1.17.0", JrubySeries::V9_1, (2, 3)),
            ("jruby-9.2.21.0", JrubySeries::V9_2, (2, 5)),
            ("jruby-9.3.15.0", JrubySeries::V9_3, (2, 6)),
            ("jruby-9.4.14.0", JrubySeries::V9_4, (3, 1)),
            ("jruby-10.0.6.0", JrubySeries::V10_0, (3, 4)),
            ("jruby-10.1.0.0", JrubySeries::V10_1, (4, 0)),
        ];
        for (identifier, series, compatibility) in expected {
            let identity = JrubyRuntimeIdentity::from_identifier(identifier)
                .expect("supported JRuby identifier must parse");
            assert_eq!(identity.series, series);
            assert_eq!(
                identity.ruby_compatibility,
                RubyCompatibilityVersion {
                    major: compatibility.0,
                    minor: compatibility.1,
                }
            );
        }
        assert_eq!(
            JrubyRuntimeIdentity::from_identifier("jruby-10.2.0.0"),
            Err(VersionError::UnsupportedSeries {
                major: 10,
                minor: 2
            })
        );
    }

    #[test]
    fn validates_reported_compatibility_against_the_engine_series() {
        let identity = JrubyRuntimeIdentity::from_version_output(
            "jruby 9.4.14.0 (3.1.4) OpenJDK 64-Bit Server VM 17",
        )
        .expect("matching JRuby version output must parse");
        assert_eq!(identity.series, JrubySeries::V9_4);
        assert!(matches!(
            JrubyRuntimeIdentity::from_version_output(
                "jruby 9.4.14.0 (3.2.0) OpenJDK 64-Bit Server VM 17"
            ),
            Err(VersionError::CompatibilityMismatch { .. })
        ));
    }

    #[test]
    fn produces_editor_neutral_family_labels() {
        assert_eq!(
            JrubySeries::V9_4.label(),
            "JRuby 9.4 (Ruby 3.1)".to_string()
        );
        assert_eq!(
            JrubySeries::V10_1.label(),
            "JRuby 10.1 (Ruby 4.0)".to_string()
        );
    }
}
