use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::engine::View;

use super::MethodLookupReceiver;
use tower_lsp::lsp_types::Location;

pub(super) fn definitions(
    view: &View<'_>,
    receiver: &MethodLookupReceiver,
    method: &RubyMethod,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let allow_private = protected_caller.is_none();
    let ranges = match receiver {
        MethodLookupReceiver::Namespace(owner) => {
            view.method_definition_ranges(owner, method, allow_private, protected_caller)
        }
        MethodLookupReceiver::Type(receiver_type) => view.method_definition_ranges_for_type(
            receiver_type,
            method,
            allow_private,
            protected_caller,
        ),
        MethodLookupReceiver::Super(owner) => view.super_definition_ranges(owner, method),
    }?;
    non_empty_locations(locations_for_ranges(view, ranges))
}
