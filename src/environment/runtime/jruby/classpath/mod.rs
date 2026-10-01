//! Project-isolated JRuby classpath discovery: runtime, gem, lockfile, and explicit artifacts.

use crate::invariant::ExpectInvariant;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

mod builder;
mod coordinates;
mod file_product;
mod manifest;
mod paths;

use builder::ClasspathBuilder;
use paths::{canonical_directory, canonical_file};

pub use file_product::ClasspathFileProductCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactOrigin {
    JrubyRuntime,
    JdkRuntime,
    JavaGem,
    Lockfile,
    Jarfile,
    ProjectRepository,
    ManifestClassPath,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    Jar,
    Jmod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClasspathArtifact {
    pub path: PathBuf,
    pub origin: ArtifactOrigin,
    pub kind: ArtifactKind,
    pub fingerprint_sha256: String,
    pub byte_length: u64,
    pub(crate) file_identity: SourceFileIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceOrigin {
    Project,
    Attached,
    Jdk,
    Explicit,
    Decompiled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRoot {
    pub path: PathBuf,
    pub origin: SourceOrigin,
    pub fingerprint_sha256: Option<String>,
    pub(crate) file_identity: Option<SourceFileIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceFileIdentity {
    pub byte_length: u64,
    pub modified: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedCoordinate {
    pub coordinate: String,
    pub origin: ArtifactOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectClasspath {
    pub project_root: PathBuf,
    pub artifacts: Vec<ClasspathArtifact>,
    pub sources: Vec<SourceRoot>,
    pub unresolved: Vec<UnresolvedCoordinate>,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ClasspathInputs {
    pub project_root: PathBuf,
    pub jruby_executable: PathBuf,
    pub java_home: PathBuf,
    pub maven_repository: Option<PathBuf>,
    pub java_gem_roots: Vec<PathBuf>,
    pub additional_classpath: Vec<String>,
    pub additional_sources: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClasspathLimits {
    pub max_artifacts: usize,
    pub max_sources: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_walk_entries: usize,
}

impl Default for ClasspathLimits {
    fn default() -> Self {
        Self {
            max_artifacts: 100_000,
            max_sources: 1_024,
            max_file_bytes: 512 * 1024 * 1024,
            max_total_bytes: 4 * 1024 * 1024 * 1024,
            max_walk_entries: 250_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClasspathError {
    MissingProjectRoot(PathBuf),
    MissingRuntimeExecutable(PathBuf),
    MissingJavaHome(PathBuf),
    InvalidProjectPattern(String),
    PatternMatchedNothing(String),
    PathEscapesProject(PathBuf),
    UnsupportedArtifact(PathBuf),
    LimitExceeded(&'static str),
    Io { path: PathBuf, message: String },
    InvalidLockEntry(String),
    InvalidManifestEntry { artifact: PathBuf, entry: String },
}

pub fn discover_project_classpath(
    inputs: &ClasspathInputs,
    limits: ClasspathLimits,
) -> Result<ProjectClasspath, ClasspathError> {
    discover_project_classpath_inner(inputs, limits, None)
}

pub fn discover_project_classpath_with_cache(
    inputs: &ClasspathInputs,
    limits: ClasspathLimits,
    file_product_cache: &ClasspathFileProductCache,
) -> Result<ProjectClasspath, ClasspathError> {
    discover_project_classpath_inner(inputs, limits, Some(file_product_cache.clone()))
}

fn discover_project_classpath_inner(
    inputs: &ClasspathInputs,
    limits: ClasspathLimits,
    file_product_cache: Option<ClasspathFileProductCache>,
) -> Result<ProjectClasspath, ClasspathError> {
    let project_root = canonical_directory(&inputs.project_root)
        .map_err(|_| ClasspathError::MissingProjectRoot(inputs.project_root.clone()))?;
    let jruby_executable = canonical_file(&inputs.jruby_executable)
        .map_err(|_| ClasspathError::MissingRuntimeExecutable(inputs.jruby_executable.clone()))?;
    let java_home = canonical_directory(&inputs.java_home)
        .map_err(|_| ClasspathError::MissingJavaHome(inputs.java_home.clone()))?;
    let jruby_home = jruby_executable
        .parent()
        .and_then(Path::parent)
        .expect_invariant(
            "canonical JRuby executable has no bin and home parents",
            "a canonical executable path lives under home/bin",
            "validate the executable location before use",
        );

    let mut builder = ClasspathBuilder::new(project_root.clone(), limits, file_product_cache);
    builder.add_known_jruby_runtime(jruby_home)?;
    builder.add_jdk_runtime(&java_home)?;
    for root in &inputs.java_gem_roots {
        builder.add_java_gem_root(root)?;
    }
    builder.add_project_repository(&project_root)?;
    builder.add_project_source_roots(&project_root)?;
    if let Some(repository) = &inputs.maven_repository {
        builder.add_locked_coordinates(&project_root, repository)?;
    }
    builder.add_project_patterns(&inputs.additional_classpath, &inputs.additional_sources)?;
    builder.finish()
}

#[cfg(test)]
mod tests;
