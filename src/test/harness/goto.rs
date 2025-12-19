//! Goto definition check function.

use tower_lsp::lsp_types::{
    GotoDefinitionParams, Location, PartialResultParams, Range, TextDocumentIdentifier,
    TextDocumentPositionParams, WorkDoneProgressParams,
};

use super::fixture::{parse_fixture, ranges_match, setup_with_fixture};
use crate::handlers::request;

/// Check that goto definition at the cursor position finds all expected definitions.
///
/// # Markers
/// - `$0` - cursor position (where the user "clicks")
/// - `<def>...</def>` - expected definition range(s), must match exactly
///
/// # Example
///
/// ```ignore
/// check_goto(r#"
/// <def>class Foo
/// end</def>
///
/// Foo$0.new
/// "#).await;
/// ```
pub async fn check_goto(fixture_text: &str) {
    let fixture = parse_fixture(fixture_text);
    let (server, uri) = setup_with_fixture(&fixture.content).await;

    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: fixture.cursor,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = request::handle_goto_definition(&server, params)
        .await
        .expect("Goto definition request failed");

    let locations: Vec<Location> = match result {
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(loc)) => vec![loc],
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Array(locs)) => locs,
        Some(tower_lsp::lsp_types::GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|l| Location {
                uri: l.target_uri,
                range: l.target_selection_range,
            })
            .collect(),
        None => vec![],
    };

    let actual_ranges: Vec<Range> = locations.iter().map(|l| l.range).collect();

    assert_eq!(
        actual_ranges.len(),
        fixture.def_ranges.len(),
        "Expected {} definitions, got {}.\nExpected: {:?}\nActual: {:?}",
        fixture.def_ranges.len(),
        actual_ranges.len(),
        fixture.def_ranges,
        actual_ranges
    );

    for expected in &fixture.def_ranges {
        assert!(
            actual_ranges.iter().any(|r| ranges_match(r, expected)),
            "Expected definition at {:?} not found.\nExpected: {:?}\nActual: {:?}",
            expected,
            fixture.def_ranges,
            actual_ranges
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_goto_class_definition() {
        check_goto(
            r#"
<def>class Foo
end</def>

Foo$0.new
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_goto_method_definition() {
        check_goto(
            r#"
class Foo
  def <def>greet</def>
  end
end

Foo.new.greet$0
"#,
        )
        .await;
    }

    #[tokio::test]
    async fn test_goto_constant_definition() {
        check_goto(
            r#"
module MyMod
  <def>VALUE = 42</def>
end

MyMod::VALUE$0
"#,
        )
        .await;
    }
}
