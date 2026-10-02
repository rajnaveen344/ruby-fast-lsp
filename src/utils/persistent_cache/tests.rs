use super::envelope::decode_envelope;
use super::PersistentProductStat;
use super::{
    CompiledWasmProductKey, PersistentCompiledWasmLookup, PersistentDerivedProductCache,
    PersistentProductKind, COMPILED_WASM_PRODUCT_MAGIC, ENVELOPE_HEADER_BYTES, ENVELOPE_SCHEMA,
    MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES,
};
use std::path::PathBuf;

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
            "utils::persistent_cache::tests::fresh_process_loads_compiled_wasm_artifact",
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
