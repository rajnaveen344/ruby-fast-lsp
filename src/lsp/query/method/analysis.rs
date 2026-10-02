use crate::lsp::query::analysis_location::locations_for_ranges;
use crate::lsp::query::EngineQuery;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};

use super::{MethodLookupReceiver, ResolvedMethodCallee};
use tower_lsp::lsp_types::Location;

pub(super) fn resolve_method_callees(
    query: &EngineQuery,
    receiver: &MethodLookupReceiver,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?.read();
    let receiver = match receiver {
        MethodLookupReceiver::Namespace(owner) => LookupReceiver::Namespace(owner),
        MethodLookupReceiver::Type(receiver_type) => LookupReceiver::Type(receiver_type),
        MethodLookupReceiver::Super(owner) => LookupReceiver::Super { owner },
    };
    lookup::method(
        &engine.view(),
        MethodRequest::new(receiver, *method, MethodWant::Callees),
    )
    .into_callees()
}

pub(super) fn find_method_definitions(
    query: &EngineQuery,
    receiver: &MethodLookupReceiver,
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let engine = query.analysis_engine()?.read();
    let analysis = engine.view();
    let ranges = match receiver {
        MethodLookupReceiver::Namespace(owner) => {
            analysis.method_definition_ranges(owner, method, allow_private, protected_caller)
        }
        MethodLookupReceiver::Type(receiver_type) => analysis.method_definition_ranges_for_type(
            receiver_type,
            method,
            allow_private,
            protected_caller,
        ),
        MethodLookupReceiver::Super(owner) => analysis.super_definition_ranges(owner, method),
    }?;
    crate::lsp::query::analysis_location::non_empty_locations(locations_for_ranges(&engine, ranges))
}
