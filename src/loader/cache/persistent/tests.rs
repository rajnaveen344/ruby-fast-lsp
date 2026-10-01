use super::envelope::decode_envelope;
use super::PersistentProductStat;
use super::{
    CompiledWasmProductKey, PersistentCompiledWasmLookup, PersistentDerivedProductCache,
    PersistentGemProductLookup, PersistentJavaArtifactLookup, PersistentProductKind,
    COMPILED_WASM_PRODUCT_MAGIC, ENVELOPE_HEADER_BYTES, ENVELOPE_SCHEMA,
    MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES, RESCAN_PUBLICATION_INTERVAL,
};
use crate::environment::runtime::jruby::java_catalog::{
    JavaArtifactProduct, JavaArtifactProductKey,
};
use crate::loader::cache::dependency_product::{
    GemDependencyFileTemplate, GemDependencyManifest, GemDependencyProduct, GemDependencySource,
};
use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphNodeFact, GraphNodeKind, RubyConstant, SourceFileId,
    SymbolFact, SymbolKind, TextRange,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, ProjectNeutralFileFactsTemplate};
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zip::write::SimpleFileOptions;

use crate::environment::runtime::jruby::classpath::{
    ArtifactKind, ArtifactOrigin, ClasspathArtifact, SourceFileIdentity,
};

fn source_with_content(physical_path: &Path, content: &str) -> GemDependencySource {
    GemDependencySource::new(
        0,
        "gems/widget/1.0.0/ruby/registry/0/widget.rb".to_string(),
        crate::test::harness::fixture_path(physical_path),
        content.to_string(),
        "widget",
        "1.0.0",
    )
    .unwrap()
}

fn manifest(physical_path: &Path) -> GemDependencyManifest {
    manifest_with_provider(physical_path, None)
}

fn manifest_with_provider(
    physical_path: &Path,
    runtime_provider_fingerprint: Option<&str>,
) -> GemDependencyManifest {
    manifest_with_inputs(
        physical_path,
        "class Widget; end",
        runtime_provider_fingerprint,
        &["widget:1.0.0:ruby:registry".to_string()],
        "class CacheSeed; end",
    )
}

fn manifest_with_inputs(
    physical_path: &Path,
    content: &str,
    runtime_provider_fingerprint: Option<&str>,
    closure_identities: &[String],
    seed_content: &str,
) -> GemDependencyManifest {
    let mut seed_engine = AnalysisEngine::new();
    let seed_file = seed_engine.register_file(ruby_analysis::engine::SourceFileInput {
        path: crate::test::harness::fixture_path("/stubs/cache_seed.rb"),
        content: seed_content.to_string(),
        kind: ruby_analysis::core::SourceKind::Stub,
    });
    let seed_range = TextRange::new(
        seed_file,
        0,
        u32::try_from(seed_content.len()).expect("test seed must fit a source range"),
    );
    let seed_constant = if seed_content.contains("ChangedCacheSeed") {
        "ChangedCacheSeed"
    } else {
        "CacheSeed"
    };
    let seed_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new(seed_constant).unwrap()]);
    seed_engine.replace_facts(
        seed_file,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                seed_fqn.clone(),
                SymbolKind::Class,
                seed_range,
            )],
            graph_nodes: vec![GraphNodeFact::new(
                seed_fqn,
                GraphNodeKind::Class,
                seed_range,
            )],
            ..FileAnalysis::default()
        },
        ruby_analysis::engine::ResolveMode::Deferred,
    );
    GemDependencyManifest::new(
        seed_engine.semantic_context_fingerprint(),
        runtime_provider_fingerprint,
        closure_identities,
        vec![source_with_content(physical_path, content)],
    )
    .unwrap()
}

fn product(manifest: &GemDependencyManifest) -> GemDependencyProduct {
    let file_id = SourceFileId(91);
    let range = TextRange::new(file_id, 0, 17);
    let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let facts = ProjectNeutralFileFactsTemplate::try_new(
        file_id,
        FileAnalysis {
            symbols: vec![SymbolFact::new(fqn.clone(), SymbolKind::Class, range)],
            graph_nodes: vec![GraphNodeFact::new(fqn, GraphNodeKind::Class, range)],
            ..FileAnalysis::default()
        },
    )
    .unwrap();
    GemDependencyProduct::new(
        manifest,
        vec![GemDependencyFileTemplate::new(
            manifest.sources()[0].logical_path.clone(),
            manifest.sources()[0].content_sha256,
            facts,
        )],
    )
    .unwrap()
}

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
fn fresh_cache_load_rebinds_exact_path_and_corruption_recovers() {
    let fixture = tempfile::tempdir().unwrap();
    let first_path = crate::test::harness::fixture_path("/projects/one/gems/widget/lib/widget.rb");
    let first_manifest = manifest(&first_path);
    let first_product = product(&first_manifest);

    let first_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentGemProductLookup::Reservation(reservation) =
        first_cache.lookup_or_reserve(&first_manifest).unwrap()
    else {
        panic!("a fresh persistent cache must reserve one producer");
    };
    reservation.publish(&first_product).unwrap();
    drop(first_cache);

    let second_path = crate::test::harness::fixture_path("/projects/two/gems/widget/lib/widget.rb");
    let second_manifest = manifest(&second_path);
    let second_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentGemProductLookup::Hit(second_product) =
        second_cache.lookup_or_reserve(&second_manifest).unwrap()
    else {
        panic!("a fresh cache instance must load the published product");
    };
    let mut second_engine = AnalysisEngine::new();
    second_product
        .bind_into(&second_manifest, &mut second_engine)
        .unwrap();
    let parts = [RubyConstant::new("Widget").unwrap()];
    let definition = AnalysisQuery::new(&second_engine).constant_definition_ranges(&parts, &[])[0];
    assert_eq!(
        second_engine.file(definition.file_id).unwrap().path,
        second_path
    );
    assert_eq!(
        second_cache
            .gem_product_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(
        second_cache
            .gem_product_snapshot()
            .get(PersistentProductStat::Producers),
        0
    );

    std::fs::write(
        second_cache.product_path_for_tests(&second_manifest),
        b"corrupt",
    )
    .unwrap();
    let recovering_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentGemProductLookup::Reservation(rebuild) = recovering_cache
        .lookup_or_reserve(&second_manifest)
        .unwrap()
    else {
        panic!("a corrupt product must reserve one deterministic rebuild");
    };
    assert_eq!(
        recovering_cache
            .gem_product_snapshot()
            .get(PersistentProductStat::Corruptions),
        1
    );
    rebuild.publish(&first_product).unwrap();
    assert!(matches!(
        recovering_cache
            .lookup_or_reserve(&second_manifest)
            .unwrap(),
        PersistentGemProductLookup::Hit(_)
    ));
}

#[test]
fn obsolete_gem_products_are_never_selected_across_semantic_input_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let physical_path = Path::new("/projects/one/gems/widget/lib/widget.rb");
    let current = manifest(physical_path);
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 16, 1024 * 1024);
    let PersistentGemProductLookup::Reservation(reservation) =
        cache.lookup_or_reserve(&current).unwrap()
    else {
        panic!("the current product must reserve its initial publisher");
    };
    reservation.publish(&product(&current)).unwrap();
    assert!(matches!(
        cache.lookup_or_reserve(&current).unwrap(),
        PersistentGemProductLookup::Hit(_)
    ));

    let changed_source = manifest_with_inputs(
        physical_path,
        "class Widget; def changed; end; end",
        None,
        &["widget:1.0.0:ruby:registry".to_string()],
        "class CacheSeed; end",
    );
    let changed_lock_closure = manifest_with_inputs(
        physical_path,
        "class Widget; end",
        None,
        &[
            "widget:1.0.0:ruby:registry".to_string(),
            "support:2.0.0:ruby:registry".to_string(),
        ],
        "class CacheSeed; end",
    );
    let changed_core_or_runtime_seed = manifest_with_inputs(
        physical_path,
        "class Widget; end",
        None,
        &["widget:1.0.0:ruby:registry".to_string()],
        "class ChangedCacheSeed; end",
    );
    let changed_jruby_classpath = manifest_with_inputs(
        physical_path,
        "class Widget; end",
        Some("jruby-classpath-b"),
        &["widget:1.0.0:ruby:registry".to_string()],
        "class CacheSeed; end",
    );

    for obsolete_identity in [
        changed_source,
        changed_lock_closure,
        changed_core_or_runtime_seed,
        changed_jruby_classpath,
    ] {
        assert!(
                matches!(
                    cache.lookup_or_reserve(&obsolete_identity).unwrap(),
                    PersistentGemProductLookup::Reservation(_)
                ),
                "a changed source, lock closure, core/runtime semantic seed, or classpath must never select the old product"
            );
    }
    assert_eq!(
        cache
            .gem_product_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(
        cache.summary().unwrap().entries,
        1,
        "unpublished replacement identities must not mutate the valid current product"
    );
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
    let PersistentJavaArtifactLookup::Reservation(reservation) =
        first_cache.lookup_java_artifact_or_reserve(&key).unwrap()
    else {
        panic!("a fresh Java artifact cache must reserve one producer");
    };
    reservation.publish(&product).unwrap();
    drop(first_cache);

    let second_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentJavaArtifactLookup::Hit(hit) =
        second_cache.lookup_java_artifact_or_reserve(&key).unwrap()
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

    let product_path =
        second_cache.product_path(PersistentProductKind::JavaArtifact, key.cache_id());
    std::fs::write(&product_path, b"corrupt").unwrap();
    let recovering_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentJavaArtifactLookup::Reservation(rebuild) = recovering_cache
        .lookup_java_artifact_or_reserve(&key)
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
            .lookup_java_artifact_or_reserve(&key)
            .unwrap(),
        PersistentJavaArtifactLookup::Hit(_)
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
    let PersistentJavaArtifactLookup::Reservation(reservation) = cache
        .lookup_java_artifact_or_reserve(&original_key)
        .unwrap()
    else {
        panic!("the original Java artifact must reserve its publisher");
    };
    reservation.publish(&original_product).unwrap();
    assert!(matches!(
        cache
            .lookup_java_artifact_or_reserve(&original_key)
            .unwrap(),
        PersistentJavaArtifactLookup::Hit(_)
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
            cache.lookup_java_artifact_or_reserve(&changed_key).unwrap(),
            PersistentJavaArtifactLookup::Reservation(_)
        ),
        "changed Java bytes must reserve a new exact product instead of selecting old metadata"
    );
}

#[test]
fn compiled_wasm_envelope_rejects_oversized_logical_payload_before_decompression() {
    let mut encoded = Vec::with_capacity(ENVELOPE_HEADER_BYTES);
    encoded.extend_from_slice(COMPILED_WASM_PRODUCT_MAGIC);
    encoded.extend_from_slice(&ENVELOPE_SCHEMA.to_le_bytes());
    encoded.extend_from_slice(&(MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES + 1).to_le_bytes());
    encoded.extend_from_slice(&0u64.to_le_bytes());
    encoded.extend_from_slice(&[0; 32]);

    let error = decode_envelope(
        COMPILED_WASM_PRODUCT_MAGIC,
        MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES,
        &encoded,
    )
    .expect_err("oversized logical Wasm payload must fail before allocation/decompression");

    assert!(error.to_string().contains("maximum is 67108864"));
}

#[test]
fn compiled_wasm_cache_validates_source_compiler_payload_and_corruption() {
    let fixture = tempfile::tempdir().unwrap();
    let source = b"\0asm exact extension bytes";
    let key = CompiledWasmProductKey::new(source, 17);
    let artifact = b"byte-exact wasmtime serialized module".to_vec();
    let first =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentCompiledWasmLookup::Reservation(reservation) =
        first.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("a fresh compiled-Wasm identity must reserve one producer");
    };
    reservation.publish(&key, &artifact).unwrap();
    drop(first);

    let second =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentCompiledWasmLookup::Hit(hit) =
        second.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("a fresh cache instance must load the exact compiled Wasm artifact");
    };
    assert_eq!(hit.as_slice(), artifact);
    assert_eq!(
        second
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );

    let changed_source = CompiledWasmProductKey::new(b"\0asm changed extension bytes", 17);
    assert!(matches!(
        second
            .lookup_compiled_wasm_or_reserve(&changed_source)
            .unwrap(),
        PersistentCompiledWasmLookup::Reservation(_)
    ));
    let changed_compiler = CompiledWasmProductKey::new(source, 18);
    assert!(matches!(
        second
            .lookup_compiled_wasm_or_reserve(&changed_compiler)
            .unwrap(),
        PersistentCompiledWasmLookup::Reservation(_)
    ));

    let product_path = second.product_path(PersistentProductKind::CompiledWasm, key.cache_id());
    std::fs::write(&product_path, b"corrupt").unwrap();
    let recovering =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentCompiledWasmLookup::Reservation(rebuild) =
        recovering.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("a corrupt compiled Wasm artifact must reserve one deterministic rebuild");
    };
    assert_eq!(
        recovering
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Corruptions),
        1
    );
    rebuild.publish(&key, &artifact).unwrap();
}

#[test]
fn in_flight_reservation_does_not_block_sibling_publication_cleanup() {
    let fixture = tempfile::tempdir().unwrap();
    let cache = PersistentDerivedProductCache::with_limits(
        fixture.path().to_path_buf(),
        4096,
        2 * 1024 * 1024 * 1024,
    );
    let held = unique_test_manifest(fixture.path(), "held");
    let PersistentGemProductLookup::Reservation(_held) = cache.lookup_or_reserve(&held).unwrap()
    else {
        panic!("held identity must reserve while siblings publish");
    };

    let started = Instant::now();
    for index in 0..=RESCAN_PUBLICATION_INTERVAL {
        let item = unique_test_manifest(fixture.path(), &format!("sibling{index}"));
        let PersistentGemProductLookup::Reservation(reservation) =
            cache.lookup_or_reserve(&item).unwrap()
        else {
            panic!("sibling identity must reserve");
        };
        reservation.publish(&product(&item)).unwrap();
    }
    assert!(
            started.elapsed() < Duration::from_secs(30),
            "sibling publication must not wait for the 180s exclusive maintenance lock while a reservation holds the shared lease"
        );
}

fn unique_test_manifest(root: &Path, name: &str) -> GemDependencyManifest {
    let path = root.join(name).join("lib/g.rb");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = format!("class {name}; end");
    std::fs::write(&path, &content).unwrap();
    manifest_with_inputs(
        &path,
        &content,
        None,
        &[format!("{name}:1.0.0:ruby:registry")],
        "class CacheSeed; end",
    )
}

#[test]
fn contended_cache_lock_is_pending_until_its_owner_releases_it() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("cache.lock");
    let owner = super::locks::open_private_lock_file(&path).unwrap();
    let contender = super::locks::open_private_lock_file(&path).unwrap();
    assert!(super::locks::try_acquire_lock(&owner, true).unwrap());
    assert!(!super::locks::try_acquire_lock(&contender, true).unwrap());
    assert!(!super::locks::try_acquire_lock(&contender, false).unwrap());
    drop(owner);
    assert!(super::locks::try_acquire_lock(&contender, true).unwrap());
}

#[test]
fn cross_instance_lock_admits_one_publisher() {
    let fixture = tempfile::tempdir().unwrap();
    let manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
    let product = product(&manifest);
    let first =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentGemProductLookup::Reservation(reservation) =
        first.lookup_or_reserve(&manifest).unwrap()
    else {
        panic!("first cache instance must own publication");
    };

    let second =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let second_thread = second.clone();
    let second_manifest = manifest.clone();
    let waiter =
        std::thread::spawn(move || second_thread.lookup_or_reserve(&second_manifest).unwrap());
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert!(
        !waiter.is_finished(),
        "a second cache instance must wait for the exact ownership lock"
    );
    reservation.publish(&product).unwrap();

    assert!(matches!(
        waiter.join().unwrap(),
        PersistentGemProductLookup::Hit(_)
    ));
    assert_eq!(
        first
            .gem_product_snapshot()
            .get(PersistentProductStat::Producers),
        1
    );
    assert_eq!(
        second
            .gem_product_snapshot()
            .get(PersistentProductStat::Producers),
        0
    );
    assert_eq!(
        second
            .gem_product_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(
        second
            .gem_product_snapshot()
            .get(PersistentProductStat::LockWaits),
        1
    );
}

#[test]
fn cross_instance_compiled_wasm_lock_admits_one_publisher() {
    let fixture = tempfile::tempdir().unwrap();
    let key = CompiledWasmProductKey::new(b"\0asm shared extension", 44);
    let artifact = b"shared serialized module".to_vec();
    let first =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentCompiledWasmLookup::Reservation(reservation) =
        first.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("first cache instance must own compiled Wasm publication");
    };

    let second =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let second_thread = second.clone();
    let second_key = key.clone();
    let waiter = std::thread::spawn(move || {
        second_thread
            .lookup_compiled_wasm_or_reserve(&second_key)
            .unwrap()
    });
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert!(
        !waiter.is_finished(),
        "a second compiled Wasm requester must wait for the exact ownership lock"
    );
    reservation.publish(&key, &artifact).unwrap();

    let PersistentCompiledWasmLookup::Hit(hit) = waiter.join().unwrap() else {
        panic!("the compiled Wasm waiter must reuse the first publisher's artifact");
    };
    assert_eq!(hit.as_slice(), artifact);
    assert_eq!(
        first
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Producers),
        1
    );
    assert_eq!(
        second
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Producers),
        0
    );
    assert_eq!(
        second
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(
        second
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::LockWaits),
        1
    );
}

#[test]
fn bounded_cleanup_and_clear_touch_only_owned_products() {
    let fixture = tempfile::tempdir().unwrap();
    let unrelated = fixture.path().join("bundler/cache/widget-1.0.0.gem");
    std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
    std::fs::write(&unrelated, b"package-manager-owned").unwrap();
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 1, 1024 * 1024);

    let first_manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
    let first_product = product(&first_manifest);
    let PersistentGemProductLookup::Reservation(first) =
        cache.lookup_or_reserve(&first_manifest).unwrap()
    else {
        panic!("first bounded product must reserve");
    };
    first.publish(&first_product).unwrap();

    let second_manifest = manifest_with_provider(
        Path::new("/projects/one/gems/widget/lib/widget.rb"),
        Some("jruby-catalog"),
    );
    let second_product = product(&second_manifest);
    let PersistentGemProductLookup::Reservation(second) =
        cache.lookup_or_reserve(&second_manifest).unwrap()
    else {
        panic!("second bounded product must reserve");
    };
    second.publish(&second_product).unwrap();
    let summary = cache.summary().unwrap();
    assert_eq!(summary.entries, 1);
    assert!(summary.bytes > 0);
    assert_eq!(
        cache
            .gem_product_snapshot()
            .get(PersistentProductStat::Evictions),
        1
    );
    assert!(matches!(
        cache.lookup_or_reserve(&second_manifest).unwrap(),
        PersistentGemProductLookup::Hit(_)
    ));

    let cleared = cache.clear().unwrap();
    assert_eq!(cleared.entries, 1);
    assert_eq!(cache.summary().unwrap().entries, 0);
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"package-manager-owned");
}

#[test]
fn fresh_process_loads_nonempty_semantic_seed_product() {
    const CHILD_ROOT: &str = "RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD_ROOT";
    let physical_path = Path::new("/projects/one/gems/widget/lib/widget.rb");
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        for index in 0..512 {
            let _ = RubyConstant::new(&format!("PersistentCacheNoise{index}")).unwrap();
        }
        let cache = PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
        assert!(matches!(
            cache.lookup_or_reserve(&manifest(physical_path)).unwrap(),
            PersistentGemProductLookup::Hit(_)
        ));
        println!("RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD=hit");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let manifest = manifest(physical_path);
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentGemProductLookup::Reservation(reservation) =
        cache.lookup_or_reserve(&manifest).unwrap()
    else {
        panic!("parent process must publish the product");
    };
    reservation.publish(&product(&manifest)).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "loader::cache::persistent::tests::fresh_process_loads_nonempty_semantic_seed_product",
            "--nocapture",
        ])
        .env(CHILD_ROOT, fixture.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "persistent-cache child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD=hit"));
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
            cache.lookup_java_artifact_or_reserve(&key).unwrap(),
            PersistentJavaArtifactLookup::Hit(_)
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
    let PersistentJavaArtifactLookup::Reservation(reservation) =
        cache.lookup_java_artifact_or_reserve(&key).unwrap()
    else {
        panic!("parent process must publish Java artifact metadata");
    };
    reservation.publish(&product).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "loader::cache::persistent::tests::fresh_process_loads_java_artifact_metadata",
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
fn fresh_process_loads_compiled_wasm_artifact() {
    const CHILD_ROOT: &str = "RUBY_FAST_LSP_WASM_CACHE_CHILD_ROOT";
    let source = b"\0asm fresh-process extension source";
    let artifact = b"fresh-process serialized Wasmtime module";
    let key = CompiledWasmProductKey::new(source, 91);
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let cache = PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
        let PersistentCompiledWasmLookup::Hit(hit) =
            cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("fresh child process must load the compiled Wasm artifact");
        };
        assert_eq!(hit.as_slice(), artifact);
        println!("RUBY_FAST_LSP_WASM_CACHE_CHILD=hit");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentCompiledWasmLookup::Reservation(reservation) =
        cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("parent process must publish the compiled Wasm artifact");
    };
    reservation.publish(&key, artifact).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "loader::cache::persistent::tests::fresh_process_loads_compiled_wasm_artifact",
            "--nocapture",
        ])
        .env(CHILD_ROOT, fixture.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "compiled Wasm cache child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("RUBY_FAST_LSP_WASM_CACHE_CHILD=hit"));
}
