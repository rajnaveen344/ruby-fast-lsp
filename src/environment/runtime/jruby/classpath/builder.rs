//! Ordered, bounded accumulation of one project's classpath artifacts and source roots.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use super::coordinates::{parse_jarfile_coordinate, parse_lock_coordinate, MavenCoordinate};
use super::file_product::{
    classpath_file_identity, read_classpath_file_product, ClasspathFileProduct,
    ClasspathFileProductCache, ClasspathFileProductKind,
};
use super::manifest::validate_manifest_class_path_entry;
use super::paths::{canonical_directory, canonical_file, project_pattern_matches, read_text};
use super::{
    ArtifactKind, ArtifactOrigin, ClasspathArtifact, ClasspathError, ClasspathLimits,
    ProjectClasspath, SourceFileIdentity, SourceOrigin, SourceRoot, UnresolvedCoordinate,
};

pub(super) struct ClasspathBuilder {
    project_root: PathBuf,
    limits: ClasspathLimits,
    file_product_cache: Option<ClasspathFileProductCache>,
    artifacts: BTreeMap<PathBuf, ClasspathArtifact>,
    sources: BTreeMap<PathBuf, SourceRoot>,
    unresolved: Vec<UnresolvedCoordinate>,
    total_bytes: u64,
}

impl ClasspathBuilder {
    pub(super) fn new(
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

    pub(super) fn add_known_jruby_runtime(
        &mut self,
        jruby_home: &Path,
    ) -> Result<(), ClasspathError> {
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

    pub(super) fn add_jdk_runtime(&mut self, java_home: &Path) -> Result<(), ClasspathError> {
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

    pub(super) fn add_java_gem_root(&mut self, root: &Path) -> Result<(), ClasspathError> {
        self.add_jars_below(root, ArtifactOrigin::JavaGem, 8)
    }

    pub(super) fn add_project_repository(
        &mut self,
        project_root: &Path,
    ) -> Result<(), ClasspathError> {
        self.add_jars_below(
            &project_root.join("lib/jars"),
            ArtifactOrigin::ProjectRepository,
            usize::MAX,
        )
    }

    pub(super) fn add_project_source_roots(
        &mut self,
        project_root: &Path,
    ) -> Result<(), ClasspathError> {
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

    pub(super) fn add_locked_coordinates(
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

    pub(super) fn add_project_patterns(
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

    pub(super) fn finish(mut self) -> Result<ProjectClasspath, ClasspathError> {
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
