use super::source_text::{indent_len, method_slug, return_expression};
use super::{FileRenderer, SourcePos, TypeAssertKind, TypeAssertSite};
use crate::simulation::graph::{MethodDefForm, MethodKind, MethodSpec, MethodTarget};

impl FileRenderer {
    pub(super) fn render_return_assignment(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        block_type_asserts: bool,
    ) {
        let slug = method_slug(&caller.name);
        let expression = return_expression(return_type);
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_if_{}", slug),
            &format!("if true then {} else {} end", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_unless_{}", slug),
            &format!("unless false then {} else {} end", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_case_{}", slug),
            &format!(
                "case :sim when :sim then {} else {} end",
                expression, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_rescue_{}", slug),
            &format!(
                "begin {}; rescue StandardError; {}; ensure nil; end",
                expression, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_rescue_modifier_{}", slug),
            &format!("{} rescue {}", expression, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_return_{}", slug),
            &expression,
        );

        if block_type_asserts {
            self.render_block_type_assignments(caller, return_type, depth, &slug, &expression);
        }
    }

    fn render_block_type_assignments(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        slug: &str,
        expression: &str,
    ) {
        let array_local = format!("__sim_array_value_{}", slug);
        self.push_line(
            depth,
            &format!("[{}].each do |{}|", expression, array_local),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_array_copy_{}", slug),
            &array_local,
        );
        self.push_line(depth, "end");

        self.push_line(depth, &format!("[{}].each do", expression));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_array_numbered_copy_{}", slug),
            "_1",
        );
        self.push_line(depth, "end");

        let pattern_local = format!("__sim_pattern_value_{}", slug);
        self.push_line(depth, &format!("case {{value: {}}}", expression));
        self.push_line(depth, &format!("in {{value: {}}}", pattern_local));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_pattern_copy_{}", slug),
            &pattern_local,
        );
        self.push_line(depth, "end");

        self.push_line(
            depth,
            &format!("__sim_lambda_builder_{} = -> {{ {} }}", slug, expression),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_lambda_result_{}", slug),
            &format!("__sim_lambda_builder_{}.call", slug),
        );

        self.push_line(
            depth,
            &format!(
                "__sim_proc_builder_{} = Proc.new {{ {} }}",
                slug, expression
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_proc_result_{}", slug),
            &format!("__sim_proc_builder_{}.call", slug),
        );

        let helper = format!("__sim_with_value_{}", slug);

        let local = format!("__sim_yield_value_{}", slug);
        let result = format!("__sim_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(depth, &format!("{} = {} do |{}|", result, helper, local));
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_block_copy_{}", slug),
            &local,
        );
        self.push_line(depth + 1, &local);
        self.push_line(depth, "end");

        self.render_typed_local_assignment(
            caller,
            return_type,
            depth,
            &format!("__sim_yield_numbered_result_{}", slug),
            &format!("{} {{ _1 }}", helper),
        );

        let forward_helper = format!("__sim_forward_value_{}", slug);
        let forward_local = format!("__sim_forward_yield_value_{}", slug);
        let forward_result = format!("__sim_forward_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + forward_result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(
            depth,
            &format!(
                "{} = {} do |{}|",
                forward_result, forward_helper, forward_local
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_forward_block_copy_{}", slug),
            &forward_local,
        );
        self.push_line(depth + 1, &forward_local);
        self.push_line(depth, "end");

        let dot_forward_helper = format!("__sim_dot_forward_value_{}", slug);
        let dot_forward_local = format!("__sim_dot_forward_yield_value_{}", slug);
        let dot_forward_result = format!("__sim_dot_forward_yield_result_{}", slug);
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + dot_forward_result.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(
            depth,
            &format!(
                "{} = {} do |{}|",
                dot_forward_result, dot_forward_helper, dot_forward_local
            ),
        );
        self.render_typed_local_assignment(
            caller,
            return_type,
            depth + 1,
            &format!("__sim_dot_forward_block_copy_{}", slug),
            &dot_forward_local,
        );
        self.push_line(depth + 1, &dot_forward_local);
        self.push_line(depth, "end");
    }

    pub(super) fn render_block_type_helper(&mut self, method: &MethodSpec, depth: usize) {
        let return_type = method.return_type.as_ref().unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: simulator method `{}` requested block type assertions without return type. This is a bug because generated block helpers need a typed yield expression. Fix: call returns(...) before with_block_type_asserts().",
                method.name
            )
        });
        assert!(
            method.kind == MethodKind::Instance && method.def_form == MethodDefForm::Regular,
            "INVARIANT VIOLATED: simulator method `{}` requested block type assertions for unsupported method form. This is a bug because helper generation currently models regular instance methods only. Fix: extend render_block_type_helper for this method form before enabling it.",
            method.name
        );

        let slug = method_slug(&method.name);
        self.push_line(depth, &format!("def __sim_with_value_{}", slug));
        self.push_line(
            depth + 1,
            &format!("yield {}", return_expression(return_type)),
        );
        self.push_line(depth, "end");
        self.push_line(depth, &format!("def __sim_forward_value_{}(&)", slug));
        self.push_line(depth + 1, &format!("__sim_with_value_{}(&)", slug));
        self.push_line(depth, "end");
        self.push_line(depth, &format!("def __sim_dot_forward_value_{}(...)", slug));
        self.push_line(depth + 1, &format!("__sim_with_value_{}(...)", slug));
        self.push_line(depth, "end");
        self.push_line(depth, "");
    }

    fn render_typed_local_assignment(
        &mut self,
        caller: &MethodTarget,
        return_type: &str,
        depth: usize,
        local: &str,
        expression: &str,
    ) {
        self.map.type_asserts.push(TypeAssertSite {
            owner: caller.clone(),
            expected: return_type.to_string(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + local.len()) as u32,
            },
            kind: TypeAssertKind::LocalAssignment,
        });
        self.push_line(depth, &format!("{} = {}", local, expression));
    }
}
