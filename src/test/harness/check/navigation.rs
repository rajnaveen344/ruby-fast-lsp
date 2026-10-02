//! Definition, references, implementation, call hierarchy, and rename.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCallsParams, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    GotoDefinitionParams, GotoDefinitionResponse, Location, PartialResultParams, ReferenceContext,
    ReferenceParams, RenameParams, Url, WorkDoneProgressParams,
};

use super::{assert_same_locations, position_params};
use crate::features::editing::rename;
use crate::features::navigation::{call_hierarchy, definition, implementation, references};
use crate::server::RubyLanguageServer;
use crate::test::harness::fixture::{Tag, TagKind};

/// `<def>`, `<ref>`, `<impl>`: the request at the cursor returns exactly the tagged ranges.
pub(super) async fn check_locations(
    server: &RubyLanguageServer,
    cursor: &Location,
    kind: TagKind,
    expected: &[Location],
) {
    let params = || GotoDefinitionParams {
        text_document_position_params: position_params(&cursor.uri, cursor.range.start),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let actual = match kind {
        TagKind::Def => goto_locations(
            definition::handle(server, params())
                .await
                .expect("definition request failed"),
        ),
        TagKind::Impl => goto_locations(
            implementation::handle(server, params())
                .await
                .expect("implementation request failed"),
        ),
        TagKind::Ref => references::handle(
            server,
            ReferenceParams {
                text_document_position: position_params(&cursor.uri, cursor.range.start),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: ReferenceContext {
                    include_declaration: true,
                },
            },
        )
        .await
        .expect("references request failed")
        .unwrap_or_default(),
        other => unreachable!("{other:?} is not a location request"),
    };
    assert_same_locations(&format!("{kind:?} locations"), actual, expected.to_vec());
}

fn goto_locations(response: Option<GotoDefinitionResponse>) -> Vec<Location> {
    match response {
        Some(GotoDefinitionResponse::Scalar(location)) => vec![location],
        Some(GotoDefinitionResponse::Array(locations)) => locations,
        Some(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|link| Location::new(link.target_uri, link.target_selection_range))
            .collect(),
        None => Vec::new(),
    }
}

/// `<incoming>`, `<outgoing>`: the hierarchy of the item at the cursor contains
/// exactly the tagged caller/callee definitions.
pub(super) async fn check_calls(
    server: &RubyLanguageServer,
    cursor: &Location,
    kind: TagKind,
    expected: &[Location],
) {
    let items = call_hierarchy::handle_prepare(
        server,
        CallHierarchyPrepareParams {
            text_document_position_params: position_params(&cursor.uri, cursor.range.start),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
    .await
    .expect("prepare call hierarchy request failed")
    .unwrap_or_default();
    assert_eq!(
        items.len(),
        1,
        "prepareCallHierarchy at {:?} must return one item, got {items:?}",
        cursor.range.start
    );
    let item = items[0].clone();
    let actual: Vec<Location> = match kind {
        TagKind::Incoming => call_hierarchy::handle_incoming(
            server,
            CallHierarchyIncomingCallsParams {
                item,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        .expect("incoming calls request failed")
        .unwrap_or_default()
        .into_iter()
        .map(|call| Location::new(call.from.uri, call.from.range))
        .collect(),
        TagKind::Outgoing => call_hierarchy::handle_outgoing(
            server,
            CallHierarchyOutgoingCallsParams {
                item,
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        .expect("outgoing calls request failed")
        .unwrap_or_default()
        .into_iter()
        .map(|call| Location::new(call.to.uri, call.to.range))
        .collect(),
        other => unreachable!("{other:?} is not a call hierarchy request"),
    };
    assert_same_locations(&format!("{kind:?} calls"), actual, expected.to_vec());
}

/// `<rename to="new">` marks where rename is requested; it and every bare
/// `<rename>` tag are exactly the edited ranges, each replaced by `new`.
pub(super) async fn check_rename(server: &RubyLanguageServer, tags: &[(&Url, &Tag)]) {
    let mut requests = tags
        .iter()
        .filter_map(|(uri, tag)| Some((*uri, tag, tag.attr("to")?)));
    let (uri, request_tag, new_name) = requests
        .next()
        .expect("<rename> tags need exactly one tag with a `to` attribute");
    assert!(
        requests.next().is_none(),
        "<rename> tags need exactly one tag with a `to` attribute"
    );

    let edit = rename::handle(
        server,
        RenameParams {
            text_document_position: position_params(uri, request_tag.range.start),
            new_name: new_name.to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
    .await
    .expect("rename request failed")
    .unwrap_or_else(|| {
        panic!(
            "rename at {:?} to `{new_name}` returned no edit",
            request_tag.range.start
        )
    });
    let changes: HashMap<Url, Vec<_>> = edit.changes.expect("rename must return `changes`");

    let mut actual = Vec::new();
    for (edit_uri, edits) in changes {
        for text_edit in edits {
            assert_eq!(
                text_edit.new_text, new_name,
                "rename edit at {:?} must insert the new name",
                text_edit.range
            );
            actual.push(Location::new(edit_uri.clone(), text_edit.range));
        }
    }
    let expected = tags
        .iter()
        .map(|(uri, tag)| Location::new((*uri).clone(), tag.range))
        .collect();
    assert_same_locations("rename edits", actual, expected);
}
