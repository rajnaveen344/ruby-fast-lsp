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
    extract_cursor, extract_tags, extract_tags_with_attributes, setup_with_fixture,
    setup_with_multi_file_fixture, Tag,
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

    // Categorize tags by kind
    let hint_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hint").collect();
    let lens_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "lens").collect();
    let err_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "err").collect();
    let warn_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "warn").collect();
    let hover_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hover").collect();
    let th_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "th").collect();
    let type_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "type").collect();

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

    // Run type check if we have type tags (no cursor needed - uses tag position)
    if !type_tags.is_empty() {
        let file_contents_owned = vec![(uri.clone(), content.as_bytes().to_vec())];
        run_type_check(&server, &uri, &content, &type_tags, &file_contents_owned).await;
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

    let (primary_filename, primary_fixture) = files[0];

    // Extract cursor if present in primary file
    let has_cursor = primary_fixture.contains("$0");
    let (cursor, text_without_cursor) = if has_cursor {
        let (pos, clean) = extract_cursor(primary_fixture);
        (Some(pos), clean)
    } else {
        (None, primary_fixture.to_string())
    };

    // Extract all tags from primary file
    let (all_tags, primary_content) = extract_tags_with_attributes(&text_without_cursor, ALL_TAGS);

    // Also extract simple tags for def/ref
    let (def_ranges, _) = extract_tags(&text_without_cursor, "def");
    let (ref_ranges, _) = extract_tags(&text_without_cursor, "ref");

    // Categorize tags
    let hint_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hint").collect();
    let lens_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "lens").collect();
    let err_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "err").collect();
    let warn_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "warn").collect();
    let hover_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "hover").collect();
    let th_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "th").collect();
    let type_tags: Vec<&Tag> = all_tags.iter().filter(|t| t.kind == "type").collect();

    // Separate none tags
    let none_err_tags: Vec<&Tag> = err_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_warn_tags: Vec<&Tag> = warn_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_hint_tags: Vec<&Tag> = hint_tags.iter().filter(|t| t.is_none()).copied().collect();
    let none_lens_tags: Vec<&Tag> = lens_tags.iter().filter(|t| t.is_none()).copied().collect();

    let positive_err_tags: Vec<&Tag> = err_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_warn_tags: Vec<&Tag> =
        warn_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_hint_tags: Vec<&Tag> =
        hint_tags.iter().filter(|t| !t.is_none()).copied().collect();
    let positive_lens_tags: Vec<&Tag> =
        lens_tags.iter().filter(|t| !t.is_none()).copied().collect();

    // Build file list with cleaned primary content
    let mut cleaned_files: Vec<(&str, String)> = Vec::new();
    cleaned_files.push((primary_filename, primary_content.clone()));
    for (filename, content) in files.iter().skip(1) {
        cleaned_files.push((filename, content.to_string()));
    }

    let file_refs: Vec<(&str, &str)> = cleaned_files
        .iter()
        .map(|(f, c)| (*f, c.as_str()))
        .collect();

    // Setup server with all files
    let (server, uris) = setup_with_multi_file_fixture(&file_refs).await;
    let primary_uri = &uris[0];

    // Track checks run
    let mut checks_run = Vec::new();

    // Run goto definition check
    if cursor.is_some() && !def_ranges.is_empty() {
        run_goto_check(&server, primary_uri, cursor.unwrap(), &def_ranges).await;
        checks_run.push("goto");
    }

    // Run references check
    if cursor.is_some() && !ref_ranges.is_empty() {
        run_references_check(&server, primary_uri, cursor.unwrap(), &ref_ranges).await;
        checks_run.push("references");
    }

    // Run type check (no cursor needed - uses tag position)
    if !type_tags.is_empty() {
        // Build file contents map for all files
        let file_contents_owned: Vec<(Url, Vec<u8>)> = cleaned_files
            .iter()
            .enumerate()
            .map(|(i, (_, content))| (uris[i].clone(), content.as_bytes().to_vec()))
            .collect();
        run_type_check(
            &server,
            primary_uri,
            &primary_content,
            &type_tags,
            &file_contents_owned,
        )
        .await;
        checks_run.push("type");
    }

    // Run inlay hints check
    if !hint_tags.is_empty() {
        run_inlay_hints_check(&server, primary_uri, &positive_hint_tags, &none_hint_tags).await;
        checks_run.push("hints");
    }

    // Run diagnostics check
    if !err_tags.is_empty() || !warn_tags.is_empty() {
        run_diagnostics_check(
            &server,
            primary_uri,
            &primary_content,
            &positive_err_tags,
            &positive_warn_tags,
            &none_err_tags,
            &none_warn_tags,
        )
        .await;
        checks_run.push("diagnostics");
    }

    // Run code lens check
    if !lens_tags.is_empty() {
        run_code_lens_check(&server, primary_uri, &positive_lens_tags, &none_lens_tags).await;
        checks_run.push("lens");
    }

    // Run hover check
    if !hover_tags.is_empty() {
        run_hover_check(&server, primary_uri, &hover_tags).await;
        checks_run.push("hover");
    }

    // Run type hierarchy check
    if cursor.is_some() && !th_tags.is_empty() {
        run_type_hierarchy_check(&server, primary_uri, cursor.unwrap(), &th_tags).await;
        checks_run.push("type_hierarchy");
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
        let (identifier_opt, _, _ancestors, _scope_stack, _namespace_kind) = analyzer.get_identifier(position);

        let type_query = TypeQuery::new(server.index.clone(), uri, content.as_bytes());

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
                        let method_fqn = crate::types::ruby_method::RubyMethod::new(
                            &iden.to_string(),
                        )
                        .ok()
                        .map(|m| {
                            crate::types::fully_qualified_name::FullyQualifiedName::method(
                                namespace.clone(),
                                m,
                            )
                        });

                        if let Some(fqn) = method_fqn {
                            let mut index = server.index.lock();
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

                        let mut index = server.index.lock();
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
                    let index = server.index.lock();
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
                    let index = server.index.lock();
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
                    let index = server.index.lock();
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
