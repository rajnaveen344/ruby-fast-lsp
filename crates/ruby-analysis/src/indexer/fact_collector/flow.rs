use super::FactCollector;
use crate::core::type_store::NamedTypeResolution;
use crate::core::{
    FullyQualifiedName, RubyType, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance,
    TypeSubject, UnknownReason,
};
use crate::engine::VariableTypeKind;
use ruby_prism::*;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct FlowState {
    pub(super) block_parameters: Vec<Vec<RubyType>>,
    pub(super) pattern_captures: Vec<HashMap<String, RubyType>>,
    /// Positional RHS element types for the active `MultiWriteNode`, consumed by
    /// `ConstantTargetNode` in left-to-right order.
    pub(super) assignment_elements: Vec<Vec<RubyType>>,
    pub(super) method_yields: HashMap<FullyQualifiedName, Vec<RubyType>>,
    pub(super) local_callables: HashMap<String, crate::inference::higher_order::KnownProcType>,
    /// Nonlocal writes currently being traversed. Their target facts are
    /// collected before Prism visits the RHS, but reads inside that RHS must
    /// observe the previous value rather than the not-yet-completed write.
    pub(super) active_writes: Vec<(TypeSubject, TextRange)>,
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
        let scope_id = u32::try_from(current_scope_id).expect(
            "INVARIANT VIOLATED: block parameter scope id exceeded u32. \
             This is a bug because ruby-analysis::core TypeSubject::Local stores u32 scope ids. \
             Fix: widen TypeSubject::Local scope_id before indexing more than u32::MAX scopes.",
        );
        self.facts.types.add(TypeFact::new(
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
    pub(super) fn get_local_var_type(
        &self,
        var_name: &str,
        location: &Location,
    ) -> Option<RubyType> {
        let byte_offset = u32::try_from(location.start_offset()).expect(
            "INVARIANT VIOLATED: Prism location offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let file_id = self.document.analysis_file_id();

        // Fact collection traverses the AST while keeping VariableScopes aligned with the
        // current lexical node. Starting from that scope preserves block capture and hard-scope
        // boundaries through get_type_at_position without rescanning every variable location.
        let scope_id = self.document.variable_scopes().current_scope().expect(
            "INVARIANT VIOLATED: local variable type inference ran without an active lexical scope. \
             This is a bug because FactCollector and VariableScopes must enter and exit AST scopes together. \
             Fix: balance the variable-scope lifecycle around every collector traversal branch.",
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

    pub(super) fn begin_nonlocal_write(&mut self, subject: TypeSubject, range: TextRange) {
        self.flow.active_writes.push((subject, range));
    }

    pub(super) fn finish_nonlocal_write(&mut self) {
        self.flow.active_writes.pop().expect(
            "INVARIANT VIOLATED: nonlocal write traversal stack underflowed. This is a bug because every variable-write exit must match one entry. Fix: keep FactCollector variable write callbacks balanced.",
        );
    }

    /// Resolve a nonlocal variable from facts collected before this exact
    /// source position. A write whose RHS is still being traversed is excluded
    /// because Ruby evaluates the RHS before updating the target.
    pub(super) fn collected_nonlocal_variable_type_before(
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
        match outcome.unknown_reason().expect(
            "INVARIANT VIOLATED: an unproven nonlocal-variable result lost its Unknown reason. This is a bug because TypeInferenceOutcome cannot represent a reasonless failure. Fix: construct every failed reaching-assignment proof with TypeInferenceOutcome::unknown.",
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
            | UnknownReason::UnsupportedCallableFlow => panic!(
                "INVARIANT VIOLATED: nonlocal reaching-assignment inference produced a method-call Unknown reason. This is a bug because the selector owns only assignment proof failures. Fix: keep reaching-assignment and method-call reason construction in their respective inference paths."
            ),
        }
    }

    pub(super) fn collected_nonlocal_variable_outcome_before(
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

    pub(super) fn collected_variable_type_outcome_before(
        &self,
        byte_offset: u32,
        matches_subject: impl Fn(&TypeSubject) -> bool,
    ) -> TypeInferenceOutcome {
        match self.facts.types.named_type_in_file_before_matching(
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

    /// Positional RHS element types for `A, B = 1, "x"` / `A, B = [1, "x"]`.
    ///
    /// Non-array RHS (e.g. method call) yields an empty vec so targets stay untyped.
    pub(super) fn multi_write_element_types(&self, value: &Node<'_>) -> Vec<RubyType> {
        let Some(array) = value.as_array_node() else {
            return Vec::new();
        };
        array
            .elements()
            .iter()
            .map(|element| self.infer_assignment_type_from_value(&element))
            .collect()
    }

    pub(super) fn pattern_capture_types_for_value(
        &self,
        pattern: &Node<'_>,
        value: &Node<'_>,
    ) -> HashMap<String, RubyType> {
        let mut captures = HashMap::new();
        self.collect_pattern_capture_types(pattern, value, &mut captures);
        captures
    }

    pub(super) fn collect_pattern_capture_types(
        &self,
        pattern: &Node<'_>,
        value: &Node<'_>,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(target) = pattern.as_local_variable_target_node() {
            let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
            captures.insert(name, self.infer_type_from_value(value));
            return;
        }

        if let Some(pattern_hash) = pattern.as_hash_pattern_node() {
            let Some(value_hash) = value.as_hash_node() else {
                return;
            };
            let value_elements = value_hash
                .elements()
                .iter()
                .filter_map(|element| {
                    let assoc = element.as_assoc_node()?;
                    Some((symbol_key(&assoc.key())?, assoc.value()))
                })
                .collect::<Vec<_>>();

            for element in pattern_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some(key) = symbol_key(&assoc.key()) else {
                    continue;
                };
                let Some((_, value_node)) = value_elements
                    .iter()
                    .find(|(value_key, _)| value_key == &key)
                else {
                    continue;
                };
                self.collect_pattern_capture_types(&assoc.value(), value_node, captures);
            }
            return;
        }

        if let Some(pattern_array) = pattern.as_array_pattern_node() {
            let Some(value_array) = value.as_array_node() else {
                return;
            };
            let value_elements = value_array.elements().iter().collect::<Vec<_>>();
            for (index, required) in pattern_array.requireds().iter().enumerate() {
                let Some(value_node) = value_elements.get(index) else {
                    continue;
                };
                self.collect_pattern_capture_types(&required, value_node, captures);
            }
        }
    }
}

pub(super) fn symbol_key(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}
