//! Unified check function for test harness.
//!
//! This module provides a single `check()` function that automatically determines
//! what to verify based on the tags present in the fixture.
//!
//! # Supported Tags
//!
//! | Tag | Requires `$0` | Purpose |
//! |-----|---------------|---------|
//! | `<def>...</def>` | Yes | Expected goto definition range |
//! | `<ref>...</ref>` | Yes | Expected reference range |
//! | `<type>...</type>` | Yes | Expected type at cursor |
//! | `<hint label="...">` | No | Expected inlay hint |
//! | `<lens title="...">` | No | Expected code lens |
//! | `<err>...</err>` | No | Expected error diagnostic |
//! | `<warn>...</warn>` | No | Expected warning diagnostic |
//! | `<th supertypes="A,B" subtypes="C,D">` | Yes | Type hierarchy check at cursor |
//! | `<complete items="a,b" excludes="c">` | Yes | Expected completion items at cursor |
//!
//! # Examples
//!
//! ```ignore
//! // Goto definition
//! check(r#"<def>class Foo</def>; Foo$0.new"#).await;
//!
//! // Inlay hints
//! check(r#"x<hint label="String"> = "hello""#).await;
//!
//! // Diagnostics
//! check(r#"class <err>end</err>"#).await;
//!
//! // Combined: hints + diagnostics
//! check(r#"x<hint label="String"><warn>;</warn> = "hello""#).await;
//!
//! // Type hierarchy
//! check(r#"
//! class Animal; end
//! class <th supertypes="Animal">Dog$0</th> < Animal; end
//! "#).await;
//! ```

use tower_lsp::lsp_types::{
    CodeLensParams, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    GotoDefinitionParams, InlayHintParams, NumberOrString, PartialResultParams, Position, Range,
    ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams,
    TypeHierarchyPrepareParams, TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Url,
    WorkDoneProgressParams,
};

use super::fake_editor::FakeEditor;
use super::fixture::{extract_cursor, extract_tags, extract_tags_with_attributes, Tag};
use super::inlay_hints::get_hint_label;
use crate::capabilities::code_lens::handle_code_lens;
use crate::capabilities::diagnostics::generate_diagnostics;
use crate::capabilities::inlay_hints::handle_inlay_hints;
use crate::capabilities::type_hierarchy;
use crate::handlers::request;
use crate::query::generate_yard_diagnostics_inner;

/// All supported tag names for extraction.
pub(super) const ALL_TAGS: &[&str] = &[
    "def", "ref", "type", "hint", "lens", "err", "warn", "hover", "th", "rename", "complete",
    "impl", "incoming", "outgoing",
];

/// Strip all markers ($0 and tags) from fixture text, returning clean Ruby source.
pub(super) fn strip_all_markers(fixture: &str) -> String {
    let without_cursor = fixture.replace("$0", "");
    let (_, clean) = extract_tags_with_attributes(&without_cursor, ALL_TAGS);
    clean
}

/// Unified check function that runs checks based on tags present in the fixture.
///
/// See module documentation for supported tags and examples.
pub async fn check(fixture_text: &str) {
    let mut editor = FakeEditor::new().await;
    let clean = strip_all_markers(fixture_text);
    editor.open("inline_test.rb", &clean).await;
    editor.check("inline_test.rb", fixture_text).await;
}

/// Core assertion dispatch: parses tags from fixture and runs all matching checks.
///
/// This is the shared engine used by both `check()` and `FakeEditor::check()`.
/// The `server` and `uri` must already be set up with `content` indexed.
/// The `fixture_text` is the original text with markers ($0, <def>, etc.).
/// `extra_file_contents` provides additional file contents for cross-file type inference.
pub(super) async fn run_checks_on_fixture(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    content: &str,
    fixture_text: &str,
    extra_file_contents: Option<&[(Url, Vec<u8>)]>,
) {
    // Extract cursor if present
    let has_cursor = fixture_text.contains("$0");
    let (_cursor_in_tagged, text_without_cursor) = if has_cursor {
        let (pos, clean) = extract_cursor(fixture_text);
        (Some(pos), clean)
    } else {
        (None, fixture_text.to_string())
    };

    // Extract all tags in one pass
    let (all_tags, _content) = extract_tags_with_attributes(&text_without_cursor, ALL_TAGS);

    // Also extract simple tags for def/ref/impl/incoming/outgoing (they don't use attributes)
    let (def_ranges, _) = extract_tags(&text_without_cursor, "def");
    let (ref_ranges, _) = extract_tags(&text_without_cursor, "ref");
    let (impl_ranges, _) = extract_tags(&text_without_cursor, "impl");
    let (incoming_ranges, _) = extract_tags(&text_without_cursor, "incoming");
    let (outgoing_ranges, _) = extract_tags(&text_without_cursor, "outgoing");

    // Recompute cursor position in clean-text coordinates by stripping tags first,
    // then finding cursor. This avoids tag characters inflating the cursor offset.
    let cursor = if has_cursor {
        // Use a sentinel to track the cursor through tag removal
        let sentinel = "\x00CURSOR\x00";
        let text_with_sentinel = fixture_text.replace("$0", sentinel);
        let (_, clean_with_sentinel) = extract_tags_with_attributes(&text_with_sentinel, ALL_TAGS);
        let sentinel_byte_pos = clean_with_sentinel
            .find(sentinel)
            .expect("Sentinel should be present after tag removal");
        let before = &clean_with_sentinel[..sentinel_byte_pos];
        let line = before.matches('\n').count() as u32;
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let character = (sentinel_byte_pos - last_newline) as u32;
        Some(Position { line, character })
    } else {
        None
    };

    // Categorize tags by kind
    let hint_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hint").collect();
    let lens_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "lens").collect();
    let err_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "err").collect();
    let warn_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "warn").collect();
    let hover_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hover").collect();
    let th_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "th").collect();
    let type_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "type").collect();
    let rename_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "rename").collect();
    let complete_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "complete").collect();

    // Separate "none" tags (range-scoped negative assertions) from positive assertions
    let none_err_tags: Vec<&Tag> = err_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_warn_tags: Vec<&Tag> = warn_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_hint_tags: Vec<&Tag> = hint_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_lens_tags: Vec<&Tag> = lens_tags.iter().filter(|t| t.is_none()).copied().collect();

    // Positive assertions (expect something at this range)
    let positive_err_tags: Vec<&Tag> = err_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_warn_tags: Vec<&Tag> =
        warn_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_hint_tags: Vec<&Tag> =
        hint_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_lens_tags: Vec<&Tag> =
        lens_tags.iter().filter(|t| !t.is_none()).copied().collect();

    // Track which checks were run
    let mut checks_run = Vec::new();

    // Run goto definition check if we have cursor and def tags
    if cursor.is_some() && !def_ranges.is_empty() {
        run_goto_check(server, uri, cursor.unwrap(), &def_ranges).await;
        checks_run.push("goto");
    }

    // Run references check if we have cursor and ref tags
    if cursor.is_some() && !ref_ranges.is_empty() {
        run_references_check(server, uri, cursor.unwrap(), &ref_ranges).await;
        checks_run.push("references");
    }

    // Run implementation check if we have cursor and impl tags
    if cursor.is_some() && !impl_ranges.is_empty() {
        run_implementation_check(server, uri, cursor.unwrap(), &impl_ranges).await;
        checks_run.push("implementation");
    }

    // Run incoming calls check if we have cursor and incoming tags
    if cursor.is_some() && !incoming_ranges.is_empty() {
        run_incoming_calls_check(server, uri, cursor.unwrap(), &incoming_ranges).await;
        checks_run.push("incoming_calls");
    }

    // Run outgoing calls check if we have cursor and outgoing tags
    if cursor.is_some() && !outgoing_ranges.is_empty() {
        run_outgoing_calls_check(server, uri, cursor.unwrap(), &outgoing_ranges).await;
        checks_run.push("outgoing_calls");
    }

    // Run type check if we have type tags (no cursor needed - uses tag position)
    if !type_tags.is_empty() {
        let default_contents = vec![(uri.clone(), content.as_bytes().to_vec())];
        let file_contents = extra_file_contents.unwrap_or(&default_contents);
        run_type_check(server, uri, content, &type_tags, file_contents).await;
        checks_run.push("type");
    }

    // Run inlay hints check if we have hint tags (positive or negative)
    if !hint_tags.is_empty() {
        run_inlay_hints_check(server, uri, &positive_hint_tags, &none_hint_tags).await;
        checks_run.push("hints");
    }

    // Run diagnostics check if we have err or warn tags (positive or negative)
    if !err_tags.is_empty() || !warn_tags.is_empty() {
        run_diagnostics_check(
            server,
            uri,
            content,
            &positive_err_tags,
            &positive_warn_tags,
            &none_err_tags,
            &none_warn_tags,
        )
        .await;
        checks_run.push("diagnostics");
    }

    // Run code lens check if we have lens tags (positive or negative)
    if !lens_tags.is_empty() {
        run_code_lens_check(server, uri, &positive_lens_tags, &none_lens_tags).await;
        checks_run.push("lens");
    }

    // Run hover check if we have hover tags
    if !hover_tags.is_empty() {
        run_hover_check(server, uri, &hover_tags).await;
        checks_run.push("hover");
    }

    // Run type hierarchy check if we have th tags
    if cursor.is_some() && !th_tags.is_empty() {
        run_type_hierarchy_check(server, uri, cursor.unwrap(), &th_tags).await;
        checks_run.push("type_hierarchy");
    }

    // Run rename check if we have rename tags
    if !rename_tags.is_empty() {
        run_rename_check(server, uri, content, &rename_tags).await;
        checks_run.push("rename");
    }

    // Run completion check if we have complete tags (requires cursor)
    if cursor.is_some() && !complete_tags.is_empty() {
        run_completion_check(server, uri, cursor.unwrap(), &complete_tags).await;
        checks_run.push("completion");
    }

    // If no checks were run, that's okay - it means the fixture is valid with no expectations
    // (e.g., testing that valid code has no errors)
}

/// Multi-file check function for testing cross-file scenarios.
///
/// The first file in the list is the "primary" file where markers are extracted and checks run.
/// Additional files provide context (e.g., class definitions, method implementations).
///
/// # Arguments
/// * `files` - List of (filename, content) tuples. First file is primary.
///
/// # Example
/// ```ignore
/// check_multi_file(&[
///     ("main.rb", r#"
///         result<hover label="String"> = Helper.get_name
///     "#),
///     ("helper.rb", r#"
///         class Helper
///           # @return [String]
///           def self.get_name
///             "hello"
///           end
///         end
///     "#),
/// ]).await;
/// ```
pub async fn check_multi_file(files: &[(&str, &str)]) {
    assert!(
        !files.is_empty(),
        "check_multi_file requires at least one file"
    );

    let mut editor = FakeEditor::new().await;

    // Strip markers from every file before opening — tags in non-primary files
    // would otherwise be treated as Ruby code.
    let cleaned: Vec<(String, String)> = files
        .iter()
        .map(|(name, fixture)| (name.to_string(), strip_all_markers(fixture)))
        .collect();

    for (name, clean) in &cleaned {
        editor.open(name, clean).await;
    }

    // Build file contents map for cross-file type inference (uses cleaned content)
    let file_contents_owned: Vec<(Url, Vec<u8>)> = cleaned
        .iter()
        .map(|(name, clean)| {
            let uri = FakeEditor::filename_to_uri(name);
            (uri, clean.as_bytes().to_vec())
        })
        .collect();

    // Run assertions on every file that has tags. Primary file (index 0) always runs
    // since it conventionally hosts cursor + main checks; others run only if their
    // fixture differs from cleaned content (i.e., contains markers).
    for (i, (name, fixture)) in files.iter().enumerate() {
        let (_, clean) = &cleaned[i];
        let has_markers = fixture.contains("$0")
            || fixture.contains("<err")
            || fixture.contains("<warn")
            || fixture.contains("<def")
            || fixture.contains("<ref")
            || fixture.contains("<hint")
            || fixture.contains("<lens")
            || fixture.contains("<hover")
            || fixture.contains("<th ")
            || fixture.contains("<rename")
            || fixture.contains("<complete")
            || fixture.contains("<type")
            || fixture.contains("<impl")
            || fixture.contains("<incoming")
            || fixture.contains("<outgoing");

        if i == 0 || has_markers {
            let uri = FakeEditor::filename_to_uri(name);
            run_checks_on_fixture(
                editor.server(),
                &uri,
                clean,
                fixture,
                Some(&file_contents_owned),
            )
            .await;
        }
    }
}

/// Run goto definition check.
async fn run_goto_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    expected_defs: &[Range],
) {
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = request::handle_goto_definition(server, params)
        .await
        .expect("Goto definition request failed");

    let locations: Vec<_> = match result {
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(loc)) => vec![loc],
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Array(locs)) => locs,
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|l| tower_lsp::lsp_types::Location {
                uri: l.target_uri,
                range: l.target_selection_range,
            })
            .collect(),
        None => vec![],
    };

    let actual_ranges: Vec<Range> = locations.iter().map(|l| l.range).collect();

    assert_eq!(
        actual_ranges.len(),
        expected_defs.len(),
        "Expected {} definitions, got {}.\nExpected: {:?}\nActual: {:?}",
        expected_defs.len(),
        actual_ranges.len(),
        expected_defs,
        actual_ranges
    );

    for expected in expected_defs {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected definition at {:?} not found.\nActual: {:?}",
            expected,
            actual_ranges
        );
    }
}

/// Run implementation check.
async fn run_implementation_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    expected_impls: &[Range],
) {
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = request::handle_goto_implementation(server, params)
        .await
        .expect("Goto implementation request failed");

    let locations: Vec<_> = match result {
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(loc)) => vec![loc],
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Array(locs)) => locs,
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|l| tower_lsp::lsp_types::Location {
                uri: l.target_uri,
                range: l.target_selection_range,
            })
            .collect(),
        None => vec![],
    };

    let actual_ranges: Vec<Range> = locations.iter().map(|l| l.range).collect();

    assert_eq!(
        actual_ranges.len(),
        expected_impls.len(),
        "Expected {} implementations, got {}.\nExpected: {:?}\nActual: {:?}",
        expected_impls.len(),
        actual_ranges.len(),
        expected_impls,
        actual_ranges
    );

    for expected in expected_impls {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected implementation at {:?} not found.\nActual: {:?}",
            expected,
            actual_ranges
        );
    }
}

/// Run incoming calls check — verifies that the method at cursor has callers
/// whose definitions match the expected ranges.
async fn run_incoming_calls_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    expected_caller_ranges: &[Range],
) {
    use tower_lsp::lsp_types::CallHierarchyPrepareParams;

    // Step 1: Prepare call hierarchy at cursor
    let prepare_params = CallHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let items = request::handle_prepare_call_hierarchy(server, prepare_params)
        .await
        .expect("Prepare call hierarchy request failed")
        .unwrap_or_default();

    assert!(
        !items.is_empty(),
        "prepareCallHierarchy returned no items at {:?}",
        cursor
    );

    // Step 2: Get incoming calls
    let incoming_params = tower_lsp::lsp_types::CallHierarchyIncomingCallsParams {
        item: items[0].clone(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let incoming = request::handle_incoming_calls(server, incoming_params)
        .await
        .expect("Incoming calls request failed")
        .unwrap_or_default();

    // Collect the caller method definition ranges
    let actual_ranges: Vec<Range> = incoming.iter().map(|c| c.from.range).collect();

    assert_eq!(
        actual_ranges.len(),
        expected_caller_ranges.len(),
        "Expected {} incoming callers, got {}.\nExpected: {:?}\nActual: {:?}",
        expected_caller_ranges.len(),
        actual_ranges.len(),
        expected_caller_ranges,
        actual_ranges
    );

    for expected in expected_caller_ranges {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected caller at {:?} not found.\nActual: {:?}",
            expected,
            actual_ranges
        );
    }
}

/// Run outgoing calls check — verifies that the method at cursor calls methods
/// whose definitions match the expected ranges.
async fn run_outgoing_calls_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    expected_callee_ranges: &[Range],
) {
    use tower_lsp::lsp_types::CallHierarchyPrepareParams;

    // Step 1: Prepare call hierarchy at cursor
    let prepare_params = CallHierarchyPrepareParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let items = request::handle_prepare_call_hierarchy(server, prepare_params)
        .await
        .expect("Prepare call hierarchy request failed")
        .unwrap_or_default();

    assert!(
        !items.is_empty(),
        "prepareCallHierarchy returned no items at {:?}",
        cursor
    );

    // Step 2: Get outgoing calls
    let outgoing_params = tower_lsp::lsp_types::CallHierarchyOutgoingCallsParams {
        item: items[0].clone(),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let outgoing = request::handle_outgoing_calls(server, outgoing_params)
        .await
        .expect("Outgoing calls request failed")
        .unwrap_or_default();

    // Collect the callee method definition ranges
    let actual_ranges: Vec<Range> = outgoing.iter().map(|c| c.to.range).collect();

    assert_eq!(
        actual_ranges.len(),
        expected_callee_ranges.len(),
        "Expected {} outgoing callees, got {}.\nExpected: {:?}\nActual: {:?}",
        expected_callee_ranges.len(),
        actual_ranges.len(),
        expected_callee_ranges,
        actual_ranges
    );

    for expected in expected_callee_ranges {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected callee at {:?} not found.\nActual: {:?}",
            expected,
            actual_ranges
        );
    }
}

/// Run references check.
async fn run_references_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    expected_refs: &[Range],
) {
    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };

    let result = request::handle_references(server, params)
        .await
        .expect("Find references request failed");

    let locations = result.unwrap_or_default();
    let actual_ranges: Vec<Range> = locations.iter().map(|l| l.range).collect();

    assert_eq!(
        actual_ranges.len(),
        expected_refs.len(),
        "Expected {} references, got {}.\nExpected: {:?}\nActual: {:?}",
        expected_refs.len(),
        actual_ranges.len(),
        expected_refs,
        actual_ranges
    );

    for expected in expected_refs {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected reference at {:?} not found.\nActual: {:?}",
            expected,
            actual_ranges
        );
    }
}

/// Run type check using TypeQuery.
///
/// The `<type label="..." kind="...">` tag checks the inferred type at the tag position.
/// Works like `<hover>` - no cursor needed.
///
/// Attributes:
/// - `label` (required): The expected type string (e.g., "String", "Integer")
/// - `kind` (optional): The kind of type check:
///   - `"return"` or `"->"`: Method return type (default for methods)
///   - `"var"` or `":"`: Variable/parameter type (default for variables)
///
/// Examples:
/// - `def gree<type label="String">ting` - infers return type (kind defaults to "return")
/// - `def gree<type label="String" kind="return">ting` - explicit return type
/// - `x<type label="String" kind="var"> = "hello"` - variable type
async fn run_type_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    content: &str,
    expected_types: &[&Tag],
    all_file_contents: &[(Url, Vec<u8>)],
) {
    use crate::analyzer_prism::RubyPrismAnalyzer;
    use crate::inferrer::r#type::ruby::RubyType;
    use crate::query::TypeQuery;

    // Build FileContentMap from all files
    let file_contents: std::collections::HashMap<&Url, &[u8]> = all_file_contents
        .iter()
        .map(|(u, c)| (u, c.as_slice()))
        .collect();

    for expected in expected_types {
        let expected_type = expected
            .attributes
            .get("label")
            .expect("type tag missing 'label' attribute");

        // Get kind attribute - defaults based on identifier type
        let kind = expected.attributes.get("kind").map(|s| s.as_str());

        let position = expected.range.start;

        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, _, _ancestors, _scope_stack, _namespace_kind) =
            analyzer.get_identifier(position);

        let type_query = TypeQuery::new(server.index_for_uri(&uri), uri, content.as_bytes());

        // Determine actual kind based on identifier type
        let (inferred_type, actual_kind): (Option<RubyType>, &str) = if let Some(identifier) =
            identifier_opt
        {
            match &identifier {
                crate::analyzer_prism::Identifier::RubyLocalVariable { name, .. } => {
                    (type_query.get_local_variable_type(name, position), "var")
                }
                crate::analyzer_prism::Identifier::RubyConstant { iden, .. } => {
                    let fqn = crate::types::fully_qualified_name::FullyQualifiedName::namespace(
                        iden.clone(),
                    );
                    (Some(RubyType::Class(fqn)), "const")
                }
                crate::analyzer_prism::Identifier::RubyMethod {
                    iden,
                    receiver,
                    namespace,
                } => {
                    let parse_result = ruby_prism::parse(content.as_bytes());
                    let node = parse_result.node();

                    let ty = if let Some(_def_node) =
                        find_def_node_at_position(&node, position, content)
                    {
                        // Build method FQN from namespace + method name
                        let method_fqn =
                            crate::types::ruby_method::RubyMethod::new(&iden.to_string())
                                .ok()
                                .map(|m| {
                                    crate::types::fully_qualified_name::FullyQualifiedName::method(
                                        namespace.clone(),
                                        m,
                                    )
                                });

                        if let Some(fqn) = method_fqn {
                            let mut index = server.index_for_uri(&uri).lock_arc();
                            crate::inferrer::return_type::infer_method_return_type(
                                &mut index,
                                &fqn,
                                None,
                                Some(&file_contents),
                            )
                        } else {
                            None
                        }
                    } else {
                        let receiver_type = match receiver {
                            crate::analyzer_prism::MethodReceiver::None
                            | crate::analyzer_prism::MethodReceiver::SelfReceiver => {
                                if namespace.is_empty() {
                                    RubyType::class("Object")
                                } else {
                                    let fqn = crate::types::fully_qualified_name::FullyQualifiedName::from(
                                            namespace.clone(),
                                        );
                                    RubyType::Class(fqn)
                                }
                            }
                            crate::analyzer_prism::MethodReceiver::Constant(path) => {
                                let fqn =
                                        crate::types::fully_qualified_name::FullyQualifiedName::Constant(
                                            path.clone().into(),
                                        );
                                RubyType::ClassReference(fqn)
                            }
                            crate::analyzer_prism::MethodReceiver::LocalVariable(name) => {
                                type_query
                                    .get_local_variable_type(name, position)
                                    .unwrap_or(RubyType::Unknown)
                            }
                            _ => RubyType::Unknown,
                        };

                        let mut index = server.index_for_uri(&uri).lock_arc();
                        crate::inferrer::return_type::infer_method_call(
                            &mut index,
                            &receiver_type,
                            &iden.to_string(),
                            Some(&file_contents),
                        )
                    };
                    (ty, "return")
                }
                crate::analyzer_prism::Identifier::RubyInstanceVariable { name, .. } => {
                    let index = server.index_for_uri(&uri).lock_arc();
                    let ty = index.file_entries(uri).iter().find_map(|entry| {
                        if let crate::indexer::entry::entry_kind::EntryKind::InstanceVariable(
                            data,
                        ) = &entry.kind
                        {
                            if &data.name == name && data.r#type != RubyType::Unknown {
                                return Some(data.r#type.clone());
                            }
                        }
                        None
                    });
                    (ty, "var")
                }
                crate::analyzer_prism::Identifier::RubyClassVariable { name, .. } => {
                    let index = server.index_for_uri(&uri).lock_arc();
                    let ty = index.file_entries(uri).iter().find_map(|entry| {
                        if let crate::indexer::entry::entry_kind::EntryKind::ClassVariable(data) =
                            &entry.kind
                        {
                            if &data.name == name && data.r#type != RubyType::Unknown {
                                return Some(data.r#type.clone());
                            }
                        }
                        None
                    });
                    (ty, "var")
                }
                crate::analyzer_prism::Identifier::RubyGlobalVariable { name, .. } => {
                    let index = server.index_for_uri(&uri).lock_arc();
                    let ty = index.file_entries(uri).iter().find_map(|entry| {
                        if let crate::indexer::entry::entry_kind::EntryKind::GlobalVariable(data) =
                            &entry.kind
                        {
                            if &data.name == name && data.r#type != RubyType::Unknown {
                                return Some(data.r#type.clone());
                            }
                        }
                        None
                    });
                    (ty, "var")
                }
                _ => (None, "unknown"),
            }
        } else {
            (None, "unknown")
        };

        // Validate kind attribute if provided
        if let Some(expected_kind) = kind {
            let kind_matches = match expected_kind {
                "return" | "->" => actual_kind == "return",
                "var" | ":" => actual_kind == "var",
                "const" => actual_kind == "const",
                _ => true, // Unknown kind, skip validation
            };
            assert!(
                kind_matches,
                "Kind mismatch at {:?}. Expected kind '{}' but found '{}'",
                position, expected_kind, actual_kind
            );
        }

        match inferred_type {
            Some(ty) => {
                let actual_str = ty.to_string();
                let matches = actual_str == *expected_type
                    || actual_str.ends_with(expected_type)
                    || actual_str.contains(expected_type);
                assert!(
                    matches,
                    "Type mismatch at {:?}.\nExpected: {} (kind: {})\nActual: {}",
                    position, expected_type, actual_kind, actual_str
                );
            }
            None => panic!(
                "Could not infer type at position {:?}.\nExpected: {} (kind: {})",
                position, expected_type, actual_kind
            ),
        }
    }
}

/// Run inlay hints check.
async fn run_inlay_hints_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    expected_hints: &[&Tag],
    none_ranges: &[&Tag],
) {
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(1000, 0),
        },
        work_done_progress_params: Default::default(),
    };

    let hints = handle_inlay_hints(server, params).await;

    // Check that no hints exist within the "none" ranges
    for none_tag in none_ranges {
        let hints_in_range: Vec<_> = hints
            .iter()
            .filter(|h| position_in_range(&h.position, &none_tag.range))
            .collect();
        assert!(
            hints_in_range.is_empty(),
            "Expected no inlay hints in range {:?}, got: {:?}",
            none_tag.range,
            hints_in_range
                .iter()
                .map(|h| format!(
                    "{}:{} '{}'",
                    h.position.line,
                    h.position.character,
                    get_hint_label(h)
                ))
                .collect::<Vec<_>>()
        );
    }

    // Check positive assertions
    for expected in expected_hints {
        let expected_label = expected
            .attributes
            .get("label")
            .expect("hint tag missing 'label' attribute");

        let found = hints.iter().find(|hint| {
            if hint.position.line != expected.range.start.line {
                return false;
            }
            let char_diff =
                (hint.position.character as i32 - expected.range.start.character as i32).abs();
            if char_diff > 2 {
                return false;
            }
            let label = get_hint_label(hint);
            label.contains(expected_label)
        });

        assert!(
            found.is_some(),
            "Expected inlay hint containing '{}' at line {}:{}, got hints: {:?}",
            expected_label,
            expected.range.start.line,
            expected.range.start.character,
            hints
                .iter()
                .map(|h| format!(
                    "{}:{} '{}'",
                    h.position.line,
                    h.position.character,
                    get_hint_label(h)
                ))
                .collect::<Vec<_>>()
        );
    }
}

/// Run diagnostics check.
async fn run_diagnostics_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    content: &str,
    err_tags: &[&Tag],
    warn_tags: &[&Tag],
    none_err_tags: &[&Tag],
    none_warn_tags: &[&Tag],
) {
    let document = server.docs.lock().get(uri).unwrap().read().clone();
    let parse_result = ruby_prism::parse(content.as_bytes());

    let mut diagnostics = generate_diagnostics(&parse_result, &document);
    {
        let index = server.index_for_uri(uri).lock_arc();
        diagnostics.extend(generate_yard_diagnostics_inner(&index, uri));
    }

    // Run IndexVisitor directly on the parsed AST to collect its diagnostics
    // (e.g., return type mismatches) without mutating server state.
    {
        use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
        use ruby_prism::Visit;
        let mut visitor = IndexVisitor::new(server.index_for_uri(uri), document.clone());
        visitor.visit(&parse_result.node());
        diagnostics.extend(visitor.diagnostics);
    }

    // Add unresolved-entry diagnostics (unresolved-constant, unresolved-method, etc.)
    // populated by indexing.
    {
        let query = crate::query::IndexQuery::new(server.index_for_uri(uri));
        diagnostics.extend(query.get_unresolved_diagnostics(uri));
    }

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    // Check that no errors exist within the "none" ranges.
    // If the `none` tag has a `code` attr, only errors of that code are forbidden.
    for none_tag in none_err_tags {
        let code_filter = none_tag.attributes.get("code");
        let errors_in_range: Vec<_> = errors
            .iter()
            .filter(|e| range_overlaps(&e.range, &none_tag.range))
            .filter(|e| match code_filter {
                Some(expected) => matches!(&e.code, Some(NumberOrString::String(s)) if s == expected),
                None => true,
            })
            .collect();
        assert!(
            errors_in_range.is_empty(),
            "Expected no errors{} in range {:?}, got: {:?}",
            code_filter.map(|c| format!(" [code={}]", c)).unwrap_or_default(),
            none_tag.range,
            errors_in_range
                .iter()
                .map(|e| (&e.range, &e.message))
                .collect::<Vec<_>>()
        );
    }

    // Check positive error assertions - each positive tag must have a matching error
    for expected_tag in err_tags {
        let found = errors
            .iter()
            .find(|e| diagnostic_matches_tag(e, expected_tag));
        assert!(
            found.is_some(),
            "Expected error at {:?} {} not found. Actual errors: {:?}",
            expected_tag.range,
            describe_tag_attrs(expected_tag),
            errors
                .iter()
                .map(|e| describe_diagnostic(e))
                .collect::<Vec<_>>()
        );
    }

    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();

    // Check that no warnings exist within the "none" ranges.
    // If the `none` tag has a `code` attr, only warnings of that code are forbidden.
    for none_tag in none_warn_tags {
        let code_filter = none_tag.attributes.get("code");
        let warnings_in_range: Vec<_> = warnings
            .iter()
            .filter(|w| range_overlaps(&w.range, &none_tag.range))
            .filter(|w| match code_filter {
                Some(expected) => matches!(&w.code, Some(NumberOrString::String(s)) if s == expected),
                None => true,
            })
            .collect();
        assert!(
            warnings_in_range.is_empty(),
            "Expected no warnings{} in range {:?}, got: {:?}",
            code_filter.map(|c| format!(" [code={}]", c)).unwrap_or_default(),
            none_tag.range,
            warnings_in_range
                .iter()
                .map(|w| (&w.range, &w.message))
                .collect::<Vec<_>>()
        );
    }

    // Check positive warning assertions
    for expected_tag in warn_tags {
        let found = warnings
            .iter()
            .find(|w| diagnostic_matches_tag(w, expected_tag));
        assert!(
            found.is_some(),
            "Expected warning at {:?} {} not found. Actual warnings: {:?}",
            expected_tag.range,
            describe_tag_attrs(expected_tag),
            warnings
                .iter()
                .map(|w| describe_diagnostic(w))
                .collect::<Vec<_>>()
        );
    }
}

/// Match a diagnostic against a tag's range and optional `code`/`message` attrs.
///
/// Range must match exactly. Severity comes from the tag name (`<err>` = ERROR,
/// `<warn>` = WARNING) — caller filters by severity before reaching here. If `code`
/// or `message` attributes are present, the diagnostic must satisfy them too.
/// Bare tags (no attrs) match any diagnostic at the range — backward compat.
fn diagnostic_matches_tag(diag: &Diagnostic, tag: &Tag) -> bool {
    if !ranges_match(&diag.range, &tag.range) {
        return false;
    }

    if let Some(expected_code) = tag.attributes.get("code") {
        let actual = match &diag.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            Some(NumberOrString::Number(_)) => return false,
            None => return false,
        };
        if actual != expected_code {
            return false;
        }
    }

    if let Some(expected_msg) = tag.attributes.get("message") {
        if !diag.message.contains(expected_msg) {
            return false;
        }
    }

    true
}

/// Pretty-print tag attribute assertions for error messages.
fn describe_tag_attrs(tag: &Tag) -> String {
    let mut parts = Vec::new();
    if let Some(c) = tag.attributes.get("code") {
        parts.push(format!("code={}", c));
    }
    if let Some(m) = tag.attributes.get("message") {
        parts.push(format!("message~={:?}", m));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}]", parts.join(", "))
    }
}

/// Pretty-print a diagnostic for error messages — shows range, code, severity, message.
fn describe_diagnostic(d: &Diagnostic) -> String {
    let code = match &d.code {
        Some(NumberOrString::String(s)) => s.clone(),
        Some(NumberOrString::Number(n)) => n.to_string(),
        None => "<no-code>".to_string(),
    };
    let sev = match d.severity {
        Some(DiagnosticSeverity::ERROR) => "ERROR",
        Some(DiagnosticSeverity::WARNING) => "WARNING",
        Some(DiagnosticSeverity::INFORMATION) => "INFO",
        Some(DiagnosticSeverity::HINT) => "HINT",
        _ => "?",
    };
    format!(
        "{}:{}-{}:{} [{} {}] {:?}",
        d.range.start.line,
        d.range.start.character,
        d.range.end.line,
        d.range.end.character,
        sev,
        code,
        d.message
    )
}

/// Run code lens check.
async fn run_code_lens_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    expected_lenses: &[&Tag],
    none_ranges: &[&Tag],
) {
    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let lenses = handle_code_lens(server, params).await.unwrap_or_default();

    // Check that no lenses exist within the "none" ranges
    for none_tag in none_ranges {
        let lenses_in_range: Vec<_> = lenses
            .iter()
            .filter(|l| range_overlaps(&l.range, &none_tag.range))
            .collect();
        assert!(
            lenses_in_range.is_empty(),
            "Expected no code lenses in range {:?}, got: {:?}",
            none_tag.range,
            lenses_in_range
                .iter()
                .filter_map(|l| l.command.as_ref().map(|c| &c.title))
                .collect::<Vec<_>>()
        );
    }

    // Check positive assertions
    for expected in expected_lenses {
        let expected_label = expected
            .attributes
            .get("title")
            .expect("lens tag missing 'title' attribute");
        let expected_line = expected.range.start.line;

        let found = lenses.iter().any(|lens| {
            if lens.range.start.line != expected_line {
                return false;
            }
            lens.command
                .as_ref()
                .map(|c| c.title.contains(expected_label))
                .unwrap_or(false)
        });

        assert!(
            found,
            "Expected code lens containing '{}' on line {}, got lenses: {:?}",
            expected_label,
            expected_line,
            lenses
                .iter()
                .map(|l| (l.range.start.line, l.command.as_ref().map(|c| &c.title)))
                .collect::<Vec<_>>()
        );
    }
}

/// Run hover check.
async fn run_hover_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    expected_hovers: &[&Tag],
) {
    use crate::capabilities::hover::handle_hover;
    use tower_lsp::lsp_types::HoverParams;

    for expected in expected_hovers {
        let expected_label = expected
            .attributes
            .get("label")
            .expect("hover tag missing 'label' attribute");

        let position = expected.range.start;

        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let hover_result = handle_hover(server, params).await;

        assert!(
            hover_result.is_some(),
            "Expected hover at {:?} but got None",
            position
        );

        let hover = hover_result.unwrap();
        let hover_content = match &hover.contents {
            tower_lsp::lsp_types::HoverContents::Scalar(text) => match text {
                tower_lsp::lsp_types::MarkedString::String(s) => s.clone(),
                tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
            },
            tower_lsp::lsp_types::HoverContents::Array(arr) => arr
                .iter()
                .map(|m| match m {
                    tower_lsp::lsp_types::MarkedString::String(s) => s.clone(),
                    tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            tower_lsp::lsp_types::HoverContents::Markup(markup) => markup.value.clone(),
        };

        assert!(
            hover_content.contains(expected_label),
            "Expected hover to contain '{}' at {:?}, got: '{}'",
            expected_label,
            position,
            hover_content
        );
    }
}

/// Find a DefNode at the given position in the AST.
fn find_def_node_at_position<'a>(
    node: &ruby_prism::Node<'a>,
    target_pos: Position,
    content: &str,
) -> Option<ruby_prism::DefNode<'a>> {
    if let Some(def_node) = node.as_def_node() {
        let loc = def_node.name_loc();
        let start_offset = loc.start_offset();
        let end_offset = loc.end_offset();

        // Convert offsets to position
        let before_start = &content.as_bytes()[..start_offset];
        let start_line = before_start.iter().filter(|&&b| b == b'\n').count() as u32;
        let last_newline = before_start
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let start_char = (start_offset - last_newline) as u32;

        let before_end = &content.as_bytes()[..end_offset];
        let end_line = before_end.iter().filter(|&&b| b == b'\n').count() as u32;
        let last_newline_end = before_end
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end_char = (end_offset - last_newline_end) as u32;

        // Check if target position is within the method name range
        if target_pos.line >= start_line
            && target_pos.line <= end_line
            && (target_pos.line > start_line || target_pos.character >= start_char)
            && (target_pos.line < end_line || target_pos.character <= end_char)
        {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_position(&stmt, target_pos, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_position(&stmt, target_pos, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_position(&stmt, target_pos, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_position(&stmt, target_pos, content) {
                return Some(found);
            }
        }
    }

    if let Some(sclass) = node.as_singleton_class_node() {
        if let Some(body) = sclass.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_position(&stmt, target_pos, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    None
}

/// Run type hierarchy check.
///
/// The `<th>` tag supports the following attributes:
/// - `supertypes="A,B,C"` - comma-separated list of expected supertype names
/// - `subtypes="X,Y,Z"` - comma-separated list of expected subtype names
///
/// Example: `<th supertypes="Animal,Walkable" subtypes="Poodle">Dog$0</th>`
async fn run_type_hierarchy_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    th_tags: &[&Tag],
) {
    // There should typically be one th tag, but we support multiple
    for tag in th_tags {
        // Prepare type hierarchy at cursor
        let prepare_params = TypeHierarchyPrepareParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: cursor,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let prepare_result =
            type_hierarchy::handle_prepare_type_hierarchy(server, prepare_params).await;

        let items = prepare_result.expect("Type hierarchy prepare should return items");
        assert!(
            !items.is_empty(),
            "Expected type hierarchy item at cursor {:?}",
            cursor
        );

        let item = &items[0];

        // Check supertypes if specified
        if let Some(expected_supertypes_str) = tag.attributes.get("supertypes") {
            let expected_supertypes: Vec<&str> = expected_supertypes_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let supertypes_params = TypeHierarchySupertypesParams {
                item: item.clone(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            };

            let supertypes = type_hierarchy::handle_supertypes(server, supertypes_params)
                .await
                .unwrap_or_default();

            let supertype_names: Vec<&str> = supertypes.iter().map(|s| s.name.as_str()).collect();

            for expected in &expected_supertypes {
                assert!(
                    supertype_names.contains(expected),
                    "Expected supertype '{}' not found for '{}'.\nExpected: {:?}\nActual: {:?}",
                    expected,
                    item.name,
                    expected_supertypes,
                    supertype_names
                );
            }
        }

        // Check subtypes if specified
        if let Some(expected_subtypes_str) = tag.attributes.get("subtypes") {
            let expected_subtypes: Vec<&str> = expected_subtypes_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let subtypes_params = TypeHierarchySubtypesParams {
                item: item.clone(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            };

            let subtypes = type_hierarchy::handle_subtypes(server, subtypes_params)
                .await
                .unwrap_or_default();

            let subtype_names: Vec<&str> = subtypes.iter().map(|s| s.name.as_str()).collect();

            for expected in &expected_subtypes {
                assert!(
                    subtype_names.contains(expected),
                    "Expected subtype '{}' not found for '{}'.\nExpected: {:?}\nActual: {:?}",
                    expected,
                    item.name,
                    expected_subtypes,
                    subtype_names
                );
            }
        }
    }
}

/// Run rename check for local variables.
///
/// The `<rename to="new_name">` tag marks a cursor position for rename testing.
///
/// Example:
/// ```ignore
/// x = 1           # <rename to="counter">
/// puts x          # should also be renamed
/// ```
///
/// The test verifies:
/// 1. Rename is supported at the cursor position
/// 2. All expected locations are returned in the WorkspaceEdit
///
/// Note: Currently only local variables are supported for rename.
async fn run_rename_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    _content: &str,
    rename_tags: &[&Tag],
) {
    use crate::capabilities::rename::handle_rename;
    use tower_lsp::lsp_types::RenameParams;

    // Find the cursor tag (the one with the "to" attribute)
    let cursor_tag = rename_tags
        .iter()
        .find(|t| t.attributes.contains_key("to"))
        .expect("At least one <rename> tag must have a 'to' attribute");

    let new_name = cursor_tag
        .attributes
        .get("to")
        .expect("rename tag missing 'to' attribute")
        .clone();

    let position = cursor_tag.range.start;

    let params = RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        },
        new_name: new_name.clone(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };

    let result = handle_rename(server, params).await;

    // All <rename> tags mark expected rename locations
    let expected_ranges: Vec<Range> = rename_tags.iter().map(|t| t.range).collect();

    let edit = result.unwrap_or_else(|| {
        panic!(
            "Rename should return a result at {:?} for '{}'",
            position, new_name
        )
    });
    let changes = edit.changes.expect("WorkspaceEdit should have changes");
    let uri_edits = changes.get(uri).expect("Should have changes for this URI");

    // Verify exact count
    assert_eq!(
        uri_edits.len(),
        expected_ranges.len(),
        "Expected {} rename locations, got {}.\nExpected ranges: {:?}\nActual edits: {:?}",
        expected_ranges.len(),
        uri_edits.len(),
        expected_ranges,
        uri_edits.iter().map(|e| &e.range).collect::<Vec<_>>()
    );

    // Verify each expected range is found in the actual edits
    for expected in &expected_ranges {
        assert!(
            uri_edits.iter().any(|e| ranges_match(&e.range, expected)),
            "Expected rename at {:?} not found in actual edits: {:?}",
            expected,
            uri_edits.iter().map(|e| &e.range).collect::<Vec<_>>()
        );
    }
}

/// Run completion check.
///
/// The `<complete items="greet,to_s">` tag asserts expected completion items at the cursor.
///
/// Attributes:
/// - `items` (required): comma-separated list of expected completion item labels
/// - `excludes` (optional): comma-separated list of items that must NOT appear
async fn run_completion_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    complete_tags: &[&Tag],
) {
    for tag in complete_tags {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: cursor,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        let result = request::handle_completion(server, params)
            .await
            .expect("Completion request failed");

        let items = match result {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(list)) => list.items,
            None => vec![],
        };

        let actual_labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        // Check expected items are present
        if let Some(expected_items_str) = tag.attributes.get("items") {
            let expected: Vec<&str> = expected_items_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            for expected_item in &expected {
                assert!(
                    actual_labels.iter().any(|l| l.contains(expected_item)),
                    "Expected completion item '{}' not found at {:?}.\nActual items: {:?}",
                    expected_item,
                    cursor,
                    actual_labels
                );
            }
        }

        // Check excluded items are NOT present
        if let Some(excludes_str) = tag.attributes.get("excludes") {
            let excluded: Vec<&str> = excludes_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            for excluded_item in &excluded {
                assert!(
                    !actual_labels.iter().any(|l| l.contains(excluded_item)),
                    "Completion item '{}' should NOT be present at {:?}.\nActual items: {:?}",
                    excluded_item,
                    cursor,
                    actual_labels
                );
            }
        }
    }
}

/// Compare ranges exactly.
fn ranges_match(actual: &Range, expected: &Range) -> bool {
    actual.start.line == expected.start.line
        && actual.start.character == expected.start.character
        && actual.end.line == expected.end.line
        && actual.end.character == expected.end.character
}

/// Check if a position is within a range.
fn position_in_range(pos: &Position, range: &Range) -> bool {
    // Position is after or at range start
    let after_start = pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character);

    // Position is before or at range end
    let before_end = pos.line < range.end.line
        || (pos.line == range.end.line && pos.character <= range.end.character);

    after_start && before_end
}

/// Check if two ranges overlap.
fn range_overlaps(a: &Range, b: &Range) -> bool {
    // a starts before b ends AND a ends after b starts
    let a_starts_before_b_ends = a.start.line < b.end.line
        || (a.start.line == b.end.line && a.start.character <= b.end.character);
    let a_ends_after_b_starts = a.end.line > b.start.line
        || (a.end.line == b.start.line && a.end.character >= b.start.character);

    a_starts_before_b_ends && a_ends_after_b_starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_inlay_hints() {
        check(r#"x<hint label="String"> = "hello""#).await;
    }

    #[tokio::test]
    async fn test_check_goto_definition() {
        check(
            r#"
<def>class Foo
end</def>

Foo$0.new
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_valid_code_no_errors() {
        // <err none>...</err> explicitly asserts no errors in the wrapped range
        check(
            r#"
<err none>
class Foo
  def bar
    "hello"
  end
end
</err>
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_code_lens() {
        check(
            r#"
module MyModule <lens title="include">
end

class MyClass
  include MyModule
end
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_no_code_lens() {
        // <lens none>...</lens> explicitly asserts no code lenses in the wrapped range
        check(
            r#"
<lens none>
module UnusedModule
end
</lens>
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_no_hints() {
        // <hint none>...</hint> explicitly asserts no inlay hints in the wrapped range
        // Constants and simple expressions don't get type hints
        check(
            r#"
<hint none>
FOO = 42
</hint>
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_type_hierarchy() {
        // <th supertypes="..." subtypes="..."> checks type hierarchy at cursor
        check(
            r#"
class Animal
end

<th supertypes="Animal" subtypes="Poodle">
class Dog$0 < Animal
end
</th>

class Poodle < Dog
end
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_completion() {
        check(
            r#"
class Greeter
end

class GreetHelper
end

Greet$0
<complete items="Greeter,GreetHelper">
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_fake_editor_lifecycle() {
        let mut editor = FakeEditor::new().await;
        editor.open("lifecycle.rb", "class Foo; end").await;
        editor
            .set(
                "lifecycle.rb",
                "class Foo\n  def greet\n    \"hi\"\n  end\nend",
            )
            .await;
        editor
            .check(
                "lifecycle.rb",
                r#"
class Foo
  def greet
    "hi"
  end
end
"#,
            )
            .await;
    }

    // ─── Diagnostic tag attribute self-tests ───────────────────────────

    #[tokio::test]
    async fn diag_tag_matches_code() {
        // Bare unresolved constant produces code="unresolved-constant", severity ERROR
        check(r#"<err code="unresolved-constant">UnknownThing</err>.new"#).await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected error")]
    async fn diag_tag_wrong_code_fails() {
        // Wrong code attribute must cause assertion failure
        check(r#"<err code="bogus-code-name">UnknownThing</err>.new"#).await;
    }

    #[tokio::test]
    async fn diag_tag_matches_message_substring() {
        check(r#"<err message="Unresolved constant">UnknownThing</err>.new"#).await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected error")]
    async fn diag_tag_wrong_message_fails() {
        check(r#"<err message="this text not in diag">UnknownThing</err>.new"#).await;
    }

    #[tokio::test]
    async fn diag_tag_combines_attrs() {
        check(
            r#"<err code="unresolved-constant" message="Unresolved">UnknownThing</err>.new"#,
        )
        .await;
    }

    #[tokio::test]
    async fn diag_tag_bare_still_works() {
        // Backward compat: bare <err> with no attrs matches any error in range
        check(r#"<err>UnknownThing</err>.new"#).await;
    }

    // ─── FakeEditor diag helper self-tests ────────────────────────────

    #[tokio::test]
    async fn fake_editor_assert_no_errors_passes_on_clean_code() {
        let mut editor = FakeEditor::new().await;
        editor
            .open("clean.rb", "class Foo\n  def bar\n    1\n  end\nend")
            .await;
        editor.assert_no_errors("clean.rb").await;
    }

    #[tokio::test]
    #[should_panic(expected = "Expected no errors")]
    async fn fake_editor_assert_no_errors_fails_when_error_present() {
        let mut editor = FakeEditor::new().await;
        editor.open("dirty.rb", "UnknownThing.new").await;
        editor.assert_no_errors("dirty.rb").await;
    }

    #[tokio::test]
    async fn fake_editor_assert_error_code_finds_match() {
        let mut editor = FakeEditor::new().await;
        editor.open("err.rb", "UnknownThing.new").await;
        let d = editor
            .assert_error_code("err.rb", "unresolved-constant")
            .await;
        assert!(d.message.contains("UnknownThing"));
    }

    #[tokio::test]
    #[should_panic(expected = "Expected error with code")]
    async fn fake_editor_assert_error_code_fails_when_missing() {
        let mut editor = FakeEditor::new().await;
        editor.open("err.rb", "UnknownThing.new").await;
        editor.assert_error_code("err.rb", "nonexistent-code").await;
    }

    // ─── Multi-file diag tag self-test ────────────────────────────────

    #[tokio::test]
    async fn check_multi_file_runs_diag_in_non_primary_file() {
        // Tags in second file must be evaluated, not treated as Ruby
        check_multi_file(&[
            (
                "main.rb",
                r#"
class Main
end
"#,
            ),
            (
                "other.rb",
                r#"
<err code="unresolved-constant">DoesNotExist</err>.new
"#,
            ),
        ])
        .await;
    }

    #[tokio::test]
    async fn test_literal_dot_completion() {
        let mut editor = FakeEditor::new().await;

        // Array literal after assignment
        editor.open("a.rb", "a = [1,2,3].").await;
        let items = editor.complete_with_trigger("a.rb", 0, 12, ".").await;
        assert!(!items.is_empty(), "Expected completions for `a = [1,2,3].`");
        assert!(
            items.iter().any(|i| i.label == "first"),
            "Expected `first` in Array completions"
        );

        // String literal after assignment
        editor.open("b.rb", r#"b = "hello"."#).await;
        let items = editor.complete_with_trigger("b.rb", 0, 12, ".").await;
        assert!(
            !items.is_empty(),
            "Expected completions for `b = \"hello\".`"
        );
        assert!(
            items.iter().any(|i| i.label == "upcase"),
            "Expected `upcase` in String completions"
        );

        // Hash literal after assignment
        editor.open("c.rb", "c = {a: 1}.").await;
        let items = editor.complete_with_trigger("c.rb", 0, 11, ".").await;
        assert!(
            !items.is_empty(),
            "Expected completions for hash literal dot completion"
        );

        // Integer literal after assignment
        editor.open("d.rb", "d = 42.").await;
        let items = editor.complete_with_trigger("d.rb", 0, 7, ".").await;
        assert!(!items.is_empty(), "Expected completions for `d = 42.`");
        assert!(
            items.iter().any(|i| i.label == "abs"),
            "Expected `abs` in Integer completions"
        );
    }
}
