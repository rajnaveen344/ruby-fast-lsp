use super::protocol::{
    diagnostic_is_unresolved_constant, diagnostic_is_unresolved_method, diagnostic_text,
    location_matches,
};
use super::SimulationRunner;
use crate::test::simulation::graph::MethodTarget;
use crate::test::simulation::oracle::OracleState;
use crate::test::simulation::ruby_gen::CallSite;

impl SimulationRunner {
    pub(super) fn invalid_private_method_calls<'a>(
        &'a self,
        oracle: &'a OracleState<'a>,
    ) -> impl Iterator<Item = &'a CallSite> + 'a {
        self.render
            .map
            .calls
            .iter()
            .filter(|call| self.open_files.contains(&call.pos.file))
            .filter(|call| call.definition_support.is_supported())
            .filter(|call| oracle.resolve_call_targets(call).is_empty())
            .filter(|call| self.observed_project.method_enabled(&call.target))
    }

    pub async fn assert_unresolved_method(&self, file: &str, method: &str) {
        let diagnostics = self.editor.diagnostics(file).await;
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic_is_unresolved_method(diagnostic)
                    && diagnostic_text(self.editor.content(file), diagnostic) == method),
            "Expected unresolved-method diagnostic for `{}` in `{}`. Actual diagnostics: {:?}",
            method,
            file,
            diagnostics
        );
    }

    pub(super) async fn assert_direct_macro_calls_do_not_warn(&self) {
        for macro_call in &self.render.map.direct_macro_calls {
            if !self.open_files.contains(&macro_call.pos.file) {
                continue;
            }
            self.assert_no_unresolved_method(&macro_call.pos.file, &macro_call.name)
                .await;
        }
    }

    pub async fn assert_no_unresolved_method(&self, file: &str, method: &str) {
        let diagnostics = self.editor.diagnostics(file).await;
        assert!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic_is_unresolved_method(diagnostic))
                .all(|diagnostic| diagnostic_text(self.editor.content(file), diagnostic) != method),
            "Expected no unresolved-method diagnostic for `{}` in `{}`. Actual diagnostics: {:?}",
            method,
            file,
            diagnostics
        );
    }

    pub(super) async fn assert_unresolved_constant(&self, file: &str, constant: &str) {
        let diagnostics = self.editor.diagnostics(file).await;
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic_is_unresolved_constant(diagnostic)
                    && diagnostic_text(self.editor.content(file), diagnostic) == constant),
            "Expected unresolved-constant diagnostic for `{}` in `{}`. Actual diagnostics: {:?}",
            constant,
            file,
            diagnostics
        );
    }

    pub(super) async fn assert_no_unresolved_constant(&self, file: &str, constant: &str) {
        let diagnostics = self.editor.diagnostics(file).await;
        assert!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic_is_unresolved_constant(diagnostic))
                .all(
                    |diagnostic| diagnostic_text(self.editor.content(file), diagnostic) != constant
                ),
            "Expected no unresolved-constant diagnostic for `{}` in `{}`. Actual diagnostics: {:?}",
            constant,
            file,
            diagnostics
        );
    }

    pub(super) async fn assert_no_stale_method_definition(
        &self,
        call_target: &MethodTarget,
        stale_target: &MethodTarget,
    ) {
        let stale_def = self.method_def_history.get(stale_target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: stale definition for `{}` is missing from source-map history. This is a bug because stale-target checks must target a method that existed earlier. Fix: update the edit expectation.",
                stale_target.signature()
            )
        });
        for call in self
            .render
            .map
            .calls
            .iter()
            .filter(|call| call.target == *call_target)
            .filter(|call| call.definition_support.is_supported())
            .filter(|call| self.open_files.contains(&call.pos.file))
        {
            let locs = self
                .editor
                .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            assert!(
                locs.iter().all(|loc| !location_matches(loc, stale_def)),
                "Expected goto from {}:{}:{} not to resolve to stale method {} at {}:{}:{}, got {:?}",
                call.pos.file,
                call.pos.line,
                call.pos.character,
                stale_target.signature(),
                stale_def.file,
                stale_def.line,
                stale_def.character,
                locs
            );
        }
    }

    pub(super) async fn assert_no_stale_constant_definition(
        &self,
        ref_target: &str,
        stale_target: &str,
    ) {
        let stale_def = self
            .constant_def_history
            .get(stale_target)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: stale definition for constant `{}` is missing from source-map history. This is a bug because stale-target checks must target a constant that existed earlier. Fix: update the edit expectation.",
                    stale_target
                )
            });
        for constant_ref in self
            .render
            .map
            .constant_refs
            .iter()
            .filter(|constant_ref| constant_ref.target == ref_target)
            .filter(|constant_ref| self.open_files.contains(&constant_ref.pos.file))
        {
            let locs = self
                .editor
                .goto_def_at(
                    &constant_ref.pos.file,
                    constant_ref.pos.line,
                    constant_ref.pos.character,
                )
                .await;
            assert!(
                locs.iter().all(|loc| !location_matches(loc, stale_def)),
                "Expected goto from {}:{}:{} not to resolve to stale constant {} at {}:{}:{}, got {:?}",
                constant_ref.pos.file,
                constant_ref.pos.line,
                constant_ref.pos.character,
                stale_target,
                stale_def.file,
                stale_def.line,
                stale_def.character,
                locs
            );
        }
    }
}
