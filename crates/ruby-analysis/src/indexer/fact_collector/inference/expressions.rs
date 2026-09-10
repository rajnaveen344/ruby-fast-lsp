use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyType, ShapeConstructionError, TextRange, TypeFact,
    TypeInferenceOutcome, TypeProvenance, TypeSubject, UnknownReason,
};
use crate::indexer::fact_collector::FactCollector;
use crate::inference::control_flow;
use crate::inference::r#type::literal::{
    infer_array_literal_type_fallible, infer_hash_literal_type_fallible, literal_key,
    literal_shape_construction_unknown_reason, project_immediate_hash_receiver_type,
    LiteralAnalyzer,
};
use crate::inference::r#type::shape as shape_reads;
use ruby_prism::*;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct ExpressionEvidence {
    pub(in crate::indexer::fact_collector) max_live_shape_aliases: usize,
    /// Immediate call proofs keyed by expression range. Lookups during
    /// nested-receiver and assignment inference must be O(1); a Vec scan was
    /// repeating work already paid during `visit_call_node`.
    pub(in crate::indexer::fact_collector) call_outcomes: HashMap<TextRange, TypeInferenceOutcome>,
    /// Call expressions that have a retained method candidate capable of
    /// producing a concrete outcome after complete engine resolution. This is
    /// collector-local and prevents terminal Unknown chains from becoming
    /// retained outer method candidates.
    pub(in crate::indexer::fact_collector) deferred_calls: HashSet<TextRange>,
    pub(in crate::indexer::fact_collector) local_reads: Vec<(TextRange, RubyType)>,
    pub(in crate::indexer::fact_collector) unknown_reasons: HashMap<TextRange, UnknownReason>,
}

impl FactCollector {
    pub fn assignment_type_and_provenance(&self, value: &Node<'_>) -> (RubyType, TypeProvenance) {
        let value_range = self.direct_range(&value.location());
        if let Some(runtime_fact) =
            self.direct_expression_fact(value_range, Some(TypeProvenance::Runtime))
        {
            return (runtime_fact.ruby_type.clone(), TypeProvenance::Runtime);
        }
        (
            self.infer_assignment_type_from_value(value),
            TypeProvenance::Assignment,
        )
    }

    pub fn direct_push_expression_type(
        &mut self,
        node: &Node<'_>,
        ruby_type: RubyType,
        provenance: TypeProvenance,
    ) {
        if ruby_type == RubyType::Unknown {
            return;
        }
        let range = self.direct_range(&node.location());
        if self.facts.expression_indexes
            .get(&range)
            .into_iter()
            .flatten()
            .any(|index| {
                self.facts.direct
                    .types
                    .get(*index)
                    .expect(
                        "INVARIANT VIOLATED: the direct expression deduplication index points outside the append-only fact vector. This is a bug because expression indexes and facts must be appended atomically. Fix: use push_direct_expression_fact for every expression fact.",
                    )
                    .ruby_type
                    == ruby_type
            })
        {
            return;
        }
        let fact = TypeFact::new(TypeSubject::Expression(range), ruby_type, range, provenance);
        self.facts.types.add(fact.clone());
        self.push_direct_expression_fact(fact);
    }

    /// Reuse a call's already-recorded proof instead of walking the same AST
    /// node again. Nested receivers and later assignment/HO lookups consult
    /// this after the inner CallNode has classified its immediate outcome.
    pub(in crate::indexer::fact_collector) fn recorded_call_expression_outcome(
        &self,
        range: TextRange,
    ) -> Option<&TypeInferenceOutcome> {
        self.expressions.call_outcomes.get(&range)
    }

    pub(in crate::indexer::fact_collector) fn record_immediate_call_outcome(
        &mut self,
        range: TextRange,
        outcome: TypeInferenceOutcome,
    ) {
        self.expressions.call_outcomes.insert(range, outcome);
    }

    /// Infer type from a value node during indexing.
    ///
    /// This recursively walks the AST to infer types:
    /// - Literals → their type (String, Integer, etc.)
    /// - Constants → ClassReference
    /// - Local variables → look up their type
    /// - Method calls → recursively infer receiver type, then resolve method return type
    pub fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        self.infer_type_from_value_with_locals(value_node, &HashMap::new())
    }

    pub(in crate::indexer::fact_collector) fn infer_type_from_value_with_locals(
        &self,
        value_node: &Node,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let expression_range = self.direct_range(&value_node.location());
        if let Some(fact) = self.direct_expression_fact(expression_range, None) {
            return fact.ruby_type.clone();
        }
        if value_node.as_call_node().is_some() {
            if let Some(outcome) = self.recorded_call_expression_outcome(expression_range) {
                return outcome.clone().into_ruby_type();
            }
        }
        if let Some(statements) = value_node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .map(|node| self.infer_type_from_value_with_locals(&node, local_types))
                .unwrap_or_else(RubyType::nil_class);
        }
        if let Some(result) = self.infer_collection_type_from_value(value_node, local_types) {
            return result.unwrap_or(RubyType::Unknown);
        }
        if let Some(if_node) = value_node.as_if_node() {
            return self.infer_if_expression_type(&if_node, local_types);
        }
        if let Some(unless_node) = value_node.as_unless_node() {
            return self.infer_unless_expression_type(&unless_node, local_types);
        }
        if let Some(case_node) = value_node.as_case_node() {
            return self.infer_case_expression_type(&case_node, local_types);
        }
        if let Some(begin_node) = value_node.as_begin_node() {
            return self.infer_begin_expression_type(&begin_node, local_types);
        }
        if let Some(rescue_modifier) = value_node.as_rescue_modifier_node() {
            return self.infer_rescue_modifier_expression_type(&rescue_modifier, local_types);
        }

        // 1. Try literal analysis first (String, Integer, Array, Hash, Symbol, etc.)
        if let Some(literal_type) = LiteralAnalyzer::new().analyze_literal(value_node) {
            return literal_type;
        }

        // 2. Constant read/path: resolve against the active lexical namespace before
        // projecting the class object. This also preserves the target identity when
        // one constant aliases another class/module object.
        if let Some(reference) = crate::indexer::mixin_ref_from_node(value_node) {
            let lexical_context = self.scope_tracker.get_ns_stack();
            if let Some((_constant, ruby_type)) = self.resolve_constant_value_type_from(
                &reference.parts,
                reference.absolute,
                &lexical_context,
            ) {
                return ruby_type;
            }
            if let Some(fqn) = self.constant_reference_type(value_node) {
                return RubyType::ClassReference(fqn);
            }
        }

        if let Some(ret) = value_node.as_return_node() {
            let Some(arguments) = ret.arguments() else {
                return RubyType::nil_class();
            };
            let args = arguments.arguments().iter().collect::<Vec<_>>();
            return match args.len() {
                0 => RubyType::nil_class(),
                1 => self.infer_type_from_value_with_locals(&args[0], local_types),
                2.. => RubyType::Array(RubyType::canonical_union_members(
                    args.iter()
                        .map(|arg| self.infer_type_from_value_with_locals(arg, local_types)),
                )),
            };
        }

        // 4. Local variable read: look up the variable's type
        if let Some(lvar_read) = value_node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(lvar_read.name().as_slice()).to_string();
            if let Some(ty) = local_types.get(&var_name) {
                return ty.clone();
            }
            if let Some(ty) = self.get_local_var_type(&var_name, &lvar_read.location()) {
                return ty;
            }
            return RubyType::Unknown;
        }

        // 5. Method call: recursively infer receiver type, then resolve method.
        // Keep the proof outcome until the final RubyType projection so the
        // indexing pass can retain the exact reason for every withheld call.
        if let Some(call_node) = value_node.as_call_node() {
            return self
                .infer_call_type_outcome_with_locals(&call_node, local_types)
                .into_ruby_type();
        }

        RubyType::Unknown
    }

    pub(in crate::indexer::fact_collector) fn infer_collection_type_from_value(
        &self,
        value_node: &Node<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Option<Result<RubyType, ShapeConstructionError>> {
        if let Some(hash) = value_node.as_hash_node() {
            return Some(infer_hash_literal_type_fallible(&hash, |value| {
                self.infer_collection_type_from_value(value, local_types)
                    .unwrap_or_else(|| {
                        Ok(self.infer_type_from_value_with_locals(value, local_types))
                    })
            }));
        }
        value_node.as_array_node().map(|array| {
            infer_array_literal_type_fallible(&array, |value| {
                self.infer_collection_type_from_value(value, local_types)
                    .unwrap_or_else(|| {
                        Ok(self.infer_type_from_value_with_locals(value, local_types))
                    })
            })
        })
    }

    pub(in crate::indexer::fact_collector) fn infer_call_type_outcome_with_locals(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> TypeInferenceOutcome {
        let expression_range = self.direct_range(&call_node.location());
        if let Some(fact) = self
            .direct_expression_fact(expression_range, None)
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
        {
            return TypeInferenceOutcome::proven(fact.ruby_type.clone());
        }
        if let Some(outcome) = self.recorded_call_expression_outcome(expression_range) {
            return outcome.clone();
        }

        if let Some(const_get_type) = self.const_get_reference_type(call_node) {
            return TypeInferenceOutcome::proven(const_get_type);
        }
        // Higher-order, yield, and proc proofs are recorded during
        // `visit_call_node`. Re-solving them here repeats prepare/block work
        // for calls that already ran that path and produced no immediate
        // outcome.

        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        let receiver_type = if let Some(receiver) = call_node.receiver() {
            let inferred = self.infer_type_from_value_with_locals(&receiver, local_types);
            project_immediate_hash_receiver_type(&receiver, inferred)
        } else {
            // No receiver means `self`, which may differ from lexical constant
            // scope inside eval- or extension-provided execution contexts.
            let (namespace, kind) = self.scope_tracker.implicit_receiver_context();
            if namespace.is_empty() {
                return TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver);
            }
            let current_fqn = FullyQualifiedName::namespace(namespace);
            match kind {
                NamespaceKind::Instance => RubyType::Class(current_fqn),
                NamespaceKind::Singleton => RubyType::ClassReference(current_fqn),
            }
        };

        if receiver_type == RubyType::Unknown {
            let reason = call_node
                .receiver()
                .and_then(|receiver| {
                    let range = self.direct_range(&receiver.location());
                    self.expressions.unknown_reasons.get(&range).copied()
                })
                .unwrap_or(UnknownReason::UnknownReceiver);
            return TypeInferenceOutcome::unknown(reason);
        }

        if shape_reads::is_shape_only(&receiver_type) {
            let argument_nodes = call_node
                .arguments()
                .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
                .unwrap_or_default();
            let argument_types = argument_nodes
                .iter()
                .map(|argument| self.infer_type_from_value_with_locals(argument, local_types))
                .collect::<Vec<_>>();
            let precise = match method_name.as_ref() {
                "[]" if argument_nodes.len() == 1 => Some(shape_reads::indexed_read(
                    &receiver_type,
                    literal_key(&argument_nodes[0]).as_ref(),
                )),
                "fetch" if matches!(argument_nodes.len(), 1 | 2) => Some(shape_reads::fetch(
                    &receiver_type,
                    literal_key(&argument_nodes[0]).as_ref(),
                    argument_types.get(1),
                )),
                "dig" if !argument_nodes.is_empty() => {
                    let keys = argument_nodes.iter().map(literal_key).collect::<Vec<_>>();
                    Some(shape_reads::dig(&receiver_type, &keys))
                }
                "key?" | "has_key?" | "include?" | "member?" if argument_nodes.len() == 1 => {
                    Some(shape_reads::key_presence(
                        &receiver_type,
                        literal_key(&argument_nodes[0]).as_ref(),
                    ))
                }
                "keys" if argument_nodes.is_empty() => Some(shape_reads::keys(&receiver_type)),
                "values" if argument_nodes.is_empty() => Some(shape_reads::values(&receiver_type)),
                "each" | "each_pair" | "each_key" | "each_value" if argument_nodes.is_empty() => {
                    Some(shape_reads::each_return(
                        &receiver_type,
                        call_node.block().is_some(),
                    ))
                }
                _ => None,
            };
            if let Some(outcome) = precise {
                return match outcome {
                    Ok(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
                    Err(reason) => TypeInferenceOutcome::unknown(reason),
                };
            }
        }

        // Object#freeze preserves the receiver identity and type. RBS
        // expresses this as `self`, which is a substitution contract rather
        // than a named return type.
        if method_name == "freeze" {
            return TypeInferenceOutcome::proven(receiver_type);
        }

        self.resolve_method_return_type_outcome_with_private(
            &receiver_type,
            &method_name,
            call_node.receiver().is_none(),
        )
    }

    pub(in crate::indexer::fact_collector) fn infer_if_expression_type(
        &self,
        if_node: &IfNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let then_diverges = if_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let then_type = if_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let (else_type, else_diverges) = if let Some(subsequent) = if_node.subsequent() {
            let diverges = control_flow::diverges(&subsequent);
            let ty = if let Some(else_node) = subsequent.as_else_node() {
                else_node
                    .statements()
                    .map(|statements| {
                        self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                    })
                    .unwrap_or_else(RubyType::nil_class)
            } else if let Some(elsif_node) = subsequent.as_if_node() {
                self.infer_if_expression_type(&elsif_node, local_types)
            } else {
                RubyType::nil_class()
            };
            (ty, diverges)
        } else {
            (RubyType::nil_class(), false)
        };

        join_non_diverging_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    pub(in crate::indexer::fact_collector) fn infer_unless_expression_type(
        &self,
        unless_node: &UnlessNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let then_diverges = unless_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let then_type = unless_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let else_diverges = unless_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let else_type = unless_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        join_non_diverging_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    pub(in crate::indexer::fact_collector) fn infer_case_expression_type(
        &self,
        case_node: &CaseNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let mut branches = Vec::new();
        for condition in case_node.conditions().iter() {
            let Some(when_node) = condition.as_when_node() else {
                continue;
            };
            let diverges = when_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = when_node
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
        }

        if let Some(else_clause) = case_node.else_clause() {
            let diverges = else_clause
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = else_clause
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
        } else {
            branches.push((RubyType::nil_class(), false));
        }

        join_non_diverging_types(&branches)
    }

    pub(in crate::indexer::fact_collector) fn infer_begin_expression_type(
        &self,
        begin_node: &BeginNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let body_diverges = begin_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let body_type = begin_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let normal_type = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or(body_type);
        let else_diverges = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let normal_diverges = body_diverges || else_diverges;

        let mut branches = vec![(normal_type, normal_diverges)];
        let mut rescue_clause = begin_node.rescue_clause();
        while let Some(rescue_node) = rescue_clause {
            let diverges = rescue_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = rescue_node
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
            rescue_clause = rescue_node.subsequent();
        }

        join_non_diverging_types(&branches)
    }

    pub(in crate::indexer::fact_collector) fn infer_rescue_modifier_expression_type(
        &self,
        rescue_modifier: &RescueModifierNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let expression = rescue_modifier.expression();
        let rescue_expression = rescue_modifier.rescue_expression();
        join_non_diverging_types(&[
            (
                self.infer_type_from_value_with_locals(&expression, local_types),
                control_flow::diverges(&expression),
            ),
            (
                self.infer_type_from_value_with_locals(&rescue_expression, local_types),
                control_flow::diverges(&rescue_expression),
            ),
        ])
    }

    pub fn infer_assignment_type_from_value(&self, value_node: &Node) -> RubyType {
        let expression_range = self.direct_range(&value_node.location());
        if let Some(fact) = self.direct_expression_fact(expression_range, None) {
            return fact.ruby_type.clone();
        }
        if self.options.resolve_analysis_method_returns {
            return self.infer_type_from_value(value_node);
        }

        if let Some(literal_type) = LiteralAnalyzer::new().analyze_literal(value_node) {
            return literal_type;
        }
        if let Some(call) = value_node.as_call_node() {
            if let Some(const_get_type) = self.const_get_reference_type(&call) {
                return const_get_type;
            }

            if call.name().as_slice() == b"new" {
                if let Some(receiver) = call.receiver() {
                    if let Some(fqn) = self.constant_reference_type(&receiver) {
                        return RubyType::Class(fqn);
                    }
                }
            }
            return RubyType::Unknown;
        }
        self.constant_reference_type(value_node)
            .map(RubyType::ClassReference)
            .unwrap_or(RubyType::Unknown)
    }

    pub(in crate::indexer::fact_collector) fn infer_assignment_type_from_value_with_reason(
        &self,
        value_node: &Node<'_>,
    ) -> (RubyType, Option<UnknownReason>) {
        match self.infer_collection_type_from_value(value_node, &HashMap::new()) {
            Some(Ok(ruby_type)) => (ruby_type, None),
            Some(Err(error)) => (
                RubyType::Unknown,
                Some(literal_shape_construction_unknown_reason(error)),
            ),
            None => (self.infer_assignment_type_from_value(value_node), None),
        }
    }

    pub(in crate::indexer::fact_collector) fn record_call_expression_type(
        &mut self,
        node: &CallNode<'_>,
        prepared_higher_order: Option<
            Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>,
        >,
    ) {
        let higher_order_outcome =
            self.infer_rbs_higher_order_call_outcome(node, &HashMap::new(), prepared_higher_order);
        let range = self.direct_range(&node.location());
        if let Some(outcome) = higher_order_outcome.as_ref() {
            if outcome.unknown_reason().is_some() {
                self.record_immediate_call_outcome(range, outcome.clone());
                self.suppress_deferred_call_outcome(range, "higher-order Unknown outcome");
                return;
            }
        }
        let proc_outcome = self.infer_proc_call_return_type(node, &HashMap::new());
        if let Some(outcome) = proc_outcome.as_ref() {
            if outcome.unknown_reason().is_some() {
                self.record_immediate_call_outcome(range, outcome.clone());
                self.suppress_deferred_call_outcome(range, "callable-body Unknown outcome");
                return;
            }
        }

        let Some(return_type) = higher_order_outcome
            .and_then(TypeInferenceOutcome::into_proven_type)
            .or_else(|| self.infer_yielding_block_return_type_for_call(node))
            .or_else(|| proc_outcome.and_then(TypeInferenceOutcome::into_proven_type))
            .or_else(|| {
                crate::indexer::inlay_hints::has_multiline_chain_continuation(
                    self.document.analysis_content().as_bytes(),
                    node.location().end_offset(),
                )
                .then(|| self.infer_type_from_value(&node.as_node()))
                .filter(|ruby_type| *ruby_type != RubyType::Unknown)
            })
        else {
            return;
        };
        self.expressions.call_outcomes.remove(&range);
        self.suppress_deferred_call_outcome(range, "proven special-call outcome");
        let fact = TypeFact::new(
            TypeSubject::Expression(range),
            return_type,
            range,
            TypeProvenance::Inferred,
        );
        self.facts.types.add(fact.clone());
        self.push_direct_expression_fact(fact);
    }

    pub(in crate::indexer::fact_collector) fn suppress_deferred_call_outcome(
        &mut self,
        range: TextRange,
        proof_kind: &str,
    ) {
        let mut suppressed_candidates = 0usize;
        for candidate in &mut self.facts.references {
            let crate::core::ReferenceCandidateKind::Method {
                call_expression_range,
                ..
            } = &mut candidate.kind
            else {
                continue;
            };
            if *call_expression_range == Some(range) {
                *call_expression_range = None;
                suppressed_candidates = suppressed_candidates.checked_add(1).expect(
                    "INVARIANT VIOLATED: suppressed call candidate count overflowed usize. This is a bug because one file cannot contain more candidates than addressable memory. Fix: bound candidate collection by the source size.",
                );
            }
        }
        assert!(
            suppressed_candidates <= 1,
            "INVARIANT VIOLATED: one {proof_kind} suppressed multiple deferred outcomes. This is a bug because each CallNode owns at most one method candidate. Fix: emit exactly one candidate for the runtime dispatch."
        );
        if suppressed_candidates == 1 {
            assert!(
                self.expressions.deferred_calls.remove(&range),
                "INVARIANT VIOLATED: a suppressed deferred call candidate has no collector-local range marker. This is a bug because candidate and marker lifecycles must be identical. Fix: insert and remove deferred ranges with the owning method candidate."
            );
        }
    }

    /// Return concrete block-owned reads and flow deltas separately from the
    /// per-file method inference record. Block types survive cold indexing
    /// without retaining the collector's scope tree in the editor cache.
    pub fn local_read_type_evidence(&self) -> Box<[(TextRange, RubyType)]> {
        let mut local_read_types = self.expressions.local_reads.clone();
        local_read_types.sort_unstable_by_key(|(range, _)| *range);
        for (range, ruby_type) in &local_read_types {
            assert!(
                *ruby_type != RubyType::Unknown,
                "INVARIANT VIOLATED: compact local-read evidence retained Unknown at {range:?}. This is a bug because Unknown reads belong in expression_unknown_reasons and cannot be published as concrete proof. Fix: filter unresolved TypeTracker reads while installing file evidence."
            );
        }
        for adjacent in local_read_types.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one local-variable read produced multiple flow types. This is a bug because bounded solver revisits must overwrite the same AST read. Fix: retain local reads in TypeTracker's range-keyed map before installing evidence."
            );
        }
        local_read_types.into_boxed_slice()
    }
}

pub(in crate::indexer::fact_collector) fn join_non_diverging_types(
    branches: &[(RubyType, bool)],
) -> RubyType {
    let surviving = branches
        .iter()
        .filter(|(_, diverges)| !*diverges)
        .map(|(ty, _)| ty.clone())
        .collect::<Vec<_>>();
    if surviving.is_empty() {
        RubyType::Unknown
    } else {
        RubyType::union(surviving)
    }
}
