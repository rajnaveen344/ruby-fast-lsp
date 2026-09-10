use crate::core::{
    FullyQualifiedName, GraphEdgeKind, InferenceTelemetry, MethodReturnEquation, RubyConstant,
    RubyMethod, RubyType, TextRange, TypeInferenceOutcome, TypeProvenance, TypeResolution,
    TypeSubject, UnknownReason,
};
use crate::engine::AnalysisQuery;
use crate::indexer::fact_collector::FactCollector;
use crate::inference::method::recursive::solve_method_return_equations_with_telemetry;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::rbs::{
    get_rbs_method_return_type_as_ruby_type, get_rbs_method_return_type_with_type_args,
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct MethodReturnEvidence {
    /// Compact method-return equations collected during the ordinary semantic
    /// traversal and solved once when the program traversal completes.
    pub(in crate::indexer::fact_collector) equations:
        BTreeMap<Vec<RubyConstant>, Vec<MethodReturnEquation>>,
    pub(in crate::indexer::fact_collector) finalized_counts: HashMap<Vec<RubyConstant>, usize>,
    pub(in crate::indexer::fact_collector) telemetry:
        BTreeMap<Vec<RubyConstant>, InferenceTelemetry>,
    pub(in crate::indexer::fact_collector) outcomes:
        BTreeMap<FullyQualifiedName, TypeInferenceOutcome>,
}

pub(in crate::indexer::fact_collector) struct InferredMethodContext {
    pub(in crate::indexer::fact_collector) fqn: FullyQualifiedName,
    pub(in crate::indexer::fact_collector) namespace_parts: Vec<RubyConstant>,
    pub(in crate::indexer::fact_collector) instance_owner_fqn: FullyQualifiedName,
    pub(in crate::indexer::fact_collector) param_types:
        Vec<(String, RubyType, TextRange, TypeProvenance)>,
    pub(in crate::indexer::fact_collector) definition_range: TextRange,
}

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn resolve_method_return_type_outcome_with_private(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> TypeInferenceOutcome {
        if *receiver_type == RubyType::Unknown {
            return TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver);
        }

        if shape_reads::is_shape_only(receiver_type) {
            if let Some(outcome) =
                shape_reads::argument_free_method_return(receiver_type, method_name)
            {
                return match outcome {
                    Ok(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
                    Err(reason) => TypeInferenceOutcome::unknown(reason),
                };
            }
            if shape_reads::operation_requires_call_arguments(method_name) {
                return TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn);
            }
            let projected =
                shape_reads::generic_hash_projection(receiver_type).unwrap_or(RubyType::Unknown);
            if projected == RubyType::Unknown {
                return TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn);
            }
            return self.resolve_method_return_type_outcome_with_private(
                &projected,
                method_name,
                allow_private,
            );
        }

        if let RubyType::Union(types) = receiver_type {
            let mut return_types = Vec::with_capacity(types.len());
            for ty in types {
                let outcome = self.resolve_method_return_type_outcome_with_private(
                    ty,
                    method_name,
                    allow_private,
                );
                let Some(return_type) = outcome.into_proven_type() else {
                    return TypeInferenceOutcome::unknown(UnknownReason::IncompleteUnionMember);
                };
                return_types.push(return_type);
            }
            return TypeInferenceOutcome::from_optional(
                (!return_types.is_empty()).then(|| RubyType::union(return_types)),
                UnknownReason::IncompleteUnionMember,
            );
        }

        if method_name == "new" {
            if let RubyType::ClassReference(fqn) = receiver_type {
                return TypeInferenceOutcome::proven(RubyType::Class(fqn.clone()));
            }
        }

        if RubyMethod::new(method_name).is_err() {
            return TypeInferenceOutcome::unknown(UnknownReason::InvalidMethodName);
        }

        let return_type = if matches!(receiver_type, RubyType::Array(_) | RubyType::Hash(_, _)) {
            resolve_rbs_method_return_type(receiver_type, method_name)
        } else if self.options.resolve_analysis_method_returns {
            self.resolve_method_return_type_from_analysis(receiver_type, method_name, allow_private)
                .or_else(|| resolve_rbs_method_return_type(receiver_type, method_name))
        } else {
            resolve_rbs_method_return_type(receiver_type, method_name)
        };
        TypeInferenceOutcome::from_optional(return_type, UnknownReason::UnresolvedMethodReturn)
    }

    fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> Option<RubyType> {
        let method = RubyMethod::new(method_name).ok()?;
        let namespace = receiver_namespace_for_analysis(receiver_type)?;
        let (caller_parts, caller_kind) = self.scope_tracker.implicit_receiver_context();
        let caller_namespace = FullyQualifiedName::namespace_with_kind(caller_parts, caller_kind);
        let local_method_fqn = FullyQualifiedName::method(namespace.namespace_parts(), method);
        if self.direct_method_fact_is_visible(
            &local_method_fqn,
            &namespace,
            &namespace,
            allow_private,
            &caller_namespace,
        ) {
            if let Some(return_type) = self.local_method_return_type(&local_method_fqn) {
                return Some(return_type);
            }
        }
        if let Some(return_type) = self.local_superclass_method_return_type(
            &namespace,
            &method,
            allow_private,
            &caller_namespace,
        ) {
            return Some(return_type);
        }

        // Same-file facts are not in the engine yet. Engine lookup is the
        // TypeTracker path: one cached receiver return, not a fresh callee
        // vector plus per-callee type_at on every expression.
        let engine = self.semantics.engine.read();
        let query = AnalysisQuery::new(&engine);
        if allow_private {
            query.method_return_type_for_receiver_cached(
                &namespace,
                &method,
                &self.semantics.query_cache,
            )
        } else {
            query.method_return_type_for_protected_receiver_cached(
                &namespace,
                &method,
                &caller_namespace,
                &self.semantics.query_cache,
            )
        }
    }

    fn local_method_return_type(&self, method_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.facts.types.type_at(
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

    fn local_superclass_method_return_type(
        &self,
        receiver: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        caller_namespace: &FullyQualifiedName,
    ) -> Option<RubyType> {
        let mut current = receiver.clone();
        let mut visited = HashSet::new();
        while visited.insert(current.clone()) {
            let mut parents = self
                .facts
                .direct
                .graph_edges
                .iter()
                .filter(|edge| edge.source == current && edge.kind == GraphEdgeKind::Superclass)
                .map(|edge| edge.target.clone())
                .collect::<Vec<_>>();
            parents.sort_by_key(ToString::to_string);
            parents.dedup();
            match parents.len() {
                0 => return None,
                1 => current = parents.pop().expect(
                    "INVARIANT VIOLATED: one local superclass disappeared after length validation. This is a bug because local same-pass inheritance lookup must be deterministic. Fix: keep parent extraction and selection atomic.",
                ),
                2.. => panic!(
                    "INVARIANT VIOLATED: namespace `{}` has multiple local superclass edges. This is a bug because Ruby classes have exactly one superclass. Fix: reject conflicting generated or parser superclass facts before same-pass inference.",
                    current
                ),
            }

            let method_fqn = FullyQualifiedName::method(current.namespace_parts(), method.clone());
            if self.direct_method_fact_is_visible(
                &method_fqn,
                &current,
                receiver,
                allow_private,
                caller_namespace,
            ) {
                if let Some(return_type) = self.local_method_return_type(&method_fqn) {
                    return Some(return_type);
                }
            }
        }

        panic!(
            "INVARIANT VIOLATED: local superclass cycle encountered while inferring `{}` on `{}`. This is a bug because inheritance cycles cannot define a valid Ruby MRO. Fix: reject cyclic graph facts before same-pass inference.",
            method, receiver
        );
    }

    fn direct_method_fact_is_visible(
        &self,
        method_fqn: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        receiver_namespace: &FullyQualifiedName,
        allow_private: bool,
        caller_namespace: &FullyQualifiedName,
    ) -> bool {
        self.facts.direct.methods.iter().any(|fact| {
            &fact.fqn == method_fqn
                && fact.owner.namespace_parts() == owner.namespace_parts()
                && fact.owner.namespace_kind() == owner.namespace_kind()
                && match self.direct_effective_visibility_for_method(
                    fact,
                    owner,
                    receiver_namespace,
                    caller_namespace,
                ) {
                    crate::core::MethodVisibility::Public => true,
                    crate::core::MethodVisibility::Private => allow_private,
                    crate::core::MethodVisibility::Protected => {
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
    ) -> crate::core::MethodVisibility {
        let FullyQualifiedName::Method(_, method) = &fact.fqn else {
            return fact.visibility;
        };
        let mut overrides = self
            .facts
            .direct
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
        RubyType::Hash(_, _) | RubyType::Shape(_) => builtin_namespace("Hash"),
        RubyType::Literal(value) => receiver_namespace_for_analysis(&value.widened_type()),
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
        RubyType::Hash(_, _) | RubyType::Shape(_) => Some("Hash".to_string()),
        RubyType::Literal(value) => rbs_class_name(&value.widened_type()),
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
        RubyType::Shape(shape) => type_args_for_receiver(&shape.generic_hash_type()),
        RubyType::Literal(_) => Vec::new(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => Vec::new(),
    }
}

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn finalize_method_return_equations_for_namespace(
        &mut self,
        namespace: &[RubyConstant],
    ) {
        let Some(equations) = self.method_returns.equations.get(namespace) else {
            return;
        };
        let equation_count = equations.len();
        if self
            .method_returns
            .finalized_counts
            .get(namespace)
            .is_some_and(|finalized| *finalized == equation_count)
        {
            return;
        }
        let solve_result = solve_method_return_equations_with_telemetry(equations);
        let solved = solve_result.outcomes;
        let file_id = self.document.analysis_file_id();
        self.facts
            .types
            .update_inferred_method_return_types_in_file(
                file_id,
                solved
                    .iter()
                    .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
            );
        self.method_returns.outcomes.extend(solved);
        self.method_returns
            .telemetry
            .insert(namespace.to_vec(), solve_result.telemetry);
        self.method_returns
            .finalized_counts
            .insert(namespace.to_vec(), equation_count);
    }

    pub(in crate::indexer::fact_collector) fn finalize_all_method_return_equations(&mut self) {
        let namespaces = self
            .method_returns
            .equations
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for namespace in namespaces {
            self.finalize_method_return_equations_for_namespace(&namespace);
        }
    }

    pub fn method_return_outcomes(&self) -> &BTreeMap<FullyQualifiedName, TypeInferenceOutcome> {
        &self.method_returns.outcomes
    }

    pub fn inference_telemetry(&self) -> InferenceTelemetry {
        let mut aggregate = InferenceTelemetry::default();
        for telemetry in self.method_returns.telemetry.values() {
            aggregate.merge(telemetry);
        }
        aggregate.observe_max_live_shape_aliases(self.expressions.max_live_shape_aliases);
        aggregate
    }
}
