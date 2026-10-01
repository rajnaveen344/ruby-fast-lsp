use super::protocol::{assert_definition_precedence, location_matches};
use super::SimulationRunner;
use crate::test::simulation::graph::NamespaceKind;
use crate::test::simulation::oracle::OracleState;
use tower_lsp::lsp_types::Position;

impl SimulationRunner {
    pub(super) async fn assert_all_supported_method_calls_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file) {
                continue;
            }
            if !call.definition_support.is_supported() {
                continue;
            }

            let targets = oracle.resolve_call_targets(call);
            if targets.is_empty()
                || targets.iter().any(|target| {
                    target.name == "method_missing" && call.target.name != "method_missing"
                })
            {
                continue;
            }
            let expected = targets
                .iter()
                .map(|target| self.def_pos(target))
                .collect::<Vec<_>>();
            let locs = self
                .editor
                .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            assert!(
                locs.len() == expected.len()
                    && expected.iter().all(|def| locs.iter().filter(|loc| location_matches(loc, def)).count() == 1),
                "exact simulation method definition targets: {} from {} at {}:{}:{}; expected {:?}, got {:?}",
                call.target.signature(), call.caller.signature(), call.pos.file,
                call.pos.line, call.pos.character, targets, locs
            );
            for (before, after) in oracle.definition_order_constraints(call) {
                assert_definition_precedence(&locs, self.def_pos(&before), self.def_pos(&after));
            }
        }
    }

    pub(super) async fn assert_invalid_private_method_calls_do_not_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in self.invalid_private_method_calls(&oracle) {
            let def = self.def_pos(&call.target);
            if !self.indexed_files.contains(&def.file) {
                continue;
            }
            let locs = self
                .editor
                .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            assert!(
                locs.iter().all(|loc| !location_matches(loc, def)),
                "Expected invalid private call from {}:{}:{} not to resolve to {}, got {:?}",
                call.pos.file,
                call.pos.line,
                call.pos.character,
                call.target.signature(),
                locs
            );
        }
    }

    pub(super) async fn assert_all_enabled_constant_refs_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for constant_ref in &self.render.map.constant_refs {
            if !self.open_files.contains(&constant_ref.pos.file) {
                continue;
            }
            let Some(target) = oracle.resolve_constant_ref(constant_ref) else {
                continue;
            };

            let def = self.const_def_pos(&target);
            if !self.indexed_files.contains(&def.file) {
                continue;
            }
            let locs = self
                .editor
                .goto_def_at(
                    &constant_ref.pos.file,
                    constant_ref.pos.line,
                    constant_ref.pos.character,
                )
                .await;
            assert!(
                locs.iter().any(|loc| location_matches(loc, def)),
                "Expected goto from {}:{}:{} to resolve to constant {} at {}:{}:{}, got {:?}",
                constant_ref.pos.file,
                constant_ref.pos.line,
                constant_ref.pos.character,
                target,
                def.file,
                def.line,
                def.character,
                locs
            );
        }
    }

    pub(super) async fn assert_all_supported_namespace_refs_resolve(&self) {
        for namespace_ref in self.namespace_refs() {
            if !self.open_files.contains(&namespace_ref.pos.file) {
                continue;
            }
            if !namespace_ref.support.is_supported() {
                continue;
            }
            if !self
                .observed_project
                .namespace_enabled(&namespace_ref.target)
            {
                continue;
            }

            let def = self.namespace_def(&namespace_ref.target);
            if !self.indexed_files.contains(&def.pos.file) {
                continue;
            }
            let locs = self
                .editor
                .goto_def_at(
                    &namespace_ref.pos.file,
                    namespace_ref.pos.line,
                    namespace_ref.pos.character,
                )
                .await;
            assert!(
                locs.iter().any(|loc| location_matches(loc, &def.pos)),
                "Expected goto from {}:{}:{} to resolve to namespace {} at {}:{}:{}, got {:?}",
                namespace_ref.pos.file,
                namespace_ref.pos.line,
                namespace_ref.pos.character,
                namespace_ref.target,
                def.pos.file,
                def.pos.line,
                def.pos.character,
                locs
            );
        }
    }

    pub(super) async fn assert_module_call_highlights_follow_reference_identity(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file)
                || !matches!(call.shape, super::graph::CallShape::Bare)
                || !self
                    .observed_project
                    .namespaces
                    .iter()
                    .any(|ns| ns.fqn == call.caller.owner && ns.kind == NamespaceKind::Module)
            {
                continue;
            }
            let highlights = self
                .editor
                .document_highlights_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            if oracle
                .resolve_unique_call(call)
                .is_some_and(|target| target.name == call.target.name)
            {
                assert!(
                    highlights.iter().any(|highlight| highlight.range.start
                        == Position::new(call.pos.line, call.pos.character)),
                    "module call highlights must include the current proven call: {:?}",
                    call.pos
                );
            } else if oracle.resolve_call_targets(call).len() > 1 {
                assert!(
                    highlights.is_empty(),
                    "ambiguous module calls have no single highlight identity"
                );
            }
        }
    }

    pub(super) async fn assert_ambiguous_calls_have_no_reference_identity(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file)
                || !call.reference_support.is_supported()
                || oracle.resolve_call_targets(call).len() < 2
            {
                continue;
            }
            let locations = self
                .editor
                .references_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            assert!(locations.is_empty(),
                "ambiguous module call must not borrow one receiver's reference identity: {} from {}; got {:?}",
                call.target.signature(), call.caller.signature(), locations);
        }
    }

    pub(super) async fn assert_reference_sets_cover_calls(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.defs {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if !self.observed_project.method_enabled(target) {
                continue;
            }

            let expected_calls = self
                .render
                .map
                .calls
                .iter()
                .filter(|call| self.open_files.contains(&call.pos.file))
                .filter(|call| call.reference_support.is_supported())
                .filter(|call| {
                    oracle
                        .resolve_unique_call(call)
                        .as_ref()
                        .is_some_and(|resolved| resolved == target)
                })
                .collect::<Vec<_>>();
            if expected_calls.is_empty() {
                continue;
            }

            let locs = self
                .editor
                .references_at(&def.file, def.line, def.character)
                .await;
            for call in expected_calls {
                assert!(
                    locs.iter().any(|loc| location_matches(loc, &call.pos)),
                    "Expected references for {} to include call from {} at {}:{}:{}, got {:?}",
                    target.signature(),
                    call.caller.signature(),
                    call.pos.file,
                    call.pos.line,
                    call.pos.character,
                    locs
                );
            }
        }
    }

    pub(super) async fn assert_call_site_reference_sets_cover_calls(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file) {
                continue;
            }
            if !call.reference_support.is_supported() {
                continue;
            }
            let Some(target) = oracle.resolve_unique_call(call) else {
                continue;
            };
            if !self.observed_project.method_enabled(&target) {
                continue;
            }

            let locs = self
                .editor
                .references_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;

            for sibling_call in self
                .render
                .map
                .calls
                .iter()
                .filter(|sibling_call| self.open_files.contains(&sibling_call.pos.file))
                .filter(|sibling_call| sibling_call.reference_support.is_supported())
                .filter(|sibling_call| {
                    oracle
                        .resolve_unique_call(sibling_call)
                        .as_ref()
                        .is_some_and(|resolved| resolved == &target)
                })
            {
                assert!(
                    locs.iter().any(|loc| location_matches(loc, &sibling_call.pos)),
                    "Expected call-site references from {}:{}:{} to include same-target call {} from {} at {}:{}:{}, got {:?}",
                    call.pos.file,
                    call.pos.line,
                    call.pos.character,
                    target.signature(),
                    sibling_call.caller.signature(),
                    sibling_call.pos.file,
                    sibling_call.pos.line,
                    sibling_call.pos.character,
                    locs
                );
            }

            for wrong_call in self
                .render
                .map
                .calls
                .iter()
                .filter(|wrong_call| self.open_files.contains(&wrong_call.pos.file))
                .filter(|wrong_call| wrong_call.reference_support.is_supported())
                .filter(|wrong_call| wrong_call.target.name == target.name)
                .filter(|wrong_call| {
                    oracle
                        .resolve_unique_call(wrong_call)
                        .as_ref()
                        .is_some_and(|resolved| resolved != &target)
                })
            {
                assert!(
                    locs.iter().all(|loc| !location_matches(loc, &wrong_call.pos)),
                    "Expected call-site references from {}:{}:{} for {} not to include wrong-owner call from {} at {}:{}:{}, got {:?}",
                    call.pos.file,
                    call.pos.line,
                    call.pos.character,
                    target.signature(),
                    wrong_call.caller.signature(),
                    wrong_call.pos.file,
                    wrong_call.pos.line,
                    wrong_call.pos.character,
                    locs
                );
            }
        }
    }

    pub(super) async fn assert_invalid_private_method_calls_excluded_from_references(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for invalid_call in self.invalid_private_method_calls(&oracle) {
            let def = self.def_pos(&invalid_call.target);
            if self.open_files.contains(&def.file) {
                let locs = self
                    .editor
                    .references_at(&def.file, def.line, def.character)
                    .await;
                assert!(
                    locs.iter()
                        .all(|loc| !location_matches(loc, &invalid_call.pos)),
                    "Expected references for {} not to include invalid private call {}:{}:{}, got {:?}",
                    invalid_call.target.signature(),
                    invalid_call.pos.file,
                    invalid_call.pos.line,
                    invalid_call.pos.character,
                    locs
                );
            }

            for valid_call in self
                .render
                .map
                .calls
                .iter()
                .filter(|call| self.open_files.contains(&call.pos.file))
                .filter(|call| call.reference_support.is_supported())
                .filter(|call| {
                    oracle
                        .resolve_unique_call(call)
                        .as_ref()
                        .is_some_and(|resolved| resolved == &invalid_call.target)
                })
            {
                let locs = self
                    .editor
                    .references_at(
                        &valid_call.pos.file,
                        valid_call.pos.line,
                        valid_call.pos.character,
                    )
                    .await;
                assert!(
                    locs.iter()
                        .all(|loc| !location_matches(loc, &invalid_call.pos)),
                    "Expected call-site references from {}:{}:{} for {} not to include invalid private call {}:{}:{}, got {:?}",
                    valid_call.pos.file,
                    valid_call.pos.line,
                    valid_call.pos.character,
                    invalid_call.target.signature(),
                    invalid_call.pos.file,
                    invalid_call.pos.line,
                    invalid_call.pos.character,
                    locs
                );
            }
        }
    }

    pub(super) async fn assert_reference_sets_cover_constant_refs(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.constants {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if !self.observed_project.constant_enabled(target) {
                continue;
            }

            let expected_refs = self
                .render
                .map
                .constant_refs
                .iter()
                .filter(|constant_ref| self.open_files.contains(&constant_ref.pos.file))
                .filter(|constant_ref| {
                    oracle
                        .resolve_constant_ref(constant_ref)
                        .as_ref()
                        .is_some_and(|resolved| resolved == target)
                })
                .collect::<Vec<_>>();
            if expected_refs.is_empty() {
                continue;
            }

            let locs = self
                .editor
                .references_at(&def.file, def.line, def.character)
                .await;
            for constant_ref in expected_refs {
                assert!(
                    locs.iter()
                        .any(|loc| location_matches(loc, &constant_ref.pos)),
                    "Expected references for constant {} to include {}:{}:{}, got {:?}",
                    target,
                    constant_ref.pos.file,
                    constant_ref.pos.line,
                    constant_ref.pos.character,
                    locs
                );
            }
        }
    }

    pub(super) async fn assert_reference_sets_cover_namespace_refs(&self) {
        for (target, def) in &self.render.map.namespaces {
            if !self.open_files.contains(&def.pos.file) {
                continue;
            }
            if !self.observed_project.namespace_enabled(target) {
                continue;
            }

            let expected_refs = self
                .namespace_refs()
                .filter(|namespace_ref| self.open_files.contains(&namespace_ref.pos.file))
                .filter(|namespace_ref| namespace_ref.target == *target)
                .filter(|namespace_ref| namespace_ref.support.is_supported())
                .collect::<Vec<_>>();
            if expected_refs.is_empty() {
                continue;
            }

            let locs = self
                .editor
                .references_at(&def.pos.file, def.pos.line, def.pos.character)
                .await;
            for namespace_ref in expected_refs {
                assert!(
                    locs.iter()
                        .any(|loc| location_matches(loc, &namespace_ref.pos)),
                    "Expected references for namespace {} to include {}:{}:{}, got {:?}",
                    target,
                    namespace_ref.pos.file,
                    namespace_ref.pos.line,
                    namespace_ref.pos.character,
                    locs
                );
            }
        }
    }
}
