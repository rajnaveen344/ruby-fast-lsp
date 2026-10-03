use crate::core::storage::type_store::NamedTypeResolution;
use crate::core::VariableTypeKind;
use crate::core::{
    FullyQualifiedName, RubyType, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance,
    TypeSubject, UnknownReason,
};
use crate::indexer::fact_collector::FactCollector;
use crate::invariant::ExpectInvariant;
use ruby_prism::*;
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct FlowState {
    pub(in crate::indexer::fact_collector) block_parameters: Vec<Vec<RubyType>>,
    pub(in crate::indexer::fact_collector) pattern_captures: Vec<HashMap<String, RubyType>>,
    /// The value type of each multiple-assignment target being visited,
    /// innermost last; a constant target outside one has no known value.
    pub(in crate::indexer::fact_collector) assignment_target_types: Vec<RubyType>,
    pub(in crate::indexer::fact_collector) method_yields:
        HashMap<FullyQualifiedName, Vec<RubyType>>,
    pub(in crate::indexer::fact_collector) local_callables:
        crate::inference::higher_order::LocalCallables,
    /// Nonlocal writes currently being traversed. Their target facts are
    /// collected before Prism visits the RHS, but reads inside that RHS must
    /// observe the previous value rather than the not-yet-completed write.
    pub(in crate::indexer::fact_collector) active_writes: Vec<(TypeSubject, TextRange)>,
}

impl FactCollector {
    pub fn assign_current_block_parameter_type(
        &mut self,
        param_name: &str,
        param_location: &ruby_prism::Location<'_>,
        param_index: usize,
    ) {
        let Some(param_type) = self
            .flow
            .block_parameters
            .last()
            .and_then(|types| types.get(param_index))
            .filter(|ty| **ty != RubyType::Unknown)
            .cloned()
        else {
            return;
        };

        let Some(current_scope_id) = self.document.variable_scopes().current_scope() else {
            return;
        };
        let range = self.document.prism_location_to_text_range(param_location);
        self.document
            .variable_scopes_mut()
            .define_variable(param_name, range);
        self.document.variable_scopes_mut().add_type_assignment(
            current_scope_id,
            param_name,
            range,
            param_type.clone(),
        );
        let scope_id = u32::try_from(current_scope_id).expect_invariant(
            "block parameter scope id exceeded u32",
            "ruby-analysis::core TypeSubject::Local stores u32 scope ids",
            "widen TypeSubject::Local scope_id before indexing more than u32::MAX scopes",
        );
        self.facts.flow_types.add(TypeFact::new(
            TypeSubject::Local {
                scope_id,
                name: param_name.to_string(),
            },
            param_type,
            range,
            TypeProvenance::Assignment,
        ));
    }

    /// Helper to get the type of a local variable by name at a given location.
    pub(in crate::indexer::fact_collector) fn get_local_var_type(
        &self,
        var_name: &str,
        location: &Location,
    ) -> Option<RubyType> {
        let byte_offset = u32::try_from(location.start_offset()).expect_invariant(
            "Prism location offset exceeded u32",
            "ruby-analysis::core TextRange currently stores u32 offsets",
            "widen TextRange offsets before indexing files larger than u32::MAX bytes",
        );
        let file_id = self.document.analysis_file_id();

        // Fact collection traverses the AST while keeping VariableScopes aligned with the
        // current lexical node. Starting from that scope preserves block capture and hard-scope
        // boundaries through get_type_at_position without rescanning every variable location.
        let scope_id = self
            .document
            .variable_scopes()
            .current_scope()
            .expect_invariant(
                "local variable type inference ran without an active lexical scope",
                "FactCollector and VariableScopes must enter and exit AST scopes together",
                "balance the variable-scope lifecycle around every collector traversal branch",
            );

        let ty = self.document.variable_scopes().get_type_at_position(
            var_name,
            scope_id,
            file_id,
            byte_offset,
        )?;

        if *ty != RubyType::Unknown {
            Some(ty.clone())
        } else {
            None
        }
    }

    pub(in crate::indexer::fact_collector) fn begin_nonlocal_write(
        &mut self,
        subject: TypeSubject,
        range: TextRange,
    ) {
        self.flow.active_writes.push((subject, range));
    }

    pub(in crate::indexer::fact_collector) fn finish_nonlocal_write(&mut self) {
        self.flow.active_writes.pop().expect_invariant(
            "nonlocal write traversal stack underflowed",
            "every variable-write exit must match one entry",
            "keep FactCollector variable write callbacks balanced",
        );
    }

    /// Resolve a nonlocal variable from facts collected before this exact
    /// source position. A write whose RHS is still being traversed is excluded
    /// because Ruby evaluates the RHS before updating the target.
    pub(in crate::indexer::fact_collector) fn collected_nonlocal_variable_type_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let outcome =
            self.collected_nonlocal_variable_outcome_before(kind, name, owner, byte_offset);
        if let Some(ruby_type) = outcome.proven_type() {
            return Some(ruby_type.clone());
        }
        match outcome.unknown_reason().expect_invariant(
            "an unproven nonlocal-variable result lost its Unknown reason",
            "TypeInferenceOutcome cannot represent a reasonless failure",
            "construct every failed reaching-assignment proof with TypeInferenceOutcome::unknown",
        ) {
            UnknownReason::NoReachingAssignment => None,
            UnknownReason::UnresolvedAssignmentValue
            | UnknownReason::AmbiguousReachingAssignment
            | UnknownReason::ShapeBoundExceeded
            | UnknownReason::MutableShapeInvalidated => Some(RubyType::Unknown),
            UnknownReason::UnknownReceiver
            | UnknownReason::InvalidMethodName
            | UnknownReason::UnresolvedMethodReturn
            | UnknownReason::IncompleteUnionMember
            | UnknownReason::UnprovenRecursiveCycle
            | UnknownReason::UnsupportedCallable
            | UnknownReason::IncompleteBlockInput
            | UnknownReason::IncompleteBlockResult
            | UnknownReason::IncompleteGenericSubstitution
            | UnknownReason::AmbiguousCallableOverload
            | UnknownReason::HigherOrderBoundExceeded
            | UnknownReason::UnsupportedBlockFlow
            | UnknownReason::UnsupportedCallableBody
            | UnknownReason::IncompleteCallableInput
            | UnknownReason::IncompleteCallableCapture
            | UnknownReason::AmbiguousCallableValue
            | UnknownReason::EscapedCallableValue
            | UnknownReason::CallableBodyBoundExceeded
            | UnknownReason::CallableRecursionUnsupported
            | UnknownReason::UnsupportedCallableFlow => unreachable_invariant!(
                what = "reaching-assignment inference produced a method-call Unknown reason",
                why = "the selector owns only assignment proof failures",
                fix = "keep assignment and method-call reasons in their own inference paths",
            ),
        }
    }

    pub(in crate::indexer::fact_collector) fn collected_nonlocal_variable_outcome_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        byte_offset: u32,
    ) -> TypeInferenceOutcome {
        self.collected_variable_type_outcome_before(byte_offset, |subject| match (subject, kind) {
            (
                TypeSubject::InstanceVariable {
                    owner: fact_owner,
                    name: fact_name,
                },
                VariableTypeKind::Instance,
            ) => fact_owner == owner && fact_name == name,
            (
                TypeSubject::ClassVariable {
                    owner: fact_owner,
                    name: fact_name,
                },
                VariableTypeKind::Class,
            ) => fact_owner.namespace_parts() == owner.namespace_parts() && fact_name == name,
            (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => fact_name == name,
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
        })
    }

    pub(in crate::indexer::fact_collector) fn collected_variable_type_outcome_before(
        &self,
        byte_offset: u32,
        matches_subject: impl Fn(&TypeSubject) -> bool,
    ) -> TypeInferenceOutcome {
        match self.facts.flow_types.named_type_in_file_before_matching(
            self.document.analysis_file_id(),
            byte_offset,
            |subject, range| {
                matches_subject(subject)
                    && !self
                        .flow
                        .active_writes
                        .iter()
                        .any(|(active_subject, active_range)| {
                            active_subject == subject && *active_range == range
                        })
            },
        ) {
            NamedTypeResolution::Unresolved => {
                TypeInferenceOutcome::unknown(UnknownReason::NoReachingAssignment)
            }
            NamedTypeResolution::Ambiguous => {
                TypeInferenceOutcome::unknown(UnknownReason::AmbiguousReachingAssignment)
            }
            NamedTypeResolution::Resolved(RubyType::Unknown) => {
                TypeInferenceOutcome::unknown(UnknownReason::UnresolvedAssignmentValue)
            }
            NamedTypeResolution::Resolved(ruby_type) => {
                TypeInferenceOutcome::proven(ruby_type.clone())
            }
        }
    }

    pub(in crate::indexer::fact_collector) fn pattern_capture_types_for_value(
        &self,
        pattern: &Node<'_>,
        value: &Node<'_>,
    ) -> HashMap<String, RubyType> {
        let mut captures = HashMap::new();
        crate::inference::r#type::pattern::capture_types_from_value(
            pattern,
            value,
            &mut captures,
            &mut |element| self.infer_type_from_value(element),
        );
        captures
    }
}
