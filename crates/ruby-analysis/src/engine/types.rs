use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, NamespaceKind, RubyConstant, RubyMethod,
    RubyType, SourceFileId, TypeFact, TypeResolution, TypeSubject,
};
use crate::engine::lookup_types::{ConstantHover, ConstantHoverKind, VariableTypeKind};
use crate::engine::query::AnalysisQuery;
use crate::engine::resolution::{
    method_facts_in_chain, method_lookup_chain, method_missing_method, namespace_target_exists,
};

type MethodVisitKey = (FullyQualifiedName, SourceFileId, u32, u32);

impl<'a> AnalysisQuery<'a> {
    pub fn method_return_type_at(
        &self,
        name: &str,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.method_return_type_at_with_kind_filter(name, None, file_id, byte_offset)
    }

    pub fn method_return_type_at_with_kind(
        &self,
        name: &str,
        namespace_kind: NamespaceKind,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.method_return_type_at_with_kind_filter(
            name,
            Some(namespace_kind),
            file_id,
            byte_offset,
        )
    }

    fn method_return_type_at_with_kind_filter(
        &self,
        name: &str,
        namespace_kind: Option<NamespaceKind>,
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
                method.as_str() == name
                    && namespace_kind
                        .map(|kind| fact.owner.namespace_kind() == Some(kind))
                        .unwrap_or(true)
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
            .filter(|fact| {
                method_fact.range.file_id == fact.range.file_id
                    && method_fact.range.start_byte <= fact.range.start_byte
                    && fact.range.end_byte <= method_fact.range.end_byte
            })
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
            .or_else(|| self.method_return_type(&method_fact))
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
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        match self.namespace_node_kind(&namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::ClassReference(constant_fqn)),
            GraphNodeKind::Module => Some(RubyType::ModuleReference(constant_fqn)),
        }
    }

    pub fn type_to_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Instance,
                ))
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Singleton,
                ))
            }
            RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Array").expect(
                    "INVARIANT VIOLATED: built-in constant `Array` is invalid. \
                     This is a bug because Ruby built-in constants must be valid Ruby constants. \
                     Fix: correct the hard-coded built-in constant name.",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Hash").expect(
                    "INVARIANT VIOLATED: built-in constant `Hash` is invalid. \
                     This is a bug because Ruby built-in constants must be valid Ruby constants. \
                     Fix: correct the hard-coded built-in constant name.",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Union(_) | RubyType::Unknown => None,
        }
    }

    pub fn constructor_return_type_for_namespace(
        &self,
        namespace_fqn: &FullyQualifiedName,
    ) -> Option<RubyType> {
        if namespace_fqn.namespace_kind() != Some(crate::core::NamespaceKind::Singleton) {
            return None;
        }

        Some(RubyType::Class(FullyQualifiedName::constant(
            namespace_fqn.namespace_parts(),
        )))
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

    pub fn constant_hover(&self, path: &[RubyConstant]) -> Option<ConstantHover> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        let name = path
            .iter()
            .map(|constant| constant.to_string())
            .collect::<Vec<_>>()
            .join("::");

        match self.namespace_node_kind(&namespace_fqn) {
            Some(GraphNodeKind::Class) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Class,
                });
            }
            Some(GraphNodeKind::Module) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Module,
                });
            }
            None => {}
        }

        self.constant_value_type(&constant_fqn)
            .map(|ruby_type| ConstantHover {
                name,
                kind: ConstantHoverKind::Value(ruby_type),
            })
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FullyQualifiedName> {
        self.engine
            .symbol_store()
            .known_namespace_fqns()
            .into_iter()
            .filter_map(|id| self.engine.fqn_for_id(id).cloned())
            .collect()
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_inner(fact, &mut seen)
    }

    fn method_return_type_inner(
        &self,
        fact: &MethodFact,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<crate::core::RubyType> {
        if !seen.insert((
            fact.fqn.clone(),
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )) {
            return None;
        }

        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => return Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => {}
        }

        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            panic!(
                "INVARIANT VIOLATED: method return lookup received a non-method fact {}. \
                 This is a bug because MethodFact FQNs must always use the Method variant. \
                 Fix: validate method facts before engine insertion.",
                fact.fqn
            );
        };
        let mut signature_types = self
            .engine
            .method_facts_matching_owner_name(&fact.owner, method)
            .into_iter()
            .filter(|signature| {
                self.engine
                    .file(signature.range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: RBS method fact references an unregistered source file. \
                         This is a bug because type overlay requires stable signature metadata. \
                         Fix: remove signature facts through per-file replacement.",
                    )
                    .kind
                    == crate::core::SourceKind::Signature
            })
            .filter_map(|signature| {
                match self.engine.type_at(
                    &TypeSubject::MethodReturn(signature.fqn),
                    signature.range.file_id,
                    signature.range.end_byte,
                ) {
                    TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
                    TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
                }
            })
            .collect::<Vec<_>>();
        signature_types.sort_by_key(|ruby_type| ruby_type.to_string());
        signature_types.dedup();
        if !signature_types.is_empty() {
            return Some(RubyType::union(signature_types));
        }

        self.delegate_method_return_type(fact, seen)
    }

    pub fn method_return_type_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(namespace_fqn, method, true, None, &mut seen)
    }

    pub fn method_return_type_for_public_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(namespace_fqn, method, false, None, &mut seen)
    }

    pub fn method_return_type_for_protected_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<crate::core::RubyType> {
        let mut seen = HashSet::new();
        self.method_return_type_for_receiver_inner(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
            &mut seen,
        )
    }

    fn method_return_type_for_receiver_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<crate::core::RubyType> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let ancestor_chain = method_lookup_chain(self.engine, namespace_fqn);
        if let Some((_owner, facts)) = method_facts_in_chain(
            self.engine,
            &ancestor_chain,
            method,
            allow_private,
            protected_caller,
        ) {
            let mut return_types = facts
                .into_iter()
                .filter_map(|fact| self.method_return_type_inner(&fact, seen))
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

        if *method != method_missing_method() {
            return self.method_return_type_for_receiver_inner(
                namespace_fqn,
                &method_missing_method(),
                allow_private,
                protected_caller,
                seen,
            );
        }

        None
    }

    fn delegate_method_return_type(
        &self,
        fact: &MethodFact,
        seen: &mut HashSet<MethodVisitKey>,
    ) -> Option<RubyType> {
        let FullyQualifiedName::Method(_, delegated_method) = &fact.fqn else {
            return None;
        };
        let receiver_method = fact.delegate_receiver?;
        let receiver_type = self.method_return_type_for_receiver_inner(
            &fact.owner,
            &receiver_method,
            true,
            None,
            seen,
        )?;

        let mut return_types = AnalysisQuery::receiver_type_to_method_namespaces(&receiver_type)
            .into_iter()
            .filter_map(|namespace| {
                self.method_return_type_for_receiver_inner(
                    &namespace,
                    delegated_method,
                    true,
                    None,
                    seen,
                )
            })
            .collect::<Vec<_>>();
        return_types.sort_by_key(|ruby_type| ruby_type.to_string());
        return_types.dedup();
        match return_types.len() {
            0 => None,
            1 => return_types.pop(),
            _ => Some(RubyType::union(return_types)),
        }
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
