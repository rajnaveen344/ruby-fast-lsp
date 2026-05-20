//! Call Hierarchy Query — Resolves incoming/outgoing calls from analysis facts.
//!
//! Implements the LSP Call Hierarchy feature for Ruby methods:
//! 1. `prepare` — Find the method at cursor position
//! 2. `incoming_calls` — Who calls this method?
//! 3. `outgoing_calls` — What does this method call?
//!
//! Call site data is stored at index time: each ReferenceFact records the FQN
//! of the enclosing method (the caller). This makes both incoming and outgoing
//! calls simple grouping operations on existing analysis data.

use log::info;
use ruby_analysis_engine::{AnalysisQuery, CallHierarchyMethod};
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, SymbolKind,
    SymbolTag, Url,
};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::analysis_location::location_for_range;
use super::EngineQuery;

// ============================================================================
// Data Structures
// ============================================================================

/// Data stored in CallHierarchyItem.data to identify the item for follow-up requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyData {
    /// The fully qualified name of the method (e.g., "Foo::Bar#baz")
    pub fqn: String,
}

// ============================================================================
// EngineQuery entry points
// ============================================================================

impl EngineQuery {
    /// Find the method at the cursor position and return a CallHierarchyItem.
    pub fn prepare_call_hierarchy(
        &self,
        uri: &Url,
        position: Position,
        content: String,
    ) -> Option<Vec<CallHierarchyItem>> {
        info!(
            "Prepare call hierarchy request for {:?} at {:?}",
            uri.path(),
            position
        );

        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content);
        let (identifier, _, ancestors, _scope_id, namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = identifier?;

        match &identifier {
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => {
                let owner_fqn = self.resolve_receiver_to_namespace(
                    receiver,
                    &ancestors,
                    namespace_kind,
                    position,
                )?;
                let method_fqn =
                    FullyQualifiedName::method(owner_fqn.namespace_parts().to_vec(), *iden);

                let engine_ref = self.analysis_engine().expect(
                    "INVARIANT VIOLATED: call hierarchy prepare requires an analysis engine. \
                     This is a bug because LSP callHierarchy should be a thin wrapper over AnalysisEngine. \
                     Fix: construct EngineQuery with with_engine().",
                );
                let engine = engine_ref.lock();
                let query = AnalysisQuery::new(&engine);
                if let Some(item) = query
                    .call_hierarchy_method(&method_fqn)
                    .and_then(|method| call_hierarchy_item_from_engine_method(&engine, method))
                {
                    return Some(vec![item]);
                }
                None
            }
            _ => {
                info!(
                    "Call hierarchy only supports methods, got: {:?}",
                    identifier
                );
                None
            }
        }
    }

    /// Get all methods that call the given method.
    pub fn get_incoming_calls(
        &self,
        data: &CallHierarchyData,
    ) -> Option<Vec<CallHierarchyIncomingCall>> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: incoming call hierarchy requires an analysis engine. \
             This is a bug because LSP callHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        let method_fqn = query.parse_method_fqn(&data.fqn)?;
        Some(
            query
                .incoming_calls(&method_fqn)
                .into_iter()
                .filter_map(|call| {
                    Some(CallHierarchyIncomingCall {
                        from: call_hierarchy_item_from_engine_method(&engine, call.from)?,
                        from_ranges: call
                            .from_ranges
                            .into_iter()
                            .filter_map(|range| location_for_range(&engine, range).map(|l| l.range))
                            .collect(),
                    })
                })
                .collect(),
        )
    }

    /// Get all methods called by the given method.
    pub fn get_outgoing_calls(
        &self,
        data: &CallHierarchyData,
    ) -> Option<Vec<CallHierarchyOutgoingCall>> {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: outgoing call hierarchy requires an analysis engine. \
             This is a bug because LSP callHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        let method_fqn = query.parse_method_fqn(&data.fqn)?;
        Some(
            query
                .outgoing_calls(&method_fqn)
                .into_iter()
                .filter_map(|call| {
                    Some(CallHierarchyOutgoingCall {
                        to: call_hierarchy_item_from_engine_method(&engine, call.to)?,
                        from_ranges: call
                            .from_ranges
                            .into_iter()
                            .filter_map(|range| location_for_range(&engine, range).map(|l| l.range))
                            .collect(),
                    })
                })
                .collect(),
        )
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn call_hierarchy_item_from_engine_method(
    engine: &ruby_analysis_engine::AnalysisEngine,
    method: CallHierarchyMethod,
) -> Option<CallHierarchyItem> {
    let location = location_for_range(engine, method.range)?;
    Some(CallHierarchyItem {
        name: method.fqn.name(),
        kind: SymbolKind::METHOD,
        tags: Some(Vec::<SymbolTag>::new()),
        detail: Some(method.fqn.to_string()),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        data: Some(
            serde_json::to_value(CallHierarchyData {
                fqn: method.fqn.to_string(),
            })
            .ok()?,
        ),
    })
}
