//! Ruby AST to analysis facts.
//!
//! This crate is editor-agnostic. It parses Ruby source with Prism and emits
//! facts consumed by `ruby-analysis::engine`.

pub mod analyzer;
pub mod analyzer_utils;
pub mod document_symbols;
pub mod fact_collector;
pub mod identifier;
pub mod identifier_visitor;
pub mod rename;
mod ruby_document;
mod scope_tracker;
pub mod semantic_tokens;
mod source_document;
mod variable_scopes;
pub mod yard;

use std::collections::HashSet;

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    MethodParamFact, MethodParamKind, RubyConstant, RubyMethod, RubyType, SourceFileId, SymbolFact,
    SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use ruby_prism::{
    visit_call_node, visit_class_node, visit_class_variable_and_write_node,
    visit_class_variable_operator_write_node, visit_class_variable_or_write_node,
    visit_class_variable_target_node, visit_class_variable_write_node,
    visit_constant_path_write_node, visit_constant_write_node, visit_def_node,
    visit_global_variable_and_write_node, visit_global_variable_operator_write_node,
    visit_global_variable_or_write_node, visit_global_variable_target_node,
    visit_global_variable_write_node, visit_instance_variable_and_write_node,
    visit_instance_variable_operator_write_node, visit_instance_variable_or_write_node,
    visit_instance_variable_target_node, visit_instance_variable_write_node,
    visit_local_variable_and_write_node, visit_local_variable_operator_write_node,
    visit_local_variable_or_write_node, visit_local_variable_target_node,
    visit_local_variable_write_node, visit_module_node, visit_singleton_class_node, CallNode,
    ClassNode, ClassVariableAndWriteNode, ClassVariableOperatorWriteNode, ClassVariableOrWriteNode,
    ClassVariableTargetNode, ClassVariableWriteNode, ConstantPathNode, ConstantPathWriteNode,
    ConstantWriteNode, DefNode, GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode,
    GlobalVariableOrWriteNode, GlobalVariableTargetNode, GlobalVariableWriteNode,
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode, LocalVariableAndWriteNode,
    LocalVariableOperatorWriteNode, LocalVariableOrWriteNode, LocalVariableTargetNode,
    LocalVariableWriteNode, ModuleNode, Node, SingletonClassNode, Visit,
};

pub use analyzer::RubyPrismAnalyzer;
pub use document_symbols::{DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext};
pub use identifier::{Identifier, MethodReceiver};
pub use identifier_visitor::{IdentifierType, IdentifierVisitor};
pub use rename::RenameVisitor;
pub use ruby_document::RubyDocument;
pub use scope_tracker::{
    build_constant_path_name, collect_namespaces, get_method_namespace_kind, mixin_ref_from_node,
    utf8_str, LocalScopeKind, MixinRef, ScopeFrame, ScopeTracker,
};
pub use semantic_tokens::{TokenVisitor, TOKEN_MODIFIERS, TOKEN_TYPES};
pub use source_document::SourceDocument;
pub use variable_scopes::{
    CaptureRef, LVScopeId, LVScopeKind, RenameTarget, RenameTargetKind, ScopeNode, TypeAssignment,
    VariableNode, VariableScopes,
};

#[derive(Debug, Clone, Default)]
pub struct AnalysisIndex {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub types: Vec<TypeFact>,
}

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

    pub fn index_node(mut self, node: &Node<'_>) -> AnalysisIndex {
        self.visit(node);
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
        self.facts.types.push(TypeFact::new(
            TypeSubject::Constant(FullyQualifiedName::constant(fqn.namespace_parts())),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(fqn.clone()),
            },
            range,
            TypeProvenance::Inferred,
        ));

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
            self.facts
                .unresolved_graph_edges
                .push(UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(self.namespace_stack.clone()),
                    kind,
                    range,
                ));
            return;
        };
        self.facts
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range));
    }

    fn push_method_fact(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: crate::core::NamespaceKind,
        method: RubyMethod,
        range: TextRange,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts.methods.push(MethodFact::new(fqn, owner, range));
    }

    fn push_attr_method_facts(&mut self, node: &CallNode<'_>, reader: bool, writer: bool) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };

            if reader {
                if let Ok(method) = RubyMethod::new(&name) {
                    self.push_method_fact(self.namespace_stack.clone(), owner_kind, method, range);
                }
            }

            if writer {
                if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                    self.push_method_fact(self.namespace_stack.clone(), owner_kind, method, range);
                }
            }
        }
    }

    fn push_module_function_facts(&mut self, node: &CallNode<'_>) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        for arg in arguments.arguments().iter() {
            let Some((name, fallback_range)) = symbol_name_and_range(&arg, self.file_id) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let instance_owner = FullyQualifiedName::namespace_with_kind(
                self.namespace_stack.clone(),
                crate::core::NamespaceKind::Instance,
            );
            let range = self
                .facts
                .methods
                .iter()
                .find(|fact| fact.fqn == fqn && fact.owner == instance_owner)
                .map(|fact| fact.range)
                .unwrap_or(fallback_range);
            let owner = FullyQualifiedName::namespace_with_kind(
                self.namespace_stack.clone(),
                crate::core::NamespaceKind::Singleton,
            );
            self.facts.methods.push(MethodFact::new(fqn, owner, range));
        }
    }

    fn push_local_variable_fact(&mut self, name: &[u8], location: ruby_prism::Location<'_>) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::local_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::LocalVariable,
                self.range(&location),
            ));
        }
    }

    fn push_instance_variable_fact(&mut self, name: &[u8], location: ruby_prism::Location<'_>) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::instance_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::InstanceVariable,
                self.range(&location),
            ));
        }
    }

    fn push_class_variable_fact(&mut self, name: &[u8], location: ruby_prism::Location<'_>) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::class_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::ClassVariable,
                self.range(&location),
            ));
        }
    }

    fn push_global_variable_fact(&mut self, name: &[u8], location: ruby_prism::Location<'_>) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::global_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::GlobalVariable,
                self.range(&location),
            ));
        }
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

    fn push_type_fact(
        &mut self,
        subject: TypeSubject,
        ruby_type: Option<RubyType>,
        location: ruby_prism::Location<'_>,
    ) {
        let Some(ruby_type) = ruby_type else {
            return;
        };
        if ruby_type == RubyType::Unknown {
            return;
        }
        self.facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            self.range(&location),
            TypeProvenance::Assignment,
        ));
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
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                owner_kind = crate::core::NamespaceKind::Singleton;
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
            owner_kind = crate::core::NamespaceKind::Singleton;
        }

        let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
        let owner =
            FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
        let range = self.range(&node.location());
        let params = method_param_facts(node);
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts.methods.push(MethodFact::with_param_facts(
            fqn.clone(),
            owner,
            range,
            params,
        ));
        if let Some(return_type) = method_body_literal_type(node) {
            self.facts.types.push(TypeFact::new(
                TypeSubject::MethodReturn(fqn.clone()),
                return_type,
                range,
                TypeProvenance::Inferred,
            ));
        }

        visit_def_node(self, node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode<'_>) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            let mut parts = self.namespace_stack.clone();
            parts.push(constant);
            let fqn = FullyQualifiedName::constant(parts);
            self.facts.symbols.push(SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&node.location()),
            ));
            self.push_type_fact(
                TypeSubject::Constant(fqn),
                literal_type(&node.value()),
                node.name_loc(),
            );
        }
        visit_constant_write_node(self, node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode<'_>) {
        let target = node.target();
        if let Some(parts) = constant_path_parts(&target) {
            let fqn = FullyQualifiedName::constant(parts);
            self.facts.symbols.push(SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&node.location()),
            ));
            self.push_type_fact(
                TypeSubject::Constant(fqn),
                literal_type(&node.value()),
                target.location(),
            );
        }
        visit_constant_path_write_node(self, node);
    }

    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        if node.receiver().is_none() {
            match node.name().as_slice() {
                b"attr_reader" => self.push_attr_method_facts(node, true, false),
                b"attr_writer" => self.push_attr_method_facts(node, false, true),
                b"attr_accessor" => self.push_attr_method_facts(node, true, true),
                b"module_function" => self.push_module_function_facts(node),
                _ => {}
            }

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

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode<'_>) {
        self.scope_stack.push(ScopeKind::Singleton);
        visit_singleton_class_node(self, node);
        self.scope_stack.pop();
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode<'_>) {
        self.push_local_variable_fact(node.name().as_slice(), node.name_loc());
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.push_type_fact(
            TypeSubject::Local { scope_id: 0, name },
            literal_type(&node.value()),
            node.name_loc(),
        );
        visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode<'_>) {
        self.push_local_variable_fact(node.name().as_slice(), node.location());
        visit_local_variable_target_node(self, node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode<'_>) {
        self.push_local_variable_fact(node.name().as_slice(), node.name_loc());
        visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode<'_>) {
        self.push_local_variable_fact(node.name().as_slice(), node.name_loc());
        visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(
        &mut self,
        node: &LocalVariableOperatorWriteNode<'_>,
    ) {
        self.push_local_variable_fact(node.name().as_slice(), node.name_loc());
        visit_local_variable_operator_write_node(self, node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode<'_>) {
        self.push_instance_variable_fact(node.name().as_slice(), node.name_loc());
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.push_type_fact(
            TypeSubject::InstanceVariable {
                owner: self.current_owner_fqn(),
                name,
            },
            literal_type(&node.value()),
            node.name_loc(),
        );
        visit_instance_variable_write_node(self, node);
    }

    fn visit_instance_variable_target_node(&mut self, node: &InstanceVariableTargetNode<'_>) {
        self.push_instance_variable_fact(node.name().as_slice(), node.location());
        visit_instance_variable_target_node(self, node);
    }

    fn visit_instance_variable_or_write_node(&mut self, node: &InstanceVariableOrWriteNode<'_>) {
        self.push_instance_variable_fact(node.name().as_slice(), node.name_loc());
        visit_instance_variable_or_write_node(self, node);
    }

    fn visit_instance_variable_and_write_node(&mut self, node: &InstanceVariableAndWriteNode<'_>) {
        self.push_instance_variable_fact(node.name().as_slice(), node.name_loc());
        visit_instance_variable_and_write_node(self, node);
    }

    fn visit_instance_variable_operator_write_node(
        &mut self,
        node: &InstanceVariableOperatorWriteNode<'_>,
    ) {
        self.push_instance_variable_fact(node.name().as_slice(), node.name_loc());
        visit_instance_variable_operator_write_node(self, node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode<'_>) {
        self.push_class_variable_fact(node.name().as_slice(), node.name_loc());
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.push_type_fact(
            TypeSubject::ClassVariable {
                owner: self.current_owner_fqn(),
                name,
            },
            literal_type(&node.value()),
            node.name_loc(),
        );
        visit_class_variable_write_node(self, node);
    }

    fn visit_class_variable_target_node(&mut self, node: &ClassVariableTargetNode<'_>) {
        self.push_class_variable_fact(node.name().as_slice(), node.location());
        visit_class_variable_target_node(self, node);
    }

    fn visit_class_variable_or_write_node(&mut self, node: &ClassVariableOrWriteNode<'_>) {
        self.push_class_variable_fact(node.name().as_slice(), node.name_loc());
        visit_class_variable_or_write_node(self, node);
    }

    fn visit_class_variable_and_write_node(&mut self, node: &ClassVariableAndWriteNode<'_>) {
        self.push_class_variable_fact(node.name().as_slice(), node.name_loc());
        visit_class_variable_and_write_node(self, node);
    }

    fn visit_class_variable_operator_write_node(
        &mut self,
        node: &ClassVariableOperatorWriteNode<'_>,
    ) {
        self.push_class_variable_fact(node.name().as_slice(), node.name_loc());
        visit_class_variable_operator_write_node(self, node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode<'_>) {
        self.push_global_variable_fact(node.name().as_slice(), node.name_loc());
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.push_type_fact(
            TypeSubject::GlobalVariable(name),
            literal_type(&node.value()),
            node.name_loc(),
        );
        visit_global_variable_write_node(self, node);
    }

    fn visit_global_variable_target_node(&mut self, node: &GlobalVariableTargetNode<'_>) {
        self.push_global_variable_fact(node.name().as_slice(), node.location());
        visit_global_variable_target_node(self, node);
    }

    fn visit_global_variable_or_write_node(&mut self, node: &GlobalVariableOrWriteNode<'_>) {
        self.push_global_variable_fact(node.name().as_slice(), node.name_loc());
        visit_global_variable_or_write_node(self, node);
    }

    fn visit_global_variable_and_write_node(&mut self, node: &GlobalVariableAndWriteNode<'_>) {
        self.push_global_variable_fact(node.name().as_slice(), node.name_loc());
        visit_global_variable_and_write_node(self, node);
    }

    fn visit_global_variable_operator_write_node(
        &mut self,
        node: &GlobalVariableOperatorWriteNode<'_>,
    ) {
        self.push_global_variable_fact(node.name().as_slice(), node.name_loc());
        visit_global_variable_operator_write_node(self, node);
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

fn attr_name_and_range(node: &Node<'_>, file_id: SourceFileId) -> Option<(String, TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &symbol.location()),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            text_range(file_id, &string.content_loc()),
        ));
    }
    None
}

fn symbol_name_and_range(node: &Node<'_>, file_id: SourceFileId) -> Option<(String, TextRange)> {
    node.as_symbol_node().map(|symbol| {
        (
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &symbol.location()),
        )
    })
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

fn method_param_facts(node: &DefNode<'_>) -> Vec<MethodParamFact> {
    let mut params = Vec::new();
    let Some(params_node) = node.parameters() else {
        return params;
    };

    for required in params_node.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice()).to_string(),
                MethodParamKind::Required,
            ));
        }
    }

    for optional in params_node.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice()).to_string(),
                MethodParamKind::Optional,
            ));
        }
    }

    if let Some(rest) = params_node.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                params.push(MethodParamFact::new(
                    String::from_utf8_lossy(name.as_slice()).to_string(),
                    MethodParamKind::Rest,
                ));
            }
        }
    }

    for keyword in params_node.keywords().iter() {
        if let Some(param) = keyword.as_required_keyword_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice())
                    .trim_end_matches(':')
                    .to_string(),
                MethodParamKind::RequiredKeyword,
            ));
        } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice())
                    .trim_end_matches(':')
                    .to_string(),
                MethodParamKind::OptionalKeyword,
            ));
        }
    }

    if let Some(kwrest) = params_node.keyword_rest() {
        if let Some(param) = kwrest.as_keyword_rest_parameter_node() {
            if let Some(name) = param.name() {
                params.push(MethodParamFact::new(
                    String::from_utf8_lossy(name.as_slice()).to_string(),
                    MethodParamKind::KeywordRest,
                ));
            }
        }
    }

    if let Some(block) = params_node.block() {
        if let Some(name) = block.name() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(name.as_slice()).to_string(),
                MethodParamKind::Block,
            ));
        }
    }

    params
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

fn literal_type(node: &Node<'_>) -> Option<RubyType> {
    if let Some(call) = node.as_call_node() {
        if call.name().as_slice() == b"new" {
            let receiver = call.receiver()?;
            let parts = constant_parts(&receiver)?;
            return Some(RubyType::Class(FullyQualifiedName::constant(parts)));
        }
    }
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).ok()?;
        return Some(RubyType::ClassReference(FullyQualifiedName::constant(
            vec![constant],
        )));
    }
    if let Some(path) = node.as_constant_path_node() {
        let parts = constant_path_parts(&path)?;
        return Some(RubyType::ClassReference(FullyQualifiedName::constant(
            parts,
        )));
    }
    if node.as_string_node().is_some() || node.as_interpolated_string_node().is_some() {
        return Some(RubyType::string());
    }
    if node.as_integer_node().is_some() {
        return Some(RubyType::integer());
    }
    if node.as_float_node().is_some() {
        return Some(RubyType::float());
    }
    if node.as_symbol_node().is_some() || node.as_interpolated_symbol_node().is_some() {
        return Some(RubyType::symbol());
    }
    if node.as_true_node().is_some() {
        return Some(RubyType::true_class());
    }
    if node.as_false_node().is_some() {
        return Some(RubyType::false_class());
    }
    if node.as_nil_node().is_some() {
        return Some(RubyType::nil_class());
    }
    if let Some(array) = node.as_array_node() {
        let mut element_types = array
            .elements()
            .iter()
            .filter_map(|element| literal_type(&element))
            .collect::<Vec<_>>();
        dedup_types(&mut element_types);
        return Some(if element_types.is_empty() {
            RubyType::Array(vec![RubyType::Unknown])
        } else {
            RubyType::Array(element_types)
        });
    }
    if let Some(hash) = node.as_hash_node() {
        let mut key_types = Vec::new();
        let mut value_types = Vec::new();
        for element in hash.elements().iter() {
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            if let Some(key_type) = literal_type(&assoc.key()) {
                key_types.push(key_type);
            }
            if let Some(value_type) = literal_type(&assoc.value()) {
                value_types.push(value_type);
            }
        }
        dedup_types(&mut key_types);
        dedup_types(&mut value_types);
        return Some(RubyType::Hash(
            if key_types.is_empty() {
                vec![RubyType::Unknown]
            } else {
                key_types
            },
            if value_types.is_empty() {
                vec![RubyType::Unknown]
            } else {
                value_types
            },
        ));
    }
    None
}

fn dedup_types(types: &mut Vec<RubyType>) {
    let mut unique = Vec::new();
    for ty in types.drain(..) {
        if !unique.contains(&ty) {
            unique.push(ty);
        }
    }
    *types = unique;
}

fn method_body_literal_type(node: &DefNode<'_>) -> Option<RubyType> {
    let body = node.body()?;
    if let Some(statements) = body.as_statements_node() {
        let last = statements.body().iter().last()?;
        return literal_type(&last);
    }
    literal_type(&body)
}

fn text_range(file_id: SourceFileId, location: &ruby_prism::Location<'_>) -> TextRange {
    TextRange::new(
        file_id,
        u32_offset(location.start_offset()),
        u32_offset(location.end_offset()),
    )
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
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#find"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
        }));
    }

    #[test]
    fn indexes_method_param_names() {
        let index = AnalysisIndexer::new(file()).index_source(
            "class User\n  def find(id, name = nil, *rest, active:, role: nil, **opts, &block)\n  end\nend\n",
        );

        let method = index
            .methods
            .iter()
            .find(|fact| fact.fqn.to_string() == "User#find")
            .expect(
                "INVARIANT VIOLATED: analysis indexer did not emit User#find. \
                 This is a bug because def nodes must produce method facts. \
                 Fix: keep visit_def_node method fact emission active.",
            );
        assert_eq!(
            method.params,
            vec!["id", "name", "rest", "active", "role", "opts", "block"]
        );
        let kinds = method
            .param_facts
            .iter()
            .map(|param| param.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                MethodParamKind::Required,
                MethodParamKind::Optional,
                MethodParamKind::Rest,
                MethodParamKind::RequiredKeyword,
                MethodParamKind::OptionalKeyword,
                MethodParamKind::KeywordRest,
                MethodParamKind::Block,
            ]
        );
    }

    #[test]
    fn indexes_singleton_class_attr_and_module_function_methods() {
        let index = AnalysisIndexer::new(file()).index_source(
            "module Utils\n  def helper\n  end\n  module_function :helper\nend\nclass User\n  attr_accessor :name\n  class << self\n    attr_reader :count\n    def build\n    end\n  end\nend\n",
        );

        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "Utils#helper"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#name"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#name="
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#count"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "User#build"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
        }));
    }

    #[test]
    fn indexes_variable_write_symbol_facts() {
        let index = AnalysisIndexer::new(file())
            .index_source("name = 1\n@name = name\n@@count = 1\n$debug = true\n");

        assert!(index.symbols.iter().any(|fact| {
            fact.fqn.to_string() == "name" && fact.kind == SymbolKind::LocalVariable
        }));
        assert!(index.symbols.iter().any(|fact| {
            fact.fqn.to_string() == "@name" && fact.kind == SymbolKind::InstanceVariable
        }));
        assert!(index.symbols.iter().any(|fact| {
            fact.fqn.to_string() == "@@count" && fact.kind == SymbolKind::ClassVariable
        }));
        assert!(index.symbols.iter().any(|fact| {
            fact.fqn.to_string() == "$debug" && fact.kind == SymbolKind::GlobalVariable
        }));
    }

    #[test]
    fn indexes_literal_assignment_type_facts() {
        let index = AnalysisIndexer::new(file())
            .index_source("A = 1\nname = \"Ada\"\n@active = true\n@@count = 1\n$debug = false\n");

        assert!(index.types.iter().any(|fact| {
            fact.subject
                == TypeSubject::Constant(FullyQualifiedName::constant(vec![
                    RubyConstant::new("A").unwrap()
                ]))
                && fact.ruby_type == RubyType::integer()
        }));
        assert!(index.types.iter().any(|fact| {
            fact.subject
                == TypeSubject::Local {
                    scope_id: 0,
                    name: "name".to_string(),
                }
                && fact.ruby_type == RubyType::string()
        }));
        assert!(index.types.iter().any(|fact| {
            matches!(
                &fact.subject,
                TypeSubject::InstanceVariable { name, .. } if name == "@active"
            ) && fact.ruby_type == RubyType::true_class()
        }));
        assert!(index.types.iter().any(|fact| {
            matches!(
                &fact.subject,
                TypeSubject::ClassVariable { name, .. } if name == "@@count"
            ) && fact.ruby_type == RubyType::integer()
        }));
        assert!(index.types.iter().any(|fact| {
            fact.subject == TypeSubject::GlobalVariable("$debug".to_string())
                && fact.ruby_type == RubyType::false_class()
        }));
    }

    #[test]
    fn indexes_namespace_constant_type_facts() {
        let index =
            AnalysisIndexer::new(file()).index_source("module Auth\nend\nclass User\nend\n");

        let auth = FullyQualifiedName::constant(vec![RubyConstant::new("Auth").unwrap()]);
        let user = FullyQualifiedName::constant(vec![RubyConstant::new("User").unwrap()]);
        assert!(index.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(auth.clone())
                && matches!(fact.ruby_type, RubyType::ModuleReference(_))
        }));
        assert!(index.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(user.clone())
                && matches!(fact.ruby_type, RubyType::ClassReference(_))
        }));
    }

    #[test]
    fn indexes_constant_object_assignment_type_fact() {
        let index = AnalysisIndexer::new(file()).index_source("MODEL = User\n");

        let model = FullyQualifiedName::constant(vec![RubyConstant::new("MODEL").unwrap()]);
        assert!(index.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(model.clone())
                && fact.ruby_type
                    == RubyType::ClassReference(FullyQualifiedName::constant(vec![
                        RubyConstant::new("User").unwrap(),
                    ]))
        }));
    }

    #[test]
    fn indexes_constructor_assignment_type_fact() {
        let index =
            AnalysisIndexer::new(file()).index_source("class User\nend\n@user = User.new\n");

        assert!(index.types.iter().any(|fact| {
            matches!(
                &fact.subject,
                TypeSubject::InstanceVariable { name, .. } if name == "@user"
            ) && fact.ruby_type
                == RubyType::Class(FullyQualifiedName::constant(vec![RubyConstant::new(
                    "User",
                )
                .unwrap()]))
        }));
    }
}
