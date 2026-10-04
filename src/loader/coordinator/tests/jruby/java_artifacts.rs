//! Cached Java artifact metadata, classpath winner order, and JDK release features.

use super::*;
use crate::utils::persistent_cache::PersistentProductStat;
use crate::utils::single_flight::SingleFlightStat;

#[test]
fn cached_java_artifact_metadata_is_reused_without_cross_project_path_leakage() {
    let fixture = TempDir::new().unwrap();
    let digits = include_str!("../../../../../crates/jvm-metadata/fixtures/minimal_class.hex")
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let class = digits
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("com/example/Demo.class", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&class).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let artifact = |path: PathBuf| {
        fs::write(&path, &bytes).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        classpath::ClasspathArtifact {
            path,
            origin: classpath::ArtifactOrigin::Explicit,
            kind: classpath::ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity: classpath::SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            },
        }
    };
    let first_artifact = artifact(fixture.path().join("project-one.jar"));
    let second_artifact = artifact(fixture.path().join("project-two.jar"));
    let classpath = |root: PathBuf, artifact: ClasspathArtifact| classpath::ProjectClasspath {
        project_root: root,
        artifacts: vec![artifact],
        sources: Vec::new(),
        unresolved: Vec::new(),
        fingerprint_sha256: "fixture-classpath".to_string(),
    };
    let first_classpath = classpath(fixture.path().join("project-one"), first_artifact.clone());
    let second_classpath = classpath(fixture.path().join("project-two"), second_artifact.clone());
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().join("cache"), 8, 1024 * 1024);
    let process_cache = java_catalog::JavaArtifactProductCache::new(8, 1024 * 1024);

    let first = build_cached_project_java_catalog(
        &first_classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();
    let second = build_cached_project_java_catalog(
        &second_classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();

    assert_eq!(
        cache
            .java_artifact_snapshot()
            .get(PersistentProductStat::Producers),
        1
    );
    assert_eq!(
        cache
            .java_artifact_snapshot()
            .get(PersistentProductStat::Hits),
        0
    );
    assert_eq!(process_cache.snapshot().get(SingleFlightStat::Lookups), 2);
    assert_eq!(process_cache.snapshot().get(SingleFlightStat::Producers), 1);
    assert_eq!(process_cache.snapshot().get(SingleFlightStat::Hits), 1);
    assert_eq!(process_cache.snapshot().get(SingleFlightStat::Entries), 1);
    assert!(process_cache.retained_weight_bytes() > 0);
    assert!(process_cache.retained_weight_bytes() <= 1024 * 1024);
    let first_demo = first.class("com/example/Demo").unwrap();
    let second_demo = second.class("com/example/Demo").unwrap();
    assert_eq!(first_demo.artifact_path, first_artifact.path);
    assert_eq!(second_demo.artifact_path, second_artifact.path);
    assert!(Arc::ptr_eq(first_demo.class, second_demo.class));
}

#[test]
fn parallel_cached_java_products_preserve_classpath_winner_order() {
    let fixture = TempDir::new().unwrap();
    let digits = include_str!("../../../../../crates/jvm-metadata/fixtures/minimal_class.hex")
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let class = digits
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let artifact = |name: &str, marker: &str| {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("com/example/Demo.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&class).unwrap();
        writer
            .start_file(format!("META-INF/{marker}"), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(marker.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let path = fixture.path().join(name);
        fs::write(&path, &bytes).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        ClasspathArtifact {
            path,
            origin: classpath::ArtifactOrigin::Explicit,
            kind: classpath::ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity: classpath::SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            },
        }
    };
    let winner = artifact("winner.jar", "winner");
    let shadowed = artifact("shadowed.jar", "shadowed");
    let classpath = classpath::ProjectClasspath {
        project_root: fixture.path().join("project"),
        artifacts: vec![winner.clone(), shadowed.clone()],
        sources: Vec::new(),
        unresolved: Vec::new(),
        fingerprint_sha256: "ordered-fixture-classpath".to_string(),
    };
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().join("cache"), 8, 1024 * 1024);
    let process_cache = JavaArtifactProductCache::new(8, 1024 * 1024);

    let catalog = build_cached_project_java_catalog(
        &classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();

    assert_eq!(
        catalog.class("com/example/Demo").unwrap().artifact_path,
        winner.path
    );
    assert_eq!(
        catalog.duplicates(),
        vec![java_catalog::DuplicateJavaClass {
            name: "com/example/Demo",
            winner: &winner.path,
            shadowed: &shadowed.path,
        }]
    );
    assert_eq!(
        cache
            .java_artifact_snapshot()
            .get(PersistentProductStat::Producers),
        2
    );
}

#[test]
fn reads_modern_and_legacy_jdk_release_features_without_guessing() {
    let modern = TempDir::new().unwrap();
    fs::write(
        modern.path().join("release"),
        "JAVA_VERSION=\"17.0.12\"\nIMPLEMENTOR=\"fixture\"\n",
    )
    .unwrap();
    assert_eq!(read_jdk_feature(modern.path()).unwrap(), 17);

    let legacy = TempDir::new().unwrap();
    fs::write(
        legacy.path().join("release"),
        "JAVA_VERSION=\"1.8.0_442\"\n",
    )
    .unwrap();
    assert_eq!(read_jdk_feature(legacy.path()).unwrap(), 8);

    let malformed = TempDir::new().unwrap();
    fs::write(
        malformed.path().join("release"),
        "IMPLEMENTOR=\"fixture\"\n",
    )
    .unwrap();
    assert!(read_jdk_feature(malformed.path()).is_err());
}
