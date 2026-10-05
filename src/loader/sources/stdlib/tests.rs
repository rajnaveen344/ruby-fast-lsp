use super::runtime_paths::RuntimeStdlibPathKey;
use super::*;
use parking_lot::RwLock;
use ruby_analysis::core::MethodVisibility;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeKind, MethodParamKind, NamespaceKind, RubyConstant, RubyMethod,
    RubyType,
};
use ruby_analysis::engine::Project;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn bundled_jruby_core_seed_is_cross_process_stable() {
    fn fingerprint() -> String {
        let extension_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/vscode/vsix");
        let mut indexer = IndexerStdlib::new(
            FileProcessor::new(),
            Some(RubyVersion::new_with_implementation(
                2,
                5,
                RuntimeImplementation::Jruby,
            )),
        );
        indexer.set_extension_path(extension_root);
        let engine = Arc::new(RwLock::new(Project::new()));
        indexer
            .index_core_stubs_blocking(engine.clone())
            .expect("bundled JRuby core stubs must index");
        let fingerprint = engine
            .read()
            .view()
            .semantic_context_fingerprint()
            .stable_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        fingerprint
    }

    const CHILD_ENV: &str = "RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        for index in 0..512 {
            let _ = RubyConstant::new(&format!("JrubyCoreFingerprintNoise{index}")).unwrap();
        }
        println!("RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT={}", fingerprint());
        return;
    }

    let expected = fingerprint();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "loader::sources::stdlib::tests::bundled_jruby_core_seed_is_cross_process_stable",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "JRuby core fingerprint child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let child = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("RUBY_FAST_LSP_JRUBY_CORE_FINGERPRINT="))
        .unwrap()
        .to_string();
    assert_eq!(child, expected);
}

#[tokio::test]
async fn bundled_ruby_25_signatures_match_observed_runtime_arities() {
    let extension_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("editors/vscode/vsix");
    let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(2, 5)));
    indexer.set_extension_path(extension_root);
    let engine = Arc::new(RwLock::new(Project::new()));
    indexer
        .index_core_stubs(engine.clone())
        .await
        .expect("bundled Ruby 2.5 core stubs must index");

    let query_guard = engine.read();
    let query = query_guard.view();
    let string = RubyConstant::new("String").expect("String must be a valid constant");
    let concat = FullyQualifiedName::method(
        vec![string],
        RubyMethod::new("concat").expect("concat must be a valid method"),
    );
    assert!(
        query.method_facts_for(&concat).iter().any(|fact| {
            fact.owner.namespace_kind() == Some(NamespaceKind::Instance)
                && fact
                    .param_facts
                    .iter()
                    .map(|param| param.kind)
                    .eq([MethodParamKind::Rest])
        }),
        "Ruby 2.5 String#concat must accept the runtime's zero-or-more positional shape"
    );

    let big_decimal = RubyConstant::new("BigDecimal").expect("BigDecimal must be a valid constant");
    let constructor = FullyQualifiedName::method(
        vec![big_decimal],
        RubyMethod::new("new").expect("new must be a valid method"),
    );
    assert!(
        query.method_facts_for(&constructor).iter().any(|fact| {
            fact.owner.namespace_kind() == Some(NamespaceKind::Singleton)
                && fact
                    .param_facts
                    .iter()
                    .map(|param| param.kind)
                    .eq([MethodParamKind::Required, MethodParamKind::Optional])
        }),
        "Ruby 2.5 BigDecimal.new must accept the runtime's one-or-two positional shape"
    );
}

#[test]
fn every_supported_jruby_series_has_a_parseable_explicit_overlay() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs");
    let common = fs::read_to_string(root.join("common/runtime.rb"))
        .expect("shared JRuby runtime overlay must exist");
    assert!(
        ruby_prism::parse(common.as_bytes())
            .errors()
            .next()
            .is_none(),
        "shared JRuby runtime overlay must parse"
    );
    for series in ruby_fast_lsp_jruby_support::JrubySeries::SUPPORTED {
        let compatibility = series.ruby_compatibility();
        assert_eq!(
            jruby_series_for_compatibility((
                u8::try_from(compatibility.major).unwrap(),
                u8::try_from(compatibility.minor).unwrap()
            )),
            Some(series.overlay_name())
        );
        let path = root.join(series.overlay_name()).join("runtime.rb");
        let source = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "supported {} overlay is missing at {}: {error}",
                series.label(),
                path.display()
            )
        });
        assert!(
            ruby_prism::parse(source.as_bytes())
                .errors()
                .next()
                .is_none(),
            "{} overlay must parse",
            series.label()
        );
    }
}

#[tokio::test]
async fn every_supported_jruby_series_composes_its_exact_runtime_overlay() {
    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs");
    for series in ruby_fast_lsp_jruby_support::JrubySeries::SUPPORTED {
        let compatibility = series.ruby_compatibility();
        let major = u8::try_from(compatibility.major).unwrap();
        let minor = u8::try_from(compatibility.minor).unwrap();
        let extension = TempDir::new().unwrap();
        let core = extension
            .path()
            .join("stubs")
            .join(format!("rubystubs{major}{minor}"));
        let common = extension.path().join("jruby-stubs/common");
        let selected = extension
            .path()
            .join("jruby-stubs")
            .join(series.overlay_name());
        fs::create_dir_all(&core).unwrap();
        fs::create_dir_all(&common).unwrap();
        fs::create_dir_all(&selected).unwrap();
        fs::write(core.join("object.rb"), "class Object\nend\n").unwrap();
        fs::copy(
            repository_root.join("common/runtime.rb"),
            common.join("runtime.rb"),
        )
        .unwrap();
        fs::copy(
            repository_root
                .join(series.overlay_name())
                .join("runtime.rb"),
            selected.join("runtime.rb"),
        )
        .unwrap();

        let mut indexer = IndexerStdlib::new(
            FileProcessor::new(),
            Some(RubyVersion::new_with_implementation(
                major,
                minor,
                RuntimeImplementation::Jruby,
            )),
        );
        indexer.set_extension_path(extension.path().to_path_buf());
        let engine = Arc::new(RwLock::new(Project::new()));
        indexer.index_core_stubs(engine.clone()).await.unwrap();

        let java_import = FullyQualifiedName::method(
            vec![RubyConstant::new("Object").unwrap()],
            RubyMethod::new("java_import").unwrap(),
        );
        let jruby_version =
            FullyQualifiedName::constant(vec![RubyConstant::new("JRUBY_VERSION").unwrap()]);
        let engine = engine.read();
        assert!(
            !engine.view().method_facts_for(&java_import).is_empty(),
            "{} must compose the shared JRuby java_import contract",
            series.label()
        );
        assert!(
            !engine.view().symbol_facts_for(&jruby_version).is_empty(),
            "{} must compose JRUBY_VERSION",
            series.label()
        );
        assert!(
            engine
                .view()
                .file_id(&selected.join("runtime.rb"))
                .is_some(),
            "{} must index its exact selected overlay file",
            series.label()
        );
        assert!(
            engine
                .view()
                .files()
                .filter(|file| file.path.ends_with("jruby-stubs/common/runtime.rb"))
                .count()
                == 1,
            "{} must compose the common overlay exactly once",
            series.label()
        );
    }
}

#[tokio::test]
async fn bundled_stub_navigation_retains_source_positions() {
    let extension = TempDir::new().unwrap();
    let stubs = extension.path().join("stubs/rubystubs30");
    fs::create_dir_all(&stubs).unwrap();
    let path = stubs.join("thread.rb");
    fs::write(&path, "# 😀\nclass Thread\nend\n").unwrap();
    let mut indexer = IndexerStdlib::new(FileProcessor::new(), None);
    indexer.set_extension_path(extension.path().to_path_buf());
    let engine = Arc::new(RwLock::new(Project::new()));
    indexer.index_core_stubs(engine.clone()).await.unwrap();
    let engine = engine.read();
    let ranges = engine
        .view()
        .constant_definition_ranges(&[RubyConstant::new("Thread").unwrap()], &[]);
    assert_eq!(ranges.len(), 1);
    let range = ranges[0];
    let file = engine.view().file(range.file_id).unwrap();
    assert_eq!(*file.path, *path);
    assert_eq!(
        file.byte_offset_to_line_character(range.start_byte),
        Some((1, 0)),
        "bundled declarations need their original source index for public navigation"
    );
    assert_eq!(
        file.byte_offset_to_line_character(range.end_byte),
        Some((2, 3)),
        "the complete declaration range must convert even after non-ASCII comments"
    );
    let locations =
        crate::features::cursor::analysis_location::locations_for_ranges(&engine.view(), ranges);
    assert_eq!(locations.len(), 1);
    assert_eq!(
        locations[0].range,
        tower_lsp::lsp_types::Range::new(
            tower_lsp::lsp_types::Position::new(1, 0),
            tower_lsp::lsp_types::Position::new(2, 3),
        )
    );
}

#[tokio::test]
async fn unknown_runtime_still_loads_default_core_stubs() {
    let extension = TempDir::new().expect("test extension directory must be created");
    let stubs = extension.path().join("stubs").join("rubystubs30");
    fs::create_dir_all(&stubs).expect("test stub directory must be created");
    fs::write(
        stubs.join("thread.rb"),
        "class Thread\n  def self.new\n  end\nend\n",
    )
    .expect("Thread stub must be written");

    let mut indexer = IndexerStdlib::new(FileProcessor::new(), None);
    indexer.set_extension_path(extension.path().to_path_buf());
    let engine = Arc::new(RwLock::new(Project::new()));

    indexer
        .index_core_stubs(engine.clone())
        .await
        .expect("bundled core stubs must remain usable without a detected runtime");

    let thread = FullyQualifiedName::namespace(vec![
        RubyConstant::new("Thread").expect("Thread must be a valid Ruby constant")
    ]);
    assert!(
        !engine.read().view().symbol_facts_for(&thread).is_empty(),
        "Thread must resolve from default bundled core stubs when runtime detection fails"
    );

    let argv = FullyQualifiedName::constant(vec![
        RubyConstant::new("ARGV").expect("ARGV must be a valid Ruby constant")
    ]);
    {
        let engine = engine.read();
        let query = engine.view();
        assert!(
            !query.symbol_facts_for(&argv).is_empty(),
            "ARGV must resolve from embedded core RBS when runtime detection fails"
        );
        assert_eq!(
            query.constant_value_type(&argv),
            Some(RubyType::array_of(RubyType::string())),
            "ARGV must retain its proven Array[String] type from embedded core RBS"
        );
    }

    let project = extension.path().join("project.rb");
    let project_uri =
        Url::from_file_path(&project).expect("temporary project path must convert to a file URI");
    let source = "ARGV.first.upcase\n";
    indexer
        .file_processor()
        .collect_file_facts_as_deferred_resolution_in_engine(
            &project_uri,
            source,
            engine.clone(),
            ruby_analysis::core::SourceKind::Project,
        )
        .expect("project source using ARGV must index");
    engine.write().resolve();

    let engine = engine.read();
    let file_id = engine
        .view()
        .file_id(&project)
        .expect("project source must remain registered");
    let query = engine.view();
    assert_eq!(
        query.expression_type_at(file_id, 14),
        Some(RubyType::string()),
        "ARGV.first.upcase must preserve the proven generic String type through the chain; reason={:?}",
        query.expression_unknown_reason_at(file_id, 14)
    );
    assert!(
        query.diagnostic_facts_in_file(file_id).is_empty(),
        "a fully proven ARGV method chain must not emit semantic diagnostics: {:?}",
        query.diagnostic_facts_in_file(file_id)
    );
}

#[test]
fn runtime_stdlib_discovery_without_an_exact_runtime_does_not_use_path() {
    let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));

    indexer
        .discover_stdlib_paths()
        .expect("missing exact runtime must be a supported stub-only state");

    assert!(
        indexer.stdlib_paths.is_empty(),
        "runtime stdlib discovery must not borrow whichever Ruby happens to be on the server PATH: {:?}",
        indexer.stdlib_paths
    );
}

#[test]
fn runtime_stdlib_path_key_changes_with_the_runtime_executable() {
    let fixture = TempDir::new().expect("runtime fixture directory must be created");
    let executable = fixture.path().join("ruby");
    fs::write(&executable, b"runtime-v1").expect("runtime fixture must be written");
    let before =
        RuntimeStdlibPathKey::new(&executable, None).expect("runtime key must be constructed");

    fs::write(&executable, b"runtime-v2-with-different-length")
        .expect("runtime fixture must be replaced");
    let after = RuntimeStdlibPathKey::new(&executable, None)
        .expect("replacement runtime key must be constructed");

    assert_ne!(
        before, after,
        "replacing a runtime in place must reserve a new immutable stdlib-path product identity"
    );
}

#[test]
fn runtime_stdlib_cannot_replace_bundled_stub_ownership() {
    let fixture = TempDir::new().expect("stdlib fixture directory must be created");
    let path = fixture.path().join("runtime_probe.rb");
    fs::write(&path, "class RuntimeProbe\nend\n").expect("stdlib fixture must be written");

    let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
    let engine = Arc::new(RwLock::new(Project::new()));
    indexer
        .index_stub_files_deterministically(std::slice::from_ref(&path), engine.clone())
        .expect("stub fixture must index");
    indexer.stdlib_paths.push(fixture.path().to_path_buf());
    indexer.add_required_module("runtime_probe".to_string());

    indexer
        .index_required_modules_blocking_with_resolution(engine.clone(), false)
        .expect("runtime stdlib collection must succeed");

    let engine = engine.read();
    let file_id = engine
        .view()
        .file_id(&path)
        .expect("stub fixture must retain a registered file");
    assert_eq!(
        engine
            .view()
            .file(file_id)
            .expect("stub fixture must exist")
            .kind,
        ruby_analysis::core::SourceKind::Stub,
        "runtime stdlib discovery must never reclassify bundled language semantics"
    );
}

#[cfg(unix)]
#[test]
fn runtime_stdlib_discovery_uses_the_exact_selected_executable() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TempDir::new().expect("runtime fixture directory must be created");
    let runtime_root = fixture.path().join("exact-runtime");
    let runtime_bin = runtime_root.join("bin");
    let runtime_stdlib = runtime_root.join("lib/ruby/stdlib");
    fs::create_dir_all(&runtime_bin).expect("runtime bin directory must be created");
    fs::create_dir_all(&runtime_stdlib).expect("runtime stdlib directory must be created");
    let executable = runtime_bin.join("ruby");
    fs::write(
        &executable,
        "#!/bin/sh\nruntime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nprintf '%s\\0' \"$runtime_root/lib/ruby/stdlib\"\n",
    )
    .expect("fake exact runtime must be written");
    let mut permissions = fs::metadata(&executable)
        .expect("fake exact runtime metadata must exist")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("fake exact runtime must be executable");

    let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
    indexer.set_selected_runtime(executable, None);
    indexer
        .discover_stdlib_paths()
        .expect("exact runtime stdlib discovery must succeed");

    assert_eq!(
        indexer.stdlib_paths,
        vec![dunce::canonicalize(runtime_stdlib).expect("runtime stdlib must canonicalize")],
        "stdlib discovery must use only the exact selected runtime's load path"
    );
}

#[test]
fn runtime_stdlib_deferred_collection_leaves_resolution_to_the_coordinator() {
    let fixture = TempDir::new().expect("stdlib fixture directory must be created");
    let child_path = fixture.path().join("runtime_probe").join("root.rb");
    let base_dir = fixture.path().join("runtime_probe").join("root");
    let base_path = base_dir.join("base.rb");
    fs::create_dir_all(&base_dir).expect("nested stdlib fixture must be created");
    fs::write(&child_path, "class RuntimeChild < RuntimeBase\nend\n")
        .expect("stdlib child fixture must be written");
    fs::write(&base_path, "class RuntimeBase\nend\n").expect("stdlib base fixture must be written");

    let mut indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(3, 0)));
    indexer.stdlib_paths.push(fixture.path().to_path_buf());
    indexer.add_required_module("runtime_probe/root".to_string());
    let engine = Arc::new(RwLock::new(Project::new()));

    indexer
        .index_required_modules_blocking_with_resolution(engine.clone(), false)
        .expect("deferred stdlib collection must succeed");

    let runtime_base =
        RubyConstant::new("RuntimeBase").expect("RuntimeBase must be a valid Ruby constant");
    assert!(
        engine
            .read()
            .view()
            .unresolved_graph_edges()
            .iter()
            .any(|edge| edge.target_parts == vec![runtime_base]),
        "deferred stdlib collection must leave cross-file inheritance for the coordinator"
    );

    engine.write().resolve();
    assert!(
        engine
            .read()
            .view()
            .unresolved_graph_edges()
            .iter()
            .all(|edge| edge.target_parts != vec![runtime_base]),
        "the coordinator's one final resolution must connect the deferred stdlib graph edge"
    );
    let runtime_child = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("RuntimeChild").expect("RuntimeChild must be a valid Ruby constant")],
        NamespaceKind::Instance,
    );
    let runtime_base_fqn =
        FullyQualifiedName::namespace_with_kind(vec![runtime_base], NamespaceKind::Instance);
    assert!(
        engine
            .read()
            .view()
            .graph_edges_from(&runtime_child)
            .iter()
            .any(|edge| edge.kind == GraphEdgeKind::Superclass && edge.target == runtime_base_fqn),
        "the coordinator's one final resolution must materialize stdlib inheritance"
    );
}

#[tokio::test]
async fn jruby_9_2_loads_jruby_overlay_without_exposing_it_to_mri() {
    let extension = TempDir::new().expect("test extension directory must be created");
    let stubs = extension.path().join("stubs").join("rubystubs25");
    let jruby_overlay = extension.path().join("jruby-stubs").join("9.2");
    let jruby_common = extension.path().join("jruby-stubs").join("common");
    fs::create_dir_all(&stubs).expect("MRI stub directory must be created");
    fs::create_dir_all(&jruby_overlay).expect("JRuby overlay directory must be created");
    fs::create_dir_all(&jruby_common).expect("JRuby common directory must be created");
    fs::write(stubs.join("object.rb"), "class Object\nend\n").expect("Object stub must be written");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs/9.2/runtime.rb"),
        jruby_overlay.join("runtime.rb"),
    )
    .expect("repository JRuby 9.2 overlay must be copied into the isolated test extension");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("support/jruby/stubs/common/runtime.rb"),
        jruby_common.join("runtime.rb"),
    )
    .expect("repository JRuby common overlay must be copied into the isolated test extension");
    fs::write(
        stubs.join("process.rb"),
        "module Process\n  def self.fork\n  end\nend\n",
    )
    .expect("Process baseline stub must be written");
    fs::write(
        stubs.join("object_space.rb"),
        "module ObjectSpace\n  def self.dump(object)\n  end\nend\n",
    )
    .expect("ObjectSpace baseline stub must be written");

    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("Object").expect("Object must be a valid Ruby constant")],
        RubyMethod::new("java_import").expect("java_import must be a valid Ruby method"),
    );

    let mut jruby_indexer = IndexerStdlib::new(
        FileProcessor::new(),
        Some(RubyVersion::new_with_implementation(
            2,
            5,
            RuntimeImplementation::Jruby,
        )),
    );
    jruby_indexer.set_extension_path(extension.path().to_path_buf());
    let jruby_engine = Arc::new(RwLock::new(Project::new()));
    jruby_indexer
        .index_core_stubs(jruby_engine.clone())
        .await
        .expect("JRuby core and overlay stubs must index");
    assert!(
        !jruby_engine
            .read()
            .view()
            .method_facts_for(&method)
            .is_empty(),
        "JRuby 9.2 must expose Object#java_import from its implementation overlay"
    );
    let required_instance_methods = [
        ("Object", "java_import", MethodVisibility::Private),
        ("Object", "java_kind_of?", MethodVisibility::Public),
        ("Module", "java_alias", MethodVisibility::Private),
        ("Module", "include_package", MethodVisibility::Private),
        ("Kernel", "java_package", MethodVisibility::Public),
        ("Kernel", "to_java", MethodVisibility::Public),
        ("Kernel", "java_signature", MethodVisibility::Public),
        ("Kernel", "java_implements", MethodVisibility::Public),
        ("JavaProxy", "java_send", MethodVisibility::Public),
        ("JavaProxy", "java_method", MethodVisibility::Public),
        ("JavaProxyMethods", "java_class", MethodVisibility::Public),
        ("JavaProxyMethods", "java_object", MethodVisibility::Public),
        ("JavaProxyMethods", "synchronized", MethodVisibility::Public),
        ("Class", "java_class", MethodVisibility::Public),
        ("String", "to_java_bytes", MethodVisibility::Public),
    ];
    let jruby_engine_guard = jruby_engine.read();
    let query = jruby_engine_guard.view();
    for (owner_name, method_name, visibility) in required_instance_methods {
        let owner_part =
            RubyConstant::new(owner_name).expect("test owner must be a valid Ruby constant");
        let owner = FullyQualifiedName::namespace(vec![owner_part]);
        let method_fqn = FullyQualifiedName::method(
            vec![owner_part],
            RubyMethod::new(method_name).expect("test method must be a valid Ruby method"),
        );
        assert!(
            query.method_facts_for(&method_fqn).iter().any(|fact| {
                fact.owner == owner && fact.visibility == visibility
            }),
            "JRuby 9.2 overlay must declare {owner_name}#{method_name} with {visibility:?} visibility"
        );
    }
    for constant_name in [
        "JRUBY_VERSION",
        "JRUBY_REVISION",
        "Java",
        "JavaUtilities",
        "JavaProxyMethods",
        "JavaProxy",
        "ConcreteJavaProxy",
        "ArrayJavaProxy",
    ] {
        let constant =
            RubyConstant::new(constant_name).expect("test constant must be a valid Ruby constant");
        let namespace = FullyQualifiedName::namespace(vec![constant]);
        let value = FullyQualifiedName::constant(vec![constant]);
        assert!(
            !query.symbol_facts_for(&namespace).is_empty()
                || !query.symbol_facts_for(&value).is_empty(),
            "JRuby 9.2 overlay must declare runtime constant {constant_name}"
        );
    }
    let process = RubyConstant::new("Process").expect("Process must be a valid Ruby constant");
    let fork = RubyMethod::new("fork").expect("fork must be a valid Ruby method");
    let effective_fork_facts = jruby_engine_guard.view().method_facts_matching_owner_name(
        &FullyQualifiedName::singleton_namespace(vec![process]),
        &fork,
    );
    assert_eq!(
        effective_fork_facts.len(),
        1,
        "the JRuby overlay must replace the compatible baseline declaration instead of making Process.fork ambiguous: {effective_fork_facts:?}"
    );
    assert!(
        matches!(
            effective_fork_facts[0].availability,
            ruby_analysis::core::MethodAvailability::Unavailable { .. }
        ),
        "Process.fork must remain known but explicitly unavailable under JRuby 9.2"
    );
    let object_space =
        RubyConstant::new("ObjectSpace").expect("ObjectSpace must be a valid Ruby constant");
    let dump = RubyMethod::new("dump").expect("dump must be a valid Ruby method");
    assert!(
        jruby_engine_guard
            .view()
            .method_facts_matching_owner_name(
                &FullyQualifiedName::singleton_namespace(vec![object_space]),
                &dump,
            )
            .is_empty(),
        "JRuby 9.2's absent ObjectSpace.dump marker must mask the MRI 2.5 baseline"
    );
    drop(jruby_engine_guard);

    let mut mri_indexer = IndexerStdlib::new(FileProcessor::new(), Some(RubyVersion::new(2, 5)));
    mri_indexer.set_extension_path(extension.path().to_path_buf());
    let mri_engine = Arc::new(RwLock::new(Project::new()));
    mri_indexer
        .index_core_stubs(mri_engine.clone())
        .await
        .expect("MRI core stubs must index");
    assert!(
        mri_engine
            .read()
            .view()
            .method_facts_for(&method)
            .is_empty(),
        "MRI must not receive JRuby-only methods"
    );
}
