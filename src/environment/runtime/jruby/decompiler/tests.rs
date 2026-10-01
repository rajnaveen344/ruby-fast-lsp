use super::*;
use crate::environment::runtime::jruby::classpath::{
    ArtifactKind, ArtifactOrigin, ClasspathArtifact, ProjectClasspath, SourceFileIdentity,
};
use crate::environment::runtime::jruby::java_catalog::build_project_java_catalog;
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::fs;
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

fn fixture_jar(class: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("fixtures/RichFixture.class", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(class).unwrap();
    writer.finish().unwrap().into_inner()
}

fn fixture_declaration(root: &std::path::Path) -> JavaClassDeclaration {
    let class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
    ));
    let jar = fixture_jar(&class);
    let path = root.join("rich.jar");
    fs::write(&path, &jar).unwrap();
    let file_identity = SourceFileIdentity {
        byte_length: jar.len() as u64,
        modified: fs::metadata(&path).unwrap().modified().unwrap(),
    };
    let classpath = ProjectClasspath {
        project_root: root.to_path_buf(),
        artifacts: vec![ClasspathArtifact {
            path,
            origin: ArtifactOrigin::Explicit,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&jar)),
            byte_length: jar.len() as u64,
            file_identity,
        }],
        sources: Vec::new(),
        unresolved: Vec::new(),
        fingerprint_sha256: "fixture-classpath".to_string(),
    };
    build_project_java_catalog(&classpath, 17, ArchiveLimits::default())
        .unwrap()
        .classes
        .remove("fixtures/RichFixture")
        .unwrap()
}

fn java_executable() -> PathBuf {
    for candidate in [
        std::env::var_os("JAVA_HOME")
            .map(PathBuf::from)
            .map(|home| {
                home.join("bin")
                    .join(format!("java{}", std::env::consts::EXE_SUFFIX))
            }),
        Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
        Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("a real JDK java executable is required for the decompiler acceptance test");
}

fn bundled_asset() -> JavaDecompilerAsset {
    JavaDecompilerAsset {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("support/jruby/decompiler/cfr-0.152.jar"),
        version: CFR_VERSION.to_string(),
        fingerprint_sha256: CFR_SHA256.to_string(),
    }
}

#[test]
fn decompiles_only_the_selected_class_and_returns_verified_implementation_ranges() {
    let _decompiler_budget = crate::test::harness::isolate_decompiler_budget();
    let fixture = tempfile::tempdir().unwrap();
    let declaration = fixture_declaration(fixture.path());
    let cache = fixture.path().join("cache");
    let decompiler = JavaDecompiler::new(
        java_executable(),
        bundled_asset(),
        cache.clone(),
        JavaDecompilerLimits::default(),
    )
    .unwrap();

    let resolved = decompiler
        .decompile(&declaration)
        .expect("decompilation must succeed")
        .expect("decompiled source must match class metadata");

    assert_eq!(resolved.origin, SourceOrigin::Decompiled);
    assert!(resolved.path.starts_with(cache));
    assert!(resolved
        .content
        .contains("return List.of(prefix + values.length);"));
    let combine = resolved
        .location
        .methods
        .iter()
        .find(|method| method.name == "combine")
        .expect("decompiled method must have a verified metadata identity");
    assert_eq!(
        &resolved.content[combine.name_range.start as usize..combine.name_range.end as usize],
        "combine"
    );
    assert_eq!(
        decompiler.decompile(&declaration).unwrap().unwrap(),
        resolved,
        "a repeated request must reuse the deterministic verified cache"
    );
}

#[test]
fn rejects_a_bundled_decompiler_checksum_mismatch_before_execution() {
    let fixture = tempfile::tempdir().unwrap();
    let declaration = fixture_declaration(fixture.path());
    let mut asset = bundled_asset();
    asset.fingerprint_sha256 = "0".repeat(64);
    let decompiler = JavaDecompiler::new(
        java_executable(),
        asset.clone(),
        fixture.path().join("cache"),
        JavaDecompilerLimits::default(),
    )
    .unwrap();

    assert_eq!(
        decompiler.decompile(&declaration),
        Err(JavaDecompilerError::AssetFingerprintMismatch(asset.path))
    );
}

#[test]
fn compact_decompiler_cache_identity_retains_every_producer_input() {
    let inputs = ["asset", "java", "artifact", "options", "fixtures/Example"];
    let baseline = decompilation_cache_id(inputs);
    assert_eq!(
        baseline.len(),
        64,
        "cache directory must remain one SHA-256 component"
    );
    for index in 0..inputs.len() {
        let mut changed = inputs;
        changed[index] = "changed";
        assert_ne!(
            decompilation_cache_id(changed),
            baseline,
            "input {index} must invalidate cached source"
        );
    }
    assert_ne!(
        decompilation_cache_id(["ab", "c", "artifact", "options", "Owner"]),
        decompilation_cache_id(["a", "bc", "artifact", "options", "Owner"]),
        "component boundaries must be retained in the cache identity"
    );
}

#[test]
fn linux_resident_report_uses_rss_and_rejects_unverifiable_values() {
    assert_eq!(
        parse_linux_resident_bytes(
            "1000-2000 ---p 00000000 00:00 0 [rollup]\nRss: 1234 kB\nPss: 567 kB\n"
        ),
        Ok(1234 * 1024)
    );
    for report in [
        "Pss: 1 kB\n",
        "Rss: -1 kB\n",
        "Rss: unknown kB\n",
        "Rss: 10 MB\n",
        "Rss: 10 kB unexpected\n",
        "Rss: 18446744073709551615 kB\n",
    ] {
        assert!(parse_linux_resident_bytes(report).is_err(), "{report}");
    }
}

#[test]
fn default_limits_bound_total_decompiler_resident_memory() {
    assert_eq!(
            JavaDecompilerLimits::default().max_process_resident_bytes,
            256 * 1024 * 1024,
            "the JVM child must fit the same conservative 256 MiB resource claim used by JRuby indexing and interactive materialization"
        );
}

#[test]
fn decompiler_process_permits_reject_excess_and_recover_after_release() {
    let _decompiler_budget = crate::test::harness::isolate_decompiler_budget();
    let limit = JavaDecompilerLimits::default().max_parallel_processes;
    let mut permits = (0..limit)
        .map(|_| ProcessPermit::acquire(limit).expect("available slot must admit a process"))
        .collect::<Vec<_>>();
    assert!(matches!(
        ProcessPermit::acquire(limit),
        Err(JavaDecompilerError::ProcessLimit)
    ));

    drop(permits.pop().expect("the full budget must hold a permit"));
    let replacement = ProcessPermit::acquire(limit)
        .expect("releasing a permit must make its slot available again");
    assert!(matches!(
        ProcessPermit::acquire(limit),
        Err(JavaDecompilerError::ProcessLimit)
    ));
    drop(replacement);
    drop(permits);
    let _restored = (0..limit)
        .map(|_| ProcessPermit::acquire(limit).expect("all released slots must be reusable"))
        .collect::<Vec<_>>();
}

#[cfg(unix)]
#[test]
fn child_exceeding_resident_memory_limit_is_killed_and_reaped() {
    let mut child = Command::new("sleep")
        .arg("5")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the platform sleep command must launch");

    let result = wait_for_bounded_child(&mut child, Duration::from_secs(2), 1);

    assert!(
        matches!(
            result,
            Err(JavaDecompilerError::ProcessMemoryLimitExceeded {
                limit_bytes: 1,
                observed_bytes
            }) if observed_bytes > 1
        ),
        "the resident-memory monitor must report the measured overage: {result:?}"
    );
    assert!(
        child.try_wait().unwrap().is_some(),
        "a memory-limited child must be reaped before the error is returned"
    );
}
