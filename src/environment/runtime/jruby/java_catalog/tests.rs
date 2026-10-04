//! Project catalog composition and process-cache retention.

use super::super::classpath::{
    ArtifactOrigin, ClasspathArtifact, ProjectClasspath, SourceRoot, UnresolvedCoordinate,
};
use super::*;
use crate::utils::persistent_cache::PersistentProduct;
use crate::utils::single_flight::SingleFlightStat;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::sync::Arc;
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

fn jar_with_marker(contents: &[u8], marker: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("com/example/Demo.class", SimpleFileOptions::default())
        .expect("fixture JAR class entry must start");
    writer
        .write_all(contents)
        .expect("fixture JAR class entry must write");
    writer
        .start_file(format!("META-INF/{marker}"), SimpleFileOptions::default())
        .expect("fixture JAR marker entry must start");
    writer
        .write_all(marker.as_bytes())
        .expect("fixture JAR marker entry must write");
    writer
        .finish()
        .expect("fixture JAR must finish")
        .into_inner()
}

fn artifact(path: PathBuf, bytes: &[u8], origin: ArtifactOrigin) -> ClasspathArtifact {
    fs::write(&path, bytes).expect("fixture artifact must be written");
    let file_identity = super::super::classpath::SourceFileIdentity {
        byte_length: bytes.len() as u64,
        modified: fs::metadata(&path).unwrap().modified().unwrap(),
    };
    ClasspathArtifact {
        path,
        origin,
        kind: ArtifactKind::Jar,
        fingerprint_sha256: format!("{:x}", Sha256::digest(bytes)),
        byte_length: bytes.len() as u64,
        file_identity,
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
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
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
    assert_eq!(catalog.class_count(), 1);
    assert_eq!(
        catalog
            .class("com/example/Demo")
            .expect("winning class must resolve")
            .artifact_path,
        first.path
    );
    assert_eq!(
        catalog.duplicates(),
        vec![DuplicateJavaClass {
            name: "com/example/Demo",
            winner: &first.path,
            shadowed: &second.path,
        }]
    );
}

#[test]
fn rejects_artifact_changed_after_classpath_discovery() {
    let fixture = tempfile::tempdir().expect("catalog fixture must be created");
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
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
        build_project_java_catalog(&classpath, 17, ArchiveLimits::default()).err(),
        Some(JavaCatalogError::ArtifactFingerprintMismatch {
            path: artifact.path
        })
    );
}

#[test]
fn persistent_artifact_product_rebinds_to_the_consumer_classpath_path() {
    let fixture = tempfile::tempdir().expect("catalog fixture must be created");
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    let bytes = jar("com/example/Demo.class", &class);
    let first = artifact(
        fixture.path().join("producer.jar"),
        &bytes,
        ArtifactOrigin::JrubyRuntime,
    );
    let limits = ArchiveLimits::default();
    let first_key = JavaArtifactProductKey::new(&first, 17, limits);
    let product = JavaArtifactProduct::build(&first, &first_key, limits)
        .expect("producer artifact product must build");
    let payload = product.encode().expect("artifact product must encode");

    let second = artifact(
        fixture.path().join("consumer.jar"),
        &bytes,
        ArtifactOrigin::Explicit,
    );
    let second_key = JavaArtifactProductKey::new(&second, 17, limits);
    assert_eq!(
        first_key.cache_id(),
        second_key.cache_id(),
        "artifact product identity must be independent of the consumer path and origin"
    );
    let decoded = JavaArtifactProduct::decode(&second_key, &payload)
        .expect("consumer must decode the immutable artifact product");
    let catalog = build_project_java_catalog_from_products(
        &classpath(fixture.path().to_path_buf(), vec![second.clone()]),
        &[decoded],
    )
    .expect("consumer catalog must compose");

    assert_eq!(
        catalog
            .class("com/example/Demo")
            .expect("consumer class must resolve")
            .artifact_path,
        second.path,
        "persistent metadata must bind to the exact consumer artifact path"
    );
}

#[test]
fn product_clones_share_class_metadata_but_rebind_project_paths() {
    let fixture = tempfile::tempdir().expect("catalog fixture must be created");
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    let bytes = jar("com/example/Demo.class", &class);
    let first = artifact(
        fixture.path().join("project-one.jar"),
        &bytes,
        ArtifactOrigin::JrubyRuntime,
    );
    let second = artifact(
        fixture.path().join("project-two.jar"),
        &bytes,
        ArtifactOrigin::Explicit,
    );
    let limits = ArchiveLimits::default();
    let key = JavaArtifactProductKey::new(&first, 17, limits);
    let product = JavaArtifactProduct::build(&first, &key, limits)
        .expect("shared artifact product must build");

    let first_catalog = build_project_java_catalog_from_products(
        &classpath(fixture.path().join("project-one"), vec![first.clone()]),
        std::slice::from_ref(&product),
    )
    .expect("first project catalog must compose");
    let second_catalog = build_project_java_catalog_from_products(
        &classpath(fixture.path().join("project-two"), vec![second.clone()]),
        &[product],
    )
    .expect("second project catalog must compose");
    let first_declaration = first_catalog
        .class("com/example/Demo")
        .expect("first project class must resolve");
    let second_declaration = second_catalog
        .class("com/example/Demo")
        .expect("second project class must resolve");

    assert_eq!(first_declaration.artifact_path, first.path);
    assert_eq!(second_declaration.artifact_path, second.path);
    assert!(
        Arc::ptr_eq(first_declaration.class, second_declaration.class),
        "isolated project declarations should share only immutable parsed class metadata"
    );
}

#[test]
fn projects_with_identical_artifact_products_share_catalog_content() {
    let fixture = tempfile::tempdir().expect("catalog fixture must be created");
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    let bytes = jar("com/example/Demo.class", &class);
    let other_bytes = jar_with_marker(&class, "other");
    let first = artifact(
        fixture.path().join("project-one.jar"),
        &bytes,
        ArtifactOrigin::JrubyRuntime,
    );
    let second = artifact(
        fixture.path().join("project-two.jar"),
        &bytes,
        ArtifactOrigin::Explicit,
    );
    let other = artifact(
        fixture.path().join("project-three.jar"),
        &other_bytes,
        ArtifactOrigin::Explicit,
    );
    let limits = ArchiveLimits::default();
    let cache = JavaArtifactProductCache::default();
    let compose = |root: &str, artifact: &ClasspathArtifact| {
        let key = JavaArtifactProductKey::new(artifact, 17, limits);
        let product = JavaArtifactProduct::build(artifact, &key, limits)
            .expect("fixture artifact product must build");
        let classpath = ProjectClasspath {
            fingerprint_sha256: format!("{root}-classpath"),
            ..classpath(fixture.path().join(root), vec![artifact.clone()])
        };
        let mut builder = ProjectJavaCatalogBuilder::new(&classpath);
        builder
            .push(&product)
            .expect("fixture product must compose");
        builder.finish_shared(&cache)
    };

    let first_catalog = compose("project-one", &first);
    let second_catalog = compose("project-two", &second);
    let other_catalog = compose("project-three", &other);

    assert!(
        first_catalog.shares_content_with(&second_catalog),
        "identical ordered artifact products must share one catalog content value"
    );
    assert!(
        !first_catalog.shares_content_with(&other_catalog),
        "different artifact content must not share catalog content"
    );
    assert_eq!(
        first_catalog.classpath_fingerprint(),
        "project-one-classpath"
    );
    assert_eq!(
        second_catalog.classpath_fingerprint(),
        "project-two-classpath"
    );
    let first_declaration = first_catalog
        .class("com/example/Demo")
        .expect("first project class must resolve");
    let second_declaration = second_catalog
        .class("com/example/Demo")
        .expect("second project class must resolve");
    assert_eq!(first_declaration.artifact_path, first.path);
    assert_eq!(
        second_declaration.artifact_path, second.path,
        "shared content must still report each project's own artifact path"
    );
    assert_eq!(second_declaration.entry_name(), "com/example/Demo.class");

    drop(first_catalog);
    drop(second_catalog);
    let rebuilt = compose("project-one", &first);
    assert_eq!(
        rebuilt
            .class("com/example/Demo")
            .map(|declaration| declaration.artifact_path),
        Some(first.path.as_path()),
        "content released by every project must be recomposed on demand"
    );
}

#[test]
fn process_cache_evicts_completed_artifact_products_to_both_bounds() {
    let fixture = tempfile::tempdir().expect("catalog fixture must be created");
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    let first_bytes = jar_with_marker(&class, "first");
    let second_bytes = jar_with_marker(&class, "second");
    let first = artifact(
        fixture.path().join("first.jar"),
        &first_bytes,
        ArtifactOrigin::Explicit,
    );
    let second = artifact(
        fixture.path().join("second.jar"),
        &second_bytes,
        ArtifactOrigin::Explicit,
    );
    let limits = ArchiveLimits::default();
    let first_key = JavaArtifactProductKey::new(&first, 17, limits);
    let second_key = JavaArtifactProductKey::new(&second, 17, limits);
    let cache = JavaArtifactProductCache::new(1, 1024 * 1024);

    for (artifact, key) in [(&first, first_key), (&second, second_key)] {
        let key_for_build = key.clone();
        cache
            .get_or_try_init(key, || {
                JavaArtifactProduct::build(artifact, &key_for_build, limits)
                    .map_err(|error| format!("{error:?}"))
            })
            .expect("bounded Java artifact product must build");
    }

    assert_eq!(cache.snapshot().get(SingleFlightStat::Entries), 1);
    assert_eq!(cache.snapshot().get(SingleFlightStat::Producers), 2);
    assert_eq!(cache.snapshot().get(SingleFlightStat::Evictions), 1);
    assert!(cache.retained_weight_bytes() > 0);
    assert!(cache.retained_weight_bytes() <= 1024 * 1024);

    let overweight_cache = JavaArtifactProductCache::new(1, 1);
    let key = JavaArtifactProductKey::new(&first, 17, limits);
    let key_for_build = key.clone();
    let product = overweight_cache
        .get_or_try_init(key, || {
            JavaArtifactProduct::build(&first, &key_for_build, limits)
                .map_err(|error| format!("{error:?}"))
        })
        .expect("an overweight product must still serve its current consumer");
    assert!(product.estimated_weight_bytes() > 1);
    assert_eq!(
        overweight_cache.snapshot().get(SingleFlightStat::Entries),
        0
    );
    assert_eq!(
        overweight_cache.snapshot().get(SingleFlightStat::Evictions),
        1
    );
    assert_eq!(overweight_cache.retained_weight_bytes(), 0);
}
