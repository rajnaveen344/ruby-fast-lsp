use super::*;
use crate::core::{
    GraphNodeFact, GraphNodeKind, MethodFact, RubyConstant, SourceKind, SymbolFact, SymbolKind,
};
use crate::engine::{AnalysisEngine, FileFacts, ResolveMode, SourceFileInput};

fn declaration(
    engine: &mut AnalysisEngine,
    path: &str,
    kind: SourceKind,
    name: &str,
) -> (FullyQualifiedName, TextRange) {
    let source = format!("class {name}\n  def label; end\nend\n");
    let file = engine.register_file(SourceFileInput {
        path: path.into(),
        content: source.clone(),
        kind,
    });
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()]);
    let range = TextRange::new(file, 0, source.len() as u32);
    let method = RubyMethod::new("label").unwrap();
    engine.replace_facts(
        file,
        FileFacts {
            symbols: vec![
                SymbolFact::new(owner.clone(), SymbolKind::Class, range),
                SymbolFact::new(
                    FullyQualifiedName::global_variable("$registry".to_owned()).unwrap(),
                    SymbolKind::GlobalVariable,
                    range,
                ),
            ],
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                range,
            )],
            methods: vec![MethodFact::with_params(
                FullyQualifiedName::method(owner.namespace_parts(), method),
                owner.clone(),
                range,
                Vec::new(),
            )],
            ..FileFacts::default()
        },
        ResolveMode::Immediate,
    );
    (owner, range)
}

#[test]
fn definition_source_preference_is_shared_and_falls_back_after_removal() {
    let mut engine = AnalysisEngine::new();
    let (_, signature) = declaration(
        &mut engine,
        "/a_record.rbs",
        SourceKind::Signature,
        "Record",
    );
    let (_, stub) = declaration(&mut engine, "/b_record.rb", SourceKind::Stub, "Record");
    let (owner, implementation) =
        declaration(&mut engine, "/z_record.rb", SourceKind::Gem, "Record");
    for expected in [implementation, stub, signature] {
        let query = engine.query();
        assert_eq!(
            query.constant_definition_ranges(&owner.namespace_parts(), &[]),
            vec![expected]
        );
        assert_eq!(
            query.yard_type_definition_ranges("Record", &[]),
            vec![expected]
        );
        for identity in [
            &owner,
            &FullyQualifiedName::constant(owner.namespace_parts()),
        ] {
            assert_eq!(query.type_name_definition_ranges(identity), vec![expected]);
        }
        assert_eq!(
            query.global_variable_definition_ranges("$registry"),
            vec![expected]
        );
        assert_eq!(
            query.method_definition_ranges(&owner, &RubyMethod::new("label").unwrap(), true, None),
            Some(vec![expected])
        );
        engine.replace_facts(
            expected.file_id,
            FileFacts::default(),
            ResolveMode::Immediate,
        );
    }
}

#[test]
fn definition_source_preference_retains_a_different_receivers_signature() {
    let mut engine = AnalysisEngine::new();
    let (other, signature) =
        declaration(&mut engine, "/a_other.rbs", SourceKind::Signature, "Other");
    let (record, implementation) =
        declaration(&mut engine, "/z_record.rb", SourceKind::Project, "Record");
    let receiver = RubyType::union(vec![RubyType::Class(other), RubyType::Class(record)]);
    assert_eq!(
        engine.query().method_definition_ranges_for_type(
            &receiver,
            &RubyMethod::new("label").unwrap(),
            true,
            None
        ),
        Some(vec![implementation, signature])
    );
}

#[test]
fn conflicting_definition_orders_keep_external_precedence_constraints() {
    let names = ["Left", "Right", "Parent", "Unrelated"];
    let owners = names
        .iter()
        .map(|name| FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()]))
        .collect::<Vec<_>>();
    let chains = vec![
        vec![owners[0].clone(), owners[1].clone(), owners[2].clone()],
        vec![owners[1].clone(), owners[0].clone(), owners[2].clone()],
    ];
    let mut layers = precedence::layers(&owners, &chains);
    for layer in &mut layers {
        layer.sort();
    }
    assert_eq!(
        layers,
        vec![vec![0, 1, 3], vec![2]],
        "opposing overrides tie, but both must remain ahead of their common parent"
    );
}
