//! Direct declaration pass that lowers one Ruby file into namespace, method,
//! variable, and seed type facts.

use std::collections::HashSet;

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
use crate::indexer::yard::parser::YardParser;
use crate::indexer::yard::types::YardMethodDoc;
use crate::invariant::ExpectInvariant;

mod constants;
mod deferred;
mod methods;
mod namespaces;
mod syntax;
mod types;
mod variables;
mod visitor;

/// Project facts outside the file being indexed, read on demand.
pub trait KnownSemantics {
    /// Whether a class or module declaration outside this walk names
    /// `namespace`.
    fn is_known_namespace(&self, namespace: &FullyQualifiedName) -> bool;
    /// The one value type every other file agrees on for `constant`.
    fn constant_type(&self, constant: &FullyQualifiedName) -> Option<RubyType>;
}

/// A file indexed on its own knows no other file's facts.
struct NoKnownSemantics;

impl KnownSemantics for NoKnownSemantics {
    fn is_known_namespace(&self, _namespace: &FullyQualifiedName) -> bool {
        false
    }

    fn constant_type(&self, _constant: &FullyQualifiedName) -> Option<RubyType> {
        None
    }
}

#[cfg(test)]
impl KnownSemantics for HashSet<FullyQualifiedName> {
    fn is_known_namespace(&self, namespace: &FullyQualifiedName) -> bool {
        self.contains(namespace)
    }

    fn constant_type(&self, _constant: &FullyQualifiedName) -> Option<RubyType> {
        None
    }
}

#[cfg(test)]
impl KnownSemantics
    for (
        HashSet<FullyQualifiedName>,
        std::collections::HashMap<FullyQualifiedName, RubyType>,
    )
{
    fn is_known_namespace(&self, namespace: &FullyQualifiedName) -> bool {
        self.0.contains(namespace)
    }

    fn constant_type(&self, constant: &FullyQualifiedName) -> Option<RubyType> {
        self.1.get(constant).cloned()
    }
}

pub struct AnalysisIndexer<'k> {
    file_id: SourceFileId,
    /// Lexical frames (`Module.nesting`) for constant writes and lookup, and
    /// execution contexts for where definitions land: a static eval block's
    /// receiver, or the enclosing owner inside a method body.
    scope: ScopeTracker,
    /// Namespaces this walk declares; `known` answers for other files.
    declared_namespaces: HashSet<FullyQualifiedName>,
    known: &'k dyn KnownSemantics,
    source: Option<String>,
    /// Byte offsets of the source's newlines, for the zero-based line of a
    /// definition without rescanning the file at each one.
    newline_offsets: Vec<usize>,
    facts: FileAnalysis,
    /// Eval calls in method bodies waiting for the file's later namespaces.
    deferred_eval_blocks: Vec<deferred::DeferredEvalBlock>,
    replaying_deferred: bool,
}

impl<'k> AnalysisIndexer<'k> {
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
        Self::with_known_semantics(file_id, &NoKnownSemantics)
    }

    pub fn with_known_semantics(file_id: SourceFileId, known: &'k dyn KnownSemantics) -> Self {
        Self {
            file_id,
            scope: ScopeTracker::new(),
            declared_namespaces: HashSet::new(),
            known,
            source: None,
            newline_offsets: Vec::new(),
            facts: FileAnalysis::default(),
            deferred_eval_blocks: Vec::new(),
            replaying_deferred: false,
        }
    }

    pub fn index_source(mut self, source: &str) -> FileAnalysis {
        self.set_source(source);
        let parse = ruby_prism::parse(source.as_bytes());
        let root = parse.node();
        self.visit(&root);
        self.replay_deferred_eval_blocks(&root);
        self.facts
    }

    pub fn index_node_with_source(mut self, node: &Node<'_>, source: &str) -> FileAnalysis {
        self.set_source(source);
        self.visit(node);
        self.replay_deferred_eval_blocks(node);
        self.facts
    }

    fn set_source(&mut self, source: &str) {
        self.source = Some(source.to_string());
        self.newline_offsets = source
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(offset, _)| offset)
            .collect();
    }

    /// The YARD documentation attached to a definition starting at `offset`.
    fn yard_doc_at(&self, offset: usize) -> Option<YardMethodDoc> {
        let source = self.source.as_deref()?;
        let line = u32::try_from(
            self.newline_offsets
                .partition_point(|&newline| newline < offset),
        )
        .expect_invariant(
            "YARD method line exceeded u32",
            "editor protocol positions use u32 line numbers",
            "reject or segment files with more than u32::MAX lines",
        );
        YardParser::extract_from_source_at_line(source, offset, line)
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
