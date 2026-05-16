//! Ruby AST to analysis facts.
//!
//! This crate is editor-agnostic. It parses Ruby source with Prism and emits
//! facts consumed by `ruby-analysis-engine`.

use std::collections::HashSet;

use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    RubyConstant, RubyMethod, SourceFileId, SymbolFact, SymbolKind, TextRange,
};
use ruby_prism::{
    visit_call_node, visit_class_node, visit_constant_path_write_node, visit_constant_write_node,
    visit_def_node, visit_module_node, CallNode, ClassNode, ConstantPathNode,
    ConstantPathWriteNode, ConstantWriteNode, DefNode, ModuleNode, Node, Visit,
};

#[derive(Debug, Clone, Default)]
pub struct AnalysisIndex {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Instance,
}

#[derive(Debug)]
pub struct AnalysisIndexer {
    file_id: SourceFileId,
    namespace_stack: Vec<RubyConstant>,
    scope_stack: Vec<ScopeKind>,
    known_namespaces: HashSet<FullyQualifiedName>,
    facts: AnalysisIndex,
}

impl AnalysisIndexer {
    pub fn new(file_id: SourceFileId) -> Self {
        Self::with_known_namespaces(file_id, HashSet::new())
    }

    pub fn with_known_namespaces(
        file_id: SourceFileId,
        known_namespaces: HashSet<FullyQualifiedName>,
    ) -> Self {
        Self {
            file_id,
            namespace_stack: Vec::new(),
            scope_stack: Vec::new(),
            known_namespaces,
            facts: AnalysisIndex::default(),
        }
    }

    pub fn index_source(mut self, source: &str) -> AnalysisIndex {
        let parse = ruby_prism::parse(source.as_bytes());
        self.visit(&parse.node());
        self.facts
    }

    fn current_scope_kind(&self) -> ScopeKind {
        self.scope_stack
            .last()
            .copied()
            .unwrap_or(ScopeKind::Instance)
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
        Some(parts)
    }

    fn pop_namespace_parts(&mut self, parts: &[RubyConstant]) {
        for _ in parts {
            self.namespace_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer namespace stack underflow. \
                 This is a bug because each class/module entry must pop exactly the pushed parts. \
                 Fix: keep class/module visitor enter/exit balanced.",
            );
        }
    }

    fn push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
    ) {
        self.known_namespaces.insert(fqn.clone());
        self.facts.symbols.push(SymbolFact::new(
            fqn.clone(),
            match kind {
                GraphNodeKind::Class => SymbolKind::Class,
                GraphNodeKind::Module => SymbolKind::Module,
            },
            range,
        ));
        self.facts
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));

        let singleton_fqn = fqn.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: namespace fact could not convert to singleton namespace. \
             This is a bug because class/module graph nodes must be namespace FQNs. \
             Fix: only call push_namespace_facts with Namespace facts.",
        );
        self.known_namespaces.insert(singleton_fqn.clone());
        self.facts
            .graph_nodes
            .push(GraphNodeFact::new(singleton_fqn, kind, range));
    }

    fn resolve_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<FullyQualifiedName> {
        let mut search = if absolute {
            Vec::new()
        } else {
            self.namespace_stack.clone()
        };

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(probe);
            if self.known_namespaces.contains(&fqn) {
                return Some(fqn);
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.known_namespaces.contains(&fqn).then_some(fqn)
    }

    fn push_edge(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        range: TextRange,
    ) {
        let Some(target) = self.resolve_namespace(parts, absolute) else {
            return;
        };
        self.facts
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range));
    }
}

impl Visit<'_> for AnalysisIndexer {
    fn visit_class_node(&mut self, node: &ClassNode<'_>) {
        let Some(parts) = self.push_namespace_from_node(&node.constant_path()) else {
            return;
        };

        let fqn = FullyQualifiedName::namespace(self.namespace_stack.clone());
        let range = self.range(&node.location());
        self.push_namespace_facts(fqn.clone(), GraphNodeKind::Class, range);

        if let Some(superclass) = node.superclass() {
            if let Some((parts, absolute)) = constant_parts_and_absolute(&superclass) {
                let super_range = self.range(&superclass.location());
                self.push_edge(
                    fqn.clone(),
                    &parts,
                    absolute,
                    GraphEdgeKind::Superclass,
                    super_range,
                );
                if let Some(source_singleton) = fqn.to_singleton_namespace() {
                    if let Some(target) = self
                        .resolve_namespace(&parts, absolute)
                        .and_then(|target| target.to_singleton_namespace())
                    {
                        self.facts.graph_edges.push(GraphEdgeFact::new(
                            source_singleton,
                            target,
                            GraphEdgeKind::Superclass,
                            super_range,
                        ));
                    }
                }
            }
        }

        self.scope_stack.push(ScopeKind::Instance);
        visit_class_node(self, node);
        self.scope_stack.pop();
        self.pop_namespace_parts(&parts);
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        let Some(parts) = self.push_namespace_from_node(&node.constant_path()) else {
            return;
        };

        let fqn = FullyQualifiedName::namespace(self.namespace_stack.clone());
        let range = self.range(&node.location());
        self.push_namespace_facts(fqn, GraphNodeKind::Module, range);

        self.scope_stack.push(ScopeKind::Instance);
        visit_module_node(self, node);
        self.scope_stack.pop();
        self.pop_namespace_parts(&parts);
    }

    fn visit_def_node(&mut self, node: &DefNode<'_>) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Ok(mut method) = RubyMethod::new(&method_name) else {
            visit_def_node(self, node);
            return;
        };

        let mut owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => ruby_analysis_core::NamespaceKind::Instance,
        };
        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                owner_kind = ruby_analysis_core::NamespaceKind::Singleton;
            } else {
                visit_def_node(self, node);
                return;
            }
        }
        if method.as_str() == "initialize" {
            method = RubyMethod::new("new").expect(
                "INVARIANT VIOLATED: `new` must be a valid Ruby method name. \
                 This is a bug because constructor normalization relies on RubyMethod validation. \
                 Fix: update RubyMethod validation to accept `new`.",
            );
            owner_kind = ruby_analysis_core::NamespaceKind::Singleton;
        }

        let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
        let owner =
            FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
        let range = self.range(&node.location());
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts.methods.push(MethodFact::new(fqn, owner, range));

        visit_def_node(self, node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode<'_>) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            let mut parts = self.namespace_stack.clone();
            parts.push(constant);
            let fqn = FullyQualifiedName::constant(parts);
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::Constant,
                self.range(&node.location()),
            ));
        }
        visit_constant_write_node(self, node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode<'_>) {
        let target = node.target();
        if let Some(parts) = constant_path_parts(&target) {
            let fqn = FullyQualifiedName::constant(parts);
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::Constant,
                self.range(&node.location()),
            ));
        }
        visit_constant_path_write_node(self, node);
    }

    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        if node.receiver().is_none() {
            let kind = match node.name().as_slice() {
                b"include" => Some(GraphEdgeKind::Include),
                b"prepend" => Some(GraphEdgeKind::Prepend),
                b"extend" => Some(GraphEdgeKind::Extend),
                _ => None,
            };
            if let (Some(kind), Some(arguments)) = (kind, node.arguments()) {
                let source = FullyQualifiedName::namespace(self.namespace_stack.clone());
                let range = self.range(&node.location());
                for arg in arguments.arguments().iter() {
                    if let Some((parts, absolute)) = constant_parts_and_absolute(&arg) {
                        self.push_edge(source.clone(), &parts, absolute, kind, range);
                        if kind == GraphEdgeKind::Extend {
                            if let Some(source_singleton) = source.to_singleton_namespace() {
                                self.push_edge(
                                    source_singleton,
                                    &parts,
                                    absolute,
                                    GraphEdgeKind::Include,
                                    range,
                                );
                            }
                        }
                    }
                }
            }
        }

        visit_call_node(self, node);
    }
}

fn constant_parts(node: &Node<'_>) -> Option<Vec<RubyConstant>> {
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        return RubyConstant::new(&name).ok().map(|constant| vec![constant]);
    }
    if let Some(path) = node.as_constant_path_node() {
        return constant_path_parts(&path);
    }
    None
}

fn constant_parts_and_absolute(node: &Node<'_>) -> Option<(Vec<RubyConstant>, bool)> {
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        return RubyConstant::new(&name)
            .ok()
            .map(|constant| (vec![constant], false));
    }
    if let Some(path) = node.as_constant_path_node() {
        let absolute = path.parent().is_none();
        return constant_path_parts(&path).map(|parts| (parts, absolute));
    }
    None
}

fn constant_path_parts(path: &ConstantPathNode<'_>) -> Option<Vec<RubyConstant>> {
    let mut parts = Vec::new();
    collect_constant_path_parts(path, &mut parts);
    (!parts.is_empty()).then_some(parts)
}

fn collect_constant_path_parts(path: &ConstantPathNode<'_>, parts: &mut Vec<RubyConstant>) {
    if let Some(parent) = path.parent() {
        if let Some(parent_path) = parent.as_constant_path_node() {
            collect_constant_path_parts(&parent_path, parts);
        } else if let Some(parent_read) = parent.as_constant_read_node() {
            let name = String::from_utf8_lossy(parent_read.name().as_slice()).to_string();
            if let Ok(constant) = RubyConstant::new(&name) {
                parts.push(constant);
            }
        }
    }
    if let Some(name) = path.name() {
        let name = String::from_utf8_lossy(name.as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            parts.push(constant);
        }
    }
}

fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect(
        "INVARIANT VIOLATED: source byte offset exceeded u32. \
         This is a bug because analysis facts currently store u32 ranges. \
         Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn indexes_class_module_method_and_mixin_facts() {
        let index = AnalysisIndexer::new(file()).index_source(
            "module Auth\nend\nclass User\n  include Auth\n  def name\n  end\n  def self.find\n  end\nend\n",
        );

        let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let auth = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
        assert!(index
            .graph_nodes
            .iter()
            .any(|fact| fact.fqn == user && fact.kind == GraphNodeKind::Class));
        assert!(index.graph_edges.iter().any(|fact| fact.source == user
            && fact.target == auth
            && fact.kind == GraphEdgeKind::Include));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#name"
                && fact.owner.namespace_kind() == Some(ruby_analysis_core::NamespaceKind::Instance)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#find"
                && fact.owner.namespace_kind() == Some(ruby_analysis_core::NamespaceKind::Singleton)
        }));
    }
}
