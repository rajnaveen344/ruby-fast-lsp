use crate::query::analysis_location::locations_for_ranges;
use crate::query::EngineQuery;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::core::RubyType;

use super::{MethodLookupReceiver, ResolvedMethodCallee};
use tower_lsp::lsp_types::Location;

pub(super) fn resolve_method_callees(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    resolve_method_callees_with_private(query, namespace_fqn, method, true, None)
}

pub(super) fn resolve_public_method_callees(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    resolve_method_callees_with_private(query, namespace_fqn, method, false, None)
}

pub(super) fn resolve_protected_method_callees(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    caller_namespace_fqn: &FullyQualifiedName,
) -> Option<Vec<ResolvedMethodCallee>> {
    resolve_method_callees_with_private(
        query,
        namespace_fqn,
        method,
        false,
        Some(caller_namespace_fqn),
    )
}

fn resolve_method_callees_with_private(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?;
    let engine = engine.read();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let callees = if allow_private {
        analysis_query.resolve_method_callees(namespace_fqn, method)?
    } else if let Some(caller) = protected_caller {
        analysis_query.resolve_protected_method_callees(namespace_fqn, method, caller)?
    } else {
        analysis_query.resolve_public_method_callees(namespace_fqn, method)?
    };

    Some(callees)
}

pub(super) fn resolve_method_callees_for_type(
    query: &EngineQuery,
    receiver_type: &RubyType,
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?;
    let engine = engine.read();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let callees = if allow_private {
        analysis_query.resolve_method_callees_for_type(receiver_type, method)?
    } else if let Some(caller) = protected_caller {
        analysis_query.resolve_protected_method_callees_for_type(receiver_type, method, caller)?
    } else {
        analysis_query.resolve_public_method_callees_for_type(receiver_type, method)?
    };

    Some(callees)
}

pub(super) fn resolve_super_method_callee(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<ResolvedMethodCallee> {
    let engine = query.analysis_engine()?;
    let engine = engine.read();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    analysis_query.resolve_super_method_callee(namespace_fqn, method)
}

pub(super) fn find_method_definitions(
    query: &EngineQuery,
    receiver: &MethodLookupReceiver,
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let engine = query.analysis_engine()?.read();
    let analysis = engine.query();
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
    crate::query::analysis_location::non_empty_locations(locations_for_ranges(&engine, ranges))
}
