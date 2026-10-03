//! Type hierarchy at the cursor.

use std::collections::BTreeSet;

use tower_lsp::lsp_types::{
    Location, PartialResultParams, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams, WorkDoneProgressParams,
};

use super::position_params;
use crate::features::navigation::type_hierarchy;
use crate::server::Server;
use crate::test::harness::fixture::Tag;

/// `<th supertypes="A,B" subtypes="C">`: each listed direction returns exactly those names.
pub(super) async fn check_type_hierarchy(server: &Server, cursor: &Location, tag: &Tag) {
    assert!(
        tag.attr("supertypes").is_some() || tag.attr("subtypes").is_some(),
        "<th> needs `supertypes` or `subtypes`"
    );
    let items = type_hierarchy::handle_prepare(
        server,
        TypeHierarchyPrepareParams {
            text_document_position_params: position_params(&cursor.uri, cursor.range.start),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
    .await
    .expect("prepare type hierarchy request failed")
    .unwrap_or_default();
    assert_eq!(
        items.len(),
        1,
        "prepareTypeHierarchy at {:?} must return one item, got {items:?}",
        cursor.range.start
    );
    let item = items[0].clone();

    if tag.attr("supertypes").is_some() {
        let supertypes = type_hierarchy::handle_supertypes(
            server,
            TypeHierarchySupertypesParams {
                item: item.clone(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        .expect("type hierarchy request failed")
        .unwrap_or_default();
        assert_names(
            "supertypes",
            &item.name,
            &tag.list("supertypes"),
            supertypes.iter().map(|s| s.name.as_str()),
        );
    }
    if tag.attr("subtypes").is_some() {
        let subtypes = type_hierarchy::handle_subtypes(
            server,
            TypeHierarchySubtypesParams {
                item: item.clone(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        .expect("type hierarchy request failed")
        .unwrap_or_default();
        assert_names(
            "subtypes",
            &item.name,
            &tag.list("subtypes"),
            subtypes.iter().map(|s| s.name.as_str()),
        );
    }
}

fn assert_names<'a>(
    direction: &str,
    item: &str,
    expected: &[&str],
    actual: impl Iterator<Item = &'a str>,
) {
    let actual: Vec<&str> = actual.collect();
    let expected_set: BTreeSet<&str> = expected.iter().copied().collect();
    let actual_set: BTreeSet<&str> = actual.iter().copied().collect();
    assert!(
        expected_set == actual_set && actual.len() == actual_set.len(),
        "{direction} of `{item}` mismatch.\nExpected: {expected_set:?}\nActual:   {actual:?}"
    );
}
