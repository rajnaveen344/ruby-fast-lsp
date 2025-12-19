//! Type inference check function.
//!
//! This module provides `check_type` to verify the inferred type at a cursor position.

use super::fixture::{parse_fixture, setup_with_fixture};
use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::type_inference::ruby_type::RubyType;
use crate::utils::position_to_offset;

/// Check that the inferred type at the cursor position matches the expected type.
///
/// # Markers
/// - `$0` - cursor position (where to check the type)
/// - `<type>ExpectedType</type>` - expected type string (optional, can use second arg)
///
/// # Example
///
/// ```ignore
/// // Using inline <type> marker:
/// check_type(r#"
/// x<type>String</type> = "hello"$0
/// "#, None).await;
///
/// // Using explicit expected type:
/// check_type(r#"
/// x = "hello"$0
/// "#, Some("String")).await;
/// ```
pub async fn check_type(fixture_text: &str, expected_type: Option<&str>) {
    let fixture = parse_fixture(fixture_text);
    let (server, uri) = setup_with_fixture(&fixture.content).await;

    // Determine expected type from marker or argument
    let expected = expected_type
        .map(|s| s.to_string())
        .or(fixture.expected_type)
        .expect("Must provide expected type via <type>...</type> marker or second argument");

    // Get the identifier at the cursor position
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), fixture.content.clone());
    let (identifier_opt, _ancestors, _scope_stack) = analyzer.get_identifier(fixture.cursor);

    let inferred_type: Option<RubyType> = if let Some(identifier) = identifier_opt {
        match &identifier {
            crate::analyzer_prism::Identifier::RubyLocalVariable { name, scope, .. } => {
                // First try document.lvars
                let mut found_type: Option<RubyType> = None;
                {
                    let docs = server.docs.lock();
                    if let Some(doc_arc) = docs.get(&uri) {
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

                // Return found type or try type narrowing
                found_type.or_else(|| {
                    let offset = position_to_offset(&fixture.content, fixture.cursor);
                    server.type_narrowing.get_narrowed_type(&uri, name, offset)
                })
            }
            crate::analyzer_prism::Identifier::RubyConstant { iden, .. } => {
                // Constants have their type as themselves
                let fqn =
                    crate::types::fully_qualified_name::FullyQualifiedName::namespace(iden.clone());
                Some(RubyType::Class(fqn))
            }
            crate::analyzer_prism::Identifier::RubyMethod { receiver, iden, .. } => {
                // For method calls, use MethodResolver static function
                use crate::type_inference::MethodResolver;
                let index = server.index.lock();

                // Determine receiver type
                let receiver_type = match receiver {
                    crate::analyzer_prism::MethodReceiver::Constant(ns) => {
                        let fqn = crate::types::fully_qualified_name::FullyQualifiedName::namespace(
                            ns.clone(),
                        );
                        Some(RubyType::Class(fqn))
                    }
                    _ => None,
                };

                if let Some(recv_type) = receiver_type {
                    MethodResolver::resolve_method_return_type(
                        &index,
                        &recv_type,
                        &iden.to_string(),
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };

    match inferred_type {
        Some(ty) => assert_type_matches(&ty, &expected),
        None => panic!(
            "Could not infer type at position {:?}.\nExpected: {}",
            fixture.cursor, expected
        ),
    }
}

/// Assert that the inferred type matches the expected type string.
fn assert_type_matches(actual: &RubyType, expected: &str) {
    let actual_str = actual.to_string();

    // Allow flexible matching: "String" matches "String", "Class" matches full FQN, etc.
    let matches =
        actual_str == expected || actual_str.ends_with(expected) || actual_str.contains(expected);

    assert!(
        matches,
        "Type mismatch.\nExpected: {}\nActual: {}",
        expected, actual_str
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_type_string_literal() {
        check_type(
            r#"
x = "hello"
x$0
"#,
            Some("String"),
        )
        .await;
    }

    #[tokio::test]
    async fn test_check_type_with_marker() {
        check_type(
            r#"
x<type>String</type> = "hello"
x$0
"#,
            None,
        )
        .await;
    }
}
