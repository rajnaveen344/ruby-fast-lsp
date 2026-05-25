use crate::core::{FullyQualifiedName, RubyConstant, RubyMethod, TypeResolution, TypeSubject};
use crate::engine::AnalysisQuery;

use crate::inference::rbs::{
    get_rbs_method_return_type_as_ruby_type, get_rbs_method_return_type_with_type_args,
};
use crate::inference::RubyType;

use super::FactCollector;

impl FactCollector {
    pub(super) fn resolve_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        self.resolve_method_return_type_with_private(receiver_type, method_name, true)
    }

    pub(super) fn resolve_method_return_type_with_private(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> Option<RubyType> {
        if *receiver_type == RubyType::Unknown {
            return None;
        }

        if let RubyType::Union(types) = receiver_type {
            let mut return_types = Vec::new();
            for ty in types {
                if let Some(return_type) =
                    self.resolve_method_return_type_with_private(ty, method_name, allow_private)
                {
                    if !return_types.contains(&return_type) {
                        return_types.push(return_type);
                    }
                }
            }
            return match return_types.len() {
                0 => None,
                1 => return_types.pop(),
                2.. => Some(RubyType::union(return_types)),
            };
        }

        if method_name == "new" {
            if let RubyType::ClassReference(fqn) = receiver_type {
                return Some(RubyType::Class(fqn.clone()));
            }
        }

        if self.resolve_analysis_method_returns {
            self.resolve_method_return_type_from_analysis(receiver_type, method_name, allow_private)
                .or_else(|| resolve_rbs_method_return_type(receiver_type, method_name))
        } else {
            resolve_rbs_method_return_type(receiver_type, method_name)
        }
    }

    fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> Option<RubyType> {
        let method = RubyMethod::new(method_name).ok()?;
        let namespace = receiver_namespace_for_analysis(receiver_type)?;
        let engine = self.analysis_engine.lock();
        let query = AnalysisQuery::new(&engine);
        let caller_namespace = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            crate::core::NamespaceKind::Instance,
        );
        let callees = if allow_private {
            query.resolve_method_callees(&namespace, &method)?
        } else {
            query.resolve_protected_method_callees(&namespace, &method, &caller_namespace)?
        };

        let mut return_types = Vec::new();
        for callee in callees {
            if callee.definition_ranges.is_empty() {
                continue;
            }

            let method_fqn =
                FullyQualifiedName::method(callee.owner.namespace_parts(), callee.method);
            if self.direct_method_fact_is_visible(
                &method_fqn,
                &callee.owner,
                &namespace,
                allow_private,
                &caller_namespace,
            ) {
                if let Some(return_type) = self.local_method_return_type(&method_fqn) {
                    if !return_types.contains(&return_type) {
                        return_types.push(return_type);
                    }
                    continue;
                }
            }

            let return_type = if allow_private {
                query.method_return_type_for_receiver(&callee.owner, &callee.method)
            } else {
                query.method_return_type_for_protected_receiver(
                    &callee.owner,
                    &callee.method,
                    &caller_namespace,
                )
            };
            if let Some(return_type) = return_type {
                if !return_types.contains(&return_type) {
                    return_types.push(return_type);
                }
            }
        }

        match return_types.len() {
            0 => None,
            1 => return_types.pop(),
            2.. => Some(RubyType::union(return_types)),
        }
    }

    fn local_method_return_type(&self, method_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.type_store.type_at(
            &TypeSubject::MethodReturn(method_fqn.clone()),
            self.document.analysis_file_id(),
            u32::MAX,
        ) {
            TypeResolution::Resolved(fact) if fact.ruby_type != RubyType::Unknown => {
                Some(fact.ruby_type)
            }
            TypeResolution::Resolved(_)
            | TypeResolution::Ambiguous(_)
            | TypeResolution::Unresolved => None,
        }
    }

    fn direct_method_fact_is_visible(
        &self,
        method_fqn: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        receiver_namespace: &FullyQualifiedName,
        allow_private: bool,
        caller_namespace: &FullyQualifiedName,
    ) -> bool {
        self.direct_facts.methods.iter().any(|fact| {
            &fact.fqn == method_fqn
                && fact.owner.namespace_parts() == owner.namespace_parts()
                && fact.owner.namespace_kind() == owner.namespace_kind()
                && match self.direct_effective_visibility_for_method(
                    fact,
                    owner,
                    receiver_namespace,
                    caller_namespace,
                ) {
                    crate::core::method_store::MethodVisibility::Public => true,
                    crate::core::method_store::MethodVisibility::Private => allow_private,
                    crate::core::method_store::MethodVisibility::Protected => {
                        allow_private
                            || fact.owner.namespace_parts() == caller_namespace.namespace_parts()
                    }
                }
        })
    }

    fn direct_effective_visibility_for_method(
        &self,
        fact: &crate::core::MethodFact,
        owner: &FullyQualifiedName,
        receiver_namespace: &FullyQualifiedName,
        caller_namespace: &FullyQualifiedName,
    ) -> crate::core::method_store::MethodVisibility {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            return fact.visibility;
        };
        let mut overrides = self
            .direct_facts
            .method_visibility_overrides
            .iter()
            .filter(|override_fact| {
                override_fact.method == *method
                    && (override_fact.owner.namespace_parts() == caller_namespace.namespace_parts()
                        || override_fact.owner.namespace_parts()
                            == receiver_namespace.namespace_parts()
                        || override_fact.owner.namespace_parts() == owner.namespace_parts())
            })
            .collect::<Vec<_>>();
        overrides.sort_by_key(|override_fact| {
            (
                override_fact.range.file_id,
                override_fact.range.start_byte,
                override_fact.range.end_byte,
            )
        });
        overrides
            .last()
            .map(|override_fact| override_fact.visibility)
            .unwrap_or(fact.visibility)
    }
}

fn receiver_namespace_for_analysis(receiver_type: &RubyType) -> Option<FullyQualifiedName> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => fqn.to_instance_namespace(),
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            fqn.to_singleton_namespace()
        }
        RubyType::Array(_) => builtin_namespace("Array"),
        RubyType::Hash(_, _) => builtin_namespace("Hash"),
        RubyType::Union(_) => None,
        RubyType::Unknown => None,
    }
}

fn builtin_namespace(name: &str) -> Option<FullyQualifiedName> {
    let constant = RubyConstant::new(name).ok()?;
    Some(FullyQualifiedName::namespace(vec![constant]))
}

fn resolve_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    let class_name = rbs_class_name(receiver_type)?;
    let is_singleton = matches!(
        receiver_type,
        RubyType::ClassReference(_) | RubyType::ModuleReference(_)
    );
    let type_args = type_args_for_receiver(receiver_type);
    if type_args.is_empty() {
        get_rbs_method_return_type_as_ruby_type(&class_name, method_name, is_singleton)
    } else {
        get_rbs_method_return_type_with_type_args(
            &class_name,
            method_name,
            is_singleton,
            &type_args,
        )
    }
}

fn rbs_class_name(receiver_type: &RubyType) -> Option<String> {
    match receiver_type {
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(ToString::to_string),
        RubyType::Array(_) => Some("Array".to_string()),
        RubyType::Hash(_, _) => Some("Hash".to_string()),
        RubyType::Union(_) => None,
        RubyType::Unknown => None,
    }
}

fn type_args_for_receiver(receiver_type: &RubyType) -> Vec<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => match element_types.len() {
            0 => Vec::new(),
            1 => vec![element_types[0].clone()],
            2.. => vec![RubyType::union(element_types.clone())],
        },
        RubyType::Hash(key_types, value_types) => {
            let key = match key_types.len() {
                0 => RubyType::Unknown,
                1 => key_types[0].clone(),
                2.. => RubyType::union(key_types.clone()),
            };
            let value = match value_types.len() {
                0 => RubyType::Unknown,
                1 => value_types[0].clone(),
                2.. => RubyType::union(value_types.clone()),
            };
            vec![key, value]
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => Vec::new(),
    }
}
