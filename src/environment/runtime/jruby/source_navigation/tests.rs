use super::*;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{AnalysisEngine, ResolveMode, SourceFileInput};
use ruby_fast_lsp_jvm_metadata::{
    locate_java_source_declarations, parse_class, ClassLimits, JavaSourceLimits,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use zip::write::SimpleFileOptions;

const RICH_FIXTURE_CLASS_HEX: &str =
    include_str!("../../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex");
const RICH_FIXTURE_SOURCE: &str =
    include_str!("../../../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");

struct CountingReader<R> {
    inner: R,
    bytes_read: Arc<AtomicU64>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.bytes_read.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}

impl<R: Seek> Seek for CountingReader<R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

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

fn rich_declaration(root: &Path) -> JavaClassDeclaration {
    JavaClassDeclaration {
        class: parse_class(&decode_hex(RICH_FIXTURE_CLASS_HEX), ClassLimits::default())
            .expect("checked class fixture must parse")
            .into(),
        artifact_path: root.join("rich.jar"),
        artifact_fingerprint_sha256: "fixture-artifact".to_string(),
        entry_name: "fixtures/RichFixture.class".to_string(),
        release: None,
    }
}

fn source_root(path: PathBuf, origin: SourceOrigin) -> SourceRoot {
    let (fingerprint_sha256, file_identity) = if path.is_file() {
        let metadata = fs::metadata(&path).unwrap();
        (
            Some(format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))),
            Some(super::super::classpath::SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            }),
        )
    } else {
        (None, None)
    };
    SourceRoot {
        path,
        origin,
        fingerprint_sha256,
        file_identity,
    }
}

fn source_archive(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, source) in entries {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("source fixture entry must start");
        writer
            .write_all(source.as_bytes())
            .expect("source fixture entry must write");
    }
    writer
        .finish()
        .expect("source fixture archive must finish")
        .into_inner()
}

#[test]
fn archive_resolution_streams_only_the_selected_entry() {
    let source = RICH_FIXTURE_SOURCE;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("padding.bin", stored).unwrap();
    writer.write_all(&vec![0_u8; 8 * 1024 * 1024]).unwrap();
    writer
        .start_file("fixtures/RichFixture.java", stored)
        .unwrap();
    writer.write_all(source.as_bytes()).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let bytes_read = Arc::new(AtomicU64::new(0));
    let reader = CountingReader {
        inner: Cursor::new(bytes.clone()),
        bytes_read: Arc::clone(&bytes_read),
    };
    let root = SourceRoot {
        path: PathBuf::from("fixture-sources.jar"),
        origin: SourceOrigin::Attached,
        fingerprint_sha256: Some("fixture".to_string()),
        file_identity: None,
    };

    let mut archive = zip::ZipArchive::new(reader).expect("streaming archive fixture must parse");
    let resolved = source_from_archive(
        &rich_declaration(Path::new("/fixture")),
        &root,
        &mut archive,
        Path::new("fixtures/RichFixture.java"),
        JavaSourceResolutionLimits::default(),
    )
    .expect("streaming archive resolution must succeed")
    .expect("selected source entry must resolve");

    assert_eq!(resolved.0, source);
    invariant!(
        bytes_read.load(Ordering::Relaxed) < 1024 * 1024,
        what = "resolving one Java source entry read the complete source archive",
        why = "classpath discovery already verified the archive identity",
        fix = "keep ZipArchive backed by a seekable file and read only the selected entry",
    );
}

#[test]
fn resolves_project_source_before_archives_even_when_roots_are_unsorted() {
    let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
    let source = RICH_FIXTURE_SOURCE;
    let project_root = fixture.path().join("src/main/java");
    let project_source = project_root.join("fixtures/RichFixture.java");
    fs::create_dir_all(project_source.parent().unwrap()).unwrap();
    fs::write(&project_source, source).unwrap();
    let attached = fixture.path().join("rich-sources.jar");
    fs::write(
        &attached,
        source_archive(&[("fixtures/RichFixture.java", source)]),
    )
    .unwrap();
    let resolver = JavaSourceResolver::new(
        vec![
            source_root(attached, SourceOrigin::Attached),
            source_root(project_root, SourceOrigin::Project),
        ],
        fixture.path().join("cache"),
        JavaSourceResolutionLimits::default(),
    );

    let resolved = resolver
        .resolve(&rich_declaration(fixture.path()))
        .expect("source resolution must succeed")
        .expect("project source must resolve");

    assert_eq!(resolved.path, project_source.canonicalize().unwrap());
    assert_eq!(resolved.origin, SourceOrigin::Project);
    assert_eq!(resolved.content, source);
    assert_eq!(resolved.location.internal_name, "fixtures/RichFixture");
}

#[test]
fn verifies_and_materializes_attached_source_archive_outside_the_project() {
    let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
    let source = RICH_FIXTURE_SOURCE;
    let attached = fixture.path().join("rich-sources.jar");
    fs::write(
        &attached,
        source_archive(&[("fixtures/RichFixture.java", source)]),
    )
    .unwrap();
    let cache = fixture.path().join("user-cache");
    let resolver = JavaSourceResolver::new(
        vec![source_root(attached, SourceOrigin::Attached)],
        cache.clone(),
        JavaSourceResolutionLimits::default(),
    );

    let resolved = resolver
        .resolve(&rich_declaration(fixture.path()))
        .expect("source resolution must succeed")
        .expect("attached source must resolve");

    assert_eq!(resolved.origin, SourceOrigin::Attached);
    assert!(resolved.path.starts_with(&cache));
    assert!(resolved.path.ends_with("fixtures/RichFixture.java"));
    assert_eq!(fs::read_to_string(&resolved.path).unwrap(), source);
    assert_eq!(resolved.content, source);
}

#[test]
fn reuses_one_parsed_archive_for_repeated_source_resolution() {
    let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
    let source = RICH_FIXTURE_SOURCE;
    let attached = fixture.path().join("rich-sources.jar");
    fs::write(
        &attached,
        source_archive(&[("fixtures/RichFixture.java", source)]),
    )
    .unwrap();
    let resolver = JavaSourceResolver::new(
        vec![source_root(attached, SourceOrigin::Attached)],
        fixture.path().join("cache"),
        JavaSourceResolutionLimits::default(),
    );
    let declaration = rich_declaration(fixture.path());

    resolver
        .resolve(&declaration)
        .expect("first source resolution must succeed")
        .expect("first source resolution must find the class");
    let first_archive = resolver.roots[0]
        .archive
        .get()
        .expect("first source resolution must initialize the archive");

    resolver
        .resolve(&declaration)
        .expect("second source resolution must succeed")
        .expect("second source resolution must find the class");
    let second_archive = resolver.roots[0]
        .archive
        .get()
        .expect("second source resolution must retain the archive");

    invariant!(
        std::ptr::eq(first_archive, second_archive),
        what = "repeated Java source resolution replaced the parsed archive",
        why = "every replacement reparses the immutable central directory",
        fix = "retain one verified ZipArchive per prepared source root",
    );
}

#[test]
fn resolves_jdk_module_prefixed_source_and_rejects_ambiguous_matches() {
    let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
    let source = RICH_FIXTURE_SOURCE;
    let jdk_source = fixture.path().join("src.zip");
    fs::write(
        &jdk_source,
        source_archive(&[("java.base/fixtures/RichFixture.java", source)]),
    )
    .unwrap();
    let resolver = JavaSourceResolver::new(
        vec![source_root(jdk_source, SourceOrigin::Jdk)],
        fixture.path().join("cache"),
        JavaSourceResolutionLimits::default(),
    );
    assert_eq!(
        resolver
            .resolve(&rich_declaration(fixture.path()))
            .expect("module-prefixed JDK source must resolve")
            .expect("JDK source must be present")
            .origin,
        SourceOrigin::Jdk
    );

    let ambiguous = fixture.path().join("ambiguous-src.zip");
    fs::write(
        &ambiguous,
        source_archive(&[
            ("java.base/fixtures/RichFixture.java", source),
            ("other.module/fixtures/RichFixture.java", source),
        ]),
    )
    .unwrap();
    let resolver = JavaSourceResolver::new(
        vec![source_root(ambiguous.clone(), SourceOrigin::Jdk)],
        fixture.path().join("cache"),
        JavaSourceResolutionLimits::default(),
    );
    assert!(matches!(
        resolver.resolve(&rich_declaration(fixture.path())),
        Err(JavaSourceResolutionError::Ambiguous {
            class_name,
            source
        }) if class_name == "fixtures/RichFixture" && source == ambiguous
    ));
}

#[test]
fn rejects_source_archive_whose_discovered_fingerprint_changed() {
    let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
    let source = RICH_FIXTURE_SOURCE;
    let attached = fixture.path().join("rich-sources.jar");
    fs::write(
        &attached,
        source_archive(&[("fixtures/RichFixture.java", source)]),
    )
    .unwrap();
    let root = source_root(attached.clone(), SourceOrigin::Attached);
    fs::write(&attached, b"changed after discovery").unwrap();
    let resolver = JavaSourceResolver::new(
        vec![root],
        fixture.path().join("cache"),
        JavaSourceResolutionLimits::default(),
    );

    assert_eq!(
        resolver.resolve(&rich_declaration(fixture.path())),
        Err(JavaSourceResolutionError::FingerprintMismatch { path: attached })
    );
}

#[test]
fn projects_only_metadata_verified_java_source_locations_into_engine_facts() {
    let class = parse_class(&decode_hex(RICH_FIXTURE_CLASS_HEX), ClassLimits::default())
        .expect("checked class fixture must parse");
    let source = RICH_FIXTURE_SOURCE;
    let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
        .expect("checked source must parse")
        .expect("checked source must match the class");
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: PathBuf::from("/external/fixtures/RichFixture.java"),
        content: source.to_string(),
        kind: SourceKind::External,
    });
    engine.replace_facts(
        file_id,
        java_source_navigation_facts(&class, &location, file_id),
        ResolveMode::Immediate,
    );
    let combine = FullyQualifiedName::method(
        ["Java", "Fixtures", "RichFixture"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
        RubyMethod::new("combine").unwrap(),
    );
    let methods = engine.query().methods_for_fqn(&combine);
    assert_eq!(methods.len(), 1);
    assert_eq!(
        &source[usize::try_from(methods[0].name_range.start_byte).unwrap()
            ..usize::try_from(methods[0].name_range.end_byte).unwrap()],
        "combine"
    );
    assert_eq!(methods[0].range.file_id, file_id);
    let proxy = FullyQualifiedName::constant(
        ["Java", "Fixtures", "RichFixture"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        engine.symbol_facts_for(&proxy).len(),
        1,
        "Java implementation class declarations must use the canonical constant identity"
    );
    assert!(engine.query().diagnostic_facts_in_file(file_id).is_empty());
}

#[test]
fn supplemental_implementation_facts_do_not_duplicate_the_class_declaration() {
    let class = parse_class(&decode_hex(RICH_FIXTURE_CLASS_HEX), ClassLimits::default())
        .expect("checked class fixture must parse");
    let source = RICH_FIXTURE_SOURCE;
    let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
        .expect("checked source must parse")
        .expect("checked source must match the class");
    let facts =
        java_source_navigation_facts_with_declaration(&class, &location, SourceFileId(7), false);

    assert!(
            facts
                .symbols
                .iter()
                .all(|fact| !matches!(fact.kind, SymbolKind::Class | SymbolKind::Module)),
            "a decompiled member supplement must not compete with the preferred exact-source class declaration"
        );
    assert!(!facts.methods.is_empty());
}
