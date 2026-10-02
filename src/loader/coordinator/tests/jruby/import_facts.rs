//! Selected JRuby catalog import facts and on-demand Java import materialization.

use super::*;

#[tokio::test]
async fn selected_jruby_catalog_contributes_import_facts_to_the_owning_project() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    let java_home = fixture.path().join("jdk");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(jruby_home.join("bin")).unwrap();
    fs::create_dir_all(java_home.join("jmods")).unwrap();
    fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
    fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
    let demo_class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    write_jar(
        &root.join("lib/runtime.jar"),
        "com/example/Demo.class",
        &demo_class,
    );
    let rich_class = decode_hex(include_str!(
        "../../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
    ));
    write_jar(
        &root.join("lib/rich.jar"),
        "fixtures/RichFixture.class",
        &rich_class,
    );
    let rich_source =
        include_str!("../../../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
    write_jar(
        &root.join("lib/rich-sources.jar"),
        "fixtures/RichFixture.java",
        rich_source.as_bytes(),
    );

    let root_string = format!("{}/", root.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root_string.clone(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: jruby_home.join("bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(java_home),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    config.jruby.projects = vec![ProjectJrubyConfig {
        root: root_string,
        additional_classpath: vec!["lib/runtime.jar".to_string(), "lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(&root).unwrap());
    let mut coordinator = IndexingCoordinator::new(root.clone(), config);
    coordinator.set_cache_root(fixture.path().join("user-cache"));
    assert_eq!(
        select_ruby_version(&mut coordinator, &server).await,
        Some(RubyVersion::new_with_implementation(
            2,
            5,
            RuntimeImplementation::Jruby
        ))
    );
    coordinator.setup_jruby_import_provider().unwrap();
    assert!(coordinator.jruby_import_provider.is_some());
    let signature_cache_root = coordinator
        .jruby_signature_cache_root(
            coordinator
                .jruby_import_provider
                .as_ref()
                .expect("fixture JRuby provider must exist"),
        )
        .unwrap();
    coordinator
        .setup_file_processor(&server.load_context_for_project(coordinator.workspace_root()));

    let source = "module Admin\n\
                          java_import fixtures.RichFixture\n\
                          class RichFixture\n\
                            java_alias :merged, :combine\n\
                          end\n\
                          INSTANCE = com.example.Demo.new\n\
                          CANONICAL = Java::Fixtures::RichFixture.new(nil)\n\
                          RICH = RichFixture.new(nil)\n\
                          RESULT = RICH.merged('value', 1)\n\
                          RUN_RESULT = RICH.java_send(:run, [])\n\
                          RUN_HANDLE = RICH.java_method(:run, [])\n\
                          UNBOUND_RUN = RichFixture.java_method(:run, [])\n\
                          DIRECT = RICH.combine('value', [1])\n\
                        end\n";
    let source_path = root.join("imports.rb");
    fs::write(&source_path, source).unwrap();
    let mut project_indexer = IndexerProject::new(
        root.clone(),
        coordinator
            .file_processor
            .as_ref()
            .expect("fixture FileProcessor must be configured")
            .clone(),
        coordinator.config.indexing.clone(),
    );
    project_indexer
        .collect_project_facts(&server.load_context_for_project(project_indexer.workspace_root()))
        .unwrap();
    let proxy = FullyQualifiedName::namespace(
        ["Java", "ComExample", "Demo"]
            .into_iter()
            .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    {
        let engine = server.analysis_engine_for_uri(
            &Url::from_file_path(signature_cache_root.join("com/example/Demo.rb")).unwrap(),
        );
        let engine = engine.read();
        let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
        assert_eq!(
            symbols.iter().filter(|fact| fact.fqn == proxy).count(),
            1,
            "the ordinary cold project pass must materialize generated metadata signatures \
                 from the same parsed sources before its final engine resolution; \
                 indexed symbols: {symbols:?}"
        );
    }
    assert!(
        !root.join(".ruby-fast-lsp").exists(),
        "generated JRuby signatures must never write cache state into the Ruby project"
    );

    let uri = Url::from_file_path(&source_path).unwrap();
    let ctx = server.load_context_for_uri(&uri);
    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .analyze_file(&uri, source, &ctx)
        .unwrap()
        .commit(&ctx);

    let alias = FullyQualifiedName::try_from("Admin::RichFixture").unwrap();
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert_eq!(
        AnalysisQuery::new(&engine).symbol_facts_for(&alias).len(),
        1,
        "the selected project's Java catalog must flow through ordinary engine facts"
    );
    let source_file = AnalysisQuery::new(&engine)
        .file_id(&source_path)
        .expect("project source must be registered");
    let import_target_offset = u32::try_from(
        source
            .find("fixtures.RichFixture")
            .expect("fixture Java import target must exist")
            + "fixtures.".len(),
    )
    .unwrap();
    let import_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, import_target_offset);
    assert!(
        import_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a catalog-proven java_import target must resolve to its exact Java source; \
             targets: {import_targets:?}"
    );
    assert!(
        !AnalysisQuery::new(&engine).navigation_must_fail_closed_at(
            source_file,
            import_target_offset,
            !import_targets.is_empty(),
        ),
        "an exact runtime-owned java_import reference must outrank an overlapping generic \
             dotted-call candidate; targets: {import_targets:?}"
    );
    let new_offset = u32::try_from(
        source
            .find("new")
            .expect("fixture constructor call must exist"),
    )
    .unwrap();
    let constructor_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, new_offset);
    assert!(
        constructor_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| file.kind == ruby_analysis::core::SourceKind::Signature)
        }),
        "constructor navigation must resolve to the generated read-only signature document; \
            targets: {constructor_targets:?}"
    );
    let rich_constructor_offset = u32::try_from(
        source
            .find("RichFixture.new")
            .expect("fixture rich constructor call must exist")
            + "RichFixture.".len(),
    )
    .unwrap();
    let rich_constructor_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, rich_constructor_offset);
    assert!(
        rich_constructor_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a source-backed constructor must navigate to the exact Java implementation source, \
             not its generated signature; targets: {rich_constructor_targets:?}"
    );
    let alias_call_offset = u32::try_from(
        source
            .rfind("merged")
            .expect("fixture alias call must exist"),
    )
    .unwrap();
    let alias_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, alias_call_offset);
    assert!(
        alias_targets
            .iter()
            .any(|target| target.file_id == source_file),
        "java_alias calls must resolve to the file-owned alias declaration; \
             targets: {alias_targets:?}"
    );
    let run_offset = u32::try_from(
        source
            .find(":run")
            .expect("fixture java_send method symbol must exist")
            + 1,
    )
    .unwrap();
    let run_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, run_offset);
    assert!(
        run_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "java_send method-name navigation must resolve to the exact Java implementation source; \
             targets: {run_targets:?}"
    );
    for (constant, expected) in [
        ("Admin::RUN_RESULT", RubyType::nil_class()),
        (
            "Admin::RUN_HANDLE",
            RubyType::Class(FullyQualifiedName::try_from("Method").unwrap()),
        ),
        (
            "Admin::UNBOUND_RUN",
            RubyType::Class(FullyQualifiedName::try_from("UnboundMethod").unwrap()),
        ),
    ] {
        let constant = FullyQualifiedName::try_from(constant).unwrap();
        let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
        assert!(
            indexed_types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(constant.clone())
                    && fact.ruby_type == expected
            }),
            "{constant} must retain the selected JRuby dispatch type {expected}; \
                 indexed types: {indexed_types:?}"
        );
    }
    let document = server
        .documents
        .read()
        .get(&uri)
        .cloned()
        .expect("processed JRuby document must exist");
    let query = crate::features::cursor::EngineQuery::with_doc_and_engine(
        document,
        server.analysis_engine_for_uri(&uri),
    );
    let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
    let rich_constant = FullyQualifiedName::try_from("Admin::RICH").unwrap();
    let rich_proxy = FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap();
    let rich_proxy_namespace = FullyQualifiedName::namespace(rich_proxy.namespace_parts().to_vec());
    assert!(
        indexed_types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(rich_constant.clone())
                && fact.ruby_type == RubyType::Class(rich_proxy.clone())
        }),
        "the imported Java constructor assignment must retain its canonical proxy type; \
             indexed types: {indexed_types:?}"
    );
    let combine_method = ruby_analysis::core::RubyMethod::new("combine").unwrap();
    let combine_return = ruby_analysis::engine::lookup::method(
        &AnalysisQuery::new(&engine),
        ruby_analysis::engine::lookup::MethodRequest::new(
            ruby_analysis::engine::lookup::LookupReceiver::Namespace(&rich_proxy_namespace),
            combine_method,
            ruby_analysis::engine::lookup::MethodWant::Return,
        ),
    )
    .into_return_type();
    assert_eq!(
        combine_return,
        Some(RubyType::Class(
            FullyQualifiedName::try_from("Java::JavaUtil::List").unwrap()
        )),
        "the exact classfile-derived method return must be queryable on the Java proxy"
    );
    let direct_line = source
        .lines()
        .position(|line| line.contains("DIRECT = RICH.combine"))
        .unwrap();
    let direct_character = source
        .lines()
        .nth(direct_line)
        .unwrap()
        .find("combine")
        .unwrap();
    let hover = query
        .get_hover_at_position(
            &uri,
            tower_lsp::lsp_types::Position::new(
                u32::try_from(direct_line).unwrap(),
                u32::try_from(direct_character + 1).unwrap(),
            ),
            source,
        )
        .expect("a Java proxy method call must produce hover information");
    assert!(
        hover.content.contains("Java::JavaUtil::List"),
        "Java proxy hover must expose the classfile-derived return type, got: {}",
        hover.content
    );
    drop(engine);

    let ctx = server.load_context_for_uri(&uri);
    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .analyze_file_current_file_resolution_forced(&uri, "module Admin\nend\n", &ctx)
        .unwrap()
        .commit(&ctx);
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert!(
        AnalysisQuery::new(&engine)
            .symbol_facts_for(&alias)
            .is_empty(),
        "removing java_import must remove its file-owned alias through ordinary replacement"
    );
    let java_alias = FullyQualifiedName::method(
        ["Java", "Fixtures", "RichFixture"]
            .into_iter()
            .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
        ruby_analysis::core::RubyMethod::new("merged").unwrap(),
    );
    assert!(
        AnalysisQuery::new(&engine)
            .method_facts_for(&java_alias)
            .is_empty(),
        "removing java_alias must remove its proxy-owned method through ordinary replacement"
    );
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, run_offset)
            .is_empty(),
        "removing JRuby dispatch calls must remove their file-owned Java method candidates"
    );
}

#[tokio::test]
async fn adding_a_java_import_after_cold_index_materializes_navigation_inputs_on_demand() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    let java_home = fixture.path().join("jdk");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(jruby_home.join("bin")).unwrap();
    fs::create_dir_all(java_home.join("jmods")).unwrap();
    fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
    fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
    write_jar(
        &root.join("lib/rich.jar"),
        "fixtures/RichFixture.class",
        &decode_hex(include_str!(
            "../../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        )),
    );
    write_jar(
        &root.join("lib/rich-sources.jar"),
        "fixtures/RichFixture.java",
        include_str!("../../../../../crates/jvm-metadata/fixtures/sources/RichFixture.java")
            .as_bytes(),
    );

    let root_string = format!("{}/", root.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root_string.clone(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: jruby_home.join("bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(java_home),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    config.jruby.projects = vec![ProjectJrubyConfig {
        root: root_string,
        additional_classpath: vec!["lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(&root).unwrap());
    let mut coordinator = IndexingCoordinator::new(root.clone(), config);
    coordinator.set_cache_root(fixture.path().join("user-cache"));
    select_ruby_version(&mut coordinator, &server).await;
    coordinator.setup_jruby_import_provider().unwrap();
    let signature_cache = coordinator
        .jruby_signature_cache_root(
            coordinator
                .jruby_import_provider
                .as_ref()
                .expect("fixture JRuby provider must exist"),
        )
        .unwrap();
    server.set_jruby_add_on(
        &root,
        coordinator
            .jruby_import_provider
            .clone()
            .map(crate::loader::jruby_add_on::JrubyAddOn::new),
    );

    let source_path = root.join("imports.rb");
    let uri = Url::from_file_path(&source_path).unwrap();
    let initial = "VALUE = 1\n";
    fs::write(&source_path, initial).unwrap();
    crate::lsp::lifecycle::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: initial.to_string(),
            },
        },
    )
    .await;
    assert!(
        !signature_cache.join("fixtures/RichFixture.rb").exists(),
        "cold indexing without a Java dependency must not eagerly materialize its signature"
    );

    let added = "java_import fixtures.RichFixture\nRICH = RichFixture.new(nil)\n";
    crate::lsp::lifecycle::indexing::handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: added.to_string(),
            }],
        },
    )
    .await;
    assert!(
        signature_cache.join("fixtures/RichFixture.rb").is_file(),
        "adding a static import must materialize its signature without restarting the project"
    );
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    let source_file = AnalysisQuery::new(&engine).file_id(&source_path).unwrap();
    let constructor_offset =
        u32::try_from(added.find("RichFixture.new").unwrap() + "RichFixture.".len()).unwrap();
    let targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, constructor_offset);
    assert!(
        targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a newly added import must navigate to its exact Java source in the same edit pass; \
             targets: {targets:?}"
    );
    let rich = FullyQualifiedName::try_from("RICH").unwrap();
    let expected_rich_type =
        RubyType::Class(FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap());
    let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
    assert!(
        indexed_types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(rich.clone())
                && fact.ruby_type == expected_rich_type
        }),
        "an imported Java constructor assignment must retain the canonical proxy instance \
             type during the same edit pass; indexed types: {indexed_types:?}"
    );
    drop(engine);

    crate::lsp::lifecycle::indexing::handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 3,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: initial.to_string(),
            }],
        },
    )
    .await;
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, constructor_offset)
            .is_empty(),
        "removing the newly added import and constructor call must clear their reference facts"
    );
    let resources = server.indexing.resources().snapshot();
    assert_eq!(
        resources.completed_tasks, 3,
        "didOpen plus two didChange passes must each own exactly one outer resource lease; \
             interactive JRuby signature/source materialization must not acquire nested admission"
    );
    assert_eq!(resources.active_tasks, 0);
    assert_eq!(resources.queued_tasks, 0);
}
