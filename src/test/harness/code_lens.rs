//! Code lens check function using inline markers.
//!
//! Markers:
//! - `<lens title="LABEL">` - expected code lens containing LABEL on this line

use std::sync::Arc;

use parking_lot::RwLock;
use tower_lsp::lsp_types::{
    CodeLens, CodeLensParams, InitializeParams, PartialResultParams, TextDocumentIdentifier, Url,
    WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

use super::fixture::extract_tags_with_attributes;
use crate::capabilities::code_lens::handle_code_lens;
use crate::indexer::file_processor::{FileProcessor, ProcessingOptions};
use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;

/// Check that code lenses match expected markers.
///
/// # Markers
/// - `<lens title="LABEL">` - expected code lens containing LABEL on this line
///
/// # Example
///
/// ```ignore
/// check_code_lens(r#"
/// module MyModule <lens title="1 include">
/// end
///
/// class MyClass
///   include MyModule
/// end
/// "#).await;
/// ```
pub async fn check_code_lens(fixture_text: &str) {
    let (expected_lenses, content) = extract_tags_with_attributes(fixture_text, &["lens"]);
    let lenses = get_code_lenses(&content).await;

    for expected in &expected_lenses {
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

/// Get code lenses for content (no markers).
async fn get_code_lenses(content: &str) -> Vec<CodeLens> {
    let server = RubyLanguageServer::default();
    let _ = server.initialize(InitializeParams::default()).await;

    let uri = Url::parse("file:///test.rb").expect("Invalid URI");

    // Create and register the document
    let document = RubyDocument::new(uri.clone(), content.to_string(), 1);
    server
        .docs
        .lock()
        .insert(uri.clone(), Arc::new(RwLock::new(document)));

    // Index the document
    {
        let indexer = FileProcessor::new(server.index.clone());
        let options = ProcessingOptions {
            index_definitions: true,
            index_references: true,
            resolve_mixins: true,
            include_local_vars: false,
        };
        let _ = indexer.process_file(&uri, content, &server, options);
    }

    let params = CodeLensParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    handle_code_lens(&server, params).await.unwrap_or_default()
}

/// Check that no code lenses are generated.
pub async fn check_no_code_lens(content: &str) {
    let lenses = get_code_lenses(content).await;
    assert!(
        lenses.is_empty(),
        "Expected no code lenses, got: {:?}",
        lenses
            .iter()
            .filter_map(|l| l.command.as_ref().map(|c| &c.title))
            .collect::<Vec<_>>()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_lens_with_marker() {
        check_code_lens(
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
    async fn test_no_usage_no_code_lens() {
        check_no_code_lens(
            r#"
module UnusedModule
end
"#,
        )
        .await;
    }
}
