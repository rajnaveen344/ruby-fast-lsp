//! File-scoped type reads through `View::type_at`.

use crate::core::{
    FileAnalysis, FullyQualifiedName, RubyType, SourceFileId, TextRange, TypeFact, TypeProvenance,
    TypeResolution, TypeSubject,
};
use crate::engine::{Project, ResolveMode, SourceFileInput};

fn constant_type_at(
    engine: &Project,
    constant: &FullyQualifiedName,
    file_id: SourceFileId,
    byte_offset: u32,
) -> Option<RubyType> {
    match engine.query().type_at(
        &TypeSubject::Constant(constant.clone()),
        file_id,
        byte_offset,
    ) {
        TypeResolution::Resolved(fact) => Some(fact.ruby_type),
        TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
    }
}

#[test]
fn file_scoped_queries_follow_replacement_without_losing_other_files() {
    let mut engine = Project::new();
    let constant = FullyQualifiedName::try_from("LABEL").unwrap();
    let mut file_ids = Vec::new();
    for (path, source, ruby_type) in [
        ("first.rb", "LABEL = 'text'", RubyType::string()),
        ("second.rb", "LABEL = 12", RubyType::integer()),
    ] {
        let file_id = engine.register_file(SourceFileInput {
            path: path.into(),
            content: source.into(),
            kind: crate::core::SourceKind::Project,
        });
        engine.update(
            file_id,
            FileAnalysis {
                types: vec![TypeFact::new(
                    TypeSubject::Constant(constant.clone()),
                    ruby_type,
                    TextRange::new(file_id, 0, 5),
                    TypeProvenance::Assignment,
                )],
                ..FileAnalysis::default()
            },
            ResolveMode::Immediate,
        );
        file_ids.push(file_id);
    }

    assert_eq!(
        constant_type_at(&engine, &constant, file_ids[0], 3),
        Some(RubyType::string()),
    );
    assert_eq!(
        constant_type_at(&engine, &constant, file_ids[1], 3),
        Some(RubyType::integer()),
    );

    let edited_id = engine.register_file(SourceFileInput {
        path: "first.rb".into(),
        content: String::new(),
        kind: crate::core::SourceKind::Project,
    });
    assert_eq!(edited_id, file_ids[0]);
    engine.update(edited_id, FileAnalysis::default(), ResolveMode::Immediate);

    assert_eq!(constant_type_at(&engine, &constant, edited_id, 3), None,);
    assert_eq!(
        constant_type_at(&engine, &constant, file_ids[1], 3),
        Some(RubyType::integer()),
    );
    assert_eq!(engine.query().all_type_facts().len(), 1);
}

#[test]
fn query_uses_domain_byte_offsets_without_source_or_protocol_coordinates() {
    let mut engine = Project::new();
    let file_id = engine.register_file(crate::engine::SourceFileInput {
        path: "sample.rb".into(),
        content: "VALUE = \"text\"".into(),
        kind: crate::core::SourceKind::Project,
    });
    let range = TextRange::new(file_id, 4, 9);
    let fqn = FullyQualifiedName::constant(vec![crate::core::RubyConstant::new("VALUE").unwrap()]);
    let fact = TypeFact::new(
        TypeSubject::Constant(fqn.clone()),
        RubyType::string(),
        range,
        TypeProvenance::Inferred,
    );
    engine.update(
        file_id,
        crate::core::FileAnalysis {
            types: vec![fact],
            ..Default::default()
        },
        crate::engine::ResolveMode::Immediate,
    );

    assert_eq!(
        constant_type_at(&engine, &fqn, file_id, 6),
        Some(RubyType::string())
    );
    assert_eq!(constant_type_at(&engine, &fqn, file_id, 2), None);
}
