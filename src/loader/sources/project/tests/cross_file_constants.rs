//! Cross-file constant and constructor resolution after early document opens.

use super::*;

#[tokio::test]
async fn project_solves_transitive_cross_file_value_constants_after_early_did_open() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let first_consumer_path = root.join("first_consumer.rb");
    let second_consumer_path = root.join("second_consumer.rb");
    let alias_path = root.join("alias.rb");
    let base_path = root.join("base.rb");
    let cycle_consumer_path = root.join("cycle_consumer.rb");
    let first_consumer = r#"module Marketplace::Platform
  module Commerce
    class OrderProcessor
      def self.first_code(retryable)
        if retryable
          code = Marketplace::Platform::Errors::PaymentCodes::RETRY
        else
          code = Marketplace::Platform::Errors::PaymentCodes::FAILED
        end
        code
      end

      def self.fallback_code
        fallback = nil
        fallback ||= Marketplace::Platform::Errors::PaymentCodes::FAILED
        fallback
      end
    end
  end
end
"#;
    let second_consumer = r#"module Marketplace::Platform
  module Commerce
    class OrderProcessor
      def self.second_code
        result = Marketplace::Platform::Errors::PaymentCodes::RETRY
        result
      end
    end
  end
end
"#;
    std::fs::write(&first_consumer_path, first_consumer).unwrap();
    std::fs::write(&second_consumer_path, second_consumer).unwrap();
    std::fs::write(
        &alias_path,
        r#"module Marketplace
  module Platform
    module Commerce
    end

    module Errors
      class PaymentCodes
        RETRY = BaseCodes::RETRY
        FAILED = BaseCodes::FAILED
      end
    end
  end
end
"#,
    )
    .unwrap();
    std::fs::write(
        &base_path,
        r#"module BaseCodes
  RETRY = "retry".freeze
  FAILED = "failed".freeze
end
"#,
    )
    .unwrap();
    std::fs::write(root.join("cycle_a.rb"), "CycleA = CycleB\n").unwrap();
    std::fs::write(root.join("cycle_b.rb"), "CycleB = CycleA\n").unwrap();
    let cycle_consumer = "cycle = CycleA\n";
    std::fs::write(&cycle_consumer_path, cycle_consumer).unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::from_file_path(&first_consumer_path).unwrap(),
                language_id: "ruby".to_string(),
                version: 1,
                text: first_consumer.to_string(),
            },
        },
    )
    .await;
    {
        let engine = workspace_state.analysis_engine.read();
        let file_id = engine.file_id(&first_consumer_path).unwrap();
        let assignment_start = u32::try_from(first_consumer.find("code =").unwrap()).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::core::VariableTypeKind::Local,
                "code",
                file_id,
                assignment_start,
                assignment_start + u32::try_from("code".len()).unwrap(),
            ),
            Some(ruby_analysis::core::RubyType::Unknown),
            "an unresolved value constant must remain Unknown; Class<...> is only sound after a class declaration is proven"
        );
    }
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["firstconsumer".to_string()]), HashSet::new());
    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
    indexer
        .collect_remaining_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let query = AnalysisQuery::new(&engine);
    for parts in [
        vec!["BaseCodes", "RETRY"],
        vec!["Marketplace", "Platform", "Errors", "PaymentCodes", "RETRY"],
    ] {
        let constant = FullyQualifiedName::constant(
            parts
                .into_iter()
                .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            query.constant_value_type(&constant),
            Some(ruby_analysis::core::RubyType::string()),
            "transitive constant equations must solve each alias before consumers"
        );
    }
    let first_file_id = engine.file_id(&first_consumer_path).unwrap();
    let final_read_offset =
        u32::try_from(first_consumer.rfind("        code\n").unwrap() + 8).unwrap();
    assert_eq!(
        query.local_read_type_at(first_file_id, final_read_offset),
        Some(ruby_analysis::core::RubyType::string()),
        "flow reads after a branch must consume the joined constant equation"
    );
    for (path, source, name) in [
        (&first_consumer_path, first_consumer, "code"),
        (&second_consumer_path, second_consumer, "result"),
    ] {
        let file_id = engine.file_id(path).unwrap();
        let assignment_start = u32::try_from(source.find(&format!("{name} =")).unwrap()).unwrap();
        let assignment_end = assignment_start + u32::try_from(name.len()).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::core::VariableTypeKind::Local,
                name,
                file_id,
                assignment_start,
                assignment_end,
            ),
            Some(ruby_analysis::core::RubyType::string()),
            "one equation solve must update every consumer of a transitive value-constant alias"
        );
    }
    for (path, source, method) in [
        (&first_consumer_path, first_consumer, "first_code"),
        (&second_consumer_path, second_consumer, "second_code"),
    ] {
        let file_id = engine.file_id(path).unwrap();
        let method_offset = u32::try_from(source.find(method).unwrap()).unwrap();
        assert_eq!(
            query.method_return_type_at(method, file_id, method_offset),
            Some(ruby_analysis::core::RubyType::string()),
            "method-return equations must retain the same constant dependency as assignment facts"
        );
    }
    let fallback_start = u32::try_from(first_consumer.find("fallback ||=").unwrap()).unwrap();
    assert_eq!(
        query.variable_assignment_type_at(
            ruby_analysis::core::VariableTypeKind::Local,
            "fallback",
            first_file_id,
            fallback_start,
            fallback_start + u32::try_from("fallback".len()).unwrap(),
        ),
        Some(ruby_analysis::core::RubyType::string()),
        "a late-resolved value constant must update the stable source assignment for ||= writes"
    );
    let cycle_file_id = engine.file_id(&cycle_consumer_path).unwrap();
    assert_eq!(
        query.variable_assignment_type_at(
            ruby_analysis::core::VariableTypeKind::Local,
            "cycle",
            cycle_file_id,
            0,
            u32::try_from("cycle".len()).unwrap(),
        ),
        Some(ruby_analysis::core::RubyType::Unknown),
        "a base-free constant cycle must stay Unknown instead of becoming a class-object guess"
    );
    drop(engine);

    let hints = crate::lsp::capabilities::presentation::inlay_hints::handle_inlay_hints(
        &server,
        InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&first_consumer_path).unwrap(),
            },
            range: Range::new(Position::new(0, 0), Position::new(100, 0)),
        },
    )
    .await;
    let assignment_lines = first_consumer
        .lines()
        .enumerate()
        .filter_map(|(line, text)| text.contains("code =").then_some(line as u32))
        .collect::<Vec<_>>();
    for line in assignment_lines {
        assert!(
            hints.iter().any(|hint| {
                hint.position.line == line
                    && crate::test::harness::get_hint_label(hint) == ": String"
            }),
            "the open document's inlay hints must prefer solved engine facts over its pre-index variable-scope snapshot; hints={hints:?}"
        );
    }
}

#[tokio::test]
async fn project_resolves_early_nested_constructor_to_later_top_level_class() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let consumer_path = root.join("consumer.rb");
    let definition_path = root.join("registry.rb");
    let consumer = r#"module Portal
  module Accounts
    class Controller
      def build
        tags = Registry.new
        tags
      end
    end
  end
end
"#;
    std::fs::write(&consumer_path, consumer).unwrap();
    std::fs::write(&definition_path, "class Registry\nend\n").unwrap();

    let server = RubyLanguageServer::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    crate::lsp::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::from_file_path(&consumer_path).unwrap(),
                language_id: "ruby".to_string(),
                version: 1,
                text: consumer.to_string(),
            },
        },
    )
    .await;

    let assignment_start = u32::try_from(consumer.find("tags =").unwrap()).unwrap();
    let assignment_end = assignment_start + u32::try_from("tags".len()).unwrap();
    {
        let engine = workspace_state.analysis_engine.read();
        let file_id = engine.file_id(&consumer_path).unwrap();
        assert_eq!(
            AnalysisQuery::new(&engine).variable_assignment_type_at(
                ruby_analysis::core::VariableTypeKind::Local,
                "tags",
                file_id,
                assignment_start,
                assignment_end,
            ),
            Some(ruby_analysis::core::RubyType::Unknown),
            "an unresolved constructor must remain Unknown instead of fabricating a class in the current lexical namespace"
        );
    }

    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["consumer".to_string()]), HashSet::new());
    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
    indexer
        .collect_remaining_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    workspace_state.analysis_engine.write().resolve();

    let engine = workspace_state.analysis_engine.read();
    let file_id = engine.file_id(&consumer_path).unwrap();
    let expected = ruby_analysis::core::RubyType::Class(FullyQualifiedName::constant(vec![
        ruby_analysis::core::RubyConstant::new("Registry").unwrap(),
    ]));
    assert_eq!(
        AnalysisQuery::new(&engine).variable_assignment_type_at(
            ruby_analysis::core::VariableTypeKind::Local,
            "tags",
            file_id,
            assignment_start,
            assignment_end,
        ),
        Some(expected),
        "final semantic resolution must bind the constructor to the proven top-level class"
    );
    let method_offset = u32::try_from(consumer.find("build").unwrap()).unwrap();
    assert_eq!(
        AnalysisQuery::new(&engine).method_return_type_at("build", file_id, method_offset),
        Some(ruby_analysis::core::RubyType::Class(
            FullyQualifiedName::constant(vec![
                ruby_analysis::core::RubyConstant::new("Registry").unwrap(),
            ]),
        )),
        "flow and method-return equations must consume the same resolved constructor identity"
    );
    drop(engine);

    let hints = crate::lsp::capabilities::presentation::inlay_hints::handle_inlay_hints(
        &server,
        InlayHintParams {
            work_done_progress_params: Default::default(),
            text_document: TextDocumentIdentifier {
                uri: Url::from_file_path(&consumer_path).unwrap(),
            },
            range: Range::new(Position::new(0, 0), Position::new(20, 0)),
        },
    )
    .await;
    assert!(
        hints.iter().any(|hint| {
            hint.position.line == 4 && crate::test::harness::get_hint_label(hint) == ": Registry"
        }),
        "the open consumer must display the resolved top-level constructor type; hints={hints:?}"
    );
}
