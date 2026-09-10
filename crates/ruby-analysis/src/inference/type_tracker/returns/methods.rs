use crate::core::{
    FullyQualifiedName, MethodReturnEquation, RubyMethod, RubyType, TypeInferenceOutcome,
    UnknownReason,
};
use crate::inference::control_flow;
use crate::inference::method::recursive::MAX_RECURSIVE_RETURN_ITERATIONS;
use crate::inference::type_tracker::returns::dependencies::join_recursive_return_approximations;
use crate::inference::type_tracker::returns::RecursiveReturnApproximation;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::HashSet;
use std::sync::Arc;

impl TypeTracker {
    /// Infer a method's explicit and fallthrough returns from its Prism body.
    pub fn track_method(&mut self, method: &DefNode) -> RubyType {
        self.track_method_outcome(method).into_ruby_type()
    }

    /// Infer a method return while retaining why proof was withheld.
    ///
    /// Direct recursion is solved as a bounded least fixed point. The private
    /// bottom value starts with no possible return and is ignored by unions;
    /// public `Unknown` remains absorbing. A cycle with no proven base, an
    /// incomplete premise, or a non-converging equation therefore stays
    /// explainable Unknown instead of becoming a guessed concrete type.
    pub fn track_method_outcome(&mut self, method: &DefNode) -> TypeInferenceOutcome {
        let mut approximation = RecursiveReturnApproximation::Bottom;
        for _iteration in 0..MAX_RECURSIVE_RETURN_ITERATIONS {
            let next = self.track_method_once(method, Some(approximation.clone()));
            if !self.returns.saw_direct_recursion {
                return next.into_outcome(UnknownReason::UnresolvedMethodReturn);
            }
            if next == approximation {
                return next.into_outcome(UnknownReason::UnprovenRecursiveCycle);
            }
            if next == RecursiveReturnApproximation::Unknown {
                return TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle);
            }
            approximation = next;
        }

        TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle)
    }

    /// Collect a compact same-file return equation during the existing AST
    /// traversal. Exact returned calls become dependencies; unsupported uses
    /// remain absorbing Unknown rather than being mistaken for recursion.
    pub(crate) fn track_method_equation(
        &mut self,
        method: &DefNode,
        method_fqn: FullyQualifiedName,
        local_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    ) -> MethodReturnEquation {
        self.analysis.method_candidates = local_method_candidates;

        let base = self.track_method_once(method, None).into_equation_base();
        let dependencies = std::mem::take(&mut self.returns.dependencies);
        self.analysis.method_candidates = Arc::new(HashSet::new());

        let constant_dependencies = std::mem::take(&mut self.returns.constant_dependencies);
        MethodReturnEquation::new(method_fqn, base, dependencies)
            .with_constant_dependencies(constant_dependencies)
    }

    pub(in crate::inference::type_tracker) fn track_method_once(
        &mut self,
        method: &DefNode,
        recursive_return_approximation: Option<RecursiveReturnApproximation>,
    ) -> RecursiveReturnApproximation {
        assert!(
            self.control_flow.rescue_entries.is_empty(),
            "INVARIANT VIOLATED: a rescue-entry accumulator escaped a previous method traversal. This is a bug because protected-body state is lexical and cannot cross method boundaries. Fix: pop every accumulator immediately after tracking its protected expression."
        );
        self.environment.clear();
        self.next_shape_identity = 0;
        self.observations.snapshots.clear();
        self.observations.local_reads.clear();
        self.observations.has_seen_control_flow = false;
        self.returns.local_terms.clear();
        self.returns.explicit_values.clear();
        self.returns.dependencies.clear();
        self.returns.constant_dependencies.clear();
        self.returns.direct_call_proofs.clear();
        self.returns.saw_direct_recursion = false;
        self.returns.approximation = recursive_return_approximation;

        let previous_method = self.context.method.clone();
        self.context.method = Some(normalized_method_name(method));

        // Add parameters to environment
        if let Some(params) = method.parameters() {
            self.add_parameters(&params);
        }

        // Track method body
        let fallthrough_type = if let Some(body) = method.body() {
            self.track_node(&body)
        } else {
            RubyType::nil_class()
        };

        // Record final state at method end
        if let Some(body) = method.body() {
            let end_offset = body.location().end_offset();
            self.record_state(end_offset);
        }

        let fallthrough_term = method
            .body()
            .and_then(|body| self.return_term_dependency_for_node(&body));
        let fallthrough_constant_dependencies = method
            .body()
            .map(|body| self.constant_dependencies_for_node(&body))
            .unwrap_or_default();
        let fallthrough = match method.body() {
            Some(body) if control_flow::diverges(&body) => RecursiveReturnApproximation::Bottom,
            Some(_)
                if fallthrough_term.as_ref().is_some_and(|(dependency, _)| {
                    self.should_track_return_dependency(
                        dependency,
                        &fallthrough_type,
                        self.returns.saw_direct_recursion,
                    )
                }) =>
            {
                let (dependency, approximation) = fallthrough_term.expect(
                    "INVARIANT VIOLATED: checked return term disappeared before use. This is a bug because the local result is immutable. Fix: destructure the option once instead of mutating dependency state between checks.",
                );
                self.returns.dependencies.insert(dependency);
                approximation
            }
            Some(_) if !fallthrough_constant_dependencies.is_empty() => {
                self.returns
                    .constant_dependencies
                    .extend(fallthrough_constant_dependencies);
                if fallthrough_type == RubyType::Unknown {
                    RecursiveReturnApproximation::Bottom
                } else {
                    RecursiveReturnApproximation::from_ruby_type(fallthrough_type)
                }
            }
            Some(_) if fallthrough_type == RubyType::Unknown => {
                RecursiveReturnApproximation::Unknown
            }
            Some(_) | None => RecursiveReturnApproximation::from_ruby_type(fallthrough_type),
        };

        let mut alternatives = std::mem::take(&mut self.returns.explicit_values);
        alternatives.push(fallthrough);
        let return_type = join_recursive_return_approximations(alternatives);

        assert!(
            self.control_flow.rescue_entries.is_empty(),
            "INVARIANT VIOLATED: method traversal finished with an active rescue-entry accumulator. This is a bug because every protected body must restore the accumulator stack before publishing inferred types. Fix: balance the push/pop in begin and rescue-modifier tracking."
        );

        self.context.method = previous_method;
        self.returns.approximation = None;
        return_type
    }
}
pub(in crate::inference::type_tracker) fn normalized_method_name(
    method: &DefNode<'_>,
) -> RubyMethod {
    let source_name = String::from_utf8_lossy(method.name().as_slice());
    let semantic_name = if source_name.as_ref() == "initialize" {
        "new"
    } else {
        source_name.as_ref()
    };
    RubyMethod::new(semantic_name).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: Prism produced invalid method name `{semantic_name}` while tracking a definition: {error}. \
             This is a bug because FactCollector validates method names before type inference. \
             Fix: keep method-name validation and TypeTracker invocation on the same definition."
        )
    })
}
