use super::classpath::{ArtifactKind, ClasspathArtifact, ProjectClasspath};
use ruby_fast_lsp_jvm_metadata::{parse_archive, ArchiveKind, ArchiveLimits, ClassFile};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaClassDeclaration {
    pub class: ClassFile,
    pub artifact_path: PathBuf,
    pub artifact_fingerprint_sha256: String,
    pub entry_name: String,
    pub release: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateJavaClass {
    pub name: String,
    pub winner: PathBuf,
    pub shadowed: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectJavaCatalog {
    pub classpath_fingerprint_sha256: String,
    pub classes: BTreeMap<String, JavaClassDeclaration>,
    pub duplicates: Vec<DuplicateJavaClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaCatalogError {
    Read { path: PathBuf, message: String },
    Archive { path: PathBuf, message: String },
    ArtifactFingerprintMismatch { path: PathBuf },
}

pub fn build_project_java_catalog(
    classpath: &ProjectClasspath,
    jdk_feature: u16,
    archive_limits: ArchiveLimits,
) -> Result<ProjectJavaCatalog, JavaCatalogError> {
    let mut classes = BTreeMap::new();
    let mut duplicates = Vec::new();
    for artifact in &classpath.artifacts {
        add_artifact(
            artifact,
            jdk_feature,
            archive_limits,
            &mut classes,
            &mut duplicates,
        )?;
    }
    duplicates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.winner.cmp(&right.winner))
            .then_with(|| left.shadowed.cmp(&right.shadowed))
    });
    Ok(ProjectJavaCatalog {
        classpath_fingerprint_sha256: classpath.fingerprint_sha256.clone(),
        classes,
        duplicates,
    })
}

fn add_artifact(
    artifact: &ClasspathArtifact,
    jdk_feature: u16,
    archive_limits: ArchiveLimits,
    classes: &mut BTreeMap<String, JavaClassDeclaration>,
    duplicates: &mut Vec<DuplicateJavaClass>,
) -> Result<(), JavaCatalogError> {
    let bytes = fs::read(&artifact.path).map_err(|error| JavaCatalogError::Read {
        path: artifact.path.clone(),
        message: error.to_string(),
    })?;
    if format!("{:x}", Sha256::digest(&bytes)) != artifact.fingerprint_sha256 {
        return Err(JavaCatalogError::ArtifactFingerprintMismatch {
            path: artifact.path.clone(),
        });
    }
    let archive_kind = match artifact.kind {
        ArtifactKind::Jar => ArchiveKind::Jar,
        ArtifactKind::Jmod => ArchiveKind::Jmod,
    };
    let archive =
        parse_archive(&bytes, archive_kind, jdk_feature, archive_limits).map_err(|error| {
            JavaCatalogError::Archive {
                path: artifact.path.clone(),
                message: format!("{error:?}"),
            }
        })?;
    assert_eq!(
        archive.fingerprint_sha256, artifact.fingerprint_sha256,
        "INVARIANT VIOLATED: archive parser content identity differs from the pre-parse SHA-256. \
         This is a bug because both hashes cover the same immutable bytes. Fix: keep artifact and \
         archive fingerprint algorithms identical."
    );
    for archived in archive.classes {
        let name = archived.class.name.clone();
        let declaration = JavaClassDeclaration {
            class: archived.class,
            artifact_path: artifact.path.clone(),
            artifact_fingerprint_sha256: artifact.fingerprint_sha256.clone(),
            entry_name: archived.entry_name,
            release: archived.release,
        };
        if let Some(existing) = classes.get(&name) {
            duplicates.push(DuplicateJavaClass {
                name,
                winner: existing.artifact_path.clone(),
                shadowed: artifact.path.clone(),
            });
        } else {
            classes.insert(name, declaration);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::classpath::{
        ArtifactOrigin, ClasspathArtifact, ProjectClasspath, SourceRoot, UnresolvedCoordinate,
    };
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn jar(entry: &str, contents: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(entry, SimpleFileOptions::default())
            .expect("fixture JAR entry must start");
        writer
            .write_all(contents)
            .expect("fixture JAR entry must write");
        writer
            .finish()
            .expect("fixture JAR must finish")
            .into_inner()
    }

    fn artifact(path: PathBuf, bytes: &[u8], origin: ArtifactOrigin) -> ClasspathArtifact {
        fs::write(&path, bytes).expect("fixture artifact must be written");
        ClasspathArtifact {
            path,
            origin,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(bytes)),
            byte_length: bytes.len() as u64,
        }
    }

    fn classpath(root: PathBuf, artifacts: Vec<ClasspathArtifact>) -> ProjectClasspath {
        ProjectClasspath {
            project_root: root,
            artifacts,
            sources: Vec::<SourceRoot>::new(),
            unresolved: Vec::<UnresolvedCoordinate>::new(),
            fingerprint_sha256: "fixture-classpath".to_string(),
        }
    }

    #[test]
    fn preserves_classpath_precedence_and_reports_duplicate_class_identity() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let first_bytes = jar("com/example/Demo.class", &class);
        let second_bytes = jar("com/example/Demo.class", &class);
        let first = artifact(
            fixture.path().join("first.jar"),
            &first_bytes,
            ArtifactOrigin::JrubyRuntime,
        );
        let second = artifact(
            fixture.path().join("second.jar"),
            &second_bytes,
            ArtifactOrigin::Explicit,
        );
        let classpath = classpath(
            fixture.path().to_path_buf(),
            vec![first.clone(), second.clone()],
        );

        let catalog = build_project_java_catalog(&classpath, 17, ArchiveLimits::default())
            .expect("fixture Java catalog must build");
        assert_eq!(catalog.classes.len(), 1);
        assert_eq!(
            catalog.classes["com/example/Demo"].artifact_path,
            first.path
        );
        assert_eq!(
            catalog.duplicates,
            vec![DuplicateJavaClass {
                name: "com/example/Demo".to_string(),
                winner: first.path,
                shadowed: second.path,
            }]
        );
    }

    #[test]
    fn rejects_artifact_changed_after_classpath_discovery() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let bytes = jar("com/example/Demo.class", &class);
        let artifact = artifact(
            fixture.path().join("changed.jar"),
            &bytes,
            ArtifactOrigin::Explicit,
        );
        fs::write(&artifact.path, b"changed after discovery")
            .expect("fixture artifact must be changed");
        let classpath = classpath(fixture.path().to_path_buf(), vec![artifact.clone()]);
        assert_eq!(
            build_project_java_catalog(&classpath, 17, ArchiveLimits::default()),
            Err(JavaCatalogError::ArtifactFingerprintMismatch {
                path: artifact.path
            })
        );
    }
}
