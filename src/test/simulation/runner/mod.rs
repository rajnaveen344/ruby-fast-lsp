//! LSP-handler simulation runner that compares observations with the model oracle.

#[cfg(test)]
mod consistency_controls;
mod diagnostics;
mod hovers;
mod navigation;
mod protocol;

use super::graph::{self, MethodTarget};
use super::oracle::OracleState;
use super::project::{EditStep, ExpectedCheck, SyntheticProject};
use super::ruby_gen::{NamespaceDefSite, NamespaceRefSite, ProjectRender, SourcePos};
use crate::test::harness::FakeEditor;
use protocol::{
    constant_name, definition_observation, diagnostic_text, location_matches, sorted_lsp_values,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, GotoDefinitionResponse, Location, Position,
};

pub struct SimulationRunner {
    /// Current workspace source, including edits not yet delivered to LSP.
    project: SyntheticProject,
    /// Independent semantic model of the last delivered revision of each file.
    observed_project: SyntheticProject,
    editor: FakeEditor,
    render: ProjectRender,
    open_files: BTreeSet<String>,
    indexed_files: BTreeSet<String>,
    /// Last contents actually delivered to LSP, including retained closed files.
    /// Rendering an edit to a closed file does not update this snapshot.
    indexed_contents: BTreeMap<String, String>,
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
        let indexed_contents = render.files.clone();
        let method_def_history = render.map.defs.clone();
        let constant_def_history = render.map.constants.clone();

        Self {
            observed_project: project.clone(),
            project,
            editor,
            render,
            open_files,
            indexed_files,
            indexed_contents,
            method_def_history,
            constant_def_history,
        }
    }

    pub async fn start_with_open_files(project: SyntheticProject, files: &[&str]) -> Self {
        let render = project.render();
        let mut editor = FakeEditor::new().await;
        let mut open_files = BTreeSet::new();
        let mut indexed_files = BTreeSet::new();
        let mut indexed_contents = BTreeMap::new();

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
            indexed_contents.insert((*file).to_string(), content.clone());
        }
        let method_def_history = render.map.defs.clone();
        let constant_def_history = render.map.constants.clone();

        Self {
            observed_project: project.clone(),
            project,
            editor,
            render,
            open_files,
            indexed_files,
            indexed_contents,
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
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for call in &self.render.map.calls {
            if !self.open_files.contains(&call.pos.file) {
                continue;
            }
            if !oracle.resolve_call_targets(call).is_empty() {
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
        self.assert_direct_macro_calls_do_not_warn().await;
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
        self.assert_module_call_highlights_follow_reference_identity()
            .await;
        self.assert_ambiguous_calls_have_no_reference_identity()
            .await;
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
        self.assert_direct_macro_calls_do_not_warn().await;
    }

    /// Compare externally visible results with a clean analysis of the exact
    /// content delivered so far, then check the independent project oracle.
    /// No file identities, engine fingerprints, or result subsets participate.
    pub async fn check_fresh_equivalence(&self) {
        self.compare_fresh_observations().await;
        self.check_all_semantics().await;
    }

    async fn compare_fresh_observations(&self) {
        let mut fresh = FakeEditor::new().await;
        for (file, content) in &self.indexed_contents {
            fresh.open(file, content).await;
        }
        for file in self.indexed_contents.keys() {
            if !self.open_files.contains(file) {
                fresh.close(file).await;
            }
        }

        // Observe both engines without rebuilding facts. Diagnostic results
        // come from the actual publication path, including stale output.
        let incremental = self.public_observations(&self.editor).await;
        let rebuilt = self.public_observations(&fresh).await;
        for (query, actual) in &incremental {
            assert_eq!(
                Some(actual),
                rebuilt.get(query),
                "fresh-equivalence mismatch in project `{}` at {query}",
                self.project.name
            );
        }
        assert_eq!(
            incremental.len(),
            rebuilt.len(),
            "INVARIANT VIOLATED: fresh-equivalence queries differ in project `{}`. This is a bug because both editors must receive the exact same query sites. Fix: inspect observation generation.",
            self.project.name
        );
        for file in &self.open_files {
            assert_eq!(
                sorted_lsp_values(self.editor.diagnostics(file).await),
                sorted_lsp_values(fresh.diagnostics(file).await),
                "fresh-equivalence diagnostics mismatch in project `{}` at {file}",
                self.project.name
            );
        }
    }

    async fn public_observations(&self, editor: &FakeEditor) -> BTreeMap<String, Vec<String>> {
        let mut observations = BTreeMap::new();
        for file in &self.open_files {
            observations.insert(
                format!("inlay hints {file}"),
                sorted_lsp_values(editor.inlay_hints(file).await),
            );
        }

        // Support belongs to each requested feature, not to the returned
        // locations. Unexpected targets and duplicate results remain visible.
        let mut sites = BTreeMap::<(String, u32, u32), [bool; 3]>::new();
        let mut add_site = |pos: &SourcePos, support: [bool; 3]| {
            if self.open_files.contains(&pos.file) {
                let enabled = sites
                    .entry((pos.file.clone(), pos.line, pos.character))
                    .or_default();
                for (enabled, supported) in enabled.iter_mut().zip(support) {
                    *enabled |= supported;
                }
            }
        };
        for call in &self.render.map.calls {
            add_site(
                &call.pos,
                [
                    call.definition_support.is_supported(),
                    call.reference_support.is_supported(),
                    call.hover_support.is_supported(),
                ],
            );
        }
        for constant_ref in &self.render.map.constant_refs {
            add_site(&constant_ref.pos, [true; 3]);
        }
        for namespace_ref in self.namespace_refs() {
            add_site(
                &namespace_ref.pos,
                [namespace_ref.support.is_supported(); 3],
            );
        }
        for pos in self
            .render
            .map
            .defs
            .values()
            .chain(self.render.map.constants.values())
            .chain(self.render.map.namespaces.values().map(|site| &site.pos))
        {
            add_site(pos, [true; 3]);
        }

        for ((file, line, character), [definition, references, hover]) in sites {
            if definition {
                observations.insert(
                    format!("definition {file}:{line}:{character}"),
                    definition_observation(
                        editor.goto_def_response_at(&file, line, character).await,
                    ),
                );
            }
            if references {
                observations.insert(
                    format!("references {file}:{line}:{character}"),
                    sorted_lsp_values(editor.references_at(&file, line, character).await),
                );
            }
            if hover {
                observations.insert(
                    format!("hover {file}:{line}:{character}"),
                    sorted_lsp_values(vec![editor.hover_at(&file, line, character).await]),
                );
            }
        }
        observations
    }

    pub async fn assert_semantic_false_positive_budget(&self, maximum: usize) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        let reviewed_sites = self
            .render
            .map
            .calls
            .iter()
            .filter(|call| {
                self.open_files.contains(&call.pos.file)
                    && !oracle.resolve_call_targets(call).is_empty()
            })
            .count()
            + self
                .render
                .map
                .constant_refs
                .iter()
                .filter(|constant_ref| {
                    self.open_files.contains(&constant_ref.pos.file)
                        && oracle.resolve_constant_ref(constant_ref).is_some()
                })
                .count()
            + self
                .render
                .map
                .direct_macro_calls
                .iter()
                .filter(|macro_call| self.open_files.contains(&macro_call.pos.file))
                .count();
        assert!(
            reviewed_sites >= 50,
            "INVARIANT VIOLATED: semantic diagnostic review corpus has only {reviewed_sites} oracle-owned valid sites. This is a bug because a zero false-positive budget needs material coverage. Fix: expand the generated project before claiming the budget."
        );
        let mut false_positives = Vec::new();
        for file in &self.open_files {
            for diagnostic in self.editor.diagnostics(file).await {
                if diagnostic.severity != Some(DiagnosticSeverity::ERROR)
                    && diagnostic.severity != Some(DiagnosticSeverity::WARNING)
                {
                    continue;
                }
                if diagnostic.code.is_none() {
                    // Prism syntax/style diagnostics are covered separately and
                    // are not engine semantic false positives.
                    continue;
                }
                if !self.diagnostic_targets_reviewed_valid_site(file, &diagnostic, &oracle) {
                    continue;
                }
                false_positives.push(format!(
                    "{}:{}:{} code={:?} text={:?} message={}",
                    file,
                    diagnostic.range.start.line,
                    diagnostic.range.start.character,
                    diagnostic.code,
                    diagnostic_text(self.editor.content(file), &diagnostic),
                    diagnostic.message
                ));
            }
        }
        false_positives.sort();
        assert!(
            false_positives.len() <= maximum,
            "INVARIANT VIOLATED: semantic diagnostic false-positive budget exceeded: {} > {}. This is a bug because the deterministic simulator oracle marks every intentional negative site. Fix: correct the diagnostic policy or explicitly model a genuinely invalid generated site.\n{}",
            false_positives.len(),
            maximum,
            false_positives.join("\n")
        );
    }

    fn diagnostic_targets_reviewed_valid_site(
        &self,
        file: &str,
        diagnostic: &Diagnostic,
        oracle: &OracleState<'_>,
    ) -> bool {
        self.render.map.calls.iter().any(|call| {
            call.pos.file == file
                && call.pos.line == diagnostic.range.start.line
                && call.pos.character == diagnostic.range.start.character
                && !oracle.resolve_call_targets(call).is_empty()
        }) || self.render.map.constant_refs.iter().any(|constant_ref| {
            constant_ref.pos.file == file
                && constant_ref.pos.line == diagnostic.range.start.line
                && constant_ref.pos.character == diagnostic.range.start.character
                && oracle.resolve_constant_ref(constant_ref).is_some()
        }) || self.render.map.direct_macro_calls.iter().any(|macro_call| {
            macro_call.pos.file == file
                && macro_call.pos.line == diagnostic.range.start.line
                && macro_call.pos.character == diagnostic.range.start.character
        })
    }

    pub async fn open_file(&mut self, file: &str) {
        assert!(
            !self.open_files.contains(file),
            "INVARIANT VIOLATED: simulation tried to open file `{}` twice. This is a bug because partial-open tests should have deterministic unique open order. Fix: remove the duplicate file.",
            file
        );
        let current_render = self.project.render();
        let content = current_render.files.get(file).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: simulation tried to open missing file `{}`. This is a bug because partial-open tests must reference generated files. Fix: update the open order.",
                file
            )
        });
        self.editor.open(file, content).await;
        self.indexed_contents
            .insert(file.to_string(), content.clone());
        self.open_files.insert(file.to_string());
        self.indexed_files.insert(file.to_string());
        self.record_delivered_model(file);
        self.render = self.observed_project.render();
    }

    fn record_delivered_model(&mut self, file: &str) {
        assert_eq!(self.observed_project.namespaces.len(), self.project.namespaces.len(),
            "INVARIANT VIOLATED: simulation namespace inventory changed during edits. This is a bug because edit operations toggle existing model nodes. Fix: model namespace additions before generating them.");
        for (observed, current) in self
            .observed_project
            .namespaces
            .iter_mut()
            .zip(&self.project.namespaces)
        {
            if super::ruby_gen::namespace_file(current) == file {
                *observed = current.clone();
            }
        }
        if let Some(content) = self.project.raw_files.get(file) {
            self.observed_project
                .raw_files
                .insert(file.to_owned(), content.clone());
        }
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
        self.apply_step_impl(step, false).await;
    }

    /// Compare the immediately edited state with fresh analysis before model checks.
    pub async fn apply_step_with_fresh_equivalence(&mut self, step: &EditStep) {
        self.apply_step_impl(step, true).await;
    }

    async fn apply_step_impl(&mut self, step: &EditStep, check_fresh_before_model: bool) {
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
                        self.indexed_contents
                            .insert(file.clone(), next_content.clone());
                        self.record_delivered_model(file);
                    }
                } else {
                    self.editor.open(file, next_content).await;
                    self.indexed_contents
                        .insert(file.clone(), next_content.clone());
                    self.open_files.insert(file.clone());
                    self.indexed_files.insert(file.clone());
                    self.record_delivered_model(file);
                }
            }
        }

        self.render = self.observed_project.render();
        if check_fresh_before_model {
            self.compare_fresh_observations().await;
        }
        self.assert_index_shape();
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );

        for expected in &step.expected {
            match expected {
                ExpectedCheck::UnresolvedMethod { file, method } => {
                    if !self.open_files.contains(file) {
                        continue;
                    }
                    let lookup_is_conclusive = self
                        .render
                        .map
                        .calls
                        .iter()
                        .filter(|call| call.pos.file == *file && call.target.name == *method)
                        .all(|call| oracle.method_lookup_is_conclusive(call));
                    if lookup_is_conclusive {
                        self.assert_unresolved_method(file, method).await;
                    } else {
                        self.assert_no_unresolved_method(file, method).await;
                    }
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
        self.close_and_reopen_impl(file, false).await;
    }

    async fn close_and_reopen_impl(&mut self, file: &str, check_fresh_before_model: bool) {
        let content = self.editor.content(file).to_string();
        self.editor.close(file).await;
        self.open_files.remove(file);
        self.editor.open(file, &content).await;
        self.indexed_contents.insert(file.to_string(), content);
        self.open_files.insert(file.to_string());
        self.indexed_files.insert(file.to_string());
        if check_fresh_before_model {
            self.compare_fresh_observations().await;
        }
        self.check_all_semantics().await;
    }

    pub async fn run_edit_script_step(&mut self, step: &super::seeded::SeededStep) {
        self.run_edit_script_step_impl(step, false).await;
    }

    /// Edits and open/close operations compare fresh observations and then
    /// validate the independent expected-result and model checks.
    pub async fn run_edit_script_step_with_fresh_equivalence(
        &mut self,
        step: &super::seeded::SeededStep,
    ) {
        self.run_edit_script_step_impl(step, true).await;
    }

    async fn run_edit_script_step_impl(
        &mut self,
        step: &super::seeded::SeededStep,
        check_fresh_before_model: bool,
    ) {
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
                self.apply_step_impl(&edit, check_fresh_before_model).await;
            }
            super::seeded::SeededStep::CloseReopen { file } => {
                self.close_and_reopen_impl(file, check_fresh_before_model)
                    .await;
            }
            super::seeded::SeededStep::OpenFile { file } => {
                self.open_file(file).await;
                if check_fresh_before_model {
                    self.compare_fresh_observations().await;
                }
                self.check_all_semantics().await;
            }
            super::seeded::SeededStep::CloseFile { file } => {
                self.close_file(file).await;
                if check_fresh_before_model {
                    self.compare_fresh_observations().await;
                }
                self.check_all_semantics().await;
            }
        }
    }

    fn assert_index_shape(&self) {
        let stats = self.editor.server().orphan_engine().read().stats();
        assert!(
            stats.files >= self.open_files.len(),
            "INVARIANT VIOLATED: simulation index has too few files. Expected at least {}, got {}. This is a bug because every indexed generated file was opened through FakeEditor. Fix: inspect didOpen indexing.",
            self.indexed_files.len(),
            stats.files
        );
        if self.indexed_files.len() == self.render.files.len() {
            assert!(
                stats.methods >= self.observed_project.enabled_method_count(),
                "INVARIANT VIOLATED: simulation index has too few methods. Expected at least {}, got {}. This is a bug because every enabled generated method should emit a method fact. Fix: inspect method fact collection.",
                self.observed_project.enabled_method_count(),
                stats.methods
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

impl EditStep {}
