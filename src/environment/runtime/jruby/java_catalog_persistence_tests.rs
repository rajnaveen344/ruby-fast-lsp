//! Java artifact metadata stored in the persistent derived-product cache:
//! exact identity selection, corruption recovery, fresh-process loads, and
//! compatibility with products written in the schema-1 envelope.

use super::classpath::{ArtifactKind, ArtifactOrigin, ClasspathArtifact, SourceFileIdentity};
use super::java_catalog::{JavaArtifactProduct, JavaArtifactProductKey};
use crate::utils::persistent_cache::{
    PersistentDerivedProductCache, PersistentProduct, PersistentProductLookup,
    PersistentProductStat,
};
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

fn java_artifact(path: PathBuf) -> ClasspathArtifact {
    let digits = include_str!("../../../../crates/jvm-metadata/fixtures/minimal_class.hex")
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
    std::fs::write(&path, &bytes).unwrap();
    let file_identity = SourceFileIdentity {
        byte_length: bytes.len() as u64,
        modified: std::fs::metadata(&path).unwrap().modified().unwrap(),
    };
    ClasspathArtifact {
        path,
        origin: ArtifactOrigin::Explicit,
        kind: ArtifactKind::Jar,
        fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
        byte_length: bytes.len() as u64,
        file_identity,
    }
}

#[test]
fn fresh_java_artifact_cache_loads_exact_metadata_and_recovers_corruption() {
    let fixture = tempfile::tempdir().unwrap();
    let artifact = java_artifact(fixture.path().join("fixture.jar"));
    let limits = ArchiveLimits::default();
    let key = JavaArtifactProductKey::new(&artifact, 17, limits);
    let product = JavaArtifactProduct::build(&artifact, &key, limits).unwrap();
    let first_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = first_cache
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("a fresh Java artifact cache must reserve one producer");
    };
    reservation.publish(&product).unwrap();
    drop(first_cache);

    let second_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Hit(hit) = second_cache
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("a fresh cache instance must load Java artifact metadata");
    };
    assert_eq!(hit.cache_id(), key.cache_id());
    assert_eq!(
        second_cache
            .java_artifact_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(second_cache.summary().unwrap().entries, 1);

    let product_path = second_cache.product_path_for_tests::<JavaArtifactProduct>(&key);
    std::fs::write(&product_path, b"corrupt").unwrap();
    let recovering_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(rebuild) = recovering_cache
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("corrupt Java metadata must reserve one deterministic rebuild");
    };
    assert_eq!(
        recovering_cache
            .java_artifact_snapshot()
            .get(PersistentProductStat::Corruptions),
        1
    );
    rebuild.publish(&product).unwrap();
    assert!(matches!(
        recovering_cache
            .lookup_or_reserve::<JavaArtifactProduct>(&key)
            .unwrap(),
        PersistentProductLookup::Hit(_)
    ));
}

#[test]
fn changed_java_artifact_identity_never_selects_persisted_old_metadata() {
    let fixture = tempfile::tempdir().unwrap();
    let artifact = java_artifact(fixture.path().join("fixture.jar"));
    let limits = ArchiveLimits::default();
    let original_key = JavaArtifactProductKey::new(&artifact, 17, limits);
    let original_product = JavaArtifactProduct::build(&artifact, &original_key, limits).unwrap();
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = cache
        .lookup_or_reserve::<JavaArtifactProduct>(&original_key)
        .unwrap()
    else {
        panic!("the original Java artifact must reserve its publisher");
    };
    reservation.publish(&original_product).unwrap();
    assert!(matches!(
        cache
            .lookup_or_reserve::<JavaArtifactProduct>(&original_key)
            .unwrap(),
        PersistentProductLookup::Hit(_)
    ));

    let mut changed_artifact = artifact.clone();
    changed_artifact.fingerprint_sha256 =
        format!("{:x}", Sha256::digest(b"changed Java artifact bytes"));
    changed_artifact.byte_length += 1;
    changed_artifact.file_identity.byte_length += 1;
    let changed_key = JavaArtifactProductKey::new(&changed_artifact, 17, limits);
    assert_ne!(original_key.cache_id(), changed_key.cache_id());
    assert!(
        matches!(
            cache
                .lookup_or_reserve::<JavaArtifactProduct>(&changed_key)
                .unwrap(),
            PersistentProductLookup::Reservation(_)
        ),
        "changed Java bytes must reserve a new exact product instead of selecting old metadata"
    );
}

#[test]
fn fresh_process_loads_java_artifact_metadata() {
    const CHILD_ROOT: &str = "RUBY_FAST_LSP_JAVA_CACHE_CHILD_ROOT";
    const CHILD_ARTIFACT: &str = "RUBY_FAST_LSP_JAVA_CACHE_CHILD_ARTIFACT";
    if let (Some(root), Some(path)) = (
        std::env::var_os(CHILD_ROOT),
        std::env::var_os(CHILD_ARTIFACT),
    ) {
        let path = PathBuf::from(path);
        let bytes = std::fs::read(&path).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let artifact = ClasspathArtifact {
            path,
            origin: ArtifactOrigin::Explicit,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity: SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            },
        };
        let key = JavaArtifactProductKey::new(&artifact, 17, ArchiveLimits::default());
        let cache = PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
        assert!(matches!(
            cache
                .lookup_or_reserve::<JavaArtifactProduct>(&key)
                .unwrap(),
            PersistentProductLookup::Hit(_)
        ));
        println!("RUBY_FAST_LSP_JAVA_CACHE_CHILD=hit");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let artifact = java_artifact(fixture.path().join("fresh-process.jar"));
    let key = JavaArtifactProductKey::new(&artifact, 17, ArchiveLimits::default());
    let product = JavaArtifactProduct::build(&artifact, &key, ArchiveLimits::default()).unwrap();
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = cache
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("parent process must publish Java artifact metadata");
    };
    reservation.publish(&product).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "environment::runtime::jruby::java_catalog_persistence_tests::fresh_process_loads_java_artifact_metadata",
            "--nocapture",
        ])
        .env(CHILD_ROOT, fixture.path())
        .env(CHILD_ARTIFACT, &artifact.path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Java artifact cache child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("RUBY_FAST_LSP_JAVA_CACHE_CHILD=hit"));
}

#[test]
fn java_artifact_written_in_schema_one_layout_still_loads() {
    let fixture = tempfile::tempdir().unwrap();
    let artifact = java_artifact(fixture.path().join("fixture.jar"));
    let limits = ArchiveLimits::default();
    let key = JavaArtifactProductKey::new(&artifact, 17, limits);
    let product = JavaArtifactProduct::build(&artifact, &key, limits).unwrap();
    let cache_id = key.cache_id();
    let path = fixture
        .path()
        .join("derived-products/java-artifacts-v1")
        .join(&cache_id[..2])
        .join(format!("{cache_id}.rflsp-product"));
    let payload = product.encode().unwrap();
    let compressed = zstd::stream::encode_all(payload.as_slice(), 3).unwrap();
    let mut written = Vec::new();
    written.extend_from_slice(b"RFLSPJ01");
    written.extend_from_slice(&1u32.to_le_bytes());
    written.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    written.extend_from_slice(&(compressed.len() as u64).to_le_bytes());
    written.extend_from_slice(&Sha256::digest(&payload));
    written.extend_from_slice(&compressed);

    let publishing =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = publishing
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("a fresh cache must reserve the Java artifact product");
    };
    reservation.publish(&product).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        written,
        "publication must keep the schema-1 path, magic, and envelope bytes"
    );
    drop(publishing);

    std::fs::write(&path, &written).unwrap();
    let reading =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Hit(hit) = reading
        .lookup_or_reserve::<JavaArtifactProduct>(&key)
        .unwrap()
    else {
        panic!("Java metadata written in the schema-1 layout must still load");
    };
    assert_eq!(hit.cache_id(), cache_id);
}
