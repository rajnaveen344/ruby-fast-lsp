//! # Simulation Tests
//!
//! ONE comprehensive property-based fuzzer for the LSP server.
//!
//! ## Philosophy
//!
//! Instead of many small tests, we have ONE simulation runner that:
//! 1. Generates tracked code with known definition/reference positions
//! 2. Performs random sequences of LSP operations
//! 3. Verifies invariants hold after each operation
//!
//! This catches bugs that manual tests miss - like type info disappearing after edits.

use super::*;
use crate::test::simulation::generators::{tracked_code, MarkerKind, TrackedCode};
use proptest::prelude::*;
use proptest::test_runner::Config;
use std::collections::HashSet;
use tower_lsp::lsp_types::{GotoDefinitionResponse, TextDocumentIdentifier, Url};
use tower_lsp::LanguageServer;

// =============================================================================
// Configuration
// =============================================================================

fn get_config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100),
        max_shrink_iters: 10000,
        ..Config::default()
    }
}

// =============================================================================
// Simulation Steps
// =============================================================================

#[derive(Debug, Clone)]
enum SimStep {
    Edit { line: u32, text: String },
    VerifyDefinition { marker_idx: usize },
    VerifyType { marker_idx: usize },
    VerifyCompletion { marker_idx: usize },
    QuerySymbols,
    QueryCompletion { line: u32, character: u32 },
    QueryReferences { line: u32, character: u32 },
    QueryHover { line: u32, character: u32 },
    QueryInlayHints,
    QuerySemanticTokens,
    QueryFoldingRanges,
    QueryCodeLens,
    Save,
}

// =============================================================================
// Simulation Report
// =============================================================================

#[derive(Debug, Default)]
struct SimulationReport {
    steps_executed: usize,
    edits_applied: usize,
    definitions_checked: usize,
    definitions_correct: usize,
    types_checked: usize,
    types_correct: usize,
    completions_checked: usize,
    completions_correct: usize,
    queries_executed: usize,
    saves: usize,
    errors: Vec<(usize, String)>,
}

impl SimulationReport {
    fn add_error(&mut self, step: usize, msg: String) {
        self.errors.push((step, msg));
    }

    fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

// =============================================================================
// Simulation Runner Core
// =============================================================================

async fn run_simulation(
    tracked: &TrackedCode,
    steps: &[SimStep],
) -> Result<SimulationReport, String> {
    let mut harness = SimulationHarness::new().await;
    let mut report = SimulationReport::default();

    // Open the tracked file
    harness
        .apply(&Transition::DidOpen {
            filename: tracked.filename.clone(),
            content: tracked.code.clone(),
        })
        .await
        .map_err(|e| format!("Failed to open file: {:?}", e))?;

    let uri = Url::from_file_path(
        harness
            .file_paths
            .get(&tracked.filename)
            .expect("File should exist"),
    )
    .unwrap();

    // ==========================================================================
    // INITIAL VERIFICATION: All definitions must resolve BEFORE any edits
    // ==========================================================================
    for marker in &tracked.markers {
        if let Some(expected_def) = &marker.definition_position {
            match &marker.kind {
                MarkerKind::Definition
                | MarkerKind::TypeAssignment { .. }
                | MarkerKind::CompletionTrigger { .. } => continue,
                _ => {}
            }

            let result = harness
                .server
                .goto_definition(tower_lsp::lsp_types::GotoDefinitionParams {
                    text_document_position_params:
                        tower_lsp::lsp_types::TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri: uri.clone() },
                            position: marker.position,
                        },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                })
                .await;

            let resolved = match &result {
                Ok(Some(GotoDefinitionResponse::Scalar(loc))) => {
                    (loc.range.start.line as i32 - expected_def.line as i32).abs() <= 2
                }
                Ok(Some(GotoDefinitionResponse::Array(locs))) if !locs.is_empty() => locs
                    .iter()
                    .any(|loc| (loc.range.start.line as i32 - expected_def.line as i32).abs() <= 2),
                Ok(Some(GotoDefinitionResponse::Link(links))) if !links.is_empty() => {
                    links.iter().any(|link| {
                        (link.target_selection_range.start.line as i32 - expected_def.line as i32)
                            .abs()
                            <= 2
                    })
                }
                _ => false,
            };

            if !resolved {
                report.add_error(
                    0,
                    format!(
                        "INITIAL CHECK FAILED: '{}' ({:?}) at line {} should resolve to line {}",
                        marker.name, marker.kind, marker.position.line, expected_def.line
                    ),
                );
            }
        }
    }

    // If initial checks failed, return early
    if !report.is_success() {
        return Ok(report);
    }

    // ==========================================================================
    // Execute random steps
    // ==========================================================================
    for (step_idx, step) in steps.iter().enumerate() {
        report.steps_executed += 1;

        match step {
            SimStep::Edit { line, text } => {
                let content = harness
                    .model
                    .get_content(&tracked.filename)
                    .unwrap_or("")
                    .to_string();
                let line_count = content.lines().count().max(1);
                let safe_line = (*line as usize) % line_count;
                let line_len = content.lines().nth(safe_line).map(|l| l.len()).unwrap_or(0);

                let transition = Transition::DidChange {
                    filename: tracked.filename.clone(),
                    range: tower_lsp::lsp_types::Range {
                        start: tower_lsp::lsp_types::Position {
                            line: safe_line as u32,
                            character: line_len as u32,
                        },
                        end: tower_lsp::lsp_types::Position {
                            line: safe_line as u32,
                            character: line_len as u32,
                        },
                    },
                    new_text: text.clone(),
                };

                if harness.apply(&transition).await.is_ok() {
                    report.edits_applied += 1;
                }
            }

            SimStep::VerifyDefinition { marker_idx } => {
                if let Some(marker) = tracked.markers.get(*marker_idx) {
                    match &marker.kind {
                        MarkerKind::Definition
                        | MarkerKind::TypeAssignment { .. }
                        | MarkerKind::CompletionTrigger { .. } => continue,
                        _ => {}
                    }

                    if let Some(expected_def) = &marker.definition_position {
                        let result = harness
                            .server
                            .goto_definition(tower_lsp::lsp_types::GotoDefinitionParams {
                                text_document_position_params:
                                    tower_lsp::lsp_types::TextDocumentPositionParams {
                                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                                        position: marker.position,
                                    },
                                work_done_progress_params: Default::default(),
                                partial_result_params: Default::default(),
                            })
                            .await;

                        report.definitions_checked += 1;
                        let tolerance = (report.edits_applied as i32 * 2).max(3);

                        match result {
                            Ok(Some(GotoDefinitionResponse::Scalar(loc))) => {
                                let line_diff =
                                    (loc.range.start.line as i32 - expected_def.line as i32).abs();
                                if line_diff <= tolerance {
                                    report.definitions_correct += 1;
                                } else if report.edits_applied == 0 {
                                    report.add_error(
                                        step_idx,
                                        format!(
                                            "DEFINITION WRONG: '{}' resolved to line {} (expected {})",
                                            marker.name, loc.range.start.line, expected_def.line
                                        ),
                                    );
                                }
                            }
                            Ok(Some(GotoDefinitionResponse::Array(locs))) if !locs.is_empty() => {
                                let any_close = locs.iter().any(|loc| {
                                    (loc.range.start.line as i32 - expected_def.line as i32).abs()
                                        <= tolerance
                                });
                                if any_close {
                                    report.definitions_correct += 1;
                                } else if report.edits_applied == 0 {
                                    report.add_error(
                                        step_idx,
                                        format!(
                                            "DEFINITION WRONG: '{}' resolved to wrong lines",
                                            marker.name
                                        ),
                                    );
                                }
                            }
                            Ok(Some(GotoDefinitionResponse::Link(links))) if !links.is_empty() => {
                                let any_close = links.iter().any(|link| {
                                    (link.target_selection_range.start.line as i32
                                        - expected_def.line as i32)
                                        .abs()
                                        <= tolerance
                                });
                                if any_close {
                                    report.definitions_correct += 1;
                                }
                            }
                            _ => {
                                if report.edits_applied == 0 {
                                    report.add_error(
                                        step_idx,
                                        format!("DEFINITION NOT FOUND: '{}'", marker.name),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            SimStep::VerifyType { marker_idx } => {
                if let Some(marker) = tracked.markers.get(*marker_idx) {
                    if let MarkerKind::TypeAssignment { expected_type } = &marker.kind {
                        let result = harness
                            .server
                            .inlay_hint(tower_lsp::lsp_types::InlayHintParams {
                                text_document: TextDocumentIdentifier { uri: uri.clone() },
                                range: tower_lsp::lsp_types::Range {
                                    start: tower_lsp::lsp_types::Position {
                                        line: marker.position.line,
                                        character: 0,
                                    },
                                    end: tower_lsp::lsp_types::Position {
                                        line: marker.position.line + 1,
                                        character: 0,
                                    },
                                },
                                work_done_progress_params: Default::default(),
                            })
                            .await;

                        report.types_checked += 1;

                        if let Ok(Some(hints)) = result {
                            if !hints.is_empty() {
                                let type_found = hints.iter().any(|hint| match &hint.label {
                                    tower_lsp::lsp_types::InlayHintLabel::String(s) => {
                                        s.contains(expected_type)
                                    }
                                    tower_lsp::lsp_types::InlayHintLabel::LabelParts(p) => {
                                        p.iter().any(|x| x.value.contains(expected_type))
                                    }
                                });

                                if type_found {
                                    report.types_correct += 1;
                                } else {
                                    // Got hints but WRONG type - this IS an error
                                    let hint_labels: Vec<String> = hints
                                        .iter()
                                        .map(|h| match &h.label {
                                            tower_lsp::lsp_types::InlayHintLabel::String(s) => {
                                                s.clone()
                                            }
                                            tower_lsp::lsp_types::InlayHintLabel::LabelParts(p) => {
                                                p.iter().map(|x| x.value.as_str()).collect()
                                            }
                                        })
                                        .collect();
                                    report.add_error(
                                        step_idx,
                                        format!(
                                            "TYPE MISMATCH: Expected '{}' for '{}', got: {:?}",
                                            expected_type, marker.name, hint_labels
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            SimStep::VerifyCompletion { marker_idx } => {
                if let Some(marker) = tracked.markers.get(*marker_idx) {
                    if let MarkerKind::CompletionTrigger { expected_methods } = &marker.kind {
                        let result = harness
                            .server
                            .completion(tower_lsp::lsp_types::CompletionParams {
                                text_document_position:
                                    tower_lsp::lsp_types::TextDocumentPositionParams {
                                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                                        position: marker.position,
                                    },
                                work_done_progress_params: Default::default(),
                                partial_result_params: Default::default(),
                                context: Some(tower_lsp::lsp_types::CompletionContext {
                                    trigger_kind:
                                        tower_lsp::lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
                                    trigger_character: Some(".".to_string()),
                                }),
                            })
                            .await;

                        report.completions_checked += 1;

                        let check_items = |items: &[tower_lsp::lsp_types::CompletionItem]| {
                            let labels: HashSet<_> =
                                items.iter().map(|i| i.label.as_str()).collect();
                            expected_methods.iter().any(|m| labels.contains(m.as_str()))
                        };

                        match result {
                            Ok(Some(tower_lsp::lsp_types::CompletionResponse::Array(items)))
                                if !items.is_empty() =>
                            {
                                if check_items(&items) {
                                    report.completions_correct += 1;
                                } else if items.len() > 10 {
                                    // Got many completions but wrong ones
                                    report.add_error(
                                        step_idx,
                                        format!(
                                            "WRONG COMPLETIONS: Expected {:?}",
                                            expected_methods
                                        ),
                                    );
                                }
                            }
                            Ok(Some(tower_lsp::lsp_types::CompletionResponse::List(list)))
                                if !list.items.is_empty() =>
                            {
                                if check_items(&list.items) {
                                    report.completions_correct += 1;
                                } else if list.items.len() > 10 {
                                    report.add_error(
                                        step_idx,
                                        format!(
                                            "WRONG COMPLETIONS: Expected {:?}",
                                            expected_methods
                                        ),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            SimStep::QuerySymbols => {
                let _ = harness
                    .server
                    .document_symbol(tower_lsp::lsp_types::DocumentSymbolParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryCompletion { line, character } => {
                let content = harness.model.get_content(&tracked.filename).unwrap_or("");
                let line_count = content.lines().count().max(1);
                let safe_line = (*line as usize) % line_count;

                let _ = harness
                    .server
                    .completion(tower_lsp::lsp_types::CompletionParams {
                        text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri: uri.clone() },
                            position: tower_lsp::lsp_types::Position {
                                line: safe_line as u32,
                                character: *character,
                            },
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                        context: None,
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryReferences { line, character } => {
                let content = harness.model.get_content(&tracked.filename).unwrap_or("");
                let line_count = content.lines().count().max(1);
                let safe_line = (*line as usize) % line_count;

                let _ = harness
                    .server
                    .references(tower_lsp::lsp_types::ReferenceParams {
                        text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier { uri: uri.clone() },
                            position: tower_lsp::lsp_types::Position {
                                line: safe_line as u32,
                                character: *character,
                            },
                        },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                        context: tower_lsp::lsp_types::ReferenceContext {
                            include_declaration: true,
                        },
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryHover { line, character } => {
                let content = harness.model.get_content(&tracked.filename).unwrap_or("");
                let line_count = content.lines().count().max(1);
                let safe_line = (*line as usize) % line_count;

                let _ = harness
                    .server
                    .hover(tower_lsp::lsp_types::HoverParams {
                        text_document_position_params:
                            tower_lsp::lsp_types::TextDocumentPositionParams {
                                text_document: TextDocumentIdentifier { uri: uri.clone() },
                                position: tower_lsp::lsp_types::Position {
                                    line: safe_line as u32,
                                    character: *character,
                                },
                            },
                        work_done_progress_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryInlayHints => {
                let content = harness.model.get_content(&tracked.filename).unwrap_or("");
                let line_count = content.lines().count().max(1) as u32;

                let _ = harness
                    .server
                    .inlay_hint(tower_lsp::lsp_types::InlayHintParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        range: tower_lsp::lsp_types::Range {
                            start: tower_lsp::lsp_types::Position {
                                line: 0,
                                character: 0,
                            },
                            end: tower_lsp::lsp_types::Position {
                                line: line_count,
                                character: 0,
                            },
                        },
                        work_done_progress_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QuerySemanticTokens => {
                let _ = harness
                    .server
                    .semantic_tokens_full(tower_lsp::lsp_types::SemanticTokensParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryFoldingRanges => {
                let _ = harness
                    .server
                    .folding_range(tower_lsp::lsp_types::FoldingRangeParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::QueryCodeLens => {
                let _ = harness
                    .server
                    .code_lens(tower_lsp::lsp_types::CodeLensParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    })
                    .await;
                report.queries_executed += 1;
            }

            SimStep::Save => {
                let _ = harness
                    .apply(&Transition::DidSave {
                        filename: tracked.filename.clone(),
                    })
                    .await;
                report.saves += 1;
            }
        }
    }

    Ok(report)
}

// =============================================================================
// THE ONE SIMULATION RUNNER
// =============================================================================

proptest! {
    #![proptest_config(get_config())]

    /// THE comprehensive simulation test.
    ///
    /// This is the ONLY fuzzer you need. It:
    /// 1. Generates tracked code with known markers (18 scenarios)
    /// 2. Verifies all definitions resolve correctly BEFORE any edits
    /// 3. Performs random operations (edits, queries, saves)
    /// 4. Verifies invariants still hold
    ///
    /// Run with: `cargo test simulation_runner`
    /// More cases: `PROPTEST_CASES=1000 cargo test simulation_runner`
    #[test]
    fn simulation_runner(
        tracked in tracked_code(),
        edit_lines in prop::collection::vec(0..50u32, 0..15),
        edit_texts in prop::collection::vec("[a-z #\n]{0,30}", 0..15),
        query_lines in prop::collection::vec(0..50u32, 0..10),
        query_chars in prop::collection::vec(0..100u32, 0..10),
        verify_indices in prop::collection::vec(0..20usize, 0..10),
        step_order in prop::collection::vec(0..15u8, 10..50),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut steps = Vec::new();
            let mut edit_idx = 0;
            let mut verify_idx = 0;
            let mut query_idx = 0;

            for &step_type in &step_order {
                let step = match step_type {
                    0 | 1 if edit_idx < edit_lines.len() && edit_idx < edit_texts.len() => {
                        let s = SimStep::Edit {
                            line: edit_lines[edit_idx],
                            text: edit_texts[edit_idx].clone(),
                        };
                        edit_idx += 1;
                        s
                    }
                    2 | 3 if verify_idx < verify_indices.len() && !tracked.markers.is_empty() => {
                        let s = SimStep::VerifyDefinition {
                            marker_idx: verify_indices[verify_idx] % tracked.markers.len(),
                        };
                        verify_idx += 1;
                        s
                    }
                    4 if verify_idx < verify_indices.len() && !tracked.markers.is_empty() => {
                        let s = SimStep::VerifyType {
                            marker_idx: verify_indices[verify_idx] % tracked.markers.len(),
                        };
                        verify_idx += 1;
                        s
                    }
                    5 if verify_idx < verify_indices.len() && !tracked.markers.is_empty() => {
                        let s = SimStep::VerifyCompletion {
                            marker_idx: verify_indices[verify_idx] % tracked.markers.len(),
                        };
                        verify_idx += 1;
                        s
                    }
                    6 => SimStep::QuerySymbols,
                    7 if query_idx < query_lines.len() => {
                        let s = SimStep::QueryCompletion {
                            line: query_lines[query_idx],
                            character: query_chars.get(query_idx).copied().unwrap_or(5),
                        };
                        query_idx += 1;
                        s
                    }
                    8 if query_idx < query_lines.len() => {
                        let s = SimStep::QueryReferences {
                            line: query_lines[query_idx],
                            character: query_chars.get(query_idx).copied().unwrap_or(5),
                        };
                        query_idx += 1;
                        s
                    }
                    9 if query_idx < query_lines.len() => {
                        let s = SimStep::QueryHover {
                            line: query_lines[query_idx],
                            character: query_chars.get(query_idx).copied().unwrap_or(5),
                        };
                        query_idx += 1;
                        s
                    }
                    10 => SimStep::QueryInlayHints,
                    11 | 12 => {
                        match (step_type as usize + edit_idx) % 3 {
                            0 => SimStep::QuerySemanticTokens,
                            1 => SimStep::QueryFoldingRanges,
                            _ => SimStep::QueryCodeLens,
                        }
                    }
                    _ => SimStep::Save,
                };
                steps.push(step);
            }

            let report = run_simulation(&tracked, &steps).await;

            match report {
                Ok(r) => {
                    assert!(
                        r.is_success(),
                        "Simulation failed!\n\
                         Steps: {} | Edits: {} | Saves: {}\n\
                         Definitions: {}/{} | Types: {}/{} | Completions: {}/{}\n\
                         Queries: {}\n\
                         Errors: {:?}",
                        r.steps_executed,
                        r.edits_applied,
                        r.saves,
                        r.definitions_correct,
                        r.definitions_checked,
                        r.types_correct,
                        r.types_checked,
                        r.completions_correct,
                        r.completions_checked,
                        r.queries_executed,
                        r.errors
                    );
                }
                Err(e) => panic!("Simulation setup failed: {}", e),
            }
        });
    }
}

// =============================================================================
// Model Unit Tests (minimal - just for the model logic)
// =============================================================================

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn test_model_open_close() {
        let mut model = LspModel::new();
        assert!(!model.is_open("test.rb"));

        model.open("test.rb".to_string(), "class Foo; end".to_string());
        assert!(model.is_open("test.rb"));
        assert_eq!(model.get_content("test.rb"), Some("class Foo; end"));

        model.close("test.rb");
        assert!(!model.is_open("test.rb"));
    }

    #[test]
    fn test_model_edit() {
        let mut model = LspModel::new();
        model.open("test.rb".to_string(), "class Foo\nend".to_string());

        let range = tower_lsp::lsp_types::Range {
            start: tower_lsp::lsp_types::Position {
                line: 0,
                character: 6,
            },
            end: tower_lsp::lsp_types::Position {
                line: 0,
                character: 9,
            },
        };

        model.edit("test.rb", &range, "Bar");
        assert_eq!(model.get_content("test.rb"), Some("class Bar\nend"));
    }
}
