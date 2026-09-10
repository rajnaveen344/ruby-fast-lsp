use crate::core::RubyType;
use crate::inference::control_flow;
use crate::inference::type_tracker::flow::branches::join_branch_types;
use crate::inference::type_tracker::flow::environment::FlowEnvironment;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(in crate::inference::type_tracker) struct RescueEntryTypes {
    pub(in crate::inference::type_tracker) locals: HashMap<String, RubyType>,
}

impl RescueEntryTypes {
    pub(in crate::inference::type_tracker) fn observe(&mut self, name: &str, ruby_type: &RubyType) {
        self.locals
            .entry(name.to_string())
            .and_modify(|observed| {
                *observed = RubyType::union([observed.clone(), ruby_type.clone()]);
            })
            .or_insert_with(|| ruby_type.clone());
    }

    pub(in crate::inference::type_tracker) fn environment_from(
        &self,
        environment_before: &FlowEnvironment,
    ) -> FlowEnvironment {
        let mut environment = environment_before.clone();
        for (name, ruby_type) in &self.locals {
            environment.insert(name.clone(), ruby_type.clone());
        }
        environment
    }
}

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn observe_rescue_entry_type(
        &mut self,
        name: &str,
        ruby_type: &RubyType,
    ) {
        for entry_types in &mut self.control_flow.rescue_entries {
            entry_types.observe(name, ruby_type);
        }
    }

    pub(in crate::inference::type_tracker) fn track_begin(
        &mut self,
        begin_node: &BeginNode,
    ) -> RubyType {
        let env_before = self.environment.clone();

        let has_rescue = begin_node.rescue_clause().is_some();
        if has_rescue {
            self.control_flow
                .rescue_entries
                .push(RescueEntryTypes::default());
        }

        self.environment = env_before.clone();
        let body_diverges = begin_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let body_type = begin_node
            .statements()
            .map(|statements| self.track_node(&statements.as_node()))
            .unwrap_or_else(RubyType::nil_class);
        let body_env = self.environment.clone();
        let rescue_entry_types = has_rescue.then(|| {
            self.control_flow.rescue_entries.pop().expect(
                "INVARIANT VIOLATED: a begin/rescue protected-body accumulator disappeared before rescue analysis. This is a bug because only the matching begin frame may pop it. Fix: keep rescue accumulator ownership stack-disciplined.",
            )
        });

        self.environment = body_env;
        let else_diverges = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let normal_type = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| self.track_node(&statements.as_node()))
            .unwrap_or(body_type);
        let normal_env = self.environment.clone();
        let normal_diverges = body_diverges || else_diverges;

        let mut branches = vec![(normal_env, normal_type, normal_diverges)];
        let mut rescue_clause = begin_node.rescue_clause();
        while let Some(rescue_node) = rescue_clause {
            self.environment = rescue_entry_types
                .as_ref()
                .expect(
                    "INVARIANT VIOLATED: a rescue clause has no protected-body entry evidence. This is a bug because has_rescue and the immutable rescue chain came from the same Prism begin node. Fix: create one accumulator whenever a rescue clause exists.",
                )
                .environment_from(&env_before);
            let diverges = rescue_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let branch_type = rescue_node
                .statements()
                .map(|statements| self.track_node(&statements.as_node()))
                .unwrap_or_else(RubyType::nil_class);
            branches.push((self.environment.clone(), branch_type, diverges));
            rescue_clause = rescue_node.subsequent();
        }

        let surviving_envs = branches
            .iter()
            .filter(|(_, _, diverges)| !*diverges)
            .map(|(env, _, _)| env)
            .collect::<Vec<_>>();
        if surviving_envs.is_empty() {
            self.environment = env_before;
        } else {
            self.environment = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        if let Some(ensure_clause) = begin_node.ensure_clause() {
            if let Some(statements) = ensure_clause.statements() {
                self.track_node(&statements.as_node());
            }
        }

        let typed_branches = branches
            .into_iter()
            .map(|(_, ty, diverges)| (ty, diverges))
            .collect::<Vec<_>>();
        join_branch_types(&typed_branches)
    }

    pub(in crate::inference::type_tracker) fn track_rescue_modifier(
        &mut self,
        rescue_modifier: &RescueModifierNode,
    ) -> RubyType {
        let env_before = self.environment.clone();

        let expression = rescue_modifier.expression();
        self.environment = env_before.clone();
        self.control_flow
            .rescue_entries
            .push(RescueEntryTypes::default());
        let expression_type = self.track_node(&expression);
        let expression_env = self.environment.clone();
        let rescue_entry_types = self.control_flow.rescue_entries.pop().expect(
            "INVARIANT VIOLATED: a rescue-modifier protected-expression accumulator disappeared before rescue analysis. This is a bug because only the matching rescue modifier may pop it. Fix: keep rescue accumulator ownership stack-disciplined.",
        );

        let rescue_expression = rescue_modifier.rescue_expression();
        self.environment = rescue_entry_types.environment_from(&env_before);
        let rescue_type = self.track_node(&rescue_expression);
        let rescue_env = self.environment.clone();

        self.environment = expression_env;
        self.merge_env(&rescue_env, false);

        join_branch_types(&[
            (expression_type, control_flow::diverges(&expression)),
            (rescue_type, control_flow::diverges(&rescue_expression)),
        ])
    }
}
