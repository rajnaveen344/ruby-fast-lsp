//! Completion at the cursor.

use tower_lsp::lsp_types::{
    CompletionParams, CompletionResponse, Location, PartialResultParams, WorkDoneProgressParams,
};

use super::position_params;
use crate::features::editing::completion;
use crate::server::Server;
use crate::test::harness::fixture::Tag;

/// `<complete items="a,b" excludes="c">`: labels `a` and `b` are offered and
/// `c` is not. Labels compare exactly.
pub(super) async fn check_completion(server: &Server, cursor: &Location, tag: &Tag) {
    let items = tag.list("items");
    let excludes = tag.list("excludes");
    assert!(
        !items.is_empty() || !excludes.is_empty(),
        "<complete> needs `items` or `excludes`"
    );
    let response = completion::handle(
        server,
        CompletionParams {
            text_document_position: position_params(&cursor.uri, cursor.range.start),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        },
    )
    .await
    .expect("completion request failed");
    let offered = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => Vec::new(),
    };
    let labels: Vec<&str> = offered.iter().map(|item| item.label.as_str()).collect();

    for expected in &items {
        assert!(
            labels.contains(expected),
            "completion `{expected}` missing at {:?}.\nOffered: {labels:?}",
            cursor.range.start
        );
    }
    for excluded in &excludes {
        assert!(
            !labels.contains(excluded),
            "completion `{excluded}` must not be offered at {:?}.\nOffered: {labels:?}",
            cursor.range.start
        );
    }
}
