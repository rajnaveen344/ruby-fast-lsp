use super::graph::{MethodTarget, NamespaceKind};
use super::oracle::OracleState;
use super::project::{EditOp, EditStep, ExpectedCheck, SyntheticProject};
use super::ruby_gen::{
    CallSite, NamespaceDefSite, NamespaceRefSite, ProjectRender, SourcePos, TypeAssertKind,
};
use crate::test::harness::{get_hint_label, FakeEditor};
use std::collections::{BTreeSet, HashMap};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, Hover, HoverContents, Location, MarkedString, NumberOrString,
    Position,
};

pub struct SimulationRunner {
    project: SyntheticProject,
    editor: FakeEditor,
    render: ProjectRender,
    open_files: BTreeSet<String>,
    indexed_files: BTreeSet<String>,
    method_def_history: HashMap<MethodTarget, SourcePos>,
    constant_def_history: HashMap<String, SourcePos>,
}

impl SimulationRunner {
    pub async fn start(project: SyntheticProject) -> Self {
        let render = project.render();
        let mut editor = FakeEditor::new().await;

        for (file, content) in &render.files {
            editor.open(file, content).await;
        }
        let open_files = render.files.keys().cloned().collect::<BTreeSet<_>>();
        let indexed_files = open_files.clone();
        let method_def_history = render.map.defs.clone();
        let constant_def_history = render.map.constants.clone();

        Self {
            project,
            editor,
            render,
            open_files,
            indexed_files,
            method_def_history,
            constant_def_history,
        }
    }

    pub async fn start_with_open_files(project: SyntheticProject, files: &[&str]) -> Self {
        let render = project.render();
        let mut editor = FakeEditor::new().await;
        let mut open_files = BTreeSet::new();
        let mut indexed_files = BTreeSet::new();

        for file in files {
            let content = render.files.get(*file).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: simulation partial open requested missing file `{}`. This is a bug because open order must reference generated files. Fix: update the partial-open test fixture.",
                    file
                )
            });
            editor.open(file, content).await;
            open_files.insert((*file).to_string());
            indexed_files.insert((*file).to_string());
        }
        let method_def_history = render.map.defs.clone();
        let constant_def_history = render.map.constants.clone();

        Self {
            project,
            editor,
            render,
            open_files,
            indexed_files,
            method_def_history,
            constant_def_history,
        }
    }

    pub async fn check_initial(&self) {
        self.assert_index_shape();
        self.check_definitions().await;
        self.check_references().await;
        self.check_hover().await;
        self.check_types().await;
        let oracle = OracleState::with_indexed_files(
            &self.project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file) {
                continue;
            }
            if oracle.resolve_call(call).is_some() {
                self.assert_no_unresolved_method(&call.pos.file, &call.target.name)
                    .await;
            }
        }
        for constant_ref in &self.render.map.constant_refs {
            if !self.open_files.contains(&constant_ref.pos.file) {
                continue;
            }
            if let Some(target) = oracle.resolve_constant_ref(constant_ref) {
                self.assert_no_unresolved_constant(&constant_ref.pos.file, constant_name(&target))
                    .await;
            }
        }
    }

    pub fn known_gap_reasons(&self) -> BTreeSet<&'static str> {
        let mut reasons = BTreeSet::new();
        for call in &self.render.map.calls {
            if let Some(reason) = call.definition_support.gap_reason() {
                reasons.insert(reason);
            }
            if let Some(reason) = call.reference_support.gap_reason() {
                reasons.insert(reason);
            }
            if let Some(reason) = call.hover_support.gap_reason() {
                reasons.insert(reason);
            }
        }
        for namespace_ref in self.namespace_refs() {
            if let Some(reason) = namespace_ref.support.gap_reason() {
                reasons.insert(reason);
            }
        }
        reasons
    }

    pub async fn check_definitions(&self) {
        self.assert_all_supported_method_calls_resolve().await;
        self.assert_invalid_private_method_calls_do_not_resolve()
            .await;
        self.assert_all_enabled_constant_refs_resolve().await;
        self.assert_all_supported_namespace_refs_resolve().await;
    }

    pub async fn check_references(&self) {
        self.assert_reference_sets_cover_calls().await;
        self.assert_call_site_reference_sets_cover_calls().await;
        self.assert_invalid_private_method_calls_excluded_from_references()
            .await;
        self.assert_reference_sets_cover_constant_refs().await;
        self.assert_reference_sets_cover_namespace_refs().await;
    }

    pub async fn check_hover(&self) {
        self.assert_namespace_hovers().await;
        self.assert_method_hovers().await;
        self.assert_constant_hovers().await;
        self.assert_type_hovers().await;
    }

    pub async fn check_types(&self) {
        self.assert_type_hints().await;
    }

    pub async fn check_all_semantics(&self) {
        self.check_definitions().await;
        self.check_references().await;
        self.check_hover().await;
        self.check_types().await;
    }

    pub async fn open_file(&mut self, file: &str) {
        assert!(
            !self.open_files.contains(file),
            "INVARIANT VIOLATED: simulation tried to open file `{}` twice. This is a bug because partial-open tests should have deterministic unique open order. Fix: remove the duplicate file.",
            file
        );
        let content = self.render.files.get(file).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: simulation tried to open missing file `{}`. This is a bug because partial-open tests must reference generated files. Fix: update the open order.",
                file
            )
        });
        self.editor.open(file, content).await;
        self.open_files.insert(file.to_string());
        self.indexed_files.insert(file.to_string());
    }

    pub async fn close_file(&mut self, file: &str) {
        assert!(
            self.open_files.contains(file),
            "INVARIANT VIOLATED: simulation tried to close unopened file `{}`. This is a bug because lifecycle steps must close only open files. Fix: inspect seeded lifecycle generation.",
            file
        );
        self.editor.close(file).await;
        self.open_files.remove(file);
    }

    pub async fn assert_call_resolves_to(&self, target: &str) {
        let target = MethodTarget::parse(target);
        let call = self
            .render
            .map
            .calls
            .iter()
            .find(|call| call.target == target)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: no generated call targets `{}`. This is a bug because test assertions must target generated calls. Fix: update the project graph.",
                    target.signature()
                )
            });
        assert!(
            self.open_files.contains(&call.pos.file),
            "INVARIANT VIOLATED: call file `{}` is not open. This is a bug because call assertions must open caller files first. Fix: update partial-open order.",
            call.pos.file
        );

        let def = self.def_pos(&target);
        let locs = self
            .editor
            .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
            .await;
        assert!(
            locs.iter().any(|loc| location_matches(loc, def)),
            "Expected goto from {}:{}:{} to resolve to {} at {}:{}:{}, got {:?}",
            call.pos.file,
            call.pos.line,
            call.pos.character,
            target.signature(),
            def.file,
            def.line,
            def.character,
            locs
        );
    }

    pub async fn assert_call_does_not_resolve_to(&self, target: &str) {
        let target = MethodTarget::parse(target);
        let call = self
            .render
            .map
            .calls
            .iter()
            .find(|call| call.target == target)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: no generated call targets `{}`. This is a bug because test assertions must target generated calls. Fix: update the project graph.",
                    target.signature()
                )
            });
        assert!(
            self.open_files.contains(&call.pos.file),
            "INVARIANT VIOLATED: call file `{}` is not open. This is a bug because call assertions must open caller files first. Fix: update partial-open order.",
            call.pos.file
        );

        let def = self.def_pos(&target);
        let locs = self
            .editor
            .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
            .await;
        assert!(
            locs.iter().all(|loc| !location_matches(loc, def)),
            "Expected goto from {}:{}:{} not to resolve to {} before partial namespace opens, got {:?}",
            call.pos.file,
            call.pos.line,
            call.pos.character,
            target.signature(),
            locs
        );
    }

    pub async fn apply_step(&mut self, step: &EditStep) {
        for op in &step.ops {
            self.project.apply_op(op);
        }

        let next_render = self.project.render();
        for (file, next_content) in &next_render.files {
            let should_set = self
                .render
                .files
                .get(file)
                .map(|old_content| old_content != next_content)
                .unwrap_or(true);

            if should_set {
                if self.render.files.contains_key(file) {
                    if self.open_files.contains(file) {
                        self.editor.set(file, next_content).await;
                    }
                } else {
                    self.editor.open(file, next_content).await;
                    self.open_files.insert(file.clone());
                    self.indexed_files.insert(file.clone());
                }
            }
        }

        self.render = next_render;
        self.assert_index_shape();

        for expected in &step.expected {
            match expected {
                ExpectedCheck::UnresolvedMethod { file, method } => {
                    if !self.open_files.contains(file) {
                        continue;
                    }
                    self.assert_unresolved_method(file, method).await;
                }
                ExpectedCheck::NoUnresolvedMethod { file, method } => {
                    if !self.open_files.contains(file) {
                        continue;
                    }
                    self.assert_no_unresolved_method(file, method).await;
                }
                ExpectedCheck::UnresolvedConstant { file, constant } => {
                    if !self.open_files.contains(file) {
                        continue;
                    }
                    self.assert_unresolved_constant(file, constant).await;
                }
                ExpectedCheck::NoUnresolvedConstant { file, constant } => {
                    if !self.open_files.contains(file) {
                        continue;
                    }
                    self.assert_no_unresolved_constant(file, constant).await;
                }
                ExpectedCheck::NoMethodDefinitionTarget {
                    call_target,
                    stale_target,
                } => {
                    self.assert_no_stale_method_definition(call_target, stale_target)
                        .await;
                }
                ExpectedCheck::NoConstantDefinitionTarget {
                    ref_target,
                    stale_target,
                } => {
                    self.assert_no_stale_constant_definition(ref_target, stale_target)
                        .await;
                }
            }
        }

        self.method_def_history.extend(self.render.map.defs.clone());
        self.constant_def_history
            .extend(self.render.map.constants.clone());

        self.check_all_semantics().await;
    }

    pub async fn close_and_reopen(&mut self, file: &str) {
        let content = self.editor.content(file).to_string();
        self.editor.close(file).await;
        self.open_files.remove(file);
        self.editor.open(file, &content).await;
        self.open_files.insert(file.to_string());
        self.indexed_files.insert(file.to_string());
        self.check_all_semantics().await;
    }

    pub async fn run_edit_script_step(&mut self, step: &super::seeded::SeededStep) {
        match step {
            super::seeded::SeededStep::CheckDefinitions => self.check_definitions().await,
            super::seeded::SeededStep::CheckReferences => self.check_references().await,
            super::seeded::SeededStep::CheckHover => self.check_hover().await,
            super::seeded::SeededStep::CheckTypes => self.check_types().await,
            super::seeded::SeededStep::ApplyEdit { index } => {
                let edit = self.project.edits.get(*index).cloned().unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: seeded script edit index `{}` is out of bounds. This is a bug because seeded scripts must reference generated edits. Fix: inspect seeded_script.",
                        index
                    )
                });
                self.apply_step(&edit).await;
            }
            super::seeded::SeededStep::CloseReopen { file } => {
                self.close_and_reopen(file).await;
            }
            super::seeded::SeededStep::OpenFile { file } => {
                self.open_file(file).await;
                self.check_all_semantics().await;
            }
            super::seeded::SeededStep::CloseFile { file } => {
                self.close_file(file).await;
                self.check_all_semantics().await;
            }
        }
    }

    fn assert_index_shape(&self) {
        let stats = self.editor.server().analysis_engine.read().stats();
        assert!(
            stats.files >= self.open_files.len(),
            "INVARIANT VIOLATED: simulation index has too few files. Expected at least {}, got {}. This is a bug because every indexed generated file was opened through FakeEditor. Fix: inspect didOpen indexing.",
            self.indexed_files.len(),
            stats.files
        );
        if self.indexed_files.len() == self.render.files.len() {
            assert!(
                stats.methods >= self.project.enabled_method_count(),
                "INVARIANT VIOLATED: simulation index has too few methods. Expected at least {}, got {}. This is a bug because every enabled generated method should emit a method fact. Fix: inspect method fact collection.",
                self.project.enabled_method_count(),
                stats.methods
            );
        }
    }

    async fn assert_all_supported_method_calls_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
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

            let Some(expected_target) = oracle.resolve_call(call) else {
                continue;
            };
            let def = self.def_pos(&expected_target);
            let locs = self
                .editor
                .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
                .await;
            assert!(
                locs.iter().any(|loc| location_matches(loc, def)),
                "Expected goto from {}:{}:{} to resolve to {} at {}:{}:{}, got {:?}",
                call.pos.file,
                call.pos.line,
                call.pos.character,
                expected_target.signature(),
                def.file,
                def.line,
                def.character,
                locs
            );
        }
    }

    async fn assert_invalid_private_method_calls_do_not_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
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

    async fn assert_all_enabled_constant_refs_resolve(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
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

    async fn assert_all_supported_namespace_refs_resolve(&self) {
        for namespace_ref in self.namespace_refs() {
            if !self.open_files.contains(&namespace_ref.pos.file) {
                continue;
            }
            if !namespace_ref.support.is_supported() {
                continue;
            }
            if !self.project.namespace_enabled(&namespace_ref.target) {
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

    async fn assert_reference_sets_cover_calls(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.defs {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if !self.project.method_enabled(target) {
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
                        .resolve_call(call)
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

    async fn assert_call_site_reference_sets_cover_calls(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
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
            let Some(target) = oracle.resolve_call(call) else {
                continue;
            };
            if !self.project.method_enabled(&target) {
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
                        .resolve_call(sibling_call)
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
                        .resolve_call(wrong_call)
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

    async fn assert_invalid_private_method_calls_excluded_from_references(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
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
                        .resolve_call(call)
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

    async fn assert_reference_sets_cover_constant_refs(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.constants {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if !self.project.constant_enabled(target) {
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

    async fn assert_reference_sets_cover_namespace_refs(&self) {
        for (target, def) in &self.render.map.namespaces {
            if !self.open_files.contains(&def.pos.file) {
                continue;
            }
            if !self.project.namespace_enabled(target) {
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

    async fn assert_namespace_hovers(&self) {
        for (target, def) in &self.render.map.namespaces {
            if !self.open_files.contains(&def.pos.file) {
                continue;
            }
            if !self.project.namespace_enabled(target) {
                continue;
            }
            self.assert_hover_contains(&def.pos, &namespace_hover_label(target, def.kind))
                .await;
        }

        for namespace_ref in self.namespace_refs() {
            if !self.open_files.contains(&namespace_ref.pos.file) {
                continue;
            }
            if !namespace_ref.support.is_supported() {
                continue;
            }
            if !self.project.namespace_enabled(&namespace_ref.target) {
                continue;
            }
            let def = self.namespace_def(&namespace_ref.target);
            if !self.indexed_files.contains(&def.pos.file) {
                continue;
            }
            self.assert_hover_contains(
                &namespace_ref.pos,
                &namespace_hover_label(&namespace_ref.target, def.kind),
            )
            .await;
        }
    }

    async fn assert_method_hovers(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.defs {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if !self.project.method_enabled(target) {
                continue;
            }
            if self.project.delegate_enabled(target) {
                continue;
            }
            if let Some(return_type) = self.project.method_return_type(target) {
                self.assert_hover_contains(def, return_type).await;
            }
        }

        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file) {
                continue;
            }
            if !call.hover_support.is_supported() {
                continue;
            }
            let Some(target) = oracle.resolve_call(call) else {
                continue;
            };
            let Some(return_type) = self.project.method_return_type(&target) else {
                continue;
            };
            let def = self.def_pos(&target);
            if !self.indexed_files.contains(&def.file) {
                continue;
            }
            self.assert_hover_contains(&call.pos, return_type).await;
        }

        for call in self.invalid_private_method_calls(&oracle) {
            let Some(return_type) = self.project.method_return_type(&call.target) else {
                continue;
            };
            self.assert_hover_not_contains(&call.pos, return_type).await;
        }
    }

    async fn assert_constant_hovers(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.constants {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if self.project.constant_enabled(target) {
                self.assert_hover_contains(def, constant_name(target)).await;
            }
        }

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
            self.assert_hover_contains(&constant_ref.pos, constant_name(&target))
                .await;
        }
    }

    async fn assert_type_hovers(&self) {
        for type_assert in &self.render.map.type_asserts {
            if !self.open_files.contains(&type_assert.pos.file) {
                continue;
            }
            if !self.project.method_enabled(&type_assert.owner) {
                continue;
            }
            if type_assert.kind != TypeAssertKind::LocalAssignment {
                continue;
            }
            self.assert_hover_contains(&type_assert.pos, &type_assert.expected)
                .await;
        }
    }

    async fn assert_type_hints(&self) {
        let mut hints_by_file = HashMap::new();
        for file in self
            .render
            .map
            .type_asserts
            .iter()
            .map(|type_assert| type_assert.pos.file.as_str())
            .filter(|file| self.open_files.contains(*file))
            .collect::<BTreeSet<_>>()
        {
            hints_by_file.insert(file.to_string(), self.editor.inlay_hints(file).await);
        }
        for type_assert in &self.render.map.type_asserts {
            if !self.open_files.contains(&type_assert.pos.file) {
                continue;
            }
            if !self.project.method_enabled(&type_assert.owner) {
                continue;
            }
            let hints = hints_by_file.get(&type_assert.pos.file).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: type hints for open file `{}` were not collected. This is a bug because assert_type_hints preloads every open type-assert file. Fix: inspect file collection.",
                    type_assert.pos.file
                )
            });
            let expected = match type_assert.kind {
                TypeAssertKind::LocalAssignment => &type_assert.expected,
                TypeAssertKind::MethodReturnHint => &type_assert.expected,
            };
            assert!(
                hints.iter().any(|hint| {
                    hint.position.line == type_assert.pos.line
                        && get_hint_label(hint).contains(expected)
                }),
                "Expected type hint containing `{}` at {}:{}:{} for {:?}, got {:?}",
                expected,
                type_assert.pos.file,
                type_assert.pos.line,
                type_assert.pos.character,
                type_assert.kind,
                hints
                    .iter()
                    .map(|hint| {
                        format!(
                            "{}:{} {}",
                            hint.position.line,
                            hint.position.character,
                            get_hint_label(hint)
                        )
                    })
                    .collect::<Vec<_>>()
            );
        }
    }

    async fn assert_hover_contains(&self, pos: &SourcePos, expected: &str) {
        let hover = self
            .editor
            .hover_at(&pos.file, pos.line, pos.character)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "Expected hover at {}:{}:{} to contain `{}`, got None",
                    pos.file, pos.line, pos.character, expected
                )
            });
        let content = hover_text(&hover);
        assert!(
            content.contains(expected),
            "Expected hover at {}:{}:{} to contain `{}`, got `{}`",
            pos.file,
            pos.line,
            pos.character,
            expected,
            content
        );
    }

    async fn assert_hover_not_contains(&self, pos: &SourcePos, forbidden: &str) {
        if let Some(hover) = self
            .editor
            .hover_at(&pos.file, pos.line, pos.character)
            .await
        {
            let content = hover_text(&hover);
            assert!(
                !content.contains(forbidden),
                "Expected hover at {}:{}:{} not to contain `{}`, got `{}`",
                pos.file,
                pos.line,
                pos.character,
                forbidden,
                content
            );
        }
    }

    fn invalid_private_method_calls<'a>(
        &'a self,
        oracle: &'a OracleState<'a>,
    ) -> impl Iterator<Item = &'a CallSite> + 'a {
        self.render
            .map
            .calls
            .iter()
            .filter(|call| self.open_files.contains(&call.pos.file))
            .filter(|call| call.definition_support.is_supported())
            .filter(|call| oracle.resolve_call(call).is_none())
            .filter(|call| self.project.method_enabled(&call.target))
    }

    async fn assert_unresolved_method(&self, file: &str, method: &str) {
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

    async fn assert_no_unresolved_method(&self, file: &str, method: &str) {
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

    async fn assert_unresolved_constant(&self, file: &str, constant: &str) {
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

    async fn assert_no_unresolved_constant(&self, file: &str, constant: &str) {
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

    async fn assert_no_stale_method_definition(
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

    async fn assert_no_stale_constant_definition(&self, ref_target: &str, stale_target: &str) {
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

    fn def_pos(&self, target: &MethodTarget) -> &SourcePos {
        self.render.map.defs.get(target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: definition for `{}` is missing from source map. This is a bug because enabled method calls must target generated definitions. Fix: inspect Ruby generator method anchors.",
                target.signature()
            )
        })
    }

    fn const_def_pos(&self, fqn: &str) -> &SourcePos {
        self.render.map.constants.get(fqn).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: definition for constant `{}` is missing from source map. This is a bug because enabled constant refs must target generated definitions. Fix: inspect Ruby generator constant anchors.",
                fqn
            )
        })
    }

    fn namespace_def(&self, fqn: &str) -> &NamespaceDefSite {
        self.render.map.namespaces.get(fqn).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: definition for namespace `{}` is missing from source map. This is a bug because enabled namespace refs must target generated definitions. Fix: inspect Ruby generator namespace anchors.",
                fqn
            )
        })
    }

    fn namespace_refs(&self) -> impl Iterator<Item = &NamespaceRefSite> {
        self.render
            .map
            .include_refs
            .iter()
            .chain(self.render.map.superclass_refs.iter())
    }
}

impl EditStep {
    pub fn touches_target(&self, target: &MethodTarget) -> bool {
        self.ops.iter().any(|op| match op {
            EditOp::DeleteMethod(op_target) | EditOp::RestoreMethod(op_target) => {
                op_target == target
            }
            EditOp::DeleteConstant(_)
            | EditOp::RestoreConstant(_)
            | EditOp::DeleteNamespace(_)
            | EditOp::RestoreNamespace(_)
            | EditOp::RemoveInclude { .. }
            | EditOp::AddInclude { .. }
            | EditOp::RemovePrepend { .. }
            | EditOp::AddPrepend { .. }
            | EditOp::ChangeSuperclass { .. }
            | EditOp::ClearSuperclass { .. } => false,
        })
    }
}

fn location_matches(loc: &Location, pos: &SourcePos) -> bool {
    loc.uri.path().ends_with(&pos.file) && loc.range.start.line == pos.line
}

fn diagnostic_is_unresolved_method(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity == Some(DiagnosticSeverity::WARNING)
        && matches!(
            &diagnostic.code,
            Some(NumberOrString::String(code)) if code == "unresolved-method"
        )
}

fn diagnostic_is_unresolved_constant(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity == Some(DiagnosticSeverity::ERROR)
        && matches!(
            &diagnostic.code,
            Some(NumberOrString::String(code)) if code == "unresolved-constant"
        )
}

fn constant_name(fqn: &str) -> &str {
    fqn.rsplit("::").next().unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: constant FQN `{}` has no name segment. This is a bug because generated constants must be fully-qualified. Fix: validate constant refs in the project model.",
            fqn
        )
    })
}

fn namespace_hover_label(fqn: &str, kind: NamespaceKind) -> String {
    match kind {
        NamespaceKind::Class | NamespaceKind::Module => namespace_name(fqn).to_string(),
    }
}

fn namespace_name(fqn: &str) -> &str {
    fqn.rsplit("::").next().unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: namespace FQN `{}` has no name segment. This is a bug because generated namespaces must be fully-qualified. Fix: validate namespace refs in the project model.",
            fqn
        )
    })
}

fn hover_text(hover: &Hover) -> String {
    match &hover.contents {
        HoverContents::Scalar(text) => marked_string_text(text),
        HoverContents::Array(items) => items
            .iter()
            .map(marked_string_text)
            .collect::<Vec<_>>()
            .join("\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn marked_string_text(text: &MarkedString) -> String {
    match text {
        MarkedString::String(value) => value.clone(),
        MarkedString::LanguageString(value) => value.value.clone(),
    }
}

fn diagnostic_text(content: &str, diagnostic: &Diagnostic) -> String {
    let start = position_to_byte_offset(content, diagnostic.range.start);
    let end = position_to_byte_offset(content, diagnostic.range.end);
    content[start..end].to_string()
}

fn position_to_byte_offset(content: &str, position: Position) -> usize {
    let mut offset = 0;
    for (line_idx, line) in content.lines().enumerate() {
        if line_idx as u32 == position.line {
            return offset + position.character as usize;
        }
        offset += line.len() + 1;
    }
    content.len()
}
