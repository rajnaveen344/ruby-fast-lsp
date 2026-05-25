use crate::query::analysis_location::locations_for_ranges;
use crate::query::EngineQuery;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;

use super::ResolvedMethodCallee;

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
    let engine = engine.lock();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let callees = if allow_private {
        analysis_query.resolve_method_callees(namespace_fqn, method)?
    } else if let Some(caller) = protected_caller {
        analysis_query.resolve_protected_method_callees(namespace_fqn, method, caller)?
    } else {
        analysis_query.resolve_public_method_callees(namespace_fqn, method)?
    };

    Some(
        callees
            .into_iter()
            .map(|callee| ResolvedMethodCallee {
                owner: callee.owner,
                method: callee.method,
                resolution: callee.resolution,
                definition_locations: locations_for_ranges(&engine, callee.definition_ranges),
            })
            .collect(),
    )
}

pub(super) fn resolve_super_method_callee(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<ResolvedMethodCallee> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
    let callee = analysis_query.resolve_super_method_callee(namespace_fqn, method)?;

    Some(ResolvedMethodCallee {
        owner: callee.owner,
        method: callee.method,
        resolution: callee.resolution,
        definition_locations: locations_for_ranges(&engine, callee.definition_ranges),
    })
}
