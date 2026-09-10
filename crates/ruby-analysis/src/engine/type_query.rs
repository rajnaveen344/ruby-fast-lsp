//! File-scoped views of already resolved type facts.
//!
//! Queries borrow the engine and use its ordinary semantic read path. They do
//! not trigger inference, clone stores, or fabricate an empty snapshot.

use crate::core::FullyQualifiedName;
use crate::core::RubyType;
use crate::core::{SourceFileId, TypeResolution, TypeSubject};
use crate::engine::{AnalysisEngine, AnalysisQuery};

/// Read-only, file-scoped queries over an engine's existing type facts.
///
/// This view neither runs inference nor owns caches or storage. The engine
/// snapshot remains borrowed for the lifetime of the query.
pub struct TypeQuery<'a> {
    query: AnalysisQuery<'a>,
    source_file_id: SourceFileId,
}

impl<'a> TypeQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine, source_file_id: SourceFileId) -> Self {
        Self {
            query: engine.query(),
            source_file_id,
        }
    }

    /// Get the value type for a constant assignment.
    ///
    /// Returns no value when this file has no fact for the constant.
    pub fn get_constant_type(&self, fqn: &FullyQualifiedName) -> Option<RubyType> {
        self.query
            .type_facts_for(&TypeSubject::Constant(fqn.clone()))
            .iter()
            .filter(|fact| fact.range.file_id == self.source_file_id)
            .next_back()
            .map(|fact| fact.ruby_type.clone())
    }

    pub fn get_constant_type_at(
        &self,
        fqn: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.query.type_at(
            &TypeSubject::Constant(fqn.clone()),
            self.source_file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => None,
            TypeResolution::Unresolved => None,
        }
    }

    pub fn get_local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.query.type_at(
            &TypeSubject::Local {
                scope_id,
                name: name.to_string(),
            },
            self.source_file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => return None,
            TypeResolution::Unresolved => {}
        }
        self.query
            .type_facts_in_file(self.source_file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter {
                    name: fact_name, ..
                } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn get_method_return_type_at(
        &self,
        fqn: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.query.type_at(
            &TypeSubject::MethodReturn(fqn.clone()),
            self.source_file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => None,
            TypeResolution::Unresolved => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TextRange, TypeFact, TypeProvenance};
    use crate::engine::{FileFacts, ResolveMode, SourceFileInput};

    #[test]
    fn file_scoped_queries_follow_replacement_without_losing_other_files() {
        let mut engine = AnalysisEngine::new();
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
            engine.replace_facts(
                file_id,
                FileFacts {
                    types: vec![TypeFact::new(
                        TypeSubject::Constant(constant.clone()),
                        ruby_type,
                        TextRange::new(file_id, 0, 5),
                        TypeProvenance::Assignment,
                    )],
                    ..FileFacts::default()
                },
                ResolveMode::Immediate,
            );
            file_ids.push(file_id);
        }

        assert_eq!(
            TypeQuery::new(&engine, file_ids[0]).get_constant_type_at(&constant, 3),
            Some(RubyType::string()),
        );
        assert_eq!(
            TypeQuery::new(&engine, file_ids[1]).get_constant_type_at(&constant, 3),
            Some(RubyType::integer()),
        );

        let edited_id = engine.register_file(SourceFileInput {
            path: "first.rb".into(),
            content: String::new(),
            kind: crate::core::SourceKind::Project,
        });
        assert_eq!(edited_id, file_ids[0]);
        engine.replace_facts(edited_id, FileFacts::default(), ResolveMode::Immediate);

        assert_eq!(
            TypeQuery::new(&engine, edited_id).get_constant_type_at(&constant, 3),
            None,
        );
        assert_eq!(
            TypeQuery::new(&engine, file_ids[1]).get_constant_type_at(&constant, 3),
            Some(RubyType::integer()),
        );
        assert_eq!(engine.query().all_type_facts().len(), 1);
    }

    #[test]
    fn query_uses_domain_byte_offsets_without_source_or_protocol_coordinates() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(crate::engine::SourceFileInput {
            path: "sample.rb".into(),
            content: "VALUE = \"text\"".into(),
            kind: crate::core::SourceKind::Project,
        });
        let range = TextRange::new(file_id, 4, 9);
        let fqn =
            FullyQualifiedName::constant(vec![crate::core::RubyConstant::new("VALUE").unwrap()]);
        let fact = TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            RubyType::string(),
            range,
            TypeProvenance::Inferred,
        );
        engine.replace_facts(
            file_id,
            crate::engine::FileFacts {
                types: vec![fact],
                ..Default::default()
            },
            crate::engine::ResolveMode::Immediate,
        );

        let query = TypeQuery::new(&engine, file_id);
        assert_eq!(
            query.get_constant_type_at(&fqn, 6),
            Some(RubyType::string())
        );
        assert_eq!(query.get_constant_type_at(&fqn, 2), None);
    }
}
