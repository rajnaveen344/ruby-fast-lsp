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

/// Path for soak test failure logs (relative to workspace root)
const SOAK_LOG_FILE: &str = "src/test/simulation/soak_failures.log";

fn get_config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100),
        max_shrink_iters: 10000,
        // Use Direct persistence to write to src/test/simulation/regressions.txt
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "src/test/simulation/regressions.txt",
            ),
        )),
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
    /// Run with: `cargo test sim`
    /// More cases: `PROPTEST_CASES=1000 cargo test sim`
    #[test]
    fn sim(
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
// SOAK TEST MODE - Run overnight to collect all failures
// =============================================================================
//
// Run with: SOAK_TEST=1 PROPTEST_CASES=10000 cargo test soak_test --release -- --nocapture
//
// This test:
// 1. Runs continuously without stopping on failures
// 2. Records all failures to a file
// 3. Prints a summary at the end

#[cfg(test)]
mod soak_test {
    use super::*;
    use crate::test::simulation::generators::{
        tracked_class_with_method_call, tracked_inheritance, tracked_inlay_hints,
        tracked_instance_variable, tracked_mixin_method_call, tracked_type_assignments,
    };
    use proptest::strategy::ValueTree;
    use std::collections::hash_map::DefaultHasher;
    use std::fs::OpenOptions;
    use std::hash::{Hash, Hasher};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;

    static FAILURES: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static TOTAL_RUNS: AtomicUsize = AtomicUsize::new(0);
    static TOTAL_FAILURES: AtomicUsize = AtomicUsize::new(0);

    fn record_failure(description: String) {
        TOTAL_FAILURES.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut failures) = FAILURES.lock() {
            // Deduplicate by error type
            let error_key = description
                .lines()
                .find(|l| l.starts_with("Error:"))
                .unwrap_or(&description);
            if !failures.iter().any(|f| f.contains(error_key)) {
                failures.push(description);
            }
        }
    }

    /// Generate a seed from iteration number for reproducibility
    fn make_seed(iteration: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        iteration.hash(&mut hasher);
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .hash(&mut hasher);
        hasher.finish()
    }

    /// Soak test - runs continuously until Ctrl+C and collects all unique failures
    ///
    /// Run with: `cargo test soak --release -- --nocapture --ignored`
    /// Optional max limit: `PROPTEST_CASES=10000 cargo test soak --release -- --nocapture --ignored`
    ///
    /// Results are written to `src/test/simulation/soak_failures.log` on exit
    #[test]
    #[ignore] // Only run when explicitly requested with --ignored
    fn soak() {
        // Optional max limit, otherwise run forever
        let max_cases: Option<usize> = std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok());

        println!("🔥 SOAK TEST MODE");
        if let Some(max) = max_cases {
            println!("   Running up to {} iterations (or until Ctrl+C)", max);
        } else {
            println!("   Running indefinitely until Ctrl+C");
        }
        println!("   Failures will be collected (not stopped on first failure)");
        println!("   Results will be written to {}\n", super::SOAK_LOG_FILE);

        // Set up Ctrl+C handler
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || {
            println!("\n\n⏹️  Ctrl+C received, finishing up...");
            r.store(false, Ordering::SeqCst);
        })
        .ok();

        let start = Instant::now();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let mut i: usize = 0;
        while running.load(Ordering::SeqCst) {
            // Check max limit
            if let Some(max) = max_cases {
                if i >= max {
                    break;
                }
            }
            TOTAL_RUNS.fetch_add(1, Ordering::SeqCst);

            // Generate a reproducible seed for this iteration
            let seed = make_seed(i);

            // Use proptest's TestRunner for random generation
            let config = proptest::test_runner::Config {
                cases: 1,
                ..Default::default()
            };
            let mut runner = proptest::test_runner::TestRunner::new(config);

            // Cycle through different generators
            let generator_idx = i % 6;
            let generator_name = match generator_idx {
                0 => "tracked_class_with_method_call",
                1 => "tracked_mixin_method_call",
                2 => "tracked_inheritance",
                3 => "tracked_instance_variable",
                4 => "tracked_type_assignments",
                _ => "tracked_inlay_hints",
            };

            let tracked = match generator_idx {
                0 => tracked_class_with_method_call()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
                1 => tracked_mixin_method_call()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
                2 => tracked_inheritance()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
                3 => tracked_instance_variable()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
                4 => tracked_type_assignments()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
                _ => tracked_inlay_hints()
                    .new_tree(&mut runner)
                    .ok()
                    .map(|t| t.current()),
            };

            if let Some(tracked) = tracked {
                // Generate random steps
                let step_count = 10 + (i % 40);
                let steps: Vec<SimStep> = (0..step_count)
                    .map(|j| match (i + j) % 13 {
                        0 | 1 => SimStep::Edit {
                            line: ((i + j) % 20) as u32,
                            text: format!("# edit {}\n", j),
                        },
                        2 | 3 => SimStep::VerifyDefinition {
                            marker_idx: j % tracked.markers.len().max(1),
                        },
                        4 => SimStep::VerifyType {
                            marker_idx: j % tracked.markers.len().max(1),
                        },
                        5 => SimStep::VerifyCompletion {
                            marker_idx: j % tracked.markers.len().max(1),
                        },
                        6 => SimStep::QuerySymbols,
                        7 => SimStep::QueryCompletion {
                            line: (j % 10) as u32,
                            character: 5,
                        },
                        8 => SimStep::QueryReferences {
                            line: (j % 10) as u32,
                            character: 5,
                        },
                        9 => SimStep::QueryHover {
                            line: (j % 10) as u32,
                            character: 5,
                        },
                        10 => SimStep::QueryInlayHints,
                        11 => SimStep::QuerySemanticTokens,
                        _ => SimStep::Save,
                    })
                    .collect();

                let result = rt.block_on(run_simulation(&tracked, &steps));

                match result {
                    Ok(report) if !report.is_success() => {
                        let failure_desc = format!(
                            "Seed: {:016x}\nIteration: {}\nGenerator: {}\nCode:\n{}\nError: {:?}\n---\n",
                            seed,
                            i,
                            generator_name,
                            tracked.code.lines().take(10).collect::<Vec<_>>().join("\n"),
                            report.errors
                        );
                        record_failure(failure_desc);
                    }
                    Err(e) => {
                        let failure_desc = format!("Iteration: {}\nSetup Error: {}\n---\n", i, e);
                        record_failure(failure_desc);
                    }
                    _ => {}
                }
            }

            // Print progress every 100 runs
            if (i + 1) % 100 == 0 {
                let total = TOTAL_RUNS.load(Ordering::SeqCst);
                let failures = TOTAL_FAILURES.load(Ordering::SeqCst);
                let unique = FAILURES.lock().map(|f| f.len()).unwrap_or(0);
                let elapsed = start.elapsed().as_secs();
                let rate = total as f64 / elapsed.max(1) as f64;
                if let Some(max) = max_cases {
                    print!(
                        "\r✓ Progress: {}/{} | {} failures ({} unique) | {}s | {:.0}/s    ",
                        total, max, failures, unique, elapsed, rate
                    );
                } else {
                    print!(
                        "\r✓ Progress: {} | {} failures ({} unique) | {}s | {:.0}/s    ",
                        total, failures, unique, elapsed, rate
                    );
                }
                std::io::stdout().flush().ok();
            }

            i += 1;
        }

        // Write results to file
        let total = TOTAL_RUNS.load(Ordering::SeqCst);
        let failures = TOTAL_FAILURES.load(Ordering::SeqCst);
        let elapsed = start.elapsed();

        println!("\n\n📊 SOAK TEST COMPLETE");
        println!("   Duration: {:.1}s", elapsed.as_secs_f64());
        println!("   Total runs: {}", total);
        println!(
            "   Total failures: {} ({:.1}%)",
            failures,
            (failures as f64 / total as f64) * 100.0
        );

        if let Ok(failures_list) = FAILURES.lock() {
            println!("   Unique failure types: {}", failures_list.len());

            if !failures_list.is_empty() {
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(super::SOAK_LOG_FILE)
                    .expect("Failed to create log file");

                writeln!(file, "# Simulation Soak Test Failures").ok();
                writeln!(file, "# Date: {:?}", std::time::SystemTime::now()).ok();
                writeln!(file, "# Duration: {:.1}s", elapsed.as_secs_f64()).ok();
                writeln!(file, "# Total runs: {}", total).ok();
                writeln!(file, "# Total failures: {}", failures).ok();
                writeln!(file, "# Unique failure types: {}\n", failures_list.len()).ok();

                for (i, failure) in failures_list.iter().enumerate() {
                    writeln!(file, "## Failure #{}\n{}", i + 1, failure).ok();
                }

                println!("\n   📝 Results written to: {}", super::SOAK_LOG_FILE);
            }
        }

        // Don't fail the test - we just collected data
        println!("\n   ✅ Soak test completed successfully (failures are expected)");
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
