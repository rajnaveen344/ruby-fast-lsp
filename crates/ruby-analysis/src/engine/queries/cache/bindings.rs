//! Parameter, RBS contract, and variable binding type queries.

use crate::core::{
    FullyQualifiedName, RubyType, SourceFileId, SourceKind, TypeFact, TypeResolution, TypeSubject,
};
use crate::engine::queries::lookup::types::VariableTypeKind;
use crate::engine::queries::View;
use crate::invariant::ExpectInvariant;

impl<'a> View<'a> {
    pub fn parameter_type_at(
        &self,
        method_name: &str,
        param_name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == method_name
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter { method, name }
                    if method == &method_fact.fqn
                        && name == param_name
                        && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
                }
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

    /// Return one complete project-RBS parameter contract for a Ruby method.
    ///
    /// Declaration ownership and source kind are part of the proof: a Ruby
    /// implementation fact cannot accidentally become its own contract, and
    /// instance/singleton homonyms remain isolated even though their method
    /// subjects share one Ruby name FQN.
    pub fn rbs_parameter_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_name: &str,
    ) -> Option<RubyType> {
        let signature_methods = self
            .engine
            .method_facts_for(method)
            .into_iter()
            .filter(|fact| {
                fact.owner == *owner
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind == SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if signature_methods.is_empty() {
            return None;
        }

        let mut contract_types = Vec::with_capacity(signature_methods.len());
        for signature in signature_methods {
            let contract = self
                .engine
                .type_store()
                .facts_in_file(signature.range.file_id)
                .into_iter()
                .find_map(|fact| match &fact.subject {
                    TypeSubject::Parameter { method, name }
                        if method == &signature.fqn
                            && name == parameter_name
                            && signature.range.start_byte <= fact.range.start_byte
                            && fact.range.end_byte <= signature.range.end_byte
                            && fact.ruby_type != RubyType::Unknown =>
                    {
                        Some(fact.ruby_type)
                    }
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_) => None,
                });
            contract_types.push(contract?);
        }

        let contract = RubyType::union(contract_types);
        (!RubyType::contains_unknown(&contract)).then_some(contract)
    }

    /// Return the exhaustive project-RBS return contract for one owner.
    /// Every matching signature declaration must carry a complete type fact;
    /// otherwise diagnostics and body inference stay fail-closed.
    pub fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        let signature_methods = self
            .engine
            .method_facts_for(method)
            .into_iter()
            .filter(|fact| {
                fact.owner == *owner
                    && self
                        .engine
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind == SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if signature_methods.is_empty() {
            return None;
        }

        let mut contract_types = Vec::with_capacity(signature_methods.len());
        for signature in signature_methods {
            let contract = self
                .engine
                .type_store()
                .facts_in_file(signature.range.file_id)
                .into_iter()
                .find_map(|fact| match &fact.subject {
                    TypeSubject::MethodReturn(method)
                        if method == &signature.fqn
                            && signature.range.start_byte <= fact.range.start_byte
                            && fact.range.end_byte <= signature.range.end_byte
                            && fact.ruby_type != RubyType::Unknown =>
                    {
                        Some(fact.ruby_type)
                    }
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_) => None,
                });
            contract_types.push(contract?);
        }

        let contract = RubyType::union(contract_types);
        (!RubyType::contains_unknown(&contract)).then_some(contract)
    }

    pub fn variable_type_before_in_owner(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        invariant!(
            matches!(
                kind,
                VariableTypeKind::Instance | VariableTypeKind::Class | VariableTypeKind::Global
            ),
            what = "an owner-aware variable query received a local or constant kind",
            why =
                "locals require a lexical scope and constants require lexical constant resolution",
            fix = "use local_variable_type_at or the constant query instead",
        );

        let matching = self
            .engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter(|fact| match (&fact.subject, kind) {
                (
                    TypeSubject::InstanceVariable {
                        owner: fact_owner,
                        name: fact_name,
                    },
                    VariableTypeKind::Instance,
                ) => fact_name == name && fact_owner == owner,
                (
                    TypeSubject::ClassVariable {
                        owner: fact_owner,
                        name: fact_name,
                    },
                    VariableTypeKind::Class,
                ) => fact_name == name && fact_owner.namespace_parts() == owner.namespace_parts(),
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => {
                    fact_name == name
                }
                (
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_),
                    VariableTypeKind::Local
                    | VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
                    | VariableTypeKind::Constant,
                ) => false,
            });

        Self::latest_unambiguous_concrete_type(matching)
    }

    /// Return the type fact attached to one exact variable write token.
    ///
    /// Unlike a flow lookup, an assignment inlay must describe this write's
    /// right-hand side. Falling back to an earlier concrete write when this
    /// exact write is Unknown would publish a type with no proof. Duplicate
    /// producers may agree on the same fact; any conflicting payload fails
    /// closed to Unknown.
    pub fn variable_assignment_type_at(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
        name_start_offset: u32,
        name_end_offset: u32,
    ) -> Option<RubyType> {
        invariant!(
            name_start_offset <= name_end_offset,
            what = "a variable assignment name range is reversed",
            why = "exact-write type queries require a normalized source range",
            fix = "pass the Prism name location without swapping its offsets",
        );
        let mut best_span = None;
        let mut best_type = None;
        let mut conflicting_best_type = false;
        for fact in self
            .engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| {
                fact.range.start_byte <= name_start_offset && name_end_offset <= fact.range.end_byte
            })
        {
            let matches = match (&fact.subject, kind) {
                (
                    TypeSubject::Local {
                        scope_id: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Local,
                ) => fact_name == name,
                (
                    TypeSubject::InstanceVariable {
                        owner: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Instance,
                ) => fact_name == name,
                (
                    TypeSubject::ClassVariable {
                        owner: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Class,
                ) => fact_name == name,
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => {
                    fact_name == name
                }
                (TypeSubject::Constant(fqn), VariableTypeKind::Constant) => fqn.name() == name,
                (
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_),
                    VariableTypeKind::Local
                    | VariableTypeKind::Instance
                    | VariableTypeKind::Class
                    | VariableTypeKind::Global
                    | VariableTypeKind::Constant,
                ) => false,
            };
            if !matches {
                continue;
            }
            let span = fact
                .range
                .end_byte
                .checked_sub(fact.range.start_byte)
                .expect_invariant(
                    "a stored type fact range is reversed",
                    "TypeFact ranges are normalized",
                    "build ranges with TextRange::new and keep them normalized on replacement",
                );
            match best_span {
                None => {
                    best_span = Some(span);
                    best_type = Some(fact.ruby_type);
                }
                Some(current_span) if span < current_span => {
                    best_span = Some(span);
                    best_type = Some(fact.ruby_type);
                    conflicting_best_type = false;
                }
                Some(current_span) if span == current_span => {
                    if best_type.as_ref() != Some(&fact.ruby_type) {
                        conflicting_best_type = true;
                    }
                }
                Some(_) => {}
            }
        }

        if conflicting_best_type {
            Some(RubyType::Unknown)
        } else {
            best_type
        }
    }

    pub fn local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        match self.engine.type_store().type_at(
            &TypeSubject::Local {
                scope_id,
                name: name.to_string(),
            },
            file_id,
            byte_offset,
        ) {
            TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
            TypeResolution::Ambiguous(_) => return None,
            TypeResolution::Unresolved => {}
        }

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Parameter {
                    method: _,
                    name: fact_name,
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

    fn latest_unambiguous_concrete_type(facts: impl Iterator<Item = TypeFact>) -> Option<RubyType> {
        let mut latest_start = None;
        let mut latest_type = None;
        let mut ambiguous = false;
        for fact in facts {
            match latest_start {
                None => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                }
                Some(start) if fact.range.start_byte > start => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                    ambiguous = false;
                }
                Some(start) if fact.range.start_byte == start => {
                    if latest_type.as_ref() != Some(&fact.ruby_type) {
                        ambiguous = true;
                    }
                }
                Some(_) => {}
            }
        }

        let latest_type = latest_type?;
        if latest_type == RubyType::Unknown || ambiguous {
            None
        } else {
            Some(latest_type)
        }
    }
}
