use crate::core::RubyType;
use crate::inference::control_flow;
use crate::inference::type_tracker::flow::environment::FlowEnvironment;
use crate::inference::type_tracker::flow::narrow;
use crate::inference::type_tracker::flow::shapes::aliases::shape_local_key_read;
use crate::inference::type_tracker::flow::shapes::narrowing::{
    hash_pattern_requirements, literal_value,
};
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum Truthiness {
    AlwaysTruthy,
    AlwaysFalsy,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum ShortCircuitOperator {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum RightExecution {
    Always,
    Never,
    Conditional,
}

impl TypeTracker {
    /// Track an if statement with branch merging
    ///
    /// Clones the environment for each branch, tracks them separately,
    /// then merges the results at the join point.
    pub(in crate::inference::type_tracker) fn track_if(&mut self, if_node: &IfNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = if_node.predicate();
        self.track_node(&predicate);

        let env_before = self.environment.clone();

        // Then branch
        self.environment = env_before.clone();
        let then_reaches = self.narrow_shape_predicate(&predicate, true);
        let then_diverges = !then_reaches
            || if_node
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let then_type = if !then_reaches {
            RubyType::nil_class()
        } else if let Some(statements) = if_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.environment.clone();

        self.environment = env_before.clone();

        // Else branch
        let else_reaches = self.narrow_shape_predicate(&predicate, false);
        let else_diverges = !else_reaches
            || if_node
                .subsequent()
                .map(|n| control_flow::diverges(&n))
                .unwrap_or(false);
        let else_type = if !else_reaches {
            RubyType::nil_class()
        } else if let Some(subsequent) = if_node.subsequent() {
            match &subsequent {
                _ if subsequent.as_else_node().is_some() => {
                    let else_node = subsequent.as_else_node().unwrap();
                    if let Some(statements) = else_node.statements() {
                        self.track_node(&statements.as_node())
                    } else {
                        RubyType::nil_class()
                    }
                }
                _ if subsequent.as_if_node().is_some() => {
                    let elsif_node = subsequent.as_if_node().unwrap();
                    self.track_if(&elsif_node)
                }
                _ => RubyType::nil_class(),
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.environment.clone();

        // Merge envs — diverging branches never reach the join point.
        match (then_diverges, else_diverges) {
            (true, true) => self.environment = env_before,
            (true, false) => {
                // then exited → predicate was false at the join.
                self.environment = else_env;
                narrow::narrow(&mut self.environment.types, &predicate, false);
            }
            (false, true) => {
                // else exited → predicate was true at the join.
                self.environment = then_env;
                narrow::narrow(&mut self.environment.types, &predicate, true);
            }
            (false, false) => {
                self.environment = then_env;
                self.merge_env(&else_env, if_node.subsequent().is_none());
            }
        }

        // Type union — exclude diverging branches.
        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Track a case statement with branch merging
    ///
    /// Each when clause is tracked separately, then all branches
    /// (including else) are merged at the join point.
    pub(in crate::inference::type_tracker) fn track_case(
        &mut self,
        case_node: &CaseNode,
    ) -> RubyType {
        // Track the predicate (the value being matched)
        let predicate = case_node.predicate();
        if let Some(predicate) = &predicate {
            self.track_node(&predicate);
        }

        let env_before = self.environment.clone();

        let when_nodes = case_node
            .conditions()
            .iter()
            .filter_map(|condition| condition.as_when_node())
            .collect::<Vec<_>>();
        let when_literal_sets = when_nodes
            .iter()
            .map(|when_node| {
                let literals = when_node
                    .conditions()
                    .iter()
                    .map(|condition| literal_value(&condition))
                    .collect::<Option<Vec<_>>>()?;
                (!literals.is_empty()).then_some(literals)
            })
            .collect::<Vec<_>>();
        let discriminator = predicate.as_ref().and_then(shape_local_key_read);
        let discriminator_is_complete =
            discriminator.is_some() && when_literal_sets.iter().all(Option::is_some);
        let all_literals = discriminator_is_complete.then(|| {
            when_literal_sets
                .iter()
                .flat_map(|literals| {
                    literals
                        .as_ref()
                        .expect(
                            "INVARIANT VIOLATED: complete case discriminator lost a literal condition set. This is a bug because completeness was checked immediately before flattening. Fix: retain the checked sets unchanged.",
                        )
                        .iter()
                        .cloned()
                })
                .collect::<Vec<_>>()
        });

        // (env, type, diverges) per branch.
        let mut branches: Vec<(FlowEnvironment, RubyType, bool)> = Vec::new();

        for (when_node, literals) in when_nodes.iter().zip(&when_literal_sets) {
            self.environment = env_before.clone();
            let reaches = match (&discriminator, literals) {
                (Some((name, key)), Some(literals)) if discriminator_is_complete => {
                    self.narrow_shape_literal_set(name, key, literals, true)
                }
                (Some(_) | None, Some(_) | None) => true,
            };
            let diverges = !reaches
                || when_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let branch_type = if !reaches {
                RubyType::nil_class()
            } else if let Some(statements) = when_node.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.environment.clone(), branch_type, diverges));
        }

        self.environment = env_before.clone();
        if let (Some((name, key)), Some(literals)) = (&discriminator, &all_literals) {
            self.narrow_shape_literal_set(name, key, literals, false);
        }
        let unmatched_env = self.environment.clone();
        if let Some(else_clause) = case_node.else_clause() {
            let diverges = else_clause
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
            let else_type = if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.environment.clone(), else_type, diverges));
        } else {
            push_unmatched_ordinary_case_path(&mut branches, &unmatched_env);
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        // Pick post-state from non-diverging branches only.
        let surviving_envs: Vec<&FlowEnvironment> = branches
            .iter()
            .filter(|(_, _, d)| !*d)
            .map(|(env, _, _)| env)
            .collect();

        if surviving_envs.is_empty() {
            // All branches diverge — code after is unreachable. Keep pre-state.
            self.environment = env_before;
        } else {
            self.environment = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        // Type union — exclude diverging branches.
        let typed_branches: Vec<(RubyType, bool)> =
            branches.into_iter().map(|(_, ty, d)| (ty, d)).collect();
        join_branch_types(&typed_branches)
    }

    pub(in crate::inference::type_tracker) fn track_case_match(
        &mut self,
        case_node: &CaseMatchNode,
    ) -> RubyType {
        let predicate = case_node.predicate();
        if let Some(predicate) = &predicate {
            self.track_node(predicate);
        }

        let env_before = self.environment.clone();
        let mut branches: Vec<(FlowEnvironment, RubyType, bool)> = Vec::new();

        let in_nodes = case_node
            .conditions()
            .iter()
            .filter_map(|condition| condition.as_in_node())
            .collect::<Vec<_>>();
        let patterns_supported = predicate.as_ref().is_some_and(|predicate| {
            predicate.as_local_variable_read_node().is_some()
                && in_nodes
                    .iter()
                    .all(|in_node| hash_pattern_requirements(&in_node.pattern()).is_some())
        });

        for (index, in_node) in in_nodes.iter().enumerate() {
            self.environment = env_before.clone();
            let reaches = if patterns_supported {
                let predicate = predicate.as_ref().expect(
                    "INVARIANT VIOLATED: supported Hash pattern case lost its predicate. This is a bug because patterns_supported requires one. Fix: retain the checked predicate for branch narrowing.",
                );
                let mut reaches = true;
                for prior in &in_nodes[..index] {
                    if !reaches {
                        break;
                    }
                    reaches = self
                        .narrow_shape_hash_pattern(predicate, &prior.pattern(), false)
                        .expect(
                            "INVARIANT VIOLATED: previously supported Hash pattern became unsupported. This is a bug because the immutable pattern was validated before branch traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
                if reaches {
                    reaches = self
                        .narrow_shape_hash_pattern(predicate, &in_node.pattern(), true)
                        .expect(
                            "INVARIANT VIOLATED: supported Hash pattern became unsupported before its branch. This is a bug because the immutable pattern was validated before traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
                reaches
            } else {
                true
            };
            if let Some(predicate) = &predicate {
                let captures = self.pattern_capture_types_for_value(&in_node.pattern(), predicate);
                for (name, ty) in captures {
                    if ty != RubyType::Unknown {
                        self.environment.insert(name, ty);
                    }
                }
            }

            let diverges = !reaches
                || in_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let branch_type = if !reaches {
                RubyType::nil_class()
            } else if let Some(statements) = in_node.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.environment.clone(), branch_type, diverges));
        }

        let has_else = case_node.else_clause().is_some();
        if has_else {
            self.environment = env_before.clone();
            let mut else_reaches = true;
            if patterns_supported {
                let predicate = predicate.as_ref().expect(
                    "INVARIANT VIOLATED: supported Hash pattern else path lost its predicate. This is a bug because patterns_supported requires one. Fix: retain the checked predicate for unmatched narrowing.",
                );
                for in_node in &in_nodes {
                    if !else_reaches {
                        break;
                    }
                    else_reaches = self
                        .narrow_shape_hash_pattern(predicate, &in_node.pattern(), false)
                        .expect(
                            "INVARIANT VIOLATED: supported Hash pattern became unsupported on the else path. This is a bug because the immutable pattern was validated before traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
            }
            let else_clause = case_node.else_clause().unwrap();
            let diverges = !else_reaches
                || else_clause
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let else_type = if !else_reaches {
                RubyType::nil_class()
            } else if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.environment.clone(), else_type, diverges));
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        let surviving_envs: Vec<&FlowEnvironment> = branches
            .iter()
            .filter(|(_, _, d)| !*d)
            .map(|(env, _, _)| env)
            .collect();

        if surviving_envs.is_empty() {
            self.environment = env_before;
        } else {
            self.environment = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        // Unlike an ordinary `case ... when`, a `case ... in` expression with
        // no matching pattern and no `else` raises NoMatchingPatternError.
        // That path cannot reach the environment after the case and therefore
        // must not add NilClass to bindings from the surviving `in` branches.
        // `merge_env` above still adds NilClass when a binding is absent from a
        // different reachable branch or from an explicit `else`.

        let typed_branches: Vec<(RubyType, bool)> =
            branches.into_iter().map(|(_, ty, d)| (ty, d)).collect();
        join_branch_types(&typed_branches)
    }

    /// Track an unless statement (inverse of if)
    pub(in crate::inference::type_tracker) fn track_unless(
        &mut self,
        unless_node: &UnlessNode,
    ) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = unless_node.predicate();
        self.track_node(&predicate);

        let env_before = self.environment.clone();

        // Then branch (executes when predicate is false)
        self.environment = env_before.clone();
        let then_reaches = self.narrow_shape_predicate(&predicate, false);
        let then_diverges = !then_reaches
            || unless_node
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let then_type = if !then_reaches {
            RubyType::nil_class()
        } else if let Some(statements) = unless_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.environment.clone();

        self.environment = env_before.clone();

        // Else branch
        let else_reaches = self.narrow_shape_predicate(&predicate, true);
        let else_diverges = !else_reaches
            || unless_node
                .else_clause()
                .and_then(|e| e.statements())
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let else_type = if !else_reaches {
            RubyType::nil_class()
        } else if let Some(else_clause) = unless_node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.environment.clone();

        match (then_diverges, else_diverges) {
            (true, true) => self.environment = env_before,
            (true, false) => {
                // unless body (executes when predicate FALSE) exited → at join, predicate was true.
                self.environment = else_env;
                narrow::narrow(&mut self.environment.types, &predicate, true);
            }
            (false, true) => {
                // else (executes when predicate TRUE) exited → at join, predicate was false.
                self.environment = then_env;
                narrow::narrow(&mut self.environment.types, &predicate, false);
            }
            (false, false) => {
                self.environment = then_env;
                self.merge_env(&else_env, unless_node.else_clause().is_none());
            }
        }

        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Track Ruby's value-returning short-circuit operators.
    ///
    /// The left operand always executes. Depending on its proven truthiness,
    /// the right operand executes always, never, or along one reachable path.
    /// Conditional execution joins the environment immediately after the left
    /// operand with the environment after the right operand. This prevents a
    /// syntactically later assignment in the right operand from being treated
    /// as unconditional by downstream receiver queries.
    pub(in crate::inference::type_tracker) fn track_short_circuit(
        &mut self,
        left: &Node<'_>,
        right: &Node<'_>,
        operator: ShortCircuitOperator,
    ) -> RubyType {
        let left_type = self.track_node(left);
        let left_env = self.environment.clone();
        let truthiness = ruby_truthiness(&left_type);
        let right_execution = match (operator, truthiness) {
            (ShortCircuitOperator::And, Truthiness::AlwaysTruthy)
            | (ShortCircuitOperator::Or, Truthiness::AlwaysFalsy) => RightExecution::Always,
            (ShortCircuitOperator::And, Truthiness::AlwaysFalsy)
            | (ShortCircuitOperator::Or, Truthiness::AlwaysTruthy) => RightExecution::Never,
            (ShortCircuitOperator::And | ShortCircuitOperator::Or, Truthiness::Conditional) => {
                RightExecution::Conditional
            }
        };

        match right_execution {
            RightExecution::Never => left_type,
            RightExecution::Always => self.track_node(right),
            RightExecution::Conditional => {
                self.environment = left_env.clone();
                let right_type = self.track_node(right);
                let right_env = self.environment.clone();

                self.environment = left_env;
                self.merge_env(&right_env, false);

                short_circuit_result_type(left_type, right_type, operator)
            }
        }
    }
}
pub(in crate::inference::type_tracker) fn ruby_truthiness(ruby_type: &RubyType) -> Truthiness {
    match ruby_type {
        RubyType::Unknown => Truthiness::Conditional,
        RubyType::Union(members) => {
            assert!(
                members.len() >= 2,
                "INVARIANT VIOLATED: RubyType::Union contains fewer than two members. This is a bug because RubyType::union must collapse empty and singleton inputs. Fix: construct unions only through the canonical RubyType helpers."
            );
            let has_falsy = members.iter().any(is_falsy_type);
            let has_truthy = members.iter().any(|member| !is_falsy_type(member));
            match (has_truthy, has_falsy) {
                (true, false) => Truthiness::AlwaysTruthy,
                (false, true) => Truthiness::AlwaysFalsy,
                (true, true) => Truthiness::Conditional,
                (false, false) => panic!(
                    "INVARIANT VIOLATED: a nonempty RubyType::Union has no truthy or falsy members. This is a bug because every concrete Ruby value has one truthiness class. Fix: update truthiness classification when adding a RubyType variant."
                ),
            }
        }
        ruby_type if is_falsy_type(ruby_type) => Truthiness::AlwaysFalsy,
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_) => Truthiness::AlwaysTruthy,
    }
}

pub(in crate::inference::type_tracker) fn is_falsy_type(ruby_type: &RubyType) -> bool {
    let RubyType::Class(fqn) = ruby_type else {
        return false;
    };
    let parts = fqn.namespace_parts_slice();
    parts.len() == 1
        && matches!(
            parts
                .first()
                .expect("INVARIANT VIOLATED: a one-part FQN lost its first part while classifying Ruby truthiness. This is a bug because the immutable slice was checked immediately before access. Fix: keep the length check and access in one expression.")
                .as_str(),
            "FalseClass" | "NilClass"
        )
}

pub(in crate::inference::type_tracker) fn short_circuit_result_type(
    left_type: RubyType,
    right_type: RubyType,
    operator: ShortCircuitOperator,
) -> RubyType {
    if left_type == RubyType::Unknown {
        return RubyType::Unknown;
    }

    let RubyType::Union(left_members) = left_type else {
        panic!(
            "INVARIANT VIOLATED: conditional short-circuit evaluation received a non-union concrete left type. This is a bug because one concrete Ruby class is always truthy or always falsy. Fix: keep ruby_truthiness and short-circuit projection exhaustive over the same RubyType variants."
        );
    };
    let mut result_members = left_members
        .into_iter()
        .filter(|member| match operator {
            ShortCircuitOperator::And => is_falsy_type(member),
            ShortCircuitOperator::Or => !is_falsy_type(member),
        })
        .collect::<Vec<_>>();
    assert!(
        !result_members.is_empty(),
        "INVARIANT VIOLATED: conditional short-circuit evaluation has no left-side result member. This is a bug because Conditional requires both an executing and a short-circuiting path. Fix: keep truthiness classification and member projection symmetric."
    );
    result_members.push(right_type);
    RubyType::union(result_members)
}

pub(in crate::inference::type_tracker) fn join_branch_types(
    branches: &[(RubyType, bool)],
) -> RubyType {
    let surviving: Vec<RubyType> = branches
        .iter()
        .filter(|(_, diverges)| !*diverges)
        .map(|(ty, _)| ty.clone())
        .collect();
    if surviving.is_empty() {
        RubyType::Unknown
    } else {
        RubyType::union(surviving)
    }
}

pub(in crate::inference::type_tracker) fn push_unmatched_ordinary_case_path(
    branches: &mut Vec<(FlowEnvironment, RubyType, bool)>,
    env_before: &FlowEnvironment,
) {
    branches.push((env_before.clone(), RubyType::nil_class(), false));
}
