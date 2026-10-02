//! Gem dependency products stored in the persistent derived-product cache:
//! exact identity selection, corruption recovery, cross-instance publication,
//! bounded cleanup, and compatibility with products written in the schema-1
//! envelope.

use super::dependency_product::{
    GemDependencyFileTemplate, GemDependencyManifest, GemDependencyProduct, GemDependencySource,
};
use crate::utils::persistent_cache::{
    PersistentDerivedProductCache, PersistentProduct, PersistentProductLookup,
    PersistentProductStat, RESCAN_PUBLICATION_INTERVAL,
};
use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphNodeFact, GraphNodeKind, RubyConstant, SourceFileId,
    SymbolFact, SymbolKind, TextRange,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, ProjectNeutralFileFactsTemplate};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

#[test]
fn fresh_cache_load_rebinds_exact_path_and_corruption_recovers() {
    let fixture = tempfile::tempdir().unwrap();
    let first_path = crate::test::harness::fixture_path("/projects/one/gems/widget/lib/widget.rb");
    let first_manifest = manifest(&first_path);
    let first_product = product(&first_manifest);

    let first_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = first_cache
        .lookup_or_reserve::<GemDependencyProduct>(&first_manifest)
        .unwrap()
    else {
        panic!("a fresh persistent cache must reserve one producer");
    };
    reservation.publish(&first_product).unwrap();
    drop(first_cache);

    let second_path = crate::test::harness::fixture_path("/projects/two/gems/widget/lib/widget.rb");
    let second_manifest = manifest(&second_path);
    let second_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Hit(second_product) = second_cache
        .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
        .unwrap()
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
        second_cache.product_path_for_tests::<GemDependencyProduct>(&second_manifest),
        b"corrupt",
    )
    .unwrap();
    let recovering_cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(rebuild) = recovering_cache
        .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
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
            .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
            .unwrap(),
        PersistentProductLookup::Hit(_)
    ));
}

#[test]
fn obsolete_gem_products_are_never_selected_across_semantic_input_changes() {
    let fixture = tempfile::tempdir().unwrap();
    let physical_path = Path::new("/projects/one/gems/widget/lib/widget.rb");
    let current = manifest(physical_path);
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 16, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = cache
        .lookup_or_reserve::<GemDependencyProduct>(&current)
        .unwrap()
    else {
        panic!("the current product must reserve its initial publisher");
    };
    reservation.publish(&product(&current)).unwrap();
    assert!(matches!(
        cache
            .lookup_or_reserve::<GemDependencyProduct>(&current)
            .unwrap(),
        PersistentProductLookup::Hit(_)
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
                    cache.lookup_or_reserve::<GemDependencyProduct>(&obsolete_identity).unwrap(),
                    PersistentProductLookup::Reservation(_)
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
fn in_flight_reservation_does_not_block_sibling_publication_cleanup() {
    let fixture = tempfile::tempdir().unwrap();
    let cache = PersistentDerivedProductCache::with_limits(
        fixture.path().to_path_buf(),
        4096,
        2 * 1024 * 1024 * 1024,
    );
    let held = unique_test_manifest(fixture.path(), "held");
    let PersistentProductLookup::Reservation(_held) = cache
        .lookup_or_reserve::<GemDependencyProduct>(&held)
        .unwrap()
    else {
        panic!("held identity must reserve while siblings publish");
    };

    let started = Instant::now();
    for index in 0..=RESCAN_PUBLICATION_INTERVAL {
        let item = unique_test_manifest(fixture.path(), &format!("sibling{index}"));
        let PersistentProductLookup::Reservation(reservation) = cache
            .lookup_or_reserve::<GemDependencyProduct>(&item)
            .unwrap()
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
fn cross_instance_lock_admits_one_publisher() {
    let fixture = tempfile::tempdir().unwrap();
    let manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
    let product = product(&manifest);
    let first =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = first
        .lookup_or_reserve::<GemDependencyProduct>(&manifest)
        .unwrap()
    else {
        panic!("first cache instance must own publication");
    };

    let second =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let second_thread = second.clone();
    let second_manifest = manifest.clone();
    let waiter = std::thread::spawn(move || {
        second_thread
            .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
            .unwrap()
    });
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert!(
        !waiter.is_finished(),
        "a second cache instance must wait for the exact ownership lock"
    );
    reservation.publish(&product).unwrap();

    assert!(matches!(
        waiter.join().unwrap(),
        PersistentProductLookup::Hit(_)
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
fn bounded_cleanup_and_clear_touch_only_owned_products() {
    let fixture = tempfile::tempdir().unwrap();
    let unrelated = fixture.path().join("bundler/cache/widget-1.0.0.gem");
    std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
    std::fs::write(&unrelated, b"package-manager-owned").unwrap();
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 1, 1024 * 1024);

    let first_manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
    let first_product = product(&first_manifest);
    let PersistentProductLookup::Reservation(first) = cache
        .lookup_or_reserve::<GemDependencyProduct>(&first_manifest)
        .unwrap()
    else {
        panic!("first bounded product must reserve");
    };
    first.publish(&first_product).unwrap();

    let second_manifest = manifest_with_provider(
        Path::new("/projects/one/gems/widget/lib/widget.rb"),
        Some("jruby-catalog"),
    );
    let second_product = product(&second_manifest);
    let PersistentProductLookup::Reservation(second) = cache
        .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
        .unwrap()
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
        cache
            .lookup_or_reserve::<GemDependencyProduct>(&second_manifest)
            .unwrap(),
        PersistentProductLookup::Hit(_)
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
            cache
                .lookup_or_reserve::<GemDependencyProduct>(&manifest(physical_path))
                .unwrap(),
            PersistentProductLookup::Hit(_)
        ));
        println!("RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD=hit");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let manifest = manifest(physical_path);
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = cache
        .lookup_or_reserve::<GemDependencyProduct>(&manifest)
        .unwrap()
    else {
        panic!("parent process must publish the product");
    };
    reservation.publish(&product(&manifest)).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "loader::cache::persistence_tests::fresh_process_loads_nonempty_semantic_seed_product",
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

/// Build a schema-1 envelope from literal format constants, independently of
/// the cache's own encoder: magic, schema, logical length, compressed length,
/// SHA-256 of the logical payload, then the zstd level-3 payload.
fn schema_one_envelope(magic: &[u8; 8], payload: &[u8]) -> Vec<u8> {
    let compressed = zstd::stream::encode_all(payload, 3).unwrap();
    let mut encoded = Vec::new();
    encoded.extend_from_slice(magic);
    encoded.extend_from_slice(&1u32.to_le_bytes());
    encoded.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    encoded.extend_from_slice(&(compressed.len() as u64).to_le_bytes());
    encoded.extend_from_slice(&Sha256::digest(payload));
    encoded.extend_from_slice(&compressed);
    encoded
}

#[test]
fn gem_product_written_in_schema_one_layout_still_loads() {
    let fixture = tempfile::tempdir().unwrap();
    let manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
    let product = product(&manifest);
    let cache_id = manifest.cache_id();
    let path = fixture
        .path()
        .join("derived-products/gem-products-v1")
        .join(&cache_id[..2])
        .join(format!("{cache_id}.rflsp-product"));
    let written = schema_one_envelope(b"RFLSPG01", &product.encode().unwrap());

    let publishing =
        PersistentDerivedProductCache::with_limits(fixture.path().to_path_buf(), 8, 1024 * 1024);
    let PersistentProductLookup::Reservation(reservation) = publishing
        .lookup_or_reserve::<GemDependencyProduct>(&manifest)
        .unwrap()
    else {
        panic!("a fresh cache must reserve the gem product");
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
        .lookup_or_reserve::<GemDependencyProduct>(&manifest)
        .unwrap()
    else {
        panic!("a product written in the schema-1 layout must still load");
    };
    assert_eq!(hit.cache_id(), cache_id);
}
