use tower_lsp::lsp_types::Location;

use crate::query::analysis_location::location_for_range;
use crate::query::IndexQuery;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

use super::ResolvedMethodCallee;

pub(super) fn resolve_method_callees(
    query: &IndexQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let analysis_query = ruby_analysis_engine::AnalysisQuery::new(&engine);
    let callees = analysis_query.resolve_method_callees(namespace_fqn, method)?;

    Some(
        callees
            .into_iter()
            .map(|callee| ResolvedMethodCallee {
                owner: callee.owner,
                method: callee.method,
                resolution: callee.resolution,
                definition_locations: callee
                    .definition_ranges
                    .into_iter()
                    .filter_map(|range| location_for_range(&engine, range))
                    .collect(),
            })
            .collect(),
    )
}

pub(super) fn resolve_constant_receiver(
    query: &IndexQuery,
    path: &[RubyConstant],
    current_namespace: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let analysis_query = ruby_analysis_engine::AnalysisQuery::new(&engine);
    Some(analysis_query.resolve_constant_receiver(path, current_namespace))
}

pub(super) fn method_locations(
    query: &IndexQuery,
    method_fqn: &FullyQualifiedName,
    ancestor_chain: &[FullyQualifiedName],
) -> Option<Vec<Location>> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let locations = engine
        .method_facts_for(method_fqn)
        .iter()
        .filter(|fact| {
            ancestor_chain.iter().any(|ancestor| {
                ancestor.namespace_parts() == fact.owner.namespace_parts()
                    && ancestor.namespace_kind() == fact.owner.namespace_kind()
            })
        })
        .filter_map(|fact| location_for_range(&engine, fact.range))
        .collect::<Vec<_>>();

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}
