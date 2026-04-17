use once_cell::unsync::OnceCell;
use ruby_prism::*;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Location, Url};

use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::indexer::index::RubyIndex;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::types::unresolved_index::UnresolvedEntry;

/// Writes produced by the reference visitor during a single file's traversal,
/// deferred so that the visit runs under shared read locks on the index. The
/// file processor flushes these under one brief write lock at end-of-file.
///
/// Phase 2 only writes references + unresolved entries, never definitions —
/// definitions are already stable from Phase 1. So buffering these writes
/// doesn't create visibility issues for the reads that happen during the
/// same visit (those reads only look at definitions).
#[derive(Default)]
pub struct PendingWrites {
    pub references: Vec<(FullyQualifiedName, Location, Option<FullyQualifiedName>)>,
    pub unresolved: Vec<(Url, UnresolvedEntry)>,
}

impl PendingWrites {
    pub fn push_reference(
        &mut self,
        fqn: FullyQualifiedName,
        location: Location,
        caller_fqn: Option<FullyQualifiedName>,
    ) {
        self.references.push((fqn, location, caller_fqn));
    }

    pub fn push_unresolved(&mut self, uri: Url, entry: UnresolvedEntry) {
        self.unresolved.push((uri, entry));
    }

    pub fn flush(self, index: &mut RubyIndex) {
        for (fqn, loc, caller) in self.references {
            index.add_reference(fqn, loc, caller);
        }
        for (uri, entry) in self.unresolved {
            index.add_unresolved_entry(uri, entry);
        }
    }
}

mod block_node;
mod call_node;
mod class_node;
mod constant_path_node;
mod constant_read_node;
mod def_node;
mod local_variable_read_node;
mod module_node;

#[cfg(test)]
mod tests;

pub struct ReferenceVisitor {
    pub index: Index<Unlocked>,
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub include_local_vars: bool,
    /// When true, track unresolved constants in the index for diagnostics
    pub track_unresolved: bool,
    /// Reference / unresolved-entry writes buffered during the visit and
    /// flushed under a single write lock by the file processor. See
    /// [`PendingWrites`] for the rationale.
    pub staged: PendingWrites,
    /// Per-file map of `varname` → inferred type from `varname = Class.new`
    /// assignments. Built lazily on first lookup via a single pass over the
    /// file content — 22k-line megafiles (goshposh/lib/platform/commerce.rb)
    /// otherwise re-scan the file once per distinct variable referenced,
    /// collapsing that to one scan total.
    pub variable_types: OnceCell<HashMap<String, RubyType>>,
}

impl ReferenceVisitor {
    pub fn new(index: Index<Unlocked>, document: RubyDocument) -> Self {
        Self::with_options(index, document, true)
    }

    pub fn with_options(
        index: Index<Unlocked>,
        document: RubyDocument,
        include_local_vars: bool,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();
        Self {
            index,
            document,
            scope_tracker,
            include_local_vars,
            track_unresolved: false,
            staged: PendingWrites::default(),
            variable_types: OnceCell::new(),
        }
    }

    /// Create a visitor that tracks unresolved constants
    pub fn with_unresolved_tracking(
        index: Index<Unlocked>,
        document: RubyDocument,
        include_local_vars: bool,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();
        Self {
            index,
            document,
            scope_tracker,
            include_local_vars,
            track_unresolved: true,
            staged: PendingWrites::default(),
            variable_types: OnceCell::new(),
        }
    }

    /// O(1) lookup of a local variable's inferred type (from
    /// `name = ClassName.new` assignments anywhere in the file). The first
    /// call triggers one linear scan that populates the whole map; all
    /// subsequent calls are HashMap lookups. See [`Self::variable_types`]
    /// for the rationale.
    pub fn infer_variable_type_cached(&self, var_name: &str) -> Option<RubyType> {
        let map = self
            .variable_types
            .get_or_init(|| build_variable_type_map(&self.document.content));
        map.get(var_name).cloned()
    }
}

/// One-pass scan that extracts every `name = ClassName.new` assignment in
/// the file. First assignment wins (matches the prior semantics of the
/// linear scan it replaces). Called once per file on demand.
fn build_variable_type_map(content: &str) -> HashMap<String, RubyType> {
    use crate::types::ruby_namespace::RubyConstant;

    let mut map: HashMap<String, RubyType> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(eq_idx) = trimmed.find('=') else {
            continue;
        };
        let lhs = trimmed[..eq_idx].trim();
        // Valid local-variable name: starts with lowercase/_, only word chars.
        let mut chars = lhs.chars();
        let first = match chars.next() {
            Some(c) if c.is_lowercase() || c == '_' => c,
            _ => continue,
        };
        let _ = first;
        if !lhs.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rhs_full = trimmed[eq_idx + 1..].trim();
        let Some(new_pos) = rhs_full.find(".new") else {
            continue;
        };
        let class_part = rhs_full[..new_pos].trim();
        if !class_part.chars().next().is_some_and(|c| c.is_uppercase()) {
            continue;
        }
        let parts: Vec<_> = class_part
            .split("::")
            .filter_map(|s| RubyConstant::new(s.trim()).ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        map.entry(lhs.to_string())
            .or_insert_with(|| RubyType::Class(FullyQualifiedName::Constant(parts)));
    }
    map
}

impl Visit<'_> for ReferenceVisitor {
    fn visit_module_node(&mut self, node: &ModuleNode) {
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }
}
