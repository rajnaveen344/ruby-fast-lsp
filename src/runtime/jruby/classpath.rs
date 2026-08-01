use crate::single_flight::{BlockingBoundedSingleFlightCache, SingleFlightSnapshot};
use globset::Glob;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use walkdir::WalkDir;

const DEFAULT_CLASSPATH_FILE_CACHE_ENTRIES: usize = 4_096;
const DEFAULT_CLASSPATH_FILE_CACHE_WEIGHT_BYTES: u64 = 16 * 1024 * 1024;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClasspathFileProductKind {
    Fingerprint,
    JarManifest { max_archive_entries: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClasspathFileProductKey {
    canonical_path: PathBuf,
    identity: SourceFileIdentity,
    kind: ClasspathFileProductKind,
}

#[derive(Debug)]
struct ClasspathFileProduct {
    fingerprint_sha256: String,
    manifest_class_path_entries: Vec<String>,
}

impl ClasspathFileProduct {
    fn estimated_weight_bytes(&self) -> u64 {
        // Include a fixed allowance for the cache key, hash-map entry, Arc,
        // String/Vec headers, and typical canonical path. Entry count is an
        // independent hard bound for paths longer than this allowance.
        self.manifest_class_path_entries
            .iter()
            .try_fold(512u64, |total, entry| {
                total.checked_add(u64::try_from(entry.len()).expect(
                    "INVARIANT VIOLATED: a manifest entry length does not fit u64. This is a bug because manifest input is bounded to one MiB. Fix: retain bounded manifest parsing before caching its logical entries.",
                ))
            })
            .and_then(|total| {
                total.checked_add(u64::try_from(self.fingerprint_sha256.len()).expect(
                    "INVARIANT VIOLATED: a SHA-256 string length does not fit u64. This is a bug because its encoded length is fixed at 64 bytes. Fix: keep fingerprints as bounded SHA-256 hex strings.",
                ))
            })
            .expect(
                "INVARIANT VIOLATED: retained classpath descriptor weight overflowed u64. This is a bug because manifest payloads and cache entry counts are bounded. Fix: inspect descriptor weight accounting.",
            )
    }
}

/// Process-owned reuse for immutable classpath file descriptors. This never
/// retains raw JAR/JMOD/source bytes and never owns project classpath order or
/// semantic facts; each isolated project composes the returned descriptor into
/// its own `ProjectClasspath`.
#[derive(Clone)]
pub struct ClasspathFileProductCache {
    inner: BlockingBoundedSingleFlightCache<
        ClasspathFileProductKey,
        ClasspathFileProduct,
        ClasspathError,
    >,
}

impl Default for ClasspathFileProductCache {
    fn default() -> Self {
        Self::new(
            DEFAULT_CLASSPATH_FILE_CACHE_ENTRIES,
            DEFAULT_CLASSPATH_FILE_CACHE_WEIGHT_BYTES,
        )
    }
}

impl ClasspathFileProductCache {
    pub fn new(max_entries: usize, max_weight_bytes: u64) -> Self {
        Self {
            inner: BlockingBoundedSingleFlightCache::new(
                max_entries,
                max_weight_bytes,
                ClasspathFileProduct::estimated_weight_bytes,
            ),
        }
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        self.inner.snapshot()
    }

    pub fn retained_weight_bytes(&self) -> u64 {
        self.inner.retained_weight()
    }

    fn get_or_read(
        &self,
        path: &Path,
        identity: SourceFileIdentity,
        kind: ClasspathFileProductKind,
        max_file_bytes: u64,
    ) -> Result<Arc<ClasspathFileProduct>, ClasspathError> {
        let key = ClasspathFileProductKey {
            canonical_path: path.to_path_buf(),
            identity,
            kind,
        };
        self.inner.get_or_try_init(key, || {
            read_classpath_file_product(path, identity, kind, max_file_bytes)
        })
    }
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
        .expect("INVARIANT VIOLATED: canonical JRuby executable must have bin and home parents");

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

struct ClasspathBuilder {
    project_root: PathBuf,
    limits: ClasspathLimits,
    file_product_cache: Option<ClasspathFileProductCache>,
    artifacts: BTreeMap<PathBuf, ClasspathArtifact>,
    sources: BTreeMap<PathBuf, SourceRoot>,
    unresolved: Vec<UnresolvedCoordinate>,
    total_bytes: u64,
}

impl ClasspathBuilder {
    fn new(
        project_root: PathBuf,
        limits: ClasspathLimits,
        file_product_cache: Option<ClasspathFileProductCache>,
    ) -> Self {
        Self {
            project_root,
            limits,
            file_product_cache,
            artifacts: BTreeMap::new(),
            sources: BTreeMap::new(),
            unresolved: Vec::new(),
            total_bytes: 0,
        }
    }

    fn add_known_jruby_runtime(&mut self, jruby_home: &Path) -> Result<(), ClasspathError> {
        self.add_if_file(
            &jruby_home.join("lib/jruby.jar"),
            ArtifactOrigin::JrubyRuntime,
        )?;
        self.add_jars_below(
            &jruby_home.join("lib/ruby/stdlib"),
            ArtifactOrigin::JrubyRuntime,
            4,
        )
    }

    fn add_jdk_runtime(&mut self, java_home: &Path) -> Result<(), ClasspathError> {
        self.add_jmods_below(&java_home.join("jmods"), ArtifactOrigin::JdkRuntime)?;
        for candidate in [
            java_home.join("jre/lib/rt.jar"),
            java_home.join("lib/rt.jar"),
            java_home.join("lib/tools.jar"),
        ] {
            self.add_if_file(&candidate, ArtifactOrigin::JdkRuntime)?;
        }
        for source in [
            java_home.join("lib/src.zip"),
            java_home.join("src.zip"),
            java_home.join("../src.zip"),
        ] {
            if source.is_file() {
                self.add_source(&source, SourceOrigin::Jdk)?;
                break;
            }
        }
        Ok(())
    }

    fn add_java_gem_root(&mut self, root: &Path) -> Result<(), ClasspathError> {
        self.add_jars_below(root, ArtifactOrigin::JavaGem, 8)
    }

    fn add_project_repository(&mut self, project_root: &Path) -> Result<(), ClasspathError> {
        self.add_jars_below(
            &project_root.join("lib/jars"),
            ArtifactOrigin::ProjectRepository,
            usize::MAX,
        )
    }

    fn add_project_source_roots(&mut self, project_root: &Path) -> Result<(), ClasspathError> {
        for candidate in [
            project_root.join("src/main/java"),
            project_root.join("src/test/java"),
            project_root.join("src/java"),
            project_root.join("java"),
        ] {
            if candidate.is_dir() {
                self.add_source(&candidate, SourceOrigin::Project)?;
            }
        }
        Ok(())
    }

    fn add_locked_coordinates(
        &mut self,
        project_root: &Path,
        repository: &Path,
    ) -> Result<(), ClasspathError> {
        let repository = canonical_directory(repository).map_err(|error| ClasspathError::Io {
            path: repository.to_path_buf(),
            message: error.to_string(),
        })?;
        let lock = project_root.join("Jars.lock");
        if lock.is_file() {
            let contents = read_text(&lock, self.limits.max_file_bytes)?;
            for line in contents.lines() {
                let Some(coordinate) = parse_lock_coordinate(line)? else {
                    continue;
                };
                self.add_coordinate(&coordinate, &repository, ArtifactOrigin::Lockfile)?;
            }
        } else {
            let jarfile = project_root.join("Jarfile");
            if jarfile.is_file() {
                let contents = read_text(&jarfile, self.limits.max_file_bytes)?;
                for line in contents.lines() {
                    let Some(coordinate) = parse_jarfile_coordinate(line)? else {
                        continue;
                    };
                    self.add_coordinate(&coordinate, &repository, ArtifactOrigin::Jarfile)?;
                }
            }
        }
        Ok(())
    }

    fn add_coordinate(
        &mut self,
        coordinate: &MavenCoordinate,
        repository: &Path,
        origin: ArtifactOrigin,
    ) -> Result<(), ClasspathError> {
        let path = coordinate.repository_path(repository);
        if path.is_file() {
            self.add_artifact(&path, origin)
        } else {
            self.unresolved.push(UnresolvedCoordinate {
                coordinate: coordinate.display(),
                origin,
            });
            Ok(())
        }
    }

    fn add_project_patterns(
        &mut self,
        classpath: &[String],
        sources: &[String],
    ) -> Result<(), ClasspathError> {
        for pattern in classpath {
            let matches =
                project_pattern_matches(&self.project_root, pattern, self.limits.max_walk_entries)?;
            if matches.is_empty() {
                return Err(ClasspathError::PatternMatchedNothing(pattern.clone()));
            }
            for path in matches {
                self.add_artifact(&path, ArtifactOrigin::Explicit)?;
            }
        }
        for pattern in sources {
            let matches =
                project_pattern_matches(&self.project_root, pattern, self.limits.max_walk_entries)?;
            if matches.is_empty() {
                return Err(ClasspathError::PatternMatchedNothing(pattern.clone()));
            }
            for path in matches {
                self.add_source(&path, SourceOrigin::Explicit)?;
            }
        }
        Ok(())
    }

    fn add_if_file(&mut self, path: &Path, origin: ArtifactOrigin) -> Result<(), ClasspathError> {
        if path.is_file() {
            self.add_artifact(path, origin)?;
        }
        Ok(())
    }

    fn add_jmods_below(
        &mut self,
        root: &Path,
        origin: ArtifactOrigin,
    ) -> Result<(), ClasspathError> {
        self.add_files_below(root, origin, 1, "jmod")
    }

    fn add_jars_below(
        &mut self,
        root: &Path,
        origin: ArtifactOrigin,
        depth: usize,
    ) -> Result<(), ClasspathError> {
        self.add_files_below(root, origin, depth, "jar")
    }

    fn add_files_below(
        &mut self,
        root: &Path,
        origin: ArtifactOrigin,
        depth: usize,
        extension: &str,
    ) -> Result<(), ClasspathError> {
        if !root.is_dir() {
            return Ok(());
        }
        let mut paths = Vec::new();
        let mut visited = 0usize;
        for entry in WalkDir::new(root)
            .max_depth(depth)
            .follow_links(false)
            .into_iter()
        {
            let entry = entry.map_err(|error| ClasspathError::Io {
                path: root.to_path_buf(),
                message: error.to_string(),
            })?;
            visited = visited
                .checked_add(1)
                .ok_or(ClasspathError::LimitExceeded("classpath walk entries"))?;
            if visited > self.limits.max_walk_entries {
                return Err(ClasspathError::LimitExceeded("classpath walk entries"));
            }
            if entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some(extension)
            {
                paths.push(entry.into_path());
            }
        }
        paths.sort();
        for path in paths {
            self.add_artifact(&path, origin)?;
        }
        Ok(())
    }

    fn add_artifact(&mut self, path: &Path, origin: ArtifactOrigin) -> Result<(), ClasspathError> {
        let path = canonical_file(path).map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        if is_sources_archive(&path) {
            return self.add_source(&path, SourceOrigin::Attached);
        }
        let kind = match path.extension().and_then(|extension| extension.to_str()) {
            Some("jar") => ArtifactKind::Jar,
            Some("jmod") => ArtifactKind::Jmod,
            _ => return Err(ClasspathError::UnsupportedArtifact(path)),
        };
        if let Some(existing) = self.artifacts.get_mut(&path) {
            if origin < existing.origin {
                existing.origin = origin;
            }
            return Ok(());
        }
        if self.artifacts.len() >= self.limits.max_artifacts {
            return Err(ClasspathError::LimitExceeded("classpath artifacts"));
        }
        let file_identity = classpath_file_identity(&path)?;
        let product_kind = match kind {
            ArtifactKind::Jar => ClasspathFileProductKind::JarManifest {
                max_archive_entries: self.limits.max_walk_entries,
            },
            ArtifactKind::Jmod => ClasspathFileProductKind::Fingerprint,
        };
        let product = self.file_product(&path, file_identity, product_kind)?;
        self.artifacts.insert(
            path.clone(),
            ClasspathArtifact {
                path: path.clone(),
                origin,
                kind,
                fingerprint_sha256: product.fingerprint_sha256.clone(),
                byte_length: file_identity.byte_length,
                file_identity,
            },
        );
        if kind == ArtifactKind::Jar {
            let source_archive = sibling_sources_archive(&path);
            if source_archive.is_file() {
                self.add_source(&source_archive, SourceOrigin::Attached)?;
            }
            for entry in &product.manifest_class_path_entries {
                let relative = validate_manifest_class_path_entry(&path, &entry)?;
                let parent = path.parent().expect(
                    "INVARIANT VIOLATED: canonical JAR path has no parent. \
                     This is a bug because filesystem artifact paths are absolute files. \
                     Fix: reject artifacts without a canonical parent before manifest expansion.",
                );
                let candidate = parent.join(relative);
                if !candidate.is_file() {
                    self.unresolved.push(UnresolvedCoordinate {
                        coordinate: candidate.to_string_lossy().to_string(),
                        origin: ArtifactOrigin::ManifestClassPath,
                    });
                    continue;
                }
                let canonical =
                    fs::canonicalize(&candidate).map_err(|error| ClasspathError::Io {
                        path: candidate.clone(),
                        message: error.to_string(),
                    })?;
                if !canonical.starts_with(parent) {
                    return Err(ClasspathError::InvalidManifestEntry {
                        artifact: path.clone(),
                        entry: entry.clone(),
                    });
                }
                self.add_artifact(&canonical, ArtifactOrigin::ManifestClassPath)?;
            }
        }
        Ok(())
    }

    fn add_source(&mut self, path: &Path, origin: SourceOrigin) -> Result<(), ClasspathError> {
        let path = fs::canonicalize(path).map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        if self.sources.contains_key(&path) {
            return Ok(());
        }
        if self.sources.len() >= self.limits.max_sources {
            return Err(ClasspathError::LimitExceeded("classpath sources"));
        }
        let (fingerprint_sha256, file_identity) = if path.is_file() {
            let (_, fingerprint, identity) = self.fingerprint_file(&path)?;
            (Some(fingerprint), Some(identity))
        } else if path.is_dir() {
            (None, None)
        } else {
            return Err(ClasspathError::Io {
                path,
                message: "source path is neither a file nor a directory".to_string(),
            });
        };
        self.sources.insert(
            path.clone(),
            SourceRoot {
                path,
                origin,
                fingerprint_sha256,
                file_identity,
            },
        );
        Ok(())
    }

    fn fingerprint_file(
        &mut self,
        path: &Path,
    ) -> Result<(u64, String, SourceFileIdentity), ClasspathError> {
        let identity = classpath_file_identity(path)?;
        let product = self.file_product(path, identity, ClasspathFileProductKind::Fingerprint)?;
        Ok((
            identity.byte_length,
            product.fingerprint_sha256.clone(),
            identity,
        ))
    }

    fn file_product(
        &mut self,
        path: &Path,
        identity: SourceFileIdentity,
        kind: ClasspathFileProductKind,
    ) -> Result<Arc<ClasspathFileProduct>, ClasspathError> {
        if identity.byte_length > self.limits.max_file_bytes {
            return Err(ClasspathError::LimitExceeded("classpath artifact bytes"));
        }
        let product = match &self.file_product_cache {
            Some(cache) => cache.get_or_read(path, identity, kind, self.limits.max_file_bytes)?,
            None => Arc::new(read_classpath_file_product(
                path,
                identity,
                kind,
                self.limits.max_file_bytes,
            )?),
        };
        let after = classpath_file_identity(path)?;
        if after != identity {
            return Err(ClasspathError::Io {
                path: path.to_path_buf(),
                message:
                    "classpath file changed while its cached checksum identity was being consumed"
                        .to_string(),
            });
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(identity.byte_length)
            .ok_or(ClasspathError::LimitExceeded("total classpath bytes"))?;
        if self.total_bytes > self.limits.max_total_bytes {
            return Err(ClasspathError::LimitExceeded("total classpath bytes"));
        }
        Ok(product)
    }

    fn finish(mut self) -> Result<ProjectClasspath, ClasspathError> {
        let mut artifacts = self.artifacts.into_values().collect::<Vec<_>>();
        artifacts.sort_by(|left, right| {
            left.origin
                .cmp(&right.origin)
                .then_with(|| left.path.cmp(&right.path))
        });
        let mut sources = self.sources.into_values().collect::<Vec<_>>();
        sources.sort_by(|left, right| {
            source_origin_precedence(left.origin)
                .cmp(&source_origin_precedence(right.origin))
                .then_with(|| left.path.cmp(&right.path))
        });
        self.unresolved.sort_by(|left, right| {
            left.origin
                .cmp(&right.origin)
                .then_with(|| left.coordinate.cmp(&right.coordinate))
        });
        self.unresolved.dedup();

        let mut fingerprint = Sha256::new();
        fingerprint.update(self.project_root.to_string_lossy().as_bytes());
        for artifact in &artifacts {
            fingerprint.update([artifact.origin as u8, artifact.kind as u8]);
            fingerprint.update(artifact.path.to_string_lossy().as_bytes());
            fingerprint.update(artifact.fingerprint_sha256.as_bytes());
        }
        for source in &sources {
            fingerprint.update(source.path.to_string_lossy().as_bytes());
            if let Some(identity) = &source.fingerprint_sha256 {
                fingerprint.update(identity.as_bytes());
            }
        }
        Ok(ProjectClasspath {
            project_root: self.project_root,
            artifacts,
            sources,
            unresolved: self.unresolved,
            fingerprint_sha256: format!("{:x}", fingerprint.finalize()),
        })
    }
}

fn classpath_file_identity(path: &Path) -> Result<SourceFileIdentity, ClasspathError> {
    let metadata = fs::metadata(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    classpath_file_identity_from_metadata(path, &metadata)
}

fn classpath_file_identity_from_metadata(
    path: &Path,
    metadata: &Metadata,
) -> Result<SourceFileIdentity, ClasspathError> {
    if !metadata.is_file() {
        return Err(ClasspathError::Io {
            path: path.to_path_buf(),
            message: "classpath product input is not a regular file".to_string(),
        });
    }
    let modified = metadata.modified().map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(SourceFileIdentity {
        byte_length: metadata.len(),
        modified,
    })
}

fn read_classpath_file_product(
    path: &Path,
    expected_identity: SourceFileIdentity,
    kind: ClasspathFileProductKind,
    max_file_bytes: u64,
) -> Result<ClasspathFileProduct, ClasspathError> {
    if expected_identity.byte_length > max_file_bytes {
        return Err(ClasspathError::LimitExceeded("classpath artifact bytes"));
    }
    let mut file = File::open(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let opened_identity = file
        .metadata()
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
        .and_then(|metadata| classpath_file_identity_from_metadata(path, &metadata))?;
    if opened_identity != expected_identity {
        return Err(classpath_file_changed(path));
    }
    let read_limit = max_file_bytes
        .checked_add(1)
        .ok_or(ClasspathError::LimitExceeded("classpath artifact bytes"))?;
    let capacity = usize::try_from(expected_identity.byte_length)
        .map_err(|_| ClasspathError::LimitExceeded("classpath artifact bytes"))?;
    let mut bytes = Vec::with_capacity(capacity);
    (&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    if u64::try_from(bytes.len()).expect(
        "INVARIANT VIOLATED: an in-memory classpath buffer length does not fit u64. This is a bug because the read is bounded far below u64::MAX. Fix: keep classpath read bounds below the addressable process size.",
    ) > max_file_bytes
    {
        return Err(ClasspathError::LimitExceeded("classpath artifact bytes"));
    }
    let handle_after_identity = file
        .metadata()
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
        .and_then(|metadata| classpath_file_identity_from_metadata(path, &metadata))?;
    let path_after_identity = classpath_file_identity(path)?;
    if u64::try_from(bytes.len()).expect(
        "INVARIANT VIOLATED: an in-memory classpath buffer length does not fit u64. This is a bug because the read is bounded far below u64::MAX. Fix: keep classpath read bounds below the addressable process size.",
    ) != expected_identity.byte_length
        || handle_after_identity != expected_identity
        || path_after_identity != expected_identity
    {
        return Err(classpath_file_changed(path));
    }

    let manifest_class_path_entries = match kind {
        ClasspathFileProductKind::Fingerprint => Vec::new(),
        ClasspathFileProductKind::JarManifest {
            max_archive_entries,
        } => manifest_class_path_entries(path, &bytes, max_archive_entries)?,
    };
    Ok(ClasspathFileProduct {
        fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
        manifest_class_path_entries,
    })
}

fn classpath_file_changed(path: &Path) -> ClasspathError {
    ClasspathError::Io {
        path: path.to_path_buf(),
        message: "classpath file changed while its checksum identity was being established"
            .to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MavenCoordinate {
    group: String,
    artifact: String,
    classifier: Option<String>,
    version: String,
}

impl MavenCoordinate {
    fn repository_path(&self, repository: &Path) -> PathBuf {
        let mut filename = format!("{}-{}", self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            filename.push('-');
            filename.push_str(classifier);
        }
        filename.push_str(".jar");
        repository
            .join(self.group.replace('.', "/"))
            .join(&self.artifact)
            .join(&self.version)
            .join(filename)
    }

    fn display(&self) -> String {
        match &self.classifier {
            Some(classifier) => format!(
                "{}:{}:{}:{}",
                self.group, self.artifact, classifier, self.version
            ),
            None => format!("{}:{}:{}", self.group, self.artifact, self.version),
        }
    }
}

fn is_sources_archive(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("jar")
        && path
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("-sources"))
}

fn sibling_sources_archive(path: &Path) -> PathBuf {
    let stem = path.file_stem().and_then(|name| name.to_str()).expect(
        "INVARIANT VIOLATED: accepted JAR artifact has no UTF-8 file stem. \
         This is a bug because deterministic source attachment names require a stable path identity. \
         Fix: reject non-UTF-8 JAR filenames during classpath discovery.",
    );
    path.with_file_name(format!("{stem}-sources.jar"))
}

fn source_origin_precedence(origin: SourceOrigin) -> u8 {
    match origin {
        SourceOrigin::Project | SourceOrigin::Explicit => 0,
        SourceOrigin::Attached => 1,
        SourceOrigin::Jdk => 2,
        SourceOrigin::Decompiled => 3,
    }
}

fn parse_lock_coordinate(line: &str) -> Result<Option<MavenCoordinate>, ClasspathError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || !line.contains(':') {
        return Ok(None);
    }
    let normalized = line.replace(":jar:", ":");
    let parts = normalized.split(':').collect::<Vec<_>>();
    if parts.len() < 5 {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    }
    let (classifier, version_index) = if parts.len() == 5 {
        (None, 2)
    } else {
        (Some(parts[2].to_string()), 3)
    };
    let coordinate = MavenCoordinate {
        group: parts[0].to_string(),
        artifact: parts[1].to_string(),
        classifier,
        version: parts[version_index].to_string(),
    };
    validate_coordinate(&coordinate, line)?;
    Ok(Some(coordinate))
}

fn parse_jarfile_coordinate(line: &str) -> Result<Option<MavenCoordinate>, ClasspathError> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let Some(arguments) = line.strip_prefix("jar ") else {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    };
    let literals = quoted_literals(arguments)?;
    if literals.len() < 2 {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    }
    let Some((group, artifact)) = literals[0].split_once(':') else {
        return Err(ClasspathError::InvalidLockEntry(line.to_string()));
    };
    let coordinate = MavenCoordinate {
        group: group.to_string(),
        artifact: artifact.to_string(),
        classifier: None,
        version: literals[1].clone(),
    };
    validate_coordinate(&coordinate, line)?;
    Ok(Some(coordinate))
}

fn quoted_literals(source: &str) -> Result<Vec<String>, ClasspathError> {
    let mut values = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, character)) = chars.next() {
        if character != '\'' && character != '"' {
            continue;
        }
        let quote = character;
        let mut value = String::new();
        let mut closed = false;
        for (_, character) in chars.by_ref() {
            if character == quote {
                closed = true;
                break;
            }
            value.push(character);
        }
        if !closed {
            return Err(ClasspathError::InvalidLockEntry(source.to_string()));
        }
        values.push(value);
    }
    Ok(values)
}

fn validate_coordinate(coordinate: &MavenCoordinate, source: &str) -> Result<(), ClasspathError> {
    let values = [
        coordinate.group.as_str(),
        coordinate.artifact.as_str(),
        coordinate.version.as_str(),
    ];
    if values
        .iter()
        .any(|value| value.is_empty() || value.contains('/') || value.contains('\\'))
        || coordinate
            .classifier
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.contains(['/', '\\']))
    {
        return Err(ClasspathError::InvalidLockEntry(source.to_string()));
    }
    Ok(())
}

fn project_pattern_matches(
    project_root: &Path,
    pattern: &str,
    max_entries: usize,
) -> Result<Vec<PathBuf>, ClasspathError> {
    validate_project_pattern(pattern)?;
    let matcher = Glob::new(pattern)
        .map_err(|_| ClasspathError::InvalidProjectPattern(pattern.to_string()))?
        .compile_matcher();
    let mut matches = Vec::new();
    let mut visited = 0usize;
    let walker = WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git");
    for entry in walker {
        let entry = entry.map_err(|error| ClasspathError::Io {
            path: project_root.to_path_buf(),
            message: error.to_string(),
        })?;
        visited += 1;
        if visited > max_entries {
            return Err(ClasspathError::LimitExceeded("project pattern entries"));
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(project_root)
            .expect("INVARIANT VIOLATED: project walker entry must remain below its root");
        if matcher.is_match(relative) {
            let canonical = fs::canonicalize(path).map_err(|error| ClasspathError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
            if !canonical.starts_with(project_root) {
                return Err(ClasspathError::PathEscapesProject(canonical));
            }
            matches.push(canonical);
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn validate_project_pattern(pattern: &str) -> Result<(), ClasspathError> {
    let path = Path::new(pattern);
    if pattern.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ClasspathError::InvalidProjectPattern(pattern.to_string()));
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> std::io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path is not a directory",
        ));
    }
    Ok(path)
}

fn canonical_file(path: &Path) -> std::io::Result<PathBuf> {
    let path = fs::canonicalize(path)?;
    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path is not a file",
        ));
    }
    Ok(path)
}

fn read_text(path: &Path, max_bytes: u64) -> Result<String, ClasspathError> {
    let metadata = fs::metadata(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > max_bytes {
        return Err(ClasspathError::LimitExceeded("classpath metadata bytes"));
    }
    fs::read_to_string(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn manifest_class_path_entries(
    path: &Path,
    bytes: &[u8],
    max_entries: usize,
) -> Result<Vec<String>, ClasspathError> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        // Catalog parsing remains the authority for rejecting malformed JARs.
        // Classpath discovery only expands a manifest when a bounded archive is
        // readable, preserving existing content-identity error ownership.
        return Ok(Vec::new());
    };
    if archive.len() > max_entries {
        return Err(ClasspathError::LimitExceeded("manifest archive entries"));
    }
    let mut manifest_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ClasspathError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        if entry.name().eq_ignore_ascii_case("META-INF/MANIFEST.MF") {
            if manifest_index.replace(index).is_some() {
                return Err(ClasspathError::InvalidManifestEntry {
                    artifact: path.to_path_buf(),
                    entry: "duplicate META-INF/MANIFEST.MF".to_string(),
                });
            }
        }
    }
    let Some(index) = manifest_index else {
        return Ok(Vec::new());
    };
    let mut manifest = archive
        .by_index(index)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
    if manifest.size() > MAX_MANIFEST_BYTES {
        return Err(ClasspathError::LimitExceeded("JAR manifest bytes"));
    }
    let mut bytes = Vec::with_capacity(manifest.size() as usize);
    manifest
        .read_to_end(&mut bytes)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let source = std::str::from_utf8(&bytes).map_err(|_| ClasspathError::InvalidManifestEntry {
        artifact: path.to_path_buf(),
        entry: "manifest is not UTF-8".to_string(),
    })?;
    let mut logical_lines = Vec::<String>::new();
    for physical in source.lines() {
        let physical = physical.strip_suffix('\r').unwrap_or(physical);
        if physical.is_empty() {
            break;
        }
        if let Some(continuation) = physical.strip_prefix(' ') {
            let Some(previous) = logical_lines.last_mut() else {
                return Err(ClasspathError::InvalidManifestEntry {
                    artifact: path.to_path_buf(),
                    entry: "orphan manifest continuation".to_string(),
                });
            };
            previous.push_str(continuation);
        } else {
            logical_lines.push(physical.to_string());
        }
    }
    let mut class_path = None;
    for line in logical_lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(ClasspathError::InvalidManifestEntry {
                artifact: path.to_path_buf(),
                entry: line,
            });
        };
        if !name.eq_ignore_ascii_case("Class-Path") {
            continue;
        }
        if class_path.replace(value.trim().to_string()).is_some() {
            return Err(ClasspathError::InvalidManifestEntry {
                artifact: path.to_path_buf(),
                entry: "duplicate Class-Path attribute".to_string(),
            });
        }
    }
    Ok(class_path
        .map(|value| value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default())
}

fn validate_manifest_class_path_entry(
    artifact: &Path,
    entry: &str,
) -> Result<PathBuf, ClasspathError> {
    let path = Path::new(entry);
    let valid = !entry.is_empty()
        && !entry.contains('\\')
        && !entry.contains(':')
        && !entry.contains('%')
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path.extension().and_then(|extension| extension.to_str()) == Some("jar");
    if !valid {
        return Err(ClasspathError::InvalidManifestEntry {
            artifact: artifact.to_path_buf(),
            entry: entry.to_string(),
        });
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(
            path.parent()
                .expect("test fixture file must have a parent directory"),
        )
        .expect("test fixture parent must be created");
        fs::write(path, bytes).expect("test fixture file must be written");
    }

    fn jar_with_manifest(class_path: Option<&str>) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        if let Some(class_path) = class_path {
            writer
                .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
                .unwrap();
            write!(
                writer,
                "Manifest-Version: 1.0\r\nClass-Path: {class_path}\r\n\r\n"
            )
            .unwrap();
        }
        writer
            .start_file("fixture.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture").unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn fixture_inputs(root: &Path) -> ClasspathInputs {
        let project = root.join("project");
        let runtime = root.join("jruby-9.2.21.0");
        let java_home = root.join("jdk-17");
        let repository = root.join("m2");
        fs::create_dir_all(&project).expect("project fixture root must be created");
        write(&runtime.join("bin/jruby"), b"fixture executable");
        write(&runtime.join("lib/jruby.jar"), b"jruby runtime");
        write(
            &runtime.join("lib/ruby/stdlib/jopenssl.jar"),
            b"jruby stdlib",
        );
        write(&java_home.join("jmods/java.base.jmod"), b"java base");
        write(&java_home.join("lib/src.zip"), b"jdk sources");
        write(
            &repository.join("com/example/demo/1.2/demo-1.2.jar"),
            b"locked demo",
        );
        write(
            &project.join("Jars.lock"),
            b"com.example:demo:jar:1.2:runtime:\ncom.missing:absent:jar:9.0:runtime:\n",
        );
        write(&project.join("vendor/jars/explicit.jar"), b"explicit");
        fs::create_dir_all(project.join("java-src"))
            .expect("explicit source fixture must be created");

        ClasspathInputs {
            project_root: project,
            jruby_executable: runtime.join("bin/jruby"),
            java_home,
            maven_repository: Some(repository),
            java_gem_roots: Vec::new(),
            additional_classpath: vec!["vendor/jars/*.jar".to_string()],
            additional_sources: vec!["java-src".to_string()],
        }
    }

    fn sibling_project_inputs(shared: &ClasspathInputs, root: &Path) -> ClasspathInputs {
        let project = root.join("project");
        fs::create_dir_all(&project).expect("sibling project fixture root must be created");
        write(
            &project.join("Jars.lock"),
            b"com.example:demo:jar:1.2:runtime:\ncom.missing:absent:jar:9.0:runtime:\n",
        );
        write(
            &project.join("vendor/jars/explicit.jar"),
            b"sibling explicit",
        );
        fs::create_dir_all(project.join("java-src"))
            .expect("sibling explicit source fixture must be created");
        ClasspathInputs {
            project_root: project,
            jruby_executable: shared.jruby_executable.clone(),
            java_home: shared.java_home.clone(),
            maven_repository: shared.maven_repository.clone(),
            java_gem_roots: shared.java_gem_roots.clone(),
            additional_classpath: shared.additional_classpath.clone(),
            additional_sources: shared.additional_sources.clone(),
        }
    }

    #[test]
    fn reuses_exact_file_products_without_merging_project_classpaths() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let left_inputs = fixture_inputs(&fixture.path().join("shared"));
        let right_inputs = sibling_project_inputs(&left_inputs, &fixture.path().join("right"));
        let cache = ClasspathFileProductCache::new(128, 1024 * 1024);

        let left =
            discover_project_classpath_with_cache(&left_inputs, ClasspathLimits::default(), &cache)
                .expect("left cached classpath must be discovered");
        let after_left = cache.snapshot();
        assert!(after_left.lookups > 0);
        assert_eq!(after_left.producers, after_left.lookups);
        assert_eq!(after_left.hits, 0);
        assert_eq!(after_left.joined_flights, 0);

        let right = discover_project_classpath_with_cache(
            &right_inputs,
            ClasspathLimits::default(),
            &cache,
        )
        .expect("right cached classpath must be discovered");
        let after_right = cache.snapshot();

        assert_ne!(left.project_root, right.project_root);
        assert_ne!(left.fingerprint_sha256, right.fingerprint_sha256);
        assert!(
            after_right.hits >= 5,
            "expected shared runtime/JDK/Maven reuse"
        );
        assert!(after_right.producers < after_right.lookups);
        assert!(left
            .artifacts
            .iter()
            .any(|artifact| artifact.path.starts_with(&left.project_root)));
        assert!(left
            .artifacts
            .iter()
            .all(|artifact| !artifact.path.starts_with(&right.project_root)));
        assert!(right
            .artifacts
            .iter()
            .any(|artifact| artifact.path.starts_with(&right.project_root)));
        assert!(right
            .artifacts
            .iter()
            .all(|artifact| !artifact.path.starts_with(&left.project_root)));
    }

    #[test]
    fn concurrent_identical_discovery_has_one_producer_per_file_identity() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let inputs = fixture_inputs(fixture.path());
        let cache = ClasspathFileProductCache::new(128, 1024 * 1024);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let inputs = inputs.clone();
            let cache = cache.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
                    .expect("concurrent cached classpath must be discovered")
            }));
        }
        barrier.wait();
        let left = workers
            .remove(0)
            .join()
            .expect("left worker must not panic");
        let right = workers
            .remove(0)
            .join()
            .expect("right worker must not panic");
        let snapshot = cache.snapshot();

        assert_eq!(left, right);
        assert!(snapshot.producers > 0);
        assert_eq!(snapshot.lookups, snapshot.producers * 2);
        assert_eq!(snapshot.hits + snapshot.joined_flights, snapshot.producers);
    }

    #[test]
    fn changed_files_miss_the_cache_and_hit_paths_still_enforce_consumer_limits() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let inputs = fixture_inputs(fixture.path());
        let cache = ClasspathFileProductCache::new(128, 1024 * 1024);
        let first =
            discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
                .expect("initial cached classpath must be discovered");
        let after_first = cache.snapshot();
        let explicit_path = inputs.project_root.join("vendor/jars/explicit.jar");
        let first_fingerprint = first
            .artifacts
            .iter()
            .find(|artifact| artifact.path == explicit_path.canonicalize().unwrap())
            .expect("initial explicit artifact must exist")
            .fingerprint_sha256
            .clone();

        write(
            &explicit_path,
            b"changed explicit artifact with a new length",
        );
        let second =
            discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
                .expect("changed cached classpath must be rediscovered");
        let after_second = cache.snapshot();
        let second_fingerprint = second
            .artifacts
            .iter()
            .find(|artifact| artifact.path == explicit_path.canonicalize().unwrap())
            .expect("changed explicit artifact must exist")
            .fingerprint_sha256
            .clone();

        assert_ne!(first_fingerprint, second_fingerprint);
        assert_eq!(after_second.producers, after_first.producers + 1);
        assert!(after_second.hits > after_first.hits);

        let mut restrictive = ClasspathLimits::default();
        restrictive.max_total_bytes = 1;
        assert_eq!(
            discover_project_classpath_with_cache(&inputs, restrictive, &cache),
            Err(ClasspathError::LimitExceeded("total classpath bytes"))
        );
    }

    #[test]
    fn file_product_retention_obeys_entry_and_weight_bounds() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let inputs = fixture_inputs(fixture.path());
        let cache = ClasspathFileProductCache::new(2, 1_200);

        discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
            .expect("bounded cached classpath must be discovered");
        let snapshot = cache.snapshot();

        assert!(snapshot.entries <= 2);
        assert!(cache.retained_weight_bytes() <= 1_200);
        assert!(snapshot.evictions > 0);
    }

    #[test]
    fn discovers_one_project_in_precedence_order_with_content_identity() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let inputs = fixture_inputs(fixture.path());
        let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
            .expect("fixture classpath must be discovered");

        assert_eq!(
            classpath
                .artifacts
                .iter()
                .map(|artifact| artifact.origin)
                .collect::<Vec<_>>(),
            vec![
                ArtifactOrigin::JrubyRuntime,
                ArtifactOrigin::JrubyRuntime,
                ArtifactOrigin::JdkRuntime,
                ArtifactOrigin::Lockfile,
                ArtifactOrigin::Explicit,
            ]
        );
        assert!(classpath
            .artifacts
            .iter()
            .all(|artifact| artifact.fingerprint_sha256.len() == 64));
        assert_eq!(classpath.sources.len(), 2);
        assert_eq!(
            classpath.unresolved,
            vec![UnresolvedCoordinate {
                coordinate: "com.missing:absent:9.0".to_string(),
                origin: ArtifactOrigin::Lockfile,
            }]
        );
        assert_eq!(classpath.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn discovers_installed_project_local_jar_repository_without_a_lockfile() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        fs::remove_file(inputs.project_root.join("Jars.lock"))
            .expect("fixture lockfile must be removed");
        write(
            &inputs
                .project_root
                .join("lib/jars/com/example/transitive/4.5/transitive-4.5.jar"),
            b"project-local transitive jar",
        );
        inputs.maven_repository = None;

        let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
            .expect("project-local installed jars must be discovered");
        let project_repository = classpath
            .artifacts
            .iter()
            .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
            .collect::<Vec<_>>();

        assert_eq!(project_repository.len(), 1);
        assert!(project_repository[0]
            .path
            .ends_with("lib/jars/com/example/transitive/4.5/transitive-4.5.jar"));
    }

    #[test]
    fn indexes_jars_only_below_the_exact_selected_java_gem_root() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        inputs.maven_repository = None;
        fs::remove_file(inputs.project_root.join("Jars.lock")).unwrap();
        inputs.additional_classpath.clear();
        let selected = fixture
            .path()
            .join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
        let unrelated = fixture
            .path()
            .join("gems/jruby-9.2.21.0/gems/bson-4.14.0-java");
        write(&selected.join("lib/bson.jar"), b"selected Java gem");
        write(&unrelated.join("lib/bson.jar"), b"unrelated Java gem");
        inputs.java_gem_roots = vec![selected.clone()];

        let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
            .expect("exact Java gem classpath must be discovered");
        let java_gems = classpath
            .artifacts
            .iter()
            .filter(|artifact| artifact.origin == ArtifactOrigin::JavaGem)
            .collect::<Vec<_>>();

        assert_eq!(java_gems.len(), 1);
        assert_eq!(
            java_gems[0].path,
            selected.join("lib/bson.jar").canonicalize().unwrap()
        );
        assert!(!classpath
            .artifacts
            .iter()
            .any(|artifact| artifact.path.starts_with(&unrelated)));
    }

    #[test]
    fn expands_bounded_manifest_class_path_without_cycles_or_parent_traversal() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        inputs.maven_repository = None;
        fs::remove_file(inputs.project_root.join("Jars.lock")).unwrap();
        inputs.additional_classpath = vec!["vendor/jars/root.jar".to_string()];
        let root_jar = inputs.project_root.join("vendor/jars/root.jar");
        let dependency_jar = inputs.project_root.join("vendor/jars/dependency.jar");
        write(&root_jar, &jar_with_manifest(Some("dependency.jar")));
        write(&dependency_jar, &jar_with_manifest(Some("root.jar")));

        let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
            .expect("manifest classpath must expand deterministically");
        assert_eq!(
            classpath
                .artifacts
                .iter()
                .filter(|artifact| artifact.path == root_jar.canonicalize().unwrap())
                .count(),
            1
        );
        assert!(classpath.artifacts.iter().any(|artifact| {
            artifact.path == dependency_jar.canonicalize().unwrap()
                && artifact.origin == ArtifactOrigin::ManifestClassPath
        }));

        write(&root_jar, &jar_with_manifest(Some("../escape.jar")));
        assert!(matches!(
            discover_project_classpath(&inputs, ClasspathLimits::default()),
            Err(ClasspathError::InvalidManifestEntry { artifact, entry })
                if artifact == root_jar.canonicalize().unwrap() && entry == "../escape.jar"
        ));
    }

    #[test]
    fn discovers_exact_attached_and_project_java_sources_without_indexing_source_jars_as_code() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        write(
            &inputs
                .maven_repository
                .as_ref()
                .expect("fixture has a Maven repository")
                .join("com/example/demo/1.2/demo-1.2-sources.jar"),
            b"locked demo sources",
        );
        write(
            &inputs.project_root.join("lib/jars/local-2.0.jar"),
            b"local binary",
        );
        write(
            &inputs.project_root.join("lib/jars/local-2.0-sources.jar"),
            b"local sources",
        );
        write(
            &inputs
                .project_root
                .join("src/main/java/com/example/ProjectType.java"),
            b"package com.example; class ProjectType {}",
        );
        inputs.additional_classpath.clear();
        inputs.additional_sources.clear();

        let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
            .expect("attached and project sources must be discovered");
        assert!(
            classpath
                .artifacts
                .iter()
                .all(|artifact| !artifact.path.to_string_lossy().contains("-sources.jar")),
            "source archives must never be parsed as Java bytecode artifacts"
        );
        for suffix in [
            "demo-1.2-sources.jar",
            "local-2.0-sources.jar",
            "src/main/java",
            "lib/src.zip",
        ] {
            assert!(
                classpath
                    .sources
                    .iter()
                    .any(|source| source.path.to_string_lossy().ends_with(suffix)),
                "missing discovered Java source root {suffix}: {:?}",
                classpath.sources
            );
        }
    }

    #[test]
    fn project_local_jar_repositories_are_isolated_and_do_not_follow_symlinks() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let left_root = fixture.path().join("left");
        let right_root = fixture.path().join("right");
        let mut left = fixture_inputs(&left_root);
        let mut right = fixture_inputs(&right_root);
        for inputs in [&mut left, &mut right] {
            fs::remove_file(inputs.project_root.join("Jars.lock"))
                .expect("fixture lockfile must be removed");
            inputs.maven_repository = None;
        }
        write(
            &left.project_root.join("lib/jars/left-only.jar"),
            b"left-only",
        );
        write(
            &right.project_root.join("lib/jars/right-only.jar"),
            b"right-only",
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                right.project_root.join("lib/jars"),
                left.project_root.join("lib/jars/linked-right"),
            )
            .expect("fixture repository symlink must be created");
        }

        let left = discover_project_classpath(&left, ClasspathLimits::default())
            .expect("left classpath must be discovered");
        let right = discover_project_classpath(&right, ClasspathLimits::default())
            .expect("right classpath must be discovered");
        let left_paths = left
            .artifacts
            .iter()
            .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
            .map(|artifact| artifact.path.as_path())
            .collect::<Vec<_>>();
        let right_paths = right
            .artifacts
            .iter()
            .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
            .map(|artifact| artifact.path.as_path())
            .collect::<Vec<_>>();

        assert_eq!(left_paths.len(), 1);
        assert!(left_paths[0].ends_with("lib/jars/left-only.jar"));
        assert_eq!(right_paths.len(), 1);
        assert!(right_paths[0].ends_with("lib/jars/right-only.jar"));
    }

    #[test]
    fn project_local_repository_walk_counts_every_visited_entry() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        fs::remove_file(inputs.project_root.join("Jars.lock"))
            .expect("fixture lockfile must be removed");
        inputs.maven_repository = None;
        for index in 0..8 {
            fs::create_dir_all(
                inputs
                    .project_root
                    .join(format!("lib/jars/empty/{index}/nested")),
            )
            .expect("fixture empty directory must be created");
        }
        let mut limits = ClasspathLimits::default();
        limits.max_walk_entries = 4;

        assert_eq!(
            discover_project_classpath(&inputs, limits),
            Err(ClasspathError::LimitExceeded("classpath walk entries"))
        );
    }

    #[test]
    fn keeps_conflicting_project_classpaths_isolated() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let left_root = fixture.path().join("left");
        let right_root = fixture.path().join("right");
        let left = fixture_inputs(&left_root);
        let right = fixture_inputs(&right_root);
        write(
            &right
                .maven_repository
                .as_ref()
                .expect("fixture has repository")
                .join("com/example/demo/1.2/demo-1.2.jar"),
            b"different locked demo",
        );

        let left = discover_project_classpath(&left, ClasspathLimits::default())
            .expect("left fixture classpath must be discovered");
        let right = discover_project_classpath(&right, ClasspathLimits::default())
            .expect("right fixture classpath must be discovered");
        assert_ne!(left.project_root, right.project_root);
        assert_ne!(left.fingerprint_sha256, right.fingerprint_sha256);
        let left_demo = left
            .artifacts
            .iter()
            .find(|artifact| artifact.origin == ArtifactOrigin::Lockfile)
            .expect("left lock artifact must exist");
        let right_demo = right
            .artifacts
            .iter()
            .find(|artifact| artifact.origin == ArtifactOrigin::Lockfile)
            .expect("right lock artifact must exist");
        assert_ne!(left_demo.fingerprint_sha256, right_demo.fingerprint_sha256);
        let left_root = fs::canonicalize(left_root).expect("left fixture root must canonicalize");
        let right_root =
            fs::canonicalize(right_root).expect("right fixture root must canonicalize");
        assert!(left
            .artifacts
            .iter()
            .all(|artifact| artifact.path.starts_with(&left_root)));
        assert!(right
            .artifacts
            .iter()
            .all(|artifact| artifact.path.starts_with(&right_root)));
    }

    #[test]
    fn rejects_escaping_patterns_and_applies_artifact_bounds() {
        let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
        let mut inputs = fixture_inputs(fixture.path());
        inputs.additional_classpath = vec!["../outside.jar".to_string()];
        assert_eq!(
            discover_project_classpath(&inputs, ClasspathLimits::default()),
            Err(ClasspathError::InvalidProjectPattern(
                "../outside.jar".to_string()
            ))
        );

        inputs.additional_classpath = vec!["vendor/jars/*.jar".to_string()];
        let mut limits = ClasspathLimits::default();
        limits.max_artifacts = 1;
        assert_eq!(
            discover_project_classpath(&inputs, limits),
            Err(ClasspathError::LimitExceeded("classpath artifacts"))
        );
    }

    #[test]
    fn parses_literal_jarfile_only_without_executing_ruby() {
        assert_eq!(
            parse_jarfile_coordinate("jar 'org.example:demo', '2.0'"),
            Ok(Some(MavenCoordinate {
                group: "org.example".to_string(),
                artifact: "demo".to_string(),
                classifier: None,
                version: "2.0".to_string(),
            }))
        );
        assert_eq!(
            parse_jarfile_coordinate("jar dynamic_coordinate, ENV['VERSION']"),
            Err(ClasspathError::InvalidLockEntry(
                "jar dynamic_coordinate, ENV['VERSION']".to_string()
            ))
        );
    }
}
