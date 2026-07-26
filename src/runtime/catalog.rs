use futures::{stream, StreamExt};
use ruby_fast_lsp_jruby_support::{
    JrubySeries, JrubyVersion, RubyCompatibilityVersion, VersionError,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

const MAX_RUNTIME_CANDIDATES: usize = 256;
const MAX_VERSION_OUTPUT_BYTES: u64 = 16 * 1024;
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);
const DISCOVERY_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeImplementation {
    Mri,
    Jruby,
    Truffleruby,
}

impl RuntimeImplementation {
    fn label(self) -> &'static str {
        match self {
            Self::Mri => "MRI",
            Self::Jruby => "JRuby",
            Self::Truffleruby => "TruffleRuby",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSupportStatus {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeDiscoverySource {
    Path,
    Rvm,
    Rbenv,
    Asdf,
    Mise,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredRuntime {
    pub implementation: RuntimeImplementation,
    pub implementation_label: String,
    pub family: String,
    pub family_label: String,
    pub compatibility_version: String,
    pub compatibility_label: String,
    pub engine_version: String,
    pub display_name: String,
    pub executable: PathBuf,
    pub discovery_source: RuntimeDiscoverySource,
    pub support_status: RuntimeSupportStatus,
    pub java_home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeImplementationOption {
    pub id: RuntimeImplementation,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeCatalog {
    pub root: PathBuf,
    pub label: String,
    pub implementations: Vec<RuntimeImplementationOption>,
    pub runtimes: Vec<DiscoveredRuntime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCatalog {
    pub projects: Vec<ProjectRuntimeCatalog>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeStatus {
    pub root: PathBuf,
    pub mode: String,
    pub implementation: Option<RuntimeImplementation>,
    pub family: Option<String>,
    pub engine_version: Option<String>,
    pub compatibility_version: Option<String>,
    pub executable: Option<PathBuf>,
    pub java_home: Option<PathBuf>,
    pub stub_overlay: Option<String>,
    pub classpath_fingerprint_sha256: Option<String>,
    pub indexing_complete: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub projects: Vec<ProjectRuntimeStatus>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RuntimeDiscoverParams {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusParams {
    pub project_root: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RuntimeMarkerError {
    Invalid(String),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Ambiguous {
        marker: String,
        executables: Vec<PathBuf>,
    },
}

impl std::fmt::Display for RuntimeMarkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(marker) => write!(formatter, "runtime marker `{marker}` is invalid"),
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "failed to read runtime marker `{}`: {source}",
                    path.display()
                )
            }
            Self::Ambiguous {
                marker,
                executables,
            } => write!(
                formatter,
                "runtime marker `{marker}` matches multiple installed runtimes: {executables:?}"
            ),
        }
    }
}

impl std::error::Error for RuntimeMarkerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Invalid(_) | Self::Ambiguous { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeMarkerIdentity {
    implementation: RuntimeImplementation,
    engine_version: String,
}

pub fn project_runtime_marker(project_root: &Path) -> Result<Option<String>, RuntimeMarkerError> {
    let ruby_version = project_root.join(".ruby-version");
    if ruby_version.exists() {
        let value =
            std::fs::read_to_string(&ruby_version).map_err(|source| RuntimeMarkerError::Read {
                path: ruby_version,
                source,
            })?;
        let value = value.trim();
        return if value.is_empty() {
            Err(RuntimeMarkerError::Invalid(value.to_string()))
        } else {
            Ok(Some(value.to_string()))
        };
    }

    let tool_versions = project_root.join(".tool-versions");
    if !tool_versions.exists() {
        return Ok(None);
    }
    let contents =
        std::fs::read_to_string(&tool_versions).map_err(|source| RuntimeMarkerError::Read {
            path: tool_versions,
            source,
        })?;
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some("ruby") {
            let value = fields
                .next()
                .ok_or_else(|| RuntimeMarkerError::Invalid(line.to_string()))?;
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

pub fn select_runtime_for_marker(
    marker: &str,
    runtimes: &[DiscoveredRuntime],
) -> Result<Option<DiscoveredRuntime>, RuntimeMarkerError> {
    if marker.trim() == "system" {
        return Ok(None);
    }
    let identity = parse_runtime_marker(marker)?;
    let mut matches = runtimes
        .iter()
        .filter(|runtime| {
            runtime.implementation == identity.implementation
                && runtime.engine_version == identity.engine_version
        })
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.executable.cmp(&right.executable));
    let path_matches = matches
        .iter()
        .filter(|runtime| runtime.discovery_source == RuntimeDiscoverySource::Path)
        .cloned()
        .collect::<Vec<_>>();
    match (path_matches.as_slice(), matches.as_slice()) {
        ([runtime], _) => Ok(Some(runtime.clone())),
        ([], []) => Ok(None),
        ([], [runtime]) => Ok(Some(runtime.clone())),
        (_, _) => Err(RuntimeMarkerError::Ambiguous {
            marker: marker.to_string(),
            executables: if path_matches.is_empty() {
                matches
                    .into_iter()
                    .map(|runtime| runtime.executable)
                    .collect()
            } else {
                path_matches
                    .into_iter()
                    .map(|runtime| runtime.executable)
                    .collect()
            },
        }),
    }
}

fn parse_runtime_marker(marker: &str) -> Result<RuntimeMarkerIdentity, RuntimeMarkerError> {
    let marker = marker.trim();
    let (implementation, version) = if let Some(version) = marker.strip_prefix("jruby-") {
        (RuntimeImplementation::Jruby, version)
    } else if let Some(version) = marker.strip_prefix("truffleruby-") {
        (RuntimeImplementation::Truffleruby, version)
    } else if let Some(version) = marker.strip_prefix("ruby-") {
        (RuntimeImplementation::Mri, version)
    } else if marker.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        (RuntimeImplementation::Mri, marker)
    } else {
        return Err(RuntimeMarkerError::Invalid(marker.to_string()));
    };
    if clean_version(version).as_deref() != Some(version) || version_family(version).is_none() {
        return Err(RuntimeMarkerError::Invalid(marker.to_string()));
    }
    Ok(RuntimeMarkerIdentity {
        implementation,
        engine_version: version.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeCandidate {
    executable: PathBuf,
    source: RuntimeDiscoverySource,
}

pub async fn discover_runtime_catalog(project_roots: Vec<PathBuf>) -> RuntimeCatalog {
    runtime_catalog_for_projects(project_roots, discover_runtimes().await)
}

pub async fn discover_runtimes() -> Vec<DiscoveredRuntime> {
    let candidates = discover_candidates();
    let java_homes = std::sync::Arc::new(discover_java_homes());
    let mut runtimes = stream::iter(candidates)
        .map(|candidate| {
            let java_homes = java_homes.clone();
            async move {
                if let Some(output) = bounded_version_output(&candidate.executable, None).await {
                    if let Some(runtime) = parse_runtime_output(
                        &output,
                        candidate.executable.clone(),
                        candidate.source,
                    ) {
                        if runtime.implementation != RuntimeImplementation::Jruby
                            || java_homes.is_empty()
                        {
                            return Some(runtime);
                        }
                    }
                }
                for java_home in java_homes.iter() {
                    let Some(output) =
                        bounded_version_output(&candidate.executable, Some(java_home)).await
                    else {
                        continue;
                    };
                    let Some(mut runtime) = parse_runtime_output(
                        &output,
                        candidate.executable.clone(),
                        candidate.source,
                    ) else {
                        continue;
                    };
                    if runtime.implementation != RuntimeImplementation::Jruby {
                        return Some(runtime);
                    }
                    runtime.java_home = Some(java_home.clone());
                    return Some(runtime);
                }
                None
            }
        })
        .buffer_unordered(DISCOVERY_CONCURRENCY)
        .filter_map(async move |runtime| runtime)
        .collect::<Vec<_>>()
        .await;
    runtimes.sort_by(|left, right| {
        left.implementation
            .cmp(&right.implementation)
            .then_with(|| left.family.cmp(&right.family))
            .then_with(|| left.engine_version.cmp(&right.engine_version))
            .then_with(|| left.executable.cmp(&right.executable))
    });
    runtimes.dedup_by(|left, right| left.executable == right.executable);
    runtimes
}

pub fn runtime_catalog_for_projects(
    project_roots: Vec<PathBuf>,
    runtimes: Vec<DiscoveredRuntime>,
) -> RuntimeCatalog {
    let implementations = [
        RuntimeImplementation::Mri,
        RuntimeImplementation::Jruby,
        RuntimeImplementation::Truffleruby,
    ]
    .into_iter()
    .map(|implementation| RuntimeImplementationOption {
        id: implementation,
        label: implementation.label().to_string(),
    })
    .collect::<Vec<_>>();
    let mut roots = project_roots
        .into_iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    RuntimeCatalog {
        projects: roots
            .into_iter()
            .map(|root| ProjectRuntimeCatalog {
                label: root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Ruby project")
                    .to_string(),
                root,
                implementations: implementations.clone(),
                runtimes: runtimes.clone(),
            })
            .collect(),
    }
}

fn discover_candidates() -> Vec<RuntimeCandidate> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            for executable in ["ruby", "jruby", "truffleruby"] {
                add_candidate(
                    &mut candidates,
                    directory.join(executable),
                    RuntimeDiscoverySource::Path,
                );
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for (root, source) in [
            (home.join(".rvm/rubies"), RuntimeDiscoverySource::Rvm),
            (home.join(".rbenv/versions"), RuntimeDiscoverySource::Rbenv),
            (
                home.join(".asdf/installs/ruby"),
                RuntimeDiscoverySource::Asdf,
            ),
            (
                home.join(".local/share/mise/installs/ruby"),
                RuntimeDiscoverySource::Mise,
            ),
        ] {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let mut directories = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            directories.sort();
            for directory in directories {
                add_candidate(&mut candidates, directory.join("bin/ruby"), source);
                add_candidate(&mut candidates, directory.join("bin/jruby"), source);
            }
        }
    }
    candidates.sort_by(|left, right| left.executable.cmp(&right.executable));
    candidates.dedup_by(|left, right| left.executable == right.executable);
    candidates.truncate(MAX_RUNTIME_CANDIDATES);
    candidates
}

fn discover_java_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    if let Some(java_home) = std::env::var_os("JAVA_HOME").map(PathBuf::from) {
        add_java_home(&mut homes, java_home);
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for root in [
            home.join(".sdkman/candidates/java"),
            home.join(".asdf/installs/java"),
            home.join(".local/share/mise/installs/java"),
            home.join("Library/Java/JavaVirtualMachines"),
        ] {
            add_java_homes_below(&mut homes, &root);
        }
    }
    for root in [
        PathBuf::from("/Library/Java/JavaVirtualMachines"),
        PathBuf::from("/opt/homebrew/opt"),
        PathBuf::from("/usr/local/opt"),
    ] {
        add_java_homes_below(&mut homes, &root);
    }
    homes.dedup();
    homes.truncate(64);
    homes
}

fn add_java_homes_below(homes: &mut Vec<PathBuf>, root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        for candidate in [
            path.clone(),
            path.join("Contents/Home"),
            path.join("libexec/openjdk.jdk/Contents/Home"),
        ] {
            add_java_home(homes, candidate);
        }
    }
}

fn add_java_home(homes: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidate.join("bin/java").is_file() || !candidate.join("release").is_file() {
        return;
    }
    let Ok(candidate) = std::fs::canonicalize(candidate) else {
        return;
    };
    if !homes.contains(&candidate) {
        homes.push(candidate);
    }
}

fn add_candidate(
    candidates: &mut Vec<RuntimeCandidate>,
    executable: PathBuf,
    source: RuntimeDiscoverySource,
) {
    if !executable.is_file() {
        return;
    }
    let Ok(executable) = std::fs::canonicalize(executable) else {
        return;
    };
    candidates.push(RuntimeCandidate { executable, source });
}

async fn bounded_version_output(executable: &Path, java_home: Option<&Path>) -> Option<String> {
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(java_home) = java_home {
        command.env("JAVA_HOME", java_home);
        let java_bin = java_home.join("bin");
        let path = std::env::var_os("PATH")
            .map(|path| {
                let mut entries = vec![java_bin];
                entries.extend(std::env::split_paths(&path));
                std::env::join_paths(entries).ok()
            })
            .flatten()
            .unwrap_or_else(|| java_home.join("bin").into_os_string());
        command.env("PATH", path);
    }
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let operation = async {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_VERSION_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        let status = child.wait().await.ok()?;
        if !status.success() || bytes.len() as u64 > MAX_VERSION_OUTPUT_BYTES {
            return None;
        }
        String::from_utf8(bytes).ok()
    };
    tokio::time::timeout(VERSION_TIMEOUT, operation)
        .await
        .ok()
        .flatten()
}

fn parse_runtime_output(
    output: &str,
    executable: PathBuf,
    discovery_source: RuntimeDiscoverySource,
) -> Option<DiscoveredRuntime> {
    let trimmed = output.trim();
    if trimmed.starts_with("jruby ") {
        return parse_jruby_output(trimmed, executable, discovery_source);
    }
    if trimmed.starts_with("truffleruby ") {
        return parse_truffleruby_output(trimmed, executable, discovery_source);
    }
    if trimmed.starts_with("ruby ") {
        return parse_mri_output(trimmed, executable, discovery_source);
    }
    None
}

fn parse_jruby_output(
    output: &str,
    executable: PathBuf,
    discovery_source: RuntimeDiscoverySource,
) -> Option<DiscoveredRuntime> {
    let engine_version = JrubyVersion::parse(output.split_whitespace().nth(1)?).ok()?;
    let reported = reported_compatibility(output)?;
    let (family, family_label, expected, support_status) =
        match JrubySeries::for_engine(&engine_version) {
            Ok(series) => (
                series.overlay_name().to_string(),
                series.label(),
                series.ruby_compatibility(),
                RuntimeSupportStatus::Supported,
            ),
            Err(VersionError::UnsupportedSeries { major, minor }) => (
                format!("{major}.{minor}"),
                format!("JRuby {major}.{minor} (Ruby {reported})"),
                reported,
                RuntimeSupportStatus::Unsupported,
            ),
            Err(_) => return None,
        };
    if support_status == RuntimeSupportStatus::Supported && reported != expected {
        return None;
    }
    let engine_version = engine_version.to_string();
    Some(DiscoveredRuntime {
        implementation: RuntimeImplementation::Jruby,
        implementation_label: RuntimeImplementation::Jruby.label().to_string(),
        family,
        family_label,
        compatibility_version: expected.to_string(),
        compatibility_label: format!("Ruby {expected}"),
        display_name: format!("JRuby {engine_version} (Ruby {expected})"),
        engine_version,
        executable,
        discovery_source,
        support_status,
        java_home: None,
    })
}

fn parse_mri_output(
    output: &str,
    executable: PathBuf,
    discovery_source: RuntimeDiscoverySource,
) -> Option<DiscoveredRuntime> {
    let engine_version = clean_version(output.split_whitespace().nth(1)?)?;
    let family = version_family(&engine_version)?;
    Some(DiscoveredRuntime {
        implementation: RuntimeImplementation::Mri,
        implementation_label: RuntimeImplementation::Mri.label().to_string(),
        family: family.clone(),
        family_label: format!("MRI {family}"),
        compatibility_version: family.clone(),
        compatibility_label: format!("Ruby {family}"),
        display_name: format!("MRI {engine_version}"),
        engine_version,
        executable,
        discovery_source,
        support_status: RuntimeSupportStatus::Supported,
        java_home: None,
    })
}

fn parse_truffleruby_output(
    output: &str,
    executable: PathBuf,
    discovery_source: RuntimeDiscoverySource,
) -> Option<DiscoveredRuntime> {
    let engine_version = clean_version(output.split_whitespace().nth(1)?.trim_end_matches(','))?;
    let family = version_family(&engine_version)?;
    let compatibility = output
        .split_once("like ruby ")
        .and_then(|(_, tail)| clean_version(tail.split_whitespace().next()?))
        .and_then(|version| version_family(&version))
        .or_else(|| reported_compatibility(output).map(|version| version.to_string()))?;
    Some(DiscoveredRuntime {
        implementation: RuntimeImplementation::Truffleruby,
        implementation_label: RuntimeImplementation::Truffleruby.label().to_string(),
        family: family.clone(),
        family_label: format!("TruffleRuby {family} (Ruby {compatibility})"),
        compatibility_version: compatibility.clone(),
        compatibility_label: format!("Ruby {compatibility}"),
        display_name: format!("TruffleRuby {engine_version} (Ruby {compatibility})"),
        engine_version,
        executable,
        discovery_source,
        support_status: RuntimeSupportStatus::Supported,
        java_home: None,
    })
}

fn reported_compatibility(output: &str) -> Option<RubyCompatibilityVersion> {
    let open = output.find('(')?;
    let close = output[open + 1..].find(')')? + open + 1;
    let family = version_family(&clean_version(&output[open + 1..close])?)?;
    let (major, minor) = family.split_once('.')?;
    Some(RubyCompatibilityVersion {
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
    })
}

fn clean_version(source: &str) -> Option<String> {
    let version = source
        .trim_matches(|character: char| !character.is_ascii_digit() && character != '.')
        .split(|character: char| !character.is_ascii_digit() && character != '.')
        .next()?;
    if version.is_empty()
        || version
            .split('.')
            .any(|component| component.is_empty() || component.parse::<u16>().is_err())
    {
        return None;
    }
    Some(version.to_string())
}

fn version_family(version: &str) -> Option<String> {
    let mut components = version.split('.');
    Some(format!("{}.{}", components.next()?, components.next()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parsed(output: &str) -> DiscoveredRuntime {
        parse_runtime_output(
            output,
            PathBuf::from("/runtime/bin/ruby"),
            RuntimeDiscoverySource::Path,
        )
        .expect("runtime fixture output must parse")
    }

    #[test]
    fn produces_all_selector_levels_from_runtime_output() {
        let mri = parsed("ruby 3.3.11 (2025-01-01 revision abc) [arm64-darwin]");
        assert_eq!(mri.family_label, "MRI 3.3");
        assert_eq!(mri.engine_version, "3.3.11");

        let jruby = parsed("jruby 9.2.21.0 (2.5.8) OpenJDK 64-Bit Server VM 17.0.2 [arm64-darwin]");
        assert_eq!(jruby.family_label, "JRuby 9.2 (Ruby 2.5)");
        assert_eq!(jruby.compatibility_version, "2.5");
        assert_eq!(jruby.support_status, RuntimeSupportStatus::Supported);

        let truffle = parsed("truffleruby 24.1.2, like ruby 3.3.3, GraalVM CE Native");
        assert_eq!(truffle.family_label, "TruffleRuby 24.1 (Ruby 3.3)");
    }

    #[test]
    fn advertises_future_jruby_as_unsupported_and_rejects_mismatched_known_series() {
        let future = parsed("jruby 10.2.0.0 (4.1.0) OpenJDK 64-Bit Server VM 25 [arm64-darwin]");
        assert_eq!(future.family, "10.2");
        assert_eq!(future.support_status, RuntimeSupportStatus::Unsupported);
        assert!(parse_runtime_output(
            "jruby 9.4.14.0 (3.2.0) OpenJDK 64-Bit Server VM 17",
            PathBuf::from("/jruby"),
            RuntimeDiscoverySource::Path,
        )
        .is_none());
    }

    #[test]
    fn java_home_candidates_require_a_complete_canonical_jdk_home() {
        let fixture = tempfile::tempdir().unwrap();
        let incomplete = fixture.path().join("incomplete");
        fs::create_dir_all(incomplete.join("bin")).unwrap();
        fs::write(incomplete.join("bin/java"), b"fixture").unwrap();
        let complete = fixture.path().join("complete");
        fs::create_dir_all(complete.join("bin")).unwrap();
        fs::write(complete.join("bin/java"), b"fixture").unwrap();
        fs::write(complete.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();

        let mut homes = Vec::new();
        add_java_home(&mut homes, incomplete);
        assert!(homes.is_empty());
        add_java_home(&mut homes, complete.clone());
        add_java_home(&mut homes, complete.clone());
        assert_eq!(homes, vec![fs::canonicalize(complete).unwrap()]);
    }

    #[test]
    fn project_runtime_marker_prefers_ruby_version_then_tool_versions() {
        let fixture = tempfile::tempdir().unwrap();
        fs::write(
            fixture.path().join(".tool-versions"),
            "nodejs 22.0.0\nruby jruby-9.4.14.0\n",
        )
        .unwrap();
        assert_eq!(
            project_runtime_marker(fixture.path()).unwrap().as_deref(),
            Some("jruby-9.4.14.0")
        );

        fs::write(fixture.path().join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
        assert_eq!(
            project_runtime_marker(fixture.path()).unwrap().as_deref(),
            Some("jruby-9.2.21.0")
        );
    }

    #[test]
    fn marker_selection_requires_exact_identity_and_prefers_the_active_path() {
        let mut rvm = parsed("jruby 9.2.21.0 (2.5.8) OpenJDK 64-Bit Server VM 17 [arm64-darwin]");
        rvm.executable = PathBuf::from("/rvm/jruby-9.2.21.0/bin/jruby");
        rvm.discovery_source = RuntimeDiscoverySource::Rvm;
        let mut active = rvm.clone();
        active.executable = PathBuf::from("/active/bin/jruby");
        active.discovery_source = RuntimeDiscoverySource::Path;
        let mri = parsed("ruby 3.3.11 (2025-01-01 revision abc) [arm64-darwin]");

        assert_eq!(
            select_runtime_for_marker("jruby-9.2.21.0", &[rvm, active.clone(), mri])
                .unwrap()
                .unwrap()
                .executable,
            active.executable
        );
        assert!(
            select_runtime_for_marker("jruby-9.2.20.0", &[active])
                .unwrap()
                .is_none(),
            "a nearby patch release must never satisfy an exact project marker"
        );
    }

    #[test]
    fn unsupported_or_malformed_runtime_markers_fail_closed() {
        let runtime = parsed("jruby 9.2.21.0 (2.5.8) OpenJDK 64-Bit Server VM 17 [arm64-darwin]");
        assert!(matches!(
            select_runtime_for_marker("jruby-current", &[runtime.clone()]),
            Err(RuntimeMarkerError::Invalid(_))
        ));
        assert!(matches!(
            select_runtime_for_marker("unknown-9.2.21.0", &[runtime]),
            Err(RuntimeMarkerError::Invalid(_))
        ));
        assert!(
            select_runtime_for_marker("system", &[]).unwrap().is_none(),
            "version-manager system markers must defer to the active environment"
        );
    }
}
