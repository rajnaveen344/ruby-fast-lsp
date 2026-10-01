use super::protocol::{constant_name, hover_text, namespace_hover_label};
use super::SimulationRunner;
use crate::test::harness::{get_hint_label, get_hint_tooltip};
use crate::test::simulation::oracle::OracleState;
use crate::test::simulation::ruby_gen::{SourcePos, TypeAssertKind};
use std::collections::{BTreeSet, HashMap};

impl SimulationRunner {
    pub(super) async fn assert_namespace_hovers(&self) {
        for (target, def) in &self.render.map.namespaces {
            if !self.open_files.contains(&def.pos.file) {
                continue;
            }
            if !self.observed_project.namespace_enabled(target) {
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
            self.assert_hover_contains(
                &namespace_ref.pos,
                &namespace_hover_label(&namespace_ref.target, def.kind),
            )
            .await;
        }
    }

    pub(super) async fn assert_method_hovers(&self) {
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
            if self.observed_project.delegate_enabled(target) {
                continue;
            }
            if let Some(return_type) = self.observed_project.method_return_type(target) {
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
            let Some(target) = oracle.resolve_unique_call(call) else {
                continue;
            };
            let Some(return_type) = self.observed_project.method_return_type(&target) else {
                continue;
            };
            let def = self.def_pos(&target);
            if !self.indexed_files.contains(&def.file) {
                continue;
            }
            self.assert_hover_contains(&call.pos, return_type).await;
        }

        for call in self.invalid_private_method_calls(&oracle) {
            let Some(return_type) = self.observed_project.method_return_type(&call.target) else {
                continue;
            };
            self.assert_hover_not_contains(&call.pos, return_type).await;
        }
    }

    pub(super) async fn assert_constant_hovers(&self) {
        let oracle = OracleState::with_indexed_files(
            &self.observed_project,
            &self.render.map,
            self.indexed_files.clone(),
        );
        for (target, def) in &self.render.map.constants {
            if !self.open_files.contains(&def.file) {
                continue;
            }
            if self.observed_project.constant_enabled(target) {
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

    pub(super) async fn assert_type_hovers(&self) {
        for type_assert in &self.render.map.type_asserts {
            if !self.open_files.contains(&type_assert.pos.file) {
                continue;
            }
            if !self.observed_project.method_enabled(&type_assert.owner) {
                continue;
            }
            if type_assert.kind != TypeAssertKind::LocalAssignment {
                continue;
            }
            self.assert_hover_contains(&type_assert.pos, &type_assert.expected)
                .await;
        }
    }

    pub(super) async fn assert_type_hints(&self) {
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
            if !self.observed_project.method_enabled(&type_assert.owner) {
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
                        // Labels summarize shapes and long names. The tooltip
                        // must still carry the complete semantic type.
                        && get_hint_tooltip(hint).is_some_and(|text| text.contains(expected))
                        && get_hint_label(hint).encode_utf16().count() <= 40
                }),
                "Expected compact type hint with tooltip containing `{}` at {}:{}:{} for {:?}, got {:?}",
                expected,
                type_assert.pos.file,
                type_assert.pos.line,
                type_assert.pos.character,
                type_assert.kind,
                hints
                    .iter()
                    .map(|hint| {
                        format!(
                            "{}:{} {} (tooltip: {:?})",
                            hint.position.line,
                            hint.position.character,
                            get_hint_label(hint),
                            get_hint_tooltip(hint)
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
}
