//! Semantic export, context, and result fingerprints.

use super::*;

#[test]
fn semantic_export_fingerprint_distinguishes_body_and_api_edits() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "def name; 'A'; end");
    let owner = FullyQualifiedName::try_from("Object").unwrap();
    let method_fqn =
        FullyQualifiedName::method(owner.namespace_parts(), RubyMethod::new("name").unwrap());
    let facts = |params: Vec<String>, start_byte: u32| FileAnalysis {
        methods: vec![MethodFact::with_params(
            method_fqn.clone(),
            owner.clone(),
            crate::core::TextRange::new(file_id, start_byte, start_byte + 4),
            params,
        )],
        ..Default::default()
    };

    assert_eq!(
        engine.replace_facts(file_id, facts(Vec::new(), 0), ResolveMode::Immediate),
        SemanticChange::InitialIndex
    );

    register_project_file(&mut engine, "app/user.rb", "\n\ndef name; 'B'; end");
    assert_eq!(
        engine.replace_facts(file_id, facts(Vec::new(), 2), ResolveMode::Immediate),
        SemanticChange::BodyOnly
    );

    assert_eq!(
        engine.replace_facts(
            file_id,
            facts(vec!["prefix".to_string()], 2),
            ResolveMode::Immediate,
        ),
        SemanticChange::ExportsChanged
    );
}

#[test]
fn semantic_context_fingerprint_is_path_independent_but_kind_and_fact_sensitive() {
    fn engine_with(path: &str, kind: SourceKind, method_name: &str) -> AnalysisEngine {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from(path),
            content: "class Shared; end".to_string(),
            kind,
        });
        let owner = FullyQualifiedName::try_from("Shared").unwrap();
        let method = RubyMethod::new(method_name).unwrap();
        engine.replace_facts(
            file_id,
            FileAnalysis {
                methods: vec![MethodFact::new(
                    FullyQualifiedName::method(owner.namespace_parts(), method),
                    owner,
                    TextRange::new(file_id, 0, 12),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        engine
    }

    let first = engine_with("/runtime/a/shared.rb", SourceKind::Stub, "call");
    let same_semantics_other_path = engine_with("/runtime/b/shared.rb", SourceKind::Stub, "call");
    let different_kind = engine_with("/runtime/a/shared.rb", SourceKind::Gem, "call");
    let different_fact = engine_with("/runtime/a/shared.rb", SourceKind::Stub, "other");

    assert_eq!(
        first.semantic_context_fingerprint(),
        same_semantics_other_path.semantic_context_fingerprint()
    );
    assert_ne!(
        first.semantic_context_fingerprint(),
        different_kind.semantic_context_fingerprint()
    );
    assert_ne!(
        first.semantic_context_fingerprint(),
        different_fact.semantic_context_fingerprint()
    );
}

#[test]
fn semantic_result_fingerprint_is_file_id_independent_and_reference_sensitive() {
    fn engine_with(target_name: &str, reverse_registration: bool) -> AnalysisEngine {
        let mut engine = AnalysisEngine::new();
        let register_definitions = |engine: &mut AnalysisEngine| {
            register_project_file(
                engine,
                "app/models.rb",
                "class Alpha; end\nclass Beta; end\n",
            )
        };
        let register_call =
            |engine: &mut AnalysisEngine| register_project_file(engine, "app/call.rb", "Alpha\n");
        let (definitions_file, call_file) = if reverse_registration {
            let call = register_call(&mut engine);
            let definitions = register_definitions(&mut engine);
            (definitions, call)
        } else {
            let definitions = register_definitions(&mut engine);
            let call = register_call(&mut engine);
            (definitions, call)
        };
        let alpha = FullyQualifiedName::constant(vec![RubyConstant::new("Alpha").unwrap()]);
        let beta = FullyQualifiedName::constant(vec![RubyConstant::new("Beta").unwrap()]);
        engine.replace_facts(
            definitions_file,
            FileAnalysis {
                symbols: vec![
                    SymbolFact::new(
                        alpha.clone(),
                        SymbolKind::Constant,
                        TextRange::new(definitions_file, 6, 11),
                    ),
                    SymbolFact::new(
                        beta.clone(),
                        SymbolKind::Constant,
                        TextRange::new(definitions_file, 23, 27),
                    ),
                ],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        let target = match target_name {
            "Alpha" => alpha,
            "Beta" => beta,
            other => panic!("unexpected semantic fingerprint fixture target {other}"),
        };
        engine.replace_facts(
            call_file,
            FileAnalysis {
                reference_candidates: vec![ReferenceCandidate::resolved(
                    TextRange::new(call_file, 0, 5),
                    target,
                    None,
                )],
                types: vec![TypeFact::new(
                    TypeSubject::Expression(TextRange::new(call_file, 0, 5)),
                    RubyType::string(),
                    TextRange::new(call_file, 0, 5),
                    TypeProvenance::Inferred,
                )],
                diagnostics: vec![DiagnosticFact::new(
                    TextRange::new(call_file, 0, 5),
                    DiagnosticSeverity::Information,
                    "fixture",
                    "stable fixture diagnostic",
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine
    }

    let alpha = engine_with("Alpha", false);
    let alpha_reversed = engine_with("Alpha", true);
    let beta = engine_with("Beta", false);

    assert_eq!(
        alpha.semantic_result_fingerprint(),
        alpha_reversed.semantic_result_fingerprint(),
        "engine-local file IDs and insertion order must not change semantic evidence"
    );
    assert_ne!(
        alpha.semantic_result_fingerprint(),
        beta.semantic_result_fingerprint(),
        "a different resolved definition target must change semantic evidence"
    );
}

#[test]
fn semantic_context_fingerprint_is_cross_process_stable() {
    fn fingerprint() -> String {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: "stubs/widget.rb".into(),
            content: "class Widget; def call(value); end; end".into(),
            kind: SourceKind::Stub,
        });
        let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
        let method = RubyMethod::new("call").unwrap();
        engine.replace_facts(
            file_id,
            FileAnalysis {
                symbols: vec![SymbolFact::new(
                    owner.clone(),
                    SymbolKind::Class,
                    TextRange::new(file_id, 0, 40),
                )],
                methods: vec![MethodFact::with_params(
                    FullyQualifiedName::method(owner.namespace_parts(), method),
                    owner.clone(),
                    TextRange::new(file_id, 14, 34),
                    vec!["value".to_string()],
                )],
                types: vec![TypeFact::new(
                    TypeSubject::MethodReturn(FullyQualifiedName::method(
                        owner.namespace_parts(),
                        method,
                    )),
                    RubyType::string(),
                    TextRange::new(file_id, 14, 34),
                    TypeProvenance::Inferred,
                )],
                graph_nodes: vec![GraphNodeFact::new(
                    owner,
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 40),
                )],
                ..FileAnalysis::default()
            },
            ResolveMode::Deferred,
        );
        engine
            .semantic_context_fingerprint()
            .stable_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    const CHILD_ENV: &str = "RUBY_FAST_LSP_FINGERPRINT_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        for index in 0..512 {
            let _ = RubyConstant::new(&format!("FingerprintNoise{index}")).unwrap();
        }
        println!("RUBY_FAST_LSP_FINGERPRINT={}", fingerprint());
        return;
    }

    let expected = fingerprint();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine::state::tests::fingerprints::semantic_context_fingerprint_is_cross_process_stable",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fingerprint child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let child = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("RUBY_FAST_LSP_FINGERPRINT="))
        .unwrap()
        .to_string();
    assert_eq!(child, expected);
}
