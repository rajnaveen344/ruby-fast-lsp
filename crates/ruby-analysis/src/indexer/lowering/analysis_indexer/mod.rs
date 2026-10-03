//! Direct declaration pass that lowers one Ruby file into namespace, method,
//! variable, and seed type facts.

use std::collections::{HashMap, HashSet};

use crate::core::{
    FileAnalysis, FullyQualifiedName, NamespaceKind, RubyConstant, RubyType, SourceFileId,
    TextRange,
};
use ruby_prism::{
    ConstantPathTargetNode, ConstantPathWriteNode, ConstantTargetNode, ConstantWriteNode, Node,
    Visit,
};

use self::syntax::{constant_parts, u32_offset};
use crate::indexer::documents::scope_tracker::ScopeTracker;

mod constants;
mod deferred;
mod methods;
mod namespaces;
mod syntax;
mod types;
mod variables;
mod visitor;

#[derive(Debug)]
pub struct AnalysisIndexer {
    file_id: SourceFileId,
    /// Lexical frames (`Module.nesting`) for constant writes and lookup, and
    /// execution contexts for where definitions land: a static eval block's
    /// receiver, or the enclosing owner inside a method body.
    scope: ScopeTracker,
    known_namespaces: HashSet<FullyQualifiedName>,
    known_constant_types: HashMap<FullyQualifiedName, RubyType>,
    source: Option<String>,
    facts: FileAnalysis,
    /// Eval calls in method bodies waiting for the file's later namespaces.
    deferred_eval_blocks: Vec<deferred::DeferredEvalBlock>,
    replaying_deferred: bool,
}

impl AnalysisIndexer {
    /// Whether the direct declaration pass can emit a value-constant symbol.
    /// Keep this aligned with the constant-write visitors in `visitor.rs`. Callers can
    /// reuse an existing parse before preparing the more expensive semantic
    /// lookup context for files with no value declarations.
    pub fn has_value_constant_declarations(node: &Node<'_>) -> bool {
        struct Finder(bool);
        impl<'pr> Visit<'pr> for Finder {
            fn visit_constant_write_node(&mut self, _node: &ConstantWriteNode<'pr>) {
                self.0 = true;
            }

            fn visit_constant_path_write_node(&mut self, _node: &ConstantPathWriteNode<'pr>) {
                self.0 = true;
            }

            fn visit_constant_target_node(&mut self, _node: &ConstantTargetNode<'pr>) {
                self.0 = true;
            }

            fn visit_constant_path_target_node(&mut self, _node: &ConstantPathTargetNode<'pr>) {
                self.0 = true;
            }
        }
        let mut finder = Finder(false);
        finder.visit(node);
        finder.0
    }

    pub fn new(file_id: SourceFileId) -> Self {
        Self::with_known_namespaces(file_id, HashSet::new())
    }

    pub fn with_known_namespaces(
        file_id: SourceFileId,
        known_namespaces: HashSet<FullyQualifiedName>,
    ) -> Self {
        Self::with_known_semantics(file_id, known_namespaces, HashMap::new())
    }

    pub fn with_known_semantics(
        file_id: SourceFileId,
        known_namespaces: HashSet<FullyQualifiedName>,
        known_constant_types: HashMap<FullyQualifiedName, RubyType>,
    ) -> Self {
        Self {
            file_id,
            scope: ScopeTracker::new(),
            known_namespaces,
            known_constant_types,
            source: None,
            facts: FileAnalysis::default(),
            deferred_eval_blocks: Vec::new(),
            replaying_deferred: false,
        }
    }

    pub fn index_source(mut self, source: &str) -> FileAnalysis {
        self.source = Some(source.to_string());
        let parse = ruby_prism::parse(source.as_bytes());
        let root = parse.node();
        self.visit(&root);
        self.replay_deferred_eval_blocks(&root);
        self.facts
    }

    pub fn index_node_with_source(mut self, node: &Node<'_>, source: &str) -> FileAnalysis {
        self.source = Some(source.to_string());
        self.visit(node);
        self.replay_deferred_eval_blocks(node);
        self.facts
    }

    /// The namespace that owns definitions here (Ruby's cref).
    fn owner_namespace(&self) -> Vec<RubyConstant> {
        self.scope.method_definition_context().0
    }

    /// The side of `owner_namespace` that a definition here lands on.
    fn owner_kind(&self) -> NamespaceKind {
        self.scope.current_macro_definition_context()
    }

    fn inside_singleton_included_method(&self) -> bool {
        matches!(
            self.scope.current_method(),
            Some((FullyQualifiedName::Method(_, method), NamespaceKind::Singleton))
                if method.as_str() == "included"
        )
    }

    fn range(&self, node: &ruby_prism::Location<'_>) -> TextRange {
        TextRange::new(
            self.file_id,
            u32_offset(node.start_offset()),
            u32_offset(node.end_offset()),
        )
    }

    /// Opens a `class`/`module` body named by `node`. The body nests inside the
    /// current lexical scope, never inside an eval receiver, and owns the
    /// definitions it contains.
    fn enter_namespace_from_node(&mut self, node: &Node<'_>) -> bool {
        let Some(parts) = constant_parts(node) else {
            return false;
        };
        self.scope.push_ns_scopes(parts);
        true
    }

    fn current_owner_fqn(&self) -> FullyQualifiedName {
        FullyQualifiedName::namespace_with_kind(self.owner_namespace(), self.owner_kind())
    }
}

#[cfg(test)]
mod tests;
