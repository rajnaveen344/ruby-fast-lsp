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
    CodeLensParams, DiagnosticSeverity, GotoDefinitionParams, InlayHintParams, PartialResultParams,
    Position, Range, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
    TextDocumentPositionParams, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams, Url, WorkDoneProgressParams,
};

use super::fixture::{
    extract_cursor, extract_tags, extract_tags_with_attributes, setup_with_fixture, Tag,
};
use super::inlay_hints::get_hint_label;
use crate::capabilities::code_lens::handle_code_lens;
use crate::capabilities::diagnostics::{generate_diagnostics, generate_yard_diagnostics};
use crate::capabilities::inlay_hints::handle_inlay_hints;
use crate::capabilities::type_hierarchy;
use crate::handlers::request;

/// All supported tag names for extraction.
const ALL_TAGS: &[&str] = &[
    "def", "ref", "type", "hint", "lens", "err", "warn", "hover", "th",
];

/// Unified check function that runs checks based on tags present in the fixture.
///
/// See module documentation for supported tags and examples.
pub async fn check(fixture_text: &str) {
    // Extract cursor if present
    let has_cursor = fixture_text.contains("$0");
    let (cursor, text_without_cursor) = if has_cursor {
        let (pos, clean) = extract_cursor(fixture_text);
        (Some(pos), clean)
    } else {
        (None, fixture_text.to_string())
    };

    // Extract all tags in one pass
    let (all_tags, content) = extract_tags_with_attributes(&text_without_cursor, ALL_TAGS);

    // Also extract simple tags for def/ref (they don't use attributes)
    let (def_ranges, _) = extract_tags(&text_without_cursor, "def");
    let (ref_ranges, _) = extract_tags(&text_without_cursor, "ref");
    let (type_tags, _) = extract_tags(&text_without_cursor, "type");

    // Categorize tags by kind
    let hint_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hint").collect();
    let lens_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "lens").collect();
    let err_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "err").collect();
    let warn_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "warn").collect();
    let hover_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hover").collect();
    let th_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "th").collect();

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

    // Setup server once
    let (server, uri) = setup_with_fixture(&content).await;

    // Track which checks were run
    let mut checks_run = Vec::new();

    // Run goto definition check if we have cursor and def tags
    if cursor.is_some() && !def_ranges.is_empty() {
        run_goto_check(&server, &uri, cursor.unwrap(), &def_ranges).await;
        checks_run.push("goto");
    }

    // Run references check if we have cursor and ref tags
    if cursor.is_some() && !ref_ranges.is_empty() {
        run_references_check(&server, &uri, cursor.unwrap(), &ref_ranges).await;
        checks_run.push("references");
    }

    // Run type check if we have cursor and type tags
    if cursor.is_some() && !type_tags.is_empty() {
        // Type check needs special handling - extract the type content
        let (_, text_for_type) = extract_cursor(fixture_text);
        run_type_check(&server, &uri, cursor.unwrap(), &text_for_type, &content).await;
        checks_run.push("type");
    }

    // Run inlay hints check if we have hint tags (positive or negative)
    if !hint_tags.is_empty() {
        run_inlay_hints_check(&server, &uri, &positive_hint_tags, &none_hint_tags).await;
        checks_run.push("hints");
    }

    // Run diagnostics check if we have err or warn tags (positive or negative)
    if !err_tags.is_empty() || !warn_tags.is_empty() {
        run_diagnostics_check(
            &server,
            &uri,
            &content,
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
        run_code_lens_check(&server, &uri, &positive_lens_tags, &none_lens_tags).await;
        checks_run.push("lens");
    }

    // Run hover check if we have hover tags
    if !hover_tags.is_empty() {
        run_hover_check(&server, &uri, &hover_tags).await;
        checks_run.push("hover");
    }

    // Run type hierarchy check if we have th tags
    if cursor.is_some() && !th_tags.is_empty() {
        run_type_hierarchy_check(&server, &uri, cursor.unwrap(), &th_tags).await;
        checks_run.push("type_hierarchy");
    }

    // If no checks were run, that's okay - it means the fixture is valid with no expectations
    // (e.g., testing that valid code has no errors)
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

/// Run type check.
async fn run_type_check(
    server: &crate::server::RubyLanguageServer,
    uri: &Url,
    cursor: Position,
    text_with_type_tag: &str,
    _content: &str,
) {
    use crate::analyzer_prism::RubyPrismAnalyzer;
    use crate::inferrer::r#type::ruby::RubyType;

    // Extract expected type from <type>...</type> marker
    let open_tag = "<type>";
    let close_tag = "</type>";
    let expected_type = if let Some(open_pos) = text_with_type_tag.find(open_tag) {
        let after_open = &text_with_type_tag[open_pos + open_tag.len()..];
        if let Some(close_pos) = after_open.find(close_tag) {
            after_open[..close_pos].to_string()
        } else {
            panic!("Unclosed <type> tag");
        }
    } else {
        panic!("Type check requires <type>...</type> marker");
    };

    // Clean content without type tag
    let clean_content =
        text_with_type_tag.replace(&format!("{}{}{}", open_tag, expected_type, close_tag), "");

    let analyzer = RubyPrismAnalyzer::new(uri.clone(), clean_content.clone());
    let (identifier_opt, _, _ancestors, _scope_stack) = analyzer.get_identifier(cursor);

    let inferred_type: Option<RubyType> = if let Some(identifier) = identifier_opt {
        match &identifier {
            crate::analyzer_prism::Identifier::RubyLocalVariable { name, scope, .. } => {
                let mut found_type: Option<RubyType> = None;
                {
                    let docs = server.docs.lock();
                    if let Some(doc_arc) = docs.get(uri) {
                        let doc = doc_arc.read();
                        if let Some(entries) = doc.get_local_var_entries(*scope) {
                            for entry in entries {
                                if let crate::indexer::entry::entry_kind::EntryKind::LocalVariable(
                                    data,
                                ) = &entry.kind
                                {
                                    if &data.name == name && data.r#type != RubyType::Unknown {
                                        found_type = Some(data.r#type.clone());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                found_type
            }
            crate::analyzer_prism::Identifier::RubyConstant { iden, .. } => {
                let fqn =
                    crate::types::fully_qualified_name::FullyQualifiedName::namespace(iden.clone());
                Some(RubyType::Class(fqn))
            }
            _ => None,
        }
    } else {
        None
    };

    match inferred_type {
        Some(ty) => {
            let actual_str = ty.to_string();
            let matches = actual_str == expected_type
                || actual_str.ends_with(&expected_type)
                || actual_str.contains(&expected_type);
            assert!(
                matches,
                "Type mismatch.\nExpected: {}\nActual: {}",
                expected_type, actual_str
            );
        }
        None => panic!(
            "Could not infer type at position {:?}.\nExpected: {}",
            cursor, expected_type
        ),
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
        let index = server.index.lock();
        diagnostics.extend(generate_yard_diagnostics(&index, uri));
    }

    // Force re-indexing to run IndexVisitor again (since setup_with_fixture already indexed it)
    {
        let docs = server.docs.lock();
        if let Some(doc_arc) = docs.get(uri) {
            let mut doc = doc_arc.write();
            doc.indexed_version = None;
        }
    }

    // Run FileProcessor to get indexing diagnostics (including return type checks)
    let processor = crate::indexer::file_processor::FileProcessor::new(server.index.clone());
    let options = crate::indexer::file_processor::ProcessingOptions {
        index_definitions: true,
        index_references: false,
        resolve_mixins: false,
        include_local_vars: true,
    };
    if let Ok(process_result) = processor.process_file(uri, content, server, options) {
        diagnostics.extend(process_result.diagnostics);
    }

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect();

    // Check that no errors exist within the "none" ranges
    for none_tag in none_err_tags {
        let errors_in_range: Vec<_> = errors
            .iter()
            .filter(|e| range_overlaps(&e.range, &none_tag.range))
            .collect();
        assert!(
            errors_in_range.is_empty(),
            "Expected no errors in range {:?}, got: {:?}",
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
            .find(|e| ranges_match(&e.range, &expected_tag.range));
        assert!(
            found.is_some(),
            "Expected error at {:?}, not found. Actual errors: {:?}",
            expected_tag.range,
            errors
                .iter()
                .map(|e| (&e.range, &e.message))
                .collect::<Vec<_>>()
        );
    }

    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .collect();

    // Check that no warnings exist within the "none" ranges
    for none_tag in none_warn_tags {
        let warnings_in_range: Vec<_> = warnings
            .iter()
            .filter(|w| range_overlaps(&w.range, &none_tag.range))
            .collect();
        assert!(
            warnings_in_range.is_empty(),
            "Expected no warnings in range {:?}, got: {:?}",
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
            .find(|w| ranges_match(&w.range, &expected_tag.range));
        assert!(
            found.is_some(),
            "Expected warning at {:?}, not found. Actual warnings: {:?}",
            expected_tag.range,
            warnings
                .iter()
                .map(|w| (&w.range, &w.message))
                .collect::<Vec<_>>()
        );
    }
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
}
