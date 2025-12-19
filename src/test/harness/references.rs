//! Find references check function.

use tower_lsp::lsp_types::{
    PartialResultParams, Range, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

use super::fixture::{parse_fixture, ranges_match, setup_with_fixture};
use crate::handlers::request;

/// Check that find references at the cursor position finds all expected references.
///
/// # Markers
/// - `$0` - cursor position (where the user "clicks")
/// - `<ref>...</ref>` - expected reference range(s), must match exactly
///
/// # Example
///
/// ```ignore
/// check_references(r#"
/// class <ref>Foo$0</ref>
/// end
///
/// <ref>Foo</ref>.new
/// "#).await;
/// ```
pub async fn check_references(fixture_text: &str) {
    let fixture = parse_fixture(fixture_text);
    let (server, uri) = setup_with_fixture(&fixture.content).await;

    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: fixture.cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };

    let result = request::handle_references(&server, params)
        .await
        .expect("Find references request failed");

    let locations = result.unwrap_or_default();
    let actual_ranges: Vec<Range> = locations.iter().map(|l| l.range).collect();

    assert_eq!(
        actual_ranges.len(),
        fixture.ref_ranges.len(),
        "Expected {} references, got {}.\nExpected: {:?}\nActual: {:?}",
        fixture.ref_ranges.len(),
        actual_ranges.len(),
        fixture.ref_ranges,
        actual_ranges
    );

    for expected in &fixture.ref_ranges {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected reference at {:?} not found.\nExpected: {:?}\nActual: {:?}",
            expected,
            fixture.ref_ranges,
            actual_ranges
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_references_class() {
        check_references(
            r#"
class <ref>Foo$0</ref>
end

<ref>Foo</ref>.new
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_references_constant() {
        check_references(
            r#"
module MyMod
  VALUE = 42
end

puts <ref>MyMod::VALUE$0</ref>
x = <ref>MyMod::VALUE</ref>
"#,
        )
        .await;
    }
}
