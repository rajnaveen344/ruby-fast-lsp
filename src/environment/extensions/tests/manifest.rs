use super::*;

#[test]
fn manifest_targets_parse_semantic_namespaces_and_methods() {
    let manifest: ExtensionManifest = toml::from_str(
        r#"
id = "semantic"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["describe"]

[[indexing.namespaces]]
owner = ["RSpec"]
declaration_kind = "module"

[[indexing.targets]]
owner = ["RSpec"]
owner_kind = "singleton"
method = "describe"
frame = true
"#,
    )
    .expect("test manifest must parse");

    let namespaces =
        parse_manifest_namespace_targets(&manifest).expect("test semantic namespaces must parse");
    assert_eq!(
        namespaces,
        vec![ExtensionNamespaceTarget {
            owner: vec![RubyConstant::new("RSpec").expect("test constant is valid")],
            declaration_kind: GraphNodeKind::Module,
        }],
        "semantic namespace parsing must retain the explicit declaration kind"
    );
    let targets =
        parse_manifest_method_targets(&manifest).expect("test semantic targets must parse");
    assert_eq!(targets.len(), 1);
    invariant_eq!(
        targets[0],
        ExtensionMethodTarget {
            owner: vec![RubyConstant::new("RSpec").expect("test constant is valid")],
            owner_kind: NamespaceKind::Singleton,
            method: RubyMethod::new("describe").expect("test method is valid"),
            frame: true,
        },
        what = "extension semantic target parsing changed",
        why = "extension dispatch must be gated by resolved method target",
        fix = "preserve owner, owner_kind, method, and frame fields",
    );
}

/// The seed the registry hands over for `engine`, or `None` when the engine
/// already holds the seed for this applicability.
fn produced_seed(
    registry: &ExtensionRegistryHandle,
    engine: &Arc<RwLock<ruby_analysis::engine::Project>>,
    project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
) -> Option<ExtensionSemanticSeed> {
    let mut produced = None;
    let identity: Arc<dyn Send + Sync> = engine.clone();
    registry.with_semantic_seed(&identity, project, |seed| produced = Some(seed));
    produced
}

fn seeds_rspec_describe(seed: &ExtensionSemanticSeed) -> bool {
    seed.analysis(SourceFileId(0)).methods.iter().any(|fact| {
        matches!(
            &fact.fqn,
            FullyQualifiedName::Method(namespace, method)
                if namespace.as_slice() == [RubyConstant::new("RSpec").expect("RSpec is a valid constant")]
                    && method.as_str() == "describe"
        )
    })
}

#[test]
fn semantic_seed_facts_are_produced_for_every_applicable_isolated_project_engine() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&config);
    let first = Arc::new(RwLock::new(ruby_analysis::engine::Project::new()));
    let second = Arc::new(RwLock::new(ruby_analysis::engine::Project::new()));
    let ineligible = Arc::new(RwLock::new(ruby_analysis::engine::Project::new()));
    let project = |project_uri: &str, version: &str| ruby_fast_lsp_extension_api::ProjectContext {
        project_uri: project_uri.to_string(),
        source_uri: format!("{project_uri}/spec/example_spec.rb"),
        source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
        workspace_trusted: true,
        ruby_version: Some("3.3.0".to_string()),
        lockfile_present: true,
        locked_gems_complete: true,
        locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
            name: "rspec-core".to_string(),
            version: version.to_string(),
            source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
        }],
    };
    let first_project = project("file:///umbrella/first", "3.12.0");
    let second_project = project("file:///umbrella/second", "3.13.5");
    let ineligible_project = project("file:///umbrella/future", "4.0.0");

    for (engine, context) in [(&first, &first_project), (&second, &second_project)] {
        let seed = produced_seed(&registry, engine, Some(context))
            .expect("an unseeded applicable engine must receive a seed");
        assert!(
            seeds_rspec_describe(&seed),
            "every applicable isolated project engine must receive the RSpec.describe semantic target"
        );
        assert_eq!(
            produced_seed(&registry, engine, Some(context)),
            None,
            "an engine that already holds the seed for this applicability must not be seeded again"
        );
    }
    let ineligible_seed = produced_seed(&registry, &ineligible, Some(&ineligible_project))
        .expect("an unseeded engine always receives its seed, even an empty one");
    assert!(
        ineligible_seed.analysis(SourceFileId(0)).methods.is_empty(),
        "an isolated project with an unsupported RSpec version must not receive semantic targets"
    );
    for engine in [&first, &second, &ineligible] {
        assert!(
            engine.read().view().method_facts().next().is_none(),
            "the registry produces seed facts and never writes the engine itself"
        );
    }
}

#[test]
fn semantic_seed_applicability_fingerprint_ignores_per_document_context() {
    let context = |source_uri: &str, source_kind| ruby_fast_lsp_extension_api::ProjectContext {
        project_uri: "file:///workspace/app".to_string(),
        source_uri: source_uri.to_string(),
        source_kind,
        workspace_trusted: true,
        ruby_version: Some("3.3.0".to_string()),
        lockfile_present: true,
        locked_gems_complete: true,
        locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
            name: "rspec-core".to_string(),
            version: "3.13.6".to_string(),
            source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
        }],
    };
    let project_source = context(
        "file:///workspace/app/spec/example_spec.rb",
        ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
    );
    let dependency_source = context(
        "file:///gems/rspec-core-3.13.6/lib/rspec/core.rb",
        ruby_fast_lsp_extension_api::ProjectSourceKind::Gem,
    );

    assert_eq!(
        extension_applicability_fingerprint(Some(&project_source)),
        extension_applicability_fingerprint(Some(&dependency_source)),
        "semantic seed identity must depend on project applicability, not the current file URI or source kind"
    );
}

#[test]
fn cached_project_snapshot_replaces_semantic_seed_after_dependency_refresh() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    });
    let project = TempDir::new().expect("project temp directory must be created");
    fs::write(
        project.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    rspec-core (3.13.1)\n",
    )
    .expect("eligible lockfile must be written");
    let mut seed = ProjectContextSeed::detect(
        "file:///umbrella/app".to_string(),
        project.path(),
        true,
        Some("3.3.0".to_string()),
    );
    let engine: Arc<dyn Send + Sync> = Arc::new(RwLock::new(ruby_analysis::engine::Project::new()));

    let eligible = seed.context_snapshot(
        "file:///umbrella/app/spec/example_spec.rb".to_string(),
        SourceKind::Project,
    );
    let mut eligible_seed = None;
    registry.with_semantic_seed_for_snapshot(&engine, &eligible, |seed| eligible_seed = Some(seed));
    assert!(
        eligible_seed.as_ref().is_some_and(seeds_rspec_describe),
        "the eligible cached snapshot must seed RSpec.describe"
    );

    fs::write(
        project.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    rspec-core (4.0.0)\n",
    )
    .expect("ineligible lockfile must be written");
    seed.refresh_dependencies(project.path());
    let ineligible = seed.context_snapshot(
        "file:///umbrella/app/spec/example_spec.rb".to_string(),
        SourceKind::Project,
    );
    let mut ineligible_seed = None;
    registry
        .with_semantic_seed_for_snapshot(&engine, &ineligible, |seed| ineligible_seed = Some(seed));

    assert!(
        ineligible_seed.is_some_and(|seed| seed.analysis(SourceFileId(0)).methods.is_empty()),
        "dependency refresh must produce a replacement seed without stale semantic targets"
    );
}

#[test]
fn extension_dispatch_skips_dependency_and_signature_sources_without_losing_applicability() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    });
    let extension = registry
        .extensions()
        .into_iter()
        .find(|extension| extension.metadata.id == "rspec-ruby")
        .expect("bundled RSpec extension must load for source policy testing");
    let context = |source_kind| ruby_fast_lsp_extension_api::ProjectContext {
        project_uri: "file:///workspace/app".to_string(),
        source_uri: "file:///workspace/app/source.rb".to_string(),
        source_kind,
        workspace_trusted: true,
        ruby_version: Some("3.3.0".to_string()),
        lockfile_present: true,
        locked_gems_complete: true,
        locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
            name: "rspec-core".to_string(),
            version: "3.13.6".to_string(),
            source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
        }],
    };

    for kind in [
        ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
        ruby_fast_lsp_extension_api::ProjectSourceKind::Excluded,
    ] {
        let project = context(kind);
        assert!(extension.applies_to(Some(&project)));
        assert!(extension.applies_to_source(Some(&project)));
    }
    for kind in [
        ruby_fast_lsp_extension_api::ProjectSourceKind::Gem,
        ruby_fast_lsp_extension_api::ProjectSourceKind::Stdlib,
        ruby_fast_lsp_extension_api::ProjectSourceKind::Stub,
        ruby_fast_lsp_extension_api::ProjectSourceKind::Signature,
    ] {
        let project = context(kind);
        assert!(
            extension.applies_to(Some(&project)),
            "locked-gem applicability must remain a project-level decision"
        );
        assert!(
            !extension.applies_to_source(Some(&project)),
            "extensions must not execute DSL hooks while indexing {kind:?} inputs"
        );
    }
}

#[test]
fn collector_applicability_snapshot_reuses_exact_project_decisions() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    });
    let project = |version: &str| ruby_fast_lsp_extension_api::ProjectContext {
        project_uri: "file:///workspace/app".to_string(),
        source_uri: "file:///workspace/app/spec/example_spec.rb".to_string(),
        source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
        workspace_trusted: true,
        ruby_version: Some("3.3.0".to_string()),
        lockfile_present: true,
        locked_gems_complete: true,
        locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
            name: "rspec-core".to_string(),
            version: version.to_string(),
            source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
        }],
    };
    let eligible = project("3.13.6");
    let ineligible = project("4.0.0");
    let extension = registry
        .extensions()
        .into_iter()
        .find(|extension| extension.metadata.id == "rspec-ruby")
        .expect("bundled RSpec extension must load for applicability snapshot testing");

    let evaluations_before = extension.test_applicability_evaluations();
    let eligible_snapshot = registry.applicability_snapshot(Some(&eligible));
    assert!(eligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&eligible)));
    for _ in 0..100 {
        assert!(
            eligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&eligible)),
            "the exact eligible project decision must remain reusable throughout one file traversal"
        );
    }
    assert_eq!(
        extension.test_applicability_evaluations(),
        evaluations_before + 1,
        "one file snapshot must evaluate each extension's locked-gem requirement exactly once"
    );

    let ineligible_snapshot = registry.applicability_snapshot(Some(&ineligible));
    assert!(
        !ineligible_snapshot.applies_to_extension(&registry, "rspec-ruby", Some(&ineligible)),
        "a changed exact locked version must produce a new fail-closed applicability decision"
    );
    assert_eq!(
        extension.test_applicability_evaluations(),
        evaluations_before + 2,
        "a changed project dependency snapshot must be evaluated independently"
    );
}

#[test]
fn manifest_frame_call_names_are_validated_separately_from_guest_handlers() {
    let manifest: ExtensionManifest = toml::from_str(
        r#"
id = "frames"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["resources"]
frame_call_names = ["draw", "namespace"]
"#,
    )
    .expect("frame manifest must parse");

    validate_manifest(&manifest).expect("valid Ruby frame names must be accepted");
    assert_eq!(
        manifest.indexing.unwrap().frame_call_names,
        ["draw", "namespace"],
        "frame call names must not need fake guest handlers"
    );

    let invalid: ExtensionManifest = toml::from_str(
        r#"
id = "frames"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[indexing]
call_names = ["resources"]
frame_call_names = ["not a call"]
"#,
    )
    .expect("invalid frame name must remain syntactically valid TOML");
    let error = validate_manifest(&invalid).expect_err("invalid Ruby frame names must fail");
    assert!(
        error.to_string().contains("frame call name"),
        "got: {error}"
    );
}

#[test]
fn invalid_manifest_package_is_skipped_without_panicking() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_dir = temp_dir.path().join("broken");
    fs::create_dir(&package_dir).expect("test package dir must be created");
    fs::write(
        package_dir.join("extension.toml"),
        r#"
id = "broken"
abi_version = 999
runtime = "mruby-wasm"
wasm = "missing.wasm"
"#,
    )
    .expect("test manifest must be written");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_dir,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "invalid extension manifest loaded successfully",
        why = "package validation must reject mismatched ABI or missing wasm",
        fix = "keep manifest validation in the recoverable load path",
    );
}

#[test]
fn initialization_option_package_without_manifest_is_skipped() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_dir = temp_dir.path().join("not-a-package");
    fs::create_dir(&package_dir).expect("test package dir must be created");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_dir,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "initialization option package without manifest loaded",
        why = "editor-installed extension packages must have extension.toml",
        fix = "keep extensionPackages stricter than extensionDirs",
    );
}

#[test]
fn incompatible_server_version_manifest_is_skipped() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_dir = temp_dir.path().join("incompatible");
    fs::create_dir(&package_dir).expect("test package dir must be created");
    fs::write(package_dir.join("extension.wasm"), b"not real wasm")
        .expect("test wasm marker must be written");
    fs::write(
        package_dir.join("extension.toml"),
        r#"
id = "incompatible"
name = "Incompatible"
version = "0.1.0"
abi_version = 1
server_version = ">=999.0.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["index.call"]
permissions = []
"#,
    )
    .expect("test manifest must be written");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_dir,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "incompatible server_version manifest loaded",
        why = "extension packages must be gated by host compatibility",
        fix = "validate manifest server_version before wasm instantiation",
    );
}

#[test]
fn checksum_mismatch_manifest_is_skipped() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_dir = temp_dir.path().join("checksum");
    fs::create_dir(&package_dir).expect("test package dir must be created");
    fs::write(package_dir.join("extension.wasm"), b"not real wasm")
        .expect("test wasm marker must be written");
    fs::write(
        package_dir.join("extension.toml"),
        r#"
id = "checksum"
name = "Checksum"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.3, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
checksum_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
capabilities = ["index.call"]
permissions = []
"#,
    )
    .expect("test manifest must be written");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_dir,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "checksum mismatch manifest loaded",
        why = "extension packages must bind manifest metadata to wasm bytes",
        fix = "validate checksum_sha256 before wasm instantiation",
    );
}

#[test]
fn bundled_extension_directory_sits_beside_the_executable_or_its_bin_directory() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_root = temp_dir.path().join("platform-package");
    let executable = package_root.join("bin").join("ruby-fast-lsp");
    fs::create_dir_all(executable.parent().unwrap()).expect("bin directory must be created");
    assert_eq!(bundled_extension_directory(&executable), None);

    fs::create_dir_all(package_root.join("extensions")).expect("extensions must be created");
    assert_eq!(
        bundled_extension_directory(&executable),
        Some(package_root.join("extensions"))
    );

    fs::create_dir_all(package_root.join("bin/extensions")).expect("extensions must be created");
    assert_eq!(
        bundled_extension_directory(&executable),
        Some(package_root.join("bin/extensions")),
        "a directory beside the executable wins over one beside `bin`"
    );
}

#[test]
fn bundled_package_loads_when_no_other_source_provides_its_id() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    copy_rspec_package(&temp_dir.path().join("rspec-ruby"), "0.1.0-bundled");
    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: Vec::new(),
        directory_paths: vec![ConfiguredExtensionPath {
            path: temp_dir.path().to_path_buf(),
            source: ExtensionPathSource::Bundled,
        }],
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    });

    let reports = registry.status_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].id, "rspec-ruby");
    assert_eq!(reports[0].version.as_deref(), Some("0.1.0-bundled"));
    assert_eq!(reports[0].status, "loaded");
}

#[test]
fn configured_package_replaces_bundled_package_with_the_same_id() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let bundled = temp_dir.path().join("bundled");
    let configured = temp_dir.path().join("configured");
    copy_rspec_package(&bundled.join("rspec-ruby"), "0.1.0-bundled");
    copy_rspec_package(&configured, "0.1.0-configured");
    for source in [
        ExtensionPathSource::InitializationOptions,
        ExtensionPathSource::ProjectLocal,
        ExtensionPathSource::Environment,
    ] {
        let config = ExtensionLoadConfig {
            package_paths: vec![ConfiguredExtensionPath {
                path: configured.clone(),
                source,
            }],
            directory_paths: vec![ConfiguredExtensionPath {
                path: bundled.clone(),
                source: ExtensionPathSource::Bundled,
            }],
            project_package_paths: Vec::new(),
            settings: BTreeMap::new(),
        };

        let reports = ExtensionRegistry::load(&config).status_reports();
        assert_eq!(reports.len(), 1, "{source:?}");
        assert_eq!(
            reports[0].version.as_deref(),
            Some("0.1.0-configured"),
            "{source:?}"
        );
    }
}

#[test]
fn bundled_package_loads_when_the_configured_package_with_its_id_fails() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let bundled = temp_dir.path().join("bundled");
    let configured = temp_dir.path().join("configured");
    copy_rspec_package(&bundled.join("rspec-ruby"), "0.1.0-bundled");
    copy_rspec_package(&configured, "0.1.0-configured");
    fs::write(
        configured.join("target/wasm32-wasip1/release/rspec-ruby.wasm"),
        b"\0asm\x01\0\0\0",
    )
    .expect("configured Wasm must be replaced");
    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: configured,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: vec![ConfiguredExtensionPath {
            path: bundled,
            source: ExtensionPathSource::Bundled,
        }],
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    });

    let reports = registry.status_reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].version.as_deref(), Some("0.1.0-bundled"));
    assert_eq!(reports[0].status, "loaded");
}
