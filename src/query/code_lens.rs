//! Code Lens Query — adapts engine mixin data to LSP code lens data.
//!
//! For each `module` definition in the file, this queries the engine for:
//! - Mixin usages (include, prepend, extend)
//! - Class definitions that include the module
//!
//! The capability handler converts `CodeLensData` → LSP `CodeLens`.

use std::collections::HashMap;

use log::debug;
use ruby_analysis::engine::{AnalysisQuery, MixinUsageKind};
use ruby_analysis::indexer::module_definitions_for_lens;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use ruby_analysis::core::FullyQualifiedName;

use super::analysis_location::location_for_range;
use super::EngineQuery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MixinType {
    Include,
    Prepend,
    Extend,
}

// ============================================================================
// Public data type
// ============================================================================

/// Domain result for a single code lens item.
///
/// LSP-agnostic: holds the data needed to build a `CodeLens`, but the final
/// `Command` construction happens in the capability wrapper.
pub struct CodeLensData {
    /// LSP range covering the `module` keyword through the constant name.
    pub range: Range,
    /// Human-readable title, e.g. "2 include", "1 class".
    pub title: String,
    /// VS Code command id, e.g. "ruby-fast-lsp.showReferences".
    pub command: String,
    /// Document URI (needed for command arguments).
    pub uri: Url,
    /// Position for the showReferences command.
    pub target_position: Position,
    /// Reference locations to display.
    pub locations: Vec<Location>,
}

// ============================================================================
// EngineQuery entry point
// ============================================================================

impl EngineQuery {
    /// Compute code lenses for every `module` definition in the file.
    ///
    /// Returns one `CodeLensData` per mixin-type bucket and one for classes,
    /// for every module that has at least one usage.
    pub fn get_code_lenses(&self, uri: &Url, content: &str) -> Vec<CodeLensData> {
        // 1. Parse AST and collect (FQN, start_offset, end_offset) for each module.
        let modules = module_definitions_for_lens(content);

        if modules.is_empty() {
            return Vec::new();
        }

        // 2. We need offset→position conversion. Use attached document.
        let doc_arc = self
            .doc()
            .expect("INVARIANT VIOLATED: get_code_lenses requires a document via with_doc_and_engine(). Fix: call EngineQuery::with_doc_and_engine() before get_code_lenses()");
        let document = doc_arc.read();

        let mut results = Vec::new();

        for module in &modules {
            let engine_ref = self.analysis_engine().expect(
                "INVARIANT VIOLATED: code lens query requires analysis engine. \
                 This is a bug because module usage lenses are derived from graph facts. \
                 Fix: construct EngineQuery with with_doc_and_engine().",
            );
            let engine = engine_ref.lock();
            let query = AnalysisQuery::new(&engine);
            let usages = mixin_usages_from_analysis(&query, &engine, &module.fqn);
            let class_locations =
                class_definition_locations_from_analysis(&query, &engine, &module.fqn);

            if usages.is_empty() && class_locations.is_empty() {
                debug!("No usages or classes found for module: {:?}", module.fqn);
                continue;
            }

            // Convert byte offsets to LSP positions.
            let start_position = document.offset_to_position(module.start_offset);
            let end_position = document.offset_to_position(module.end_offset);
            let range = Range {
                start: start_position,
                end: end_position,
            };

            // Group mixin usages by type.
            let mut usages_by_type: HashMap<MixinType, Vec<Location>> = HashMap::new();
            for (mixin_type, location) in usages {
                usages_by_type.entry(mixin_type).or_default().push(location);
            }

            // One CodeLensData per mixin type.
            let mixin_types = [
                (MixinType::Include, "include"),
                (MixinType::Prepend, "prepend"),
                (MixinType::Extend, "extend"),
            ];

            for (mixin_type, type_name) in &mixin_types {
                if let Some(locations) = usages_by_type.get(mixin_type) {
                    results.push(CodeLensData {
                        range,
                        title: format!("{} {}", locations.len(), type_name),
                        command: "ruby-fast-lsp.showReferences".to_string(),
                        uri: uri.clone(),
                        target_position: start_position,
                        locations: locations.clone(),
                    });
                }
            }

            // One CodeLensData for classes.
            if !class_locations.is_empty() {
                let count = class_locations.len();
                results.push(CodeLensData {
                    range,
                    title: format!("{} {}", count, if count == 1 { "class" } else { "classes" }),
                    command: "ruby-fast-lsp.showReferences".to_string(),
                    uri: uri.clone(),
                    target_position: start_position,
                    locations: class_locations,
                });
            }
        }

        results
    }
}

fn mixin_usages_from_analysis(
    query: &AnalysisQuery<'_>,
    engine: &ruby_analysis::engine::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<(MixinType, Location)> {
    let mut usages = query
        .module_mixin_usages(module_fqn)
        .into_iter()
        .filter_map(|usage| {
            let mixin_type = mixin_type_from_usage_kind(usage.kind);
            let location = location_for_range(engine, usage.range)?;
            Some((mixin_type, location))
        })
        .collect::<Vec<_>>();
    usages.sort_by_key(|(mixin_type, location)| {
        (
            mixin_type_sort_key(*mixin_type),
            location.uri.to_string(),
            location.range.start.line,
            location.range.start.character,
        )
    });
    usages
}

fn class_definition_locations_from_analysis(
    query: &AnalysisQuery<'_>,
    engine: &ruby_analysis::engine::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<Location> {
    let mut result = query
        .module_including_class_definition_ranges(module_fqn)
        .into_iter()
        .filter_map(|range| location_for_range(engine, range))
        .collect::<Vec<_>>();
    result.sort_by_key(|location| {
        (
            location.uri.to_string(),
            location.range.start.line,
            location.range.start.character,
        )
    });
    result.dedup_by(|left, right| {
        left.uri == right.uri
            && left.range.start == right.range.start
            && left.range.end == right.range.end
    });
    result
}

fn mixin_type_from_usage_kind(kind: MixinUsageKind) -> MixinType {
    match kind {
        MixinUsageKind::Include => MixinType::Include,
        MixinUsageKind::Prepend => MixinType::Prepend,
        MixinUsageKind::Extend => MixinType::Extend,
    }
}

fn mixin_type_sort_key(mixin_type: MixinType) -> u8 {
    match mixin_type {
        MixinType::Include => 0,
        MixinType::Prepend => 1,
        MixinType::Extend => 2,
    }
}
