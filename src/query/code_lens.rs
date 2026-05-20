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
use ruby_prism::{ModuleNode, Node, Visit};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyConstant;

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
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let mut collector = CodeLensCollector::new();
        collector.visit(&root);

        if collector.modules.is_empty() {
            return Vec::new();
        }

        // 2. We need offset→position conversion. Use attached document.
        let doc_arc = self
            .doc()
            .expect("INVARIANT VIOLATED: get_code_lenses requires a document via with_doc_and_engine(). Fix: call EngineQuery::with_doc_and_engine() before get_code_lenses()");
        let document = doc_arc.read();

        let mut results = Vec::new();

        for (fqn, start_offset, end_offset) in &collector.modules {
            let engine_ref = self.analysis_engine().expect(
                "INVARIANT VIOLATED: code lens query requires analysis engine. \
                 This is a bug because module usage lenses are derived from graph facts. \
                 Fix: construct EngineQuery with with_doc_and_engine().",
            );
            let engine = engine_ref.lock();
            let query = AnalysisQuery::new(&engine);
            let usages = mixin_usages_from_analysis(&query, &engine, fqn);
            let class_locations = class_definition_locations_from_analysis(&query, &engine, fqn);

            if usages.is_empty() && class_locations.is_empty() {
                debug!("No usages or classes found for module: {:?}", fqn);
                continue;
            }

            // Convert byte offsets to LSP positions.
            let start_position = document.offset_to_position(*start_offset);
            let end_position = document.offset_to_position(*end_offset);
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

// ============================================================================
// Internal AST collector
// ============================================================================

/// Walks the AST and collects `(FullyQualifiedName, start_offset, end_offset)`
/// for every `module` definition.
struct CodeLensCollector {
    modules: Vec<(FullyQualifiedName, usize, usize)>,
    namespace_stack: Vec<String>,
}

impl CodeLensCollector {
    fn new() -> Self {
        Self {
            modules: Vec::new(),
            namespace_stack: Vec::new(),
        }
    }

    /// Build an FQN from the current namespace stack plus a module name.
    fn compute_fqn(&self, module_name: &str) -> Option<FullyQualifiedName> {
        let mut constants = Vec::new();

        for part in &self.namespace_stack {
            match RubyConstant::new(part) {
                Ok(c) => constants.push(c),
                Err(_) => return None,
            }
        }

        for part in module_name.split("::") {
            match RubyConstant::new(part) {
                Ok(c) => constants.push(c),
                Err(_) => return None,
            }
        }

        Some(FullyQualifiedName::from(constants))
    }

    /// Extract the constant name from a node (handles both simple and namespaced).
    fn extract_constant_name(&self, node: &Node) -> String {
        if let Some(constant_read) = node.as_constant_read_node() {
            String::from_utf8_lossy(constant_read.name().as_slice()).to_string()
        } else if node.as_constant_path_node().is_some() {
            let mut parts = Vec::new();
            self.collect_constant_path_parts(node, &mut parts);
            parts.join("::")
        } else {
            String::new()
        }
    }

    /// Recursively collect parts of a constant path (e.g. A::B → ["A", "B"]).
    fn collect_constant_path_parts(&self, node: &Node, parts: &mut Vec<String>) {
        if let Some(constant_path) = node.as_constant_path_node() {
            if let Some(parent) = constant_path.parent() {
                self.collect_constant_path_parts(&parent, parts);
            }
            if let Some(name_bytes) = constant_path.name() {
                parts.push(String::from_utf8_lossy(name_bytes.as_slice()).to_string());
            }
        } else if let Some(constant_read) = node.as_constant_read_node() {
            parts.push(String::from_utf8_lossy(constant_read.name().as_slice()).to_string());
        }
    }
}

impl Visit<'_> for CodeLensCollector {
    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        let constant_path = node.constant_path();
        let module_name = self.extract_constant_name(&constant_path);

        if !module_name.is_empty() {
            if let Some(fqn) = self.compute_fqn(&module_name) {
                let start_offset = node.location().start_offset();
                let end_offset = constant_path.location().end_offset();
                self.modules.push((fqn, start_offset, end_offset));
            }

            // Push only the last segment for nested namespace resolution.
            let simple_name = module_name.split("::").last().unwrap_or(&module_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        // Visit children.
        if let Some(body) = node.body() {
            self.visit(&body);
        }

        if !module_name.is_empty() {
            self.namespace_stack.pop();
        }
    }

    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'_>) {
        let constant_path = node.constant_path();
        let class_name = self.extract_constant_name(&constant_path);

        if !class_name.is_empty() {
            let simple_name = class_name.split("::").last().unwrap_or(&class_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        if let Some(body) = node.body() {
            self.visit(&body);
        }

        if !class_name.is_empty() {
            self.namespace_stack.pop();
        }
    }
}
