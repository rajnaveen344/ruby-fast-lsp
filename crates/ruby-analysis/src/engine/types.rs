use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, RubyConstant, RubyMethod, RubyType,
    SourceFileId, SymbolKind, TypeFact, TypeResolution, TypeSubject,
};
use crate::engine::query::AnalysisQuery;
use crate::engine::query_types::VariableTypeKind;
use crate::engine::resolution::{method_lookup_chain, namespace_target_exists};

impl<'a> AnalysisQuery<'a> {
    pub fn method_return_type_at(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_store()
            .facts_in_file(file_id)
            .into_iter()
            .find(|fact| {
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    return false;
                };
                method.as_str() == name
                    && fact.range.start_byte <= byte_offset
                    && byte_offset <= fact.range.end_byte
            })?;

        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| match &fact.subject {
                TypeSubject::MethodReturn(method) if method == &method_fact.fqn => Some(fact),
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

    pub fn parameter_type_at(
        &self,
        method_name: &str,
        param_name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let method_fact = self
            .engine
            .method_store()
            .facts_in_file(file_id)
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

    pub fn variable_type_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match (&fact.subject, kind) {
                (
                    TypeSubject::Local {
                        scope_id: _,
                        name: fact_name,
                    },
                    VariableTypeKind::Local,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (
                    TypeSubject::InstanceVariable {
                        name: fact_name, ..
                    },
                    VariableTypeKind::Instance,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (
                    TypeSubject::ClassVariable {
                        name: fact_name, ..
                    },
                    VariableTypeKind::Class,
                ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global)
                    if fact_name == name && fact.ruby_type != RubyType::Unknown =>
                {
                    Some(fact)
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
                ) => None,
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn variable_type_any_before(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Local {
                    scope_id: _,
                    name: fact_name,
                }
                | TypeSubject::InstanceVariable {
                    name: fact_name, ..
                }
                | TypeSubject::ClassVariable {
                    name: fact_name, ..
                } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                TypeSubject::GlobalVariable(fact_name)
                    if fact_name == name && fact.ruby_type != RubyType::Unknown =>
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

    pub fn variable_type_in_file(
        &self,
        kind: VariableTypeKind,
        name: &str,
        file_id: SourceFileId,
    ) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| Self::variable_type_fact_match(fact, kind, name))
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    pub fn namespace_node_kind(&self, namespace_fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.engine
            .graph_nodes_for(namespace_fqn)
            .iter()
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
    }

    pub fn namespace_type(&self, namespace_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.namespace_node_kind(namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::Class(namespace_fqn.clone())),
            GraphNodeKind::Module => Some(RubyType::Module(namespace_fqn.clone())),
        }
    }

    pub fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::Constant(path.to_vec());
        match self.namespace_node_kind(&namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::ClassReference(constant_fqn)),
            GraphNodeKind::Module => Some(RubyType::ModuleReference(constant_fqn)),
        }
    }

    pub fn constant_value_type(&self, constant_fqn: &FullyQualifiedName) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_for(&TypeSubject::Constant(constant_fqn.clone()))
            .iter()
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.ruby_type.clone())
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FullyQualifiedName> {
        self.engine
            .all_symbol_facts()
            .into_iter()
            .filter(|fact| matches!(fact.kind, SymbolKind::Class | SymbolKind::Module))
            .filter_map(|fact| fact.fqn.to_instance_namespace())
            .collect()
    }

    pub fn method_fact_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let mut facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .cloned()
                .collect::<Vec<_>>();

            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();

            match facts.len() {
                0 => continue,
                1 => return facts.pop(),
                _ => return None,
            }
        }

        None
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<crate::core::RubyType> {
        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
        }
    }

    pub fn method_return_type_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .collect::<Vec<_>>();

            if facts.is_empty() {
                continue;
            }

            let mut return_types = facts
                .into_iter()
                .filter_map(|fact| self.method_return_type(fact))
                .collect::<Vec<_>>();

            if return_types.is_empty() {
                return None;
            }

            return_types.sort_by_key(|ruby_type| ruby_type.to_string());
            return_types.dedup();
            return match return_types.len() {
                1 => return_types.pop(),
                _ => Some(crate::core::RubyType::union(return_types)),
            };
        }

        None
    }

    fn variable_type_fact_match(
        fact: TypeFact,
        kind: VariableTypeKind,
        name: &str,
    ) -> Option<TypeFact> {
        match (&fact.subject, kind) {
            (
                TypeSubject::Local {
                    scope_id: _,
                    name: fact_name,
                },
                VariableTypeKind::Local,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (
                TypeSubject::InstanceVariable {
                    name: fact_name, ..
                },
                VariableTypeKind::Instance,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (
                TypeSubject::ClassVariable {
                    name: fact_name, ..
                },
                VariableTypeKind::Class,
            ) if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
            (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global)
                if fact_name == name && fact.ruby_type != RubyType::Unknown =>
            {
                Some(fact)
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
            ) => None,
        }
    }
}
