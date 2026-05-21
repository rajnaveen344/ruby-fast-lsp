use crate::query::analysis_location::location_for_range;
use crate::query::EngineQuery;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;

use super::ResolvedMethodCallee;

pub(super) fn resolve_method_callees(
    query: &EngineQuery,
    namespace_fqn: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<Vec<ResolvedMethodCallee>> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let analysis_query = ruby_analysis::engine::AnalysisQuery::new(&engine);
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
