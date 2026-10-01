//! Direct declaration pass that lowers one Ruby file into namespace, method,
//! variable, and seed type facts.

use crate::invariant::ExpectInvariant;
use std::collections::{HashMap, HashSet};

use crate::core::{
    FileAnalysis, FullyQualifiedName, MethodVisibility, NamespaceKind, RubyConstant, RubyMethod,
    RubyType, SourceFileId, TextRange,
};
use ruby_prism::{ConstantPathWriteNode, ConstantWriteNode, Node, Visit};

use self::syntax::{constant_parts, u32_offset};

mod methods;
mod namespaces;
mod syntax;
mod types;
mod variables;
mod visitor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Instance,
    Singleton,
}

#[derive(Debug)]
pub struct AnalysisIndexer {
    file_id: SourceFileId,
    namespace_stack: Vec<RubyConstant>,
    scope_stack: Vec<ScopeKind>,
    method_context_stack: Vec<(RubyMethod, NamespaceKind)>,
    eval_context_depths: Vec<(usize, usize)>,
    module_function_mode_stack: Vec<bool>,
    visibility_stack: Vec<MethodVisibility>,
    known_namespaces: HashSet<FullyQualifiedName>,
    known_constant_types: HashMap<FullyQualifiedName, RubyType>,
    source: Option<String>,
    facts: FileAnalysis,
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
            namespace_stack: Vec::new(),
            scope_stack: Vec::new(),
            method_context_stack: Vec::new(),
            eval_context_depths: Vec::new(),
            module_function_mode_stack: Vec::new(),
            visibility_stack: vec![MethodVisibility::Public],
            known_namespaces,
            known_constant_types,
            source: None,
            facts: FileAnalysis::default(),
        }
    }

    pub fn index_source(mut self, source: &str) -> FileAnalysis {
        self.source = Some(source.to_string());
        let parse = ruby_prism::parse(source.as_bytes());
        self.visit(&parse.node());
        self.facts
    }

    pub fn index_node_with_source(mut self, node: &Node<'_>, source: &str) -> FileAnalysis {
        self.source = Some(source.to_string());
        self.visit(node);
        self.facts
    }

    fn current_scope_kind(&self) -> ScopeKind {
        self.scope_stack
            .last()
            .copied()
            .unwrap_or(ScopeKind::Instance)
    }

    fn eval_context_active(&self) -> bool {
        self.eval_context_depths
            .last()
            .is_some_and(|(scope_depth, method_depth)| {
                *scope_depth == self.scope_stack.len()
                    && *method_depth == self.method_context_stack.len()
            })
    }

    fn inside_singleton_included_method(&self) -> bool {
        matches!(
            self.method_context_stack.last(),
            Some((method, NamespaceKind::Singleton)) if method.as_str() == "included"
        )
    }

    fn range(&self, node: &ruby_prism::Location<'_>) -> TextRange {
        TextRange::new(
            self.file_id,
            u32_offset(node.start_offset()),
            u32_offset(node.end_offset()),
        )
    }

    fn push_namespace_from_node(&mut self, node: &Node<'_>) -> Option<Vec<RubyConstant>> {
        let parts = constant_parts(node)?;
        self.namespace_stack.extend(parts.iter().cloned());
        self.module_function_mode_stack.push(false);
        self.visibility_stack.push(MethodVisibility::Public);
        Some(parts)
    }

    fn pop_namespace_parts(&mut self, parts: &[RubyConstant]) {
        for _ in parts {
            self.namespace_stack.pop().expect_invariant(
                "analysis indexer namespace stack underflow",
                "each class/module entry must pop exactly the pushed parts",
                "keep class/module visitor enter/exit balanced",
            );
        }
        self.module_function_mode_stack.pop().expect_invariant(
            "analysis indexer module_function mode stack underflow",
            "each namespace frame must pop exactly one module_function flag",
            "keep class/module visitor enter/exit balanced",
        );
        self.visibility_stack.pop().expect_invariant(
            "analysis indexer visibility stack underflow",
            "each namespace frame must pop exactly one visibility flag",
            "keep class/module visitor enter/exit balanced",
        );
    }

    fn current_visibility(&self) -> MethodVisibility {
        self.visibility_stack.last().copied().unwrap_or_else(|| {
            unreachable_invariant!(
                what = "analysis indexer visibility stack is empty",
                why = "the indexer starts with a root public visibility",
                fix = "initialize AnalysisIndexer with a root visibility frame",
            )
        })
    }

    fn set_current_visibility(&mut self, visibility: MethodVisibility) {
        let Some(current) = self.visibility_stack.last_mut() else {
            unreachable_invariant!(
                what = "analysis indexer visibility stack is empty",
                why = "the indexer starts with a root public visibility",
                fix = "initialize AnalysisIndexer with a root visibility frame",
            );
        };
        *current = visibility;
    }

    fn current_owner_fqn(&self) -> FullyQualifiedName {
        FullyQualifiedName::namespace_with_kind(
            self.namespace_stack.clone(),
            match self.current_scope_kind() {
                ScopeKind::Instance => crate::core::NamespaceKind::Instance,
                ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
            },
        )
    }
}

#[cfg(test)]
mod tests;
