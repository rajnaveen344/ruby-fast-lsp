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
use ruby_analysis_engine::AnalysisEngine;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, SymbolTag, Url,
};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

use super::analysis_location::location_for_range;
use super::IndexQuery;

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
// IndexQuery entry points
// ============================================================================

impl IndexQuery {
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

                if let Some(engine_ref) = self.analysis_engine() {
                    let engine = engine_ref.lock();
                    if let Some(item) =
                        build_call_hierarchy_item_from_analysis(&engine, &method_fqn)
                    {
                        return Some(vec![item]);
                    }
                }

                let index = self.index.lock();
                build_call_hierarchy_item_from_index(&index, &method_fqn).map(|item| vec![item])
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
        let method_fqn = parse_method_fqn_string(&data.fqn)?;

        if let Some(engine_ref) = self.analysis_engine() {
            let engine = engine_ref.lock();
            return Some(incoming_calls_from_analysis(&engine, &method_fqn));
        }

        let index = self.index.lock();

        let calls = index.incoming_calls(&method_fqn);
        if calls.is_empty() {
            return Some(vec![]);
        }

        let mut results = Vec::with_capacity(calls.len());
        for (caller_fqn, from_ranges) in calls {
            if let Some(item) = build_call_hierarchy_item_from_index(&index, &caller_fqn) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges,
                });
            }
        }

        Some(results)
    }

    /// Get all methods called by the given method.
    pub fn get_outgoing_calls(
        &self,
        data: &CallHierarchyData,
    ) -> Option<Vec<CallHierarchyOutgoingCall>> {
        let method_fqn = parse_method_fqn_string(&data.fqn)?;

        if let Some(engine_ref) = self.analysis_engine() {
            let engine = engine_ref.lock();
            return Some(outgoing_calls_from_analysis(&engine, &method_fqn));
        }

        let index = self.index.lock();

        let calls = index.outgoing_calls(&method_fqn);
        if calls.is_empty() {
            return Some(vec![]);
        }

        let mut results = Vec::with_capacity(calls.len());
        for (callee_fqn, from_ranges) in calls {
            if let Some(item) = build_call_hierarchy_item_from_index(&index, &callee_fqn) {
                results.push(CallHierarchyOutgoingCall {
                    to: item,
                    from_ranges,
                });
            }
        }

        Some(results)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Build a CallHierarchyItem from a method FQN by looking up its definition in the index.
fn build_call_hierarchy_item_from_index(
    index: &RubyIndex,
    method_fqn: &FullyQualifiedName,
) -> Option<CallHierarchyItem> {
    let entries = index.get(method_fqn)?;
    let entry = entries
        .iter()
        .find(|e| matches!(e.kind, EntryKind::Method(_)))?;

    let location = index.to_lsp_location(&entry.location)?;

    Some(CallHierarchyItem {
        name: method_fqn.name(),
        kind: SymbolKind::METHOD,
        tags: Some(Vec::<SymbolTag>::new()),
        detail: Some(method_fqn.to_string()),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        data: Some(
            serde_json::to_value(CallHierarchyData {
                fqn: method_fqn.to_string(),
            })
            .ok()?,
        ),
    })
}

fn build_call_hierarchy_item_from_analysis(
    engine: &AnalysisEngine,
    method_fqn: &FullyQualifiedName,
) -> Option<CallHierarchyItem> {
    let fact = engine.method_facts_for(method_fqn).first()?;
    let location = location_for_range(engine, fact.range)?;
    Some(call_hierarchy_item(
        method_fqn,
        location.uri,
        location.range,
    )?)
}

fn call_hierarchy_item(
    method_fqn: &FullyQualifiedName,
    uri: Url,
    range: Range,
) -> Option<CallHierarchyItem> {
    Some(CallHierarchyItem {
        name: method_fqn.name(),
        kind: SymbolKind::METHOD,
        tags: Some(Vec::<SymbolTag>::new()),
        detail: Some(method_fqn.to_string()),
        uri,
        range,
        selection_range: range,
        data: Some(
            serde_json::to_value(CallHierarchyData {
                fqn: method_fqn.to_string(),
            })
            .ok()?,
        ),
    })
}

fn incoming_calls_from_analysis(
    engine: &AnalysisEngine,
    method_fqn: &FullyQualifiedName,
) -> Vec<CallHierarchyIncomingCall> {
    let mut grouped: Vec<(FullyQualifiedName, Vec<Range>)> = Vec::new();
    for fact in engine.reference_facts_for(method_fqn) {
        let Some(caller) = &fact.caller else {
            continue;
        };
        let Some(location) = location_for_range(engine, fact.range) else {
            continue;
        };
        push_grouped_range(&mut grouped, caller.clone(), location.range);
    }

    grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
    grouped
        .into_iter()
        .filter_map(|(caller_fqn, from_ranges)| {
            Some(CallHierarchyIncomingCall {
                from: build_call_hierarchy_item_from_analysis(engine, &caller_fqn)?,
                from_ranges,
            })
        })
        .collect()
}

fn outgoing_calls_from_analysis(
    engine: &AnalysisEngine,
    method_fqn: &FullyQualifiedName,
) -> Vec<CallHierarchyOutgoingCall> {
    let mut grouped: Vec<(FullyQualifiedName, Vec<Range>)> = Vec::new();
    for fact in engine.reference_store().all_facts() {
        if fact.caller.as_ref() != Some(method_fqn) {
            continue;
        }
        let Some(location) = location_for_range(engine, fact.range) else {
            continue;
        };
        push_grouped_range(&mut grouped, fact.target, location.range);
    }

    grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
    grouped
        .into_iter()
        .filter_map(|(callee_fqn, from_ranges)| {
            Some(CallHierarchyOutgoingCall {
                to: build_call_hierarchy_item_from_analysis(engine, &callee_fqn)?,
                from_ranges,
            })
        })
        .collect()
}

fn push_grouped_range(
    grouped: &mut Vec<(FullyQualifiedName, Vec<Range>)>,
    fqn: FullyQualifiedName,
    range: Range,
) {
    if let Some((_, ranges)) = grouped.iter_mut().find(|(existing, _)| *existing == fqn) {
        ranges.push(range);
        return;
    }
    grouped.push((fqn, vec![range]));
}

/// Parse a method FQN string like "Foo::Bar#method_name" back into a FullyQualifiedName.
///
/// Method FQNs are formatted as `Namespace::Path#method_name` by the Display impl.
/// This reverses that format.
fn parse_method_fqn_string(fqn_str: &str) -> Option<FullyQualifiedName> {
    // Split on '#' — left side is namespace, right side is method name
    let (namespace_str, method_str) = fqn_str.rsplit_once('#')?;

    let method = RubyMethod::new(method_str).ok()?;

    let namespace: Vec<RubyConstant> = if namespace_str.is_empty() {
        Vec::new()
    } else {
        namespace_str
            .split("::")
            .map(|p| RubyConstant::new(p))
            .collect::<Result<Vec<_>, _>>()
            .ok()?
    };

    Some(FullyQualifiedName::method(namespace, method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_method_fqn() {
        let fqn = parse_method_fqn_string("Foo#bar").unwrap();
        assert_eq!(fqn.to_string(), "Foo#bar");
    }

    #[test]
    fn parse_namespaced_method_fqn() {
        let fqn = parse_method_fqn_string("Foo::Bar#baz").unwrap();
        assert_eq!(fqn.to_string(), "Foo::Bar#baz");
    }

    #[test]
    fn parse_top_level_method_fqn() {
        let fqn = parse_method_fqn_string("#foo").unwrap();
        assert_eq!(fqn.to_string(), "#foo");
    }

    #[test]
    fn parse_invalid_no_hash() {
        assert!(parse_method_fqn_string("Foo::bar").is_none());
    }
}
