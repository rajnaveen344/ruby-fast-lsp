//! # Simulation Tests
//!
//! Property-based tests using proptest.
//! These tests generate random sequences of LSP operations and verify invariants.

use super::*;
use proptest::prelude::*;
use proptest::test_runner::Config;
use tower_lsp::LanguageServer;

// =============================================================================
// Test Configuration
// =============================================================================

/// Get proptest config from environment or use defaults
fn get_config() -> Config {
    Config {
        // Default to 100 cases, can be overridden with PROPTEST_CASES
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100),
        // Allow more shrinking iterations for better minimal examples
        max_shrink_iters: 10000,
        ..Config::default()
    }
}

/// Generate deterministic Ruby content based on a seed
/// This allows proptest to shrink the seed while keeping content reproducible
fn generate_ruby_content(seed: u32) -> String {
    let class_names = ["Foo", "Bar", "Baz", "Qux", "Alpha", "Beta", "Gamma"];
    let method_names = ["run", "call", "execute", "process", "handle", "compute"];

    let class_idx = (seed as usize) % class_names.len();
    let method_idx = ((seed as usize) / 7) % method_names.len();
    let num_methods = (seed % 4) as usize;

    let class_name = class_names[class_idx];
    let mut code = format!("class {}\n", class_name);

    for i in 0..=num_methods {
        let m_idx = (method_idx + i) % method_names.len();
        code.push_str(&format!(
            "  def {}_{}\n    @value = {}\n  end\n\n",
            method_names[m_idx],
            i,
            seed + i as u32
        ));
    }

    code.push_str("end\n");

    // Sometimes add a module too
    if seed % 3 == 0 {
        code.push_str(&format!(
            "\nmodule {}Helper\n  def helper_method\n    nil\n  end\nend\n",
            class_name
        ));
    }

    // Sometimes add invalid syntax to test error recovery
    if seed % 7 == 0 {
        code.push_str("\n# Invalid syntax below\ndef incomplete(");
    }

    code
}

// =============================================================================
// Level 1: Basic Safety Tests
// =============================================================================

proptest! {
    #![proptest_config(get_config())]

    /// Property: Server should never crash regardless of operation sequence.
    ///
    /// This generates TRULY RANDOM sequences of ALL 15 LSP operations.
    /// Each iteration picks random operations based on current state.
    #[test]
    fn server_never_crashes(
        // Generate a sequence of random operation indices (0-14 for 15 operation types)
        // We use indices because we can't generate Transitions directly without state
        operation_indices in prop::collection::vec(0..15u8, 1..100),
        // Random seeds for content generation
        content_seeds in prop::collection::vec(0..1000u32, 1..10),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut harness = SimulationHarness::new().await;

            // Open initial file with some content
            let initial_content = generate_ruby_content(content_seeds.get(0).copied().unwrap_or(0));
            let transition = Transition::DidOpen {
                filename: "test.rb".to_string(),
                content: initial_content,
            };
            let _ = harness.apply(&transition).await;

            // Apply random operations based on indices
            for (i, &op_idx) in operation_indices.iter().enumerate() {
                let files = harness.model.open_files();
                if files.is_empty() {
                    // Reopen a file if all closed
                    let content = generate_ruby_content(content_seeds.get(i % content_seeds.len()).copied().unwrap_or(0));
                    let _ = harness.apply(&Transition::DidOpen {
                        filename: format!("file_{}.rb", i),
                        content,
                    }).await;
                    continue;
                }

                let filename = files[0].clone();
                let content = harness.model.get_content(&filename).unwrap_or("").to_string();
                let line_count = content.lines().count().max(1);
                let rand_line = (op_idx as usize * 7 + i) % line_count;
                let rand_char = (op_idx as usize * 13 + i) % 50;

                let position = tower_lsp::lsp_types::Position {
                    line: rand_line as u32,
                    character: rand_char as u32,
                };

                let transition = match op_idx % 15 {
                    // Document Lifecycle (4 ops)
                    0 => Transition::DidOpen {
                        filename: format!("file_{}.rb", i),
                        content: generate_ruby_content(i as u32),
                    },
                    1 => {
                        // DidChange - insert at random position
                        let line_len = content.lines().nth(rand_line).map(|l| l.len()).unwrap_or(0);
                        let safe_char = rand_char.min(line_len);
                        Transition::DidChange {
                            filename: filename.clone(),
                            range: tower_lsp::lsp_types::Range {
                                start: tower_lsp::lsp_types::Position {
                                    line: rand_line as u32,
                                    character: safe_char as u32,
                                },
                                end: tower_lsp::lsp_types::Position {
                                    line: rand_line as u32,
                                    character: safe_char as u32,
                                },
                            },
                            new_text: format!(" # edit {} ", i),
                        }
                    },
                    2 => Transition::DidSave { filename: filename.clone() },
                    3 => Transition::DidClose { filename: filename.clone() },

                    // Navigation (2 ops)
                    4 => Transition::GotoDefinition { filename: filename.clone(), position },
                    5 => Transition::FindReferences {
                        filename: filename.clone(),
                        position,
                        include_declaration: i % 2 == 0,
                    },

                    // Intelligence (4 ops)
                    6 => Transition::Completion { filename: filename.clone(), position },
                    7 => Transition::Hover { filename: filename.clone(), position },
                    8 => Transition::InlayHints {
                        filename: filename.clone(),
                        range: tower_lsp::lsp_types::Range {
                            start: tower_lsp::lsp_types::Position { line: 0, character: 0 },
                            end: position,
                        },
                    },
                    9 => Transition::SemanticTokens { filename: filename.clone() },

                    // Structure (4 ops)
                    10 => Transition::DocumentSymbols { filename: filename.clone() },
                    11 => Transition::WorkspaceSymbols { query: format!("query{}", i % 10) },
                    12 => Transition::FoldingRange { filename: filename.clone() },
                    13 => Transition::CodeLens { filename: filename.clone() },

                    // Formatting (1 op)
                    _ => Transition::OnTypeFormatting {
                        filename: filename.clone(),
                        position,
                        character: if i % 2 == 0 { '\n' } else { 'd' },
                    },
                };

                // Apply and continue even on error (we're testing for panics)
                let _ = harness.apply(&transition).await;
            }

            // If we got here without panic, the test passes
        });
    }

    /// Property: Text synchronization should always hold.
    ///
    /// After any sequence of DidOpen/DidChange/DidClose operations,
    /// the model's content should match the server's content.
    #[test]
    fn text_sync_maintained(
        initial_content in generators::ruby_content(),
        edits in prop::collection::vec(
            ("[a-z \\n]{0,20}", 0..10u32, 0..50u32),
            0..10
        ),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut harness = SimulationHarness::new().await;

            // Open file
            let transition = Transition::DidOpen {
                filename: "test.rb".to_string(),
                content: initial_content.clone(),
            };
            harness.apply(&transition).await.expect("DidOpen should succeed");

            // Apply edits
            for (new_text, line, char) in edits {
                let line_count = harness.model.line_count("test.rb");
                if line_count == 0 {
                    continue;
                }

                let line = (line as usize) % line_count;
                let line_len = harness.model.line_length("test.rb", line);
                let char = if line_len > 0 { (char as usize) % line_len } else { 0 };

                let range = tower_lsp::lsp_types::Range {
                    start: tower_lsp::lsp_types::Position {
                        line: line as u32,
                        character: char as u32,
                    },
                    end: tower_lsp::lsp_types::Position {
                        line: line as u32,
                        character: char as u32,
                    },
                };

                let transition = Transition::DidChange {
                    filename: "test.rb".to_string(),
                    range,
                    new_text,
                };

                // This will check text sync invariant
                if let Err(e) = harness.apply(&transition).await {
                    panic!(
                        "Text sync failed after edit!\nError: {}\nLog: {:?}",
                        e,
                        harness.get_log()
                    );
                }
            }
        });
    }
}

// =============================================================================
// Level 2: Semantic Tests (Marker Strategy)
// =============================================================================

proptest! {
    #![proptest_config(get_config())]

    /// Property: Document symbols should find all classes we generate.
    ///
    /// We generate Ruby code with known class names, then verify
    /// document symbols returns all of them.
    #[test]
    fn document_symbols_finds_classes(
        class_names in prop::collection::vec(generators::ruby_class_name(), 1..5)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut harness = SimulationHarness::new().await;

            // Generate code with known classes
            let code = class_names
                .iter()
                .map(|name| format!("class {}\nend", name))
                .collect::<Vec<_>>()
                .join("\n\n");

            // Open file
            let transition = Transition::DidOpen {
                filename: "test.rb".to_string(),
                content: code,
            };
            harness.apply(&transition).await.expect("DidOpen should succeed");

            // Get document symbols
            let uri = tower_lsp::lsp_types::Url::from_file_path(
                harness.file_paths.get("test.rb").unwrap()
            ).unwrap();

            let result = harness.server.document_symbol(
                tower_lsp::lsp_types::DocumentSymbolParams {
                    text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                }
            ).await;

            // Extract symbol names
            let found_names: std::collections::HashSet<String> = match result {
                Ok(Some(tower_lsp::lsp_types::DocumentSymbolResponse::Nested(symbols))) => {
                    symbols.iter().map(|s| s.name.clone()).collect()
                }
                Ok(Some(tower_lsp::lsp_types::DocumentSymbolResponse::Flat(symbols))) => {
                    symbols.iter().map(|s| s.name.clone()).collect()
                }
                Ok(None) | Err(_) => std::collections::HashSet::new(),
            };

            // Verify all generated classes are found
            for name in &class_names {
                assert!(
                    found_names.contains(name),
                    "Document symbols should find class '{}'. Found: {:?}",
                    name,
                    found_names
                );
            }
        });
    }

    /// Property: Semantic tokens should be deterministic.
    ///
    /// Calling semantic_tokens_full twice on the same content
    /// should return identical results.
    #[test]
    fn semantic_tokens_deterministic(content in generators::ruby_content()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut harness = SimulationHarness::new().await;

            // Open file
            let transition = Transition::DidOpen {
                filename: "test.rb".to_string(),
                content,
            };
            harness.apply(&transition).await.expect("DidOpen should succeed");

            let uri = tower_lsp::lsp_types::Url::from_file_path(
                harness.file_paths.get("test.rb").unwrap()
            ).unwrap();

            let params = tower_lsp::lsp_types::SemanticTokensParams {
                text_document: tower_lsp::lsp_types::TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            // Call twice
            let result1 = harness.server.semantic_tokens_full(params.clone()).await;
            let result2 = harness.server.semantic_tokens_full(params).await;

            // Compare
            assert_eq!(
                format!("{:?}", result1),
                format!("{:?}", result2),
                "Semantic tokens should be deterministic"
            );
        });
    }
}

// =============================================================================
// Standalone Tests (Not proptest)
// =============================================================================

#[cfg(test)]
mod standalone_tests {
    use super::*;

    /// Smoke test: Basic harness functionality
    #[tokio::test]
    async fn harness_smoke_test() {
        let mut harness = SimulationHarness::new().await;

        // Open a file
        let transition = Transition::DidOpen {
            filename: "test.rb".to_string(),
            content: "class Foo\nend".to_string(),
        };
        harness
            .apply(&transition)
            .await
            .expect("DidOpen should work");

        // Verify model state
        assert!(harness.model.is_open("test.rb"));
        assert_eq!(harness.model.get_content("test.rb"), Some("class Foo\nend"));

        // Close the file
        let transition = Transition::DidClose {
            filename: "test.rb".to_string(),
        };
        harness
            .apply(&transition)
            .await
            .expect("DidClose should work");

        // Verify model state
        assert!(!harness.model.is_open("test.rb"));
    }

    /// Test: Edit operations maintain text sync
    #[tokio::test]
    async fn edit_maintains_sync() {
        let mut harness = SimulationHarness::new().await;

        // Open a file
        let transition = Transition::DidOpen {
            filename: "test.rb".to_string(),
            content: "class Foo\nend".to_string(),
        };
        harness
            .apply(&transition)
            .await
            .expect("DidOpen should work");

        // Edit: Insert "Bar" after "Foo"
        let transition = Transition::DidChange {
            filename: "test.rb".to_string(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 9,
                },
                end: tower_lsp::lsp_types::Position {
                    line: 0,
                    character: 9,
                },
            },
            new_text: "Bar".to_string(),
        };
        harness
            .apply(&transition)
            .await
            .expect("DidChange should work");

        // Verify model state
        assert_eq!(
            harness.model.get_content("test.rb"),
            Some("class FooBar\nend")
        );
    }

    /// Test: Query operations don't crash
    #[tokio::test]
    async fn queries_dont_crash() {
        let mut harness = SimulationHarness::new().await;

        // Open a file with some content
        let transition = Transition::DidOpen {
            filename: "test.rb".to_string(),
            content: "class Foo\n  def bar\n    @x = 1\n  end\nend".to_string(),
        };
        harness
            .apply(&transition)
            .await
            .expect("DidOpen should work");

        // Run various queries - none should crash
        let queries = vec![
            Transition::DocumentSymbols {
                filename: "test.rb".to_string(),
            },
            Transition::SemanticTokens {
                filename: "test.rb".to_string(),
            },
            Transition::FoldingRange {
                filename: "test.rb".to_string(),
            },
            Transition::CodeLens {
                filename: "test.rb".to_string(),
            },
            Transition::GotoDefinition {
                filename: "test.rb".to_string(),
                position: tower_lsp::lsp_types::Position {
                    line: 2,
                    character: 4,
                },
            },
            Transition::Completion {
                filename: "test.rb".to_string(),
                position: tower_lsp::lsp_types::Position {
                    line: 2,
                    character: 4,
                },
            },
        ];

        for query in queries {
            harness
                .apply(&query)
                .await
                .expect(&format!("Query should not crash: {:?}", query));
        }
    }
}
