use std::collections::HashSet;

use crate::core::method_store::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    MethodParamFact, MethodParamKind, MethodVisibilityOverrideFact, NamespaceKind, RubyConstant,
    RubyMethod, RubyType, SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use ruby_prism::{
    visit_alias_method_node, visit_call_node, visit_class_node,
    visit_class_variable_and_write_node, visit_class_variable_operator_write_node,
    visit_class_variable_or_write_node, visit_class_variable_target_node,
    visit_class_variable_write_node, visit_constant_path_write_node, visit_constant_write_node,
    visit_def_node, visit_global_variable_and_write_node,
    visit_global_variable_operator_write_node, visit_global_variable_or_write_node,
    visit_global_variable_target_node, visit_global_variable_write_node,
    visit_instance_variable_and_write_node, visit_instance_variable_operator_write_node,
    visit_instance_variable_or_write_node, visit_instance_variable_target_node,
    visit_instance_variable_write_node, visit_local_variable_and_write_node,
    visit_local_variable_operator_write_node, visit_local_variable_or_write_node,
    visit_local_variable_target_node, visit_local_variable_write_node, visit_module_node,
    visit_singleton_class_node, AliasMethodNode, CallNode, ClassNode, ClassVariableAndWriteNode,
    ClassVariableOperatorWriteNode, ClassVariableOrWriteNode, ClassVariableTargetNode,
    ClassVariableWriteNode, ConstantPathNode, ConstantPathWriteNode, ConstantWriteNode, DefNode,
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode, InstanceVariableAndWriteNode,
    InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode, InstanceVariableTargetNode,
    InstanceVariableWriteNode, LocalVariableAndWriteNode, LocalVariableOperatorWriteNode,
    LocalVariableOrWriteNode, LocalVariableTargetNode, LocalVariableWriteNode, ModuleNode, Node,
    SingletonClassNode, Visit,
};

#[derive(Debug, Clone, Default)]
pub struct AnalysisIndex {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
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
    method_context_stack: Vec<(RubyMethod, NamespaceKind)>,
    module_function_mode_stack: Vec<bool>,
    visibility_stack: Vec<MethodVisibility>,
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
            method_context_stack: Vec::new(),
            module_function_mode_stack: Vec::new(),
            visibility_stack: vec![MethodVisibility::Public],
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
            self.namespace_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer namespace stack underflow. \
                 This is a bug because each class/module entry must pop exactly the pushed parts. \
                 Fix: keep class/module visitor enter/exit balanced.",
            );
        }
        self.module_function_mode_stack.pop().expect(
            "INVARIANT VIOLATED: analysis indexer module_function mode stack underflow. \
             This is a bug because each namespace frame must pop exactly one module_function flag. \
             Fix: keep class/module visitor enter/exit balanced.",
        );
        self.visibility_stack.pop().expect(
            "INVARIANT VIOLATED: analysis indexer visibility stack underflow. \
             This is a bug because each namespace frame must pop exactly one visibility flag. \
             Fix: keep class/module visitor enter/exit balanced.",
        );
    }

    fn current_visibility(&self) -> MethodVisibility {
        self.visibility_stack.last().copied().unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: analysis indexer visibility stack is empty. \
                 This is a bug because the indexer starts with a root public visibility. \
                 Fix: initialize AnalysisIndexer with a root visibility frame."
            )
        })
    }

    fn set_current_visibility(&mut self, visibility: MethodVisibility) {
        let Some(current) = self.visibility_stack.last_mut() else {
            panic!(
                "INVARIANT VIOLATED: analysis indexer visibility stack is empty. \
                 This is a bug because the indexer starts with a root public visibility. \
                 Fix: initialize AnalysisIndexer with a root visibility frame."
            );
        };
        *current = visibility;
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

    fn push_included_hook_mixin_edges(&mut self, node: &CallNode<'_>) {
        if !self.inside_singleton_included_method() {
            return;
        }
        if node.receiver().is_none() {
            return;
        }

        let Some((kind, first_mixin_index)) = included_hook_mixin_call_kind(node, self.file_id)
        else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            return;
        };

        let source = FullyQualifiedName::namespace(self.namespace_stack.clone());
        let range = self.range(&node.location());
        for arg in arguments.arguments().iter().skip(first_mixin_index) {
            let Some((parts, absolute)) = constant_parts_and_absolute(&arg) else {
                continue;
            };
            self.push_edge(source.clone(), &parts, absolute, kind, range);
        }
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
        self.facts
            .methods
            .push(MethodFact::new(fqn, owner, range).with_visibility(self.current_visibility()));
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

    fn push_class_attribute_method_facts(&mut self, node: &CallNode<'_>) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        let namespace = node
            .receiver()
            .and_then(|receiver| self.resolve_constant_receiver_namespace(&receiver))
            .unwrap_or_else(|| self.namespace_stack.clone());

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };

            if let Ok(method) = RubyMethod::new(&name) {
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Singleton,
                    method,
                    range,
                );
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Instance,
                    method,
                    range,
                );
            }

            if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Singleton,
                    method,
                    range,
                );
                self.push_method_fact(
                    namespace.clone(),
                    crate::core::NamespaceKind::Instance,
                    method,
                    range,
                );
            }
        }
    }

    fn push_module_function_facts(&mut self, node: &CallNode<'_>) {
        let Some(arguments) = node.arguments() else {
            if let Some(mode) = self.module_function_mode_stack.last_mut() {
                *mode = true;
            }
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            if let Some(mode) = self.module_function_mode_stack.last_mut() {
                *mode = true;
            }
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
            self.facts.methods.push(
                MethodFact::new(fqn, owner, range).with_visibility(self.current_visibility()),
            );
        }
    }

    fn push_visibility_modifier(&mut self, node: &CallNode<'_>, visibility: MethodVisibility) {
        let Some(arguments) = node.arguments() else {
            self.set_current_visibility(visibility);
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.set_current_visibility(visibility);
            return;
        }

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = attr_name_and_range(&arg, self.file_id) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            self.set_method_visibility(method, visibility, range);
        }
    }

    fn set_method_visibility(
        &mut self,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.namespace_stack.clone(),
            match self.current_scope_kind() {
                ScopeKind::Instance => NamespaceKind::Instance,
                ScopeKind::Singleton => NamespaceKind::Singleton,
            },
        );
        self.facts
            .method_visibility_overrides
            .push(MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                visibility,
                range,
            ));
        for fact in &mut self.facts.methods {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                continue;
            };
            if *fact_method == method && fact.owner == owner {
                fact.visibility = visibility;
            }
        }
    }

    fn push_alias_method_call_fact(&mut self, node: &CallNode<'_>) {
        let Some((new_name, old_name)) = call_two_symbol_or_string_args(node, self.file_id) else {
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            return;
        };

        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        self.push_method_fact(self.namespace_stack.clone(), owner_kind, new_method, range);

        let old_fqn = FullyQualifiedName::method(self.namespace_stack.clone(), old_method);
        let new_fqn = FullyQualifiedName::method(
            self.namespace_stack.clone(),
            RubyMethod::new(&new_name).expect(
                "INVARIANT VIOLATED: alias_method new method became invalid after validation. \
                 This is a bug because the same string was already accepted. \
                 Fix: keep alias_method validation single-sourced.",
            ),
        );
        if let Some(old_type) = self
            .facts
            .types
            .iter()
            .find(|fact| fact.subject == TypeSubject::MethodReturn(old_fqn.clone()))
            .cloned()
        {
            self.facts.types.push(TypeFact::new(
                TypeSubject::MethodReturn(new_fqn),
                old_type.ruby_type,
                range,
                old_type.provenance,
            ));
        }
    }

    fn push_define_method_fact(&mut self, node: &CallNode<'_>) {
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        self.push_method_fact(self.namespace_stack.clone(), owner_kind, method, range);
    }

    fn push_send_define_method_fact(&mut self, node: &CallNode<'_>) {
        if node.name().as_slice() != b"send" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut args = arguments.arguments().iter();
        let Some((selector, _)) = args
            .next()
            .and_then(|arg| attr_name_and_range(&arg, self.file_id))
        else {
            return;
        };
        if selector != "define_method" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) else {
            return;
        };
        let Some((name, range)) = define_method_name_and_range(node, self.file_id, 1) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        self.push_method_fact(
            namespace,
            crate::core::NamespaceKind::Instance,
            method,
            range,
        );
    }

    fn resolve_constant_receiver_namespace(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if let Some(namespace) = self.resolve_const_get_receiver_namespace(receiver) {
            return Some(namespace);
        }

        let (parts, absolute) = constant_parts_and_absolute(receiver)?;
        if absolute {
            let fqn = FullyQualifiedName::namespace(parts.clone());
            return self.known_namespaces.contains(&fqn).then_some(parts);
        }

        let mut search = self.namespace_stack.clone();
        loop {
            let mut candidate = search.clone();
            candidate.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.known_namespaces.contains(&fqn) {
                return Some(candidate);
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.clone());
        self.known_namespaces.contains(&fqn).then_some(parts)
    }

    fn resolve_const_get_receiver_namespace(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        let call = receiver.as_call_node()?;
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        let Some(base_receiver) = call.receiver() else {
            return None;
        };
        let arguments = call.arguments()?;
        let first = arguments.arguments().iter().next()?;
        let (name, _) = attr_name_and_range(&first, self.file_id)?;
        let Ok(constant) = RubyConstant::new(&name) else {
            return None;
        };
        let mut namespace = self.resolve_constant_receiver_namespace(&base_receiver)?;
        namespace.push(constant);
        let fqn = FullyQualifiedName::namespace(namespace.clone());
        self.known_namespaces.contains(&fqn).then_some(namespace)
    }

    fn push_delegate_method_facts(&mut self, node: &CallNode<'_>) {
        let Some((methods, receiver_method)) = delegate_methods_and_receiver(node, self.file_id)
        else {
            return;
        };
        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for method_name in methods {
            let Ok(method) = RubyMethod::new(&method_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let owner =
                FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
            self.facts
                .symbols
                .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.current_visibility()),
            );
        }
    }

    fn push_forwardable_delegate_method_facts(&mut self, node: &CallNode<'_>) {
        let Some((receiver_method, methods)) =
            forwardable_delegates_and_receiver(node, self.file_id)
        else {
            return;
        };
        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };
        for (defined_name, _target_name) in methods {
            let Ok(method) = RubyMethod::new(&defined_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
            let owner =
                FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
            self.facts
                .symbols
                .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
            self.facts.methods.push(
                MethodFact::with_delegate_receiver(fqn, owner, range, receiver_method)
                    .with_visibility(self.current_visibility()),
            );
        }
    }

    fn static_eval_block_namespace(&self, node: &CallNode<'_>) -> Option<Vec<RubyConstant>> {
        if !matches!(node.name().as_slice(), b"class_eval" | b"module_eval") {
            return None;
        }
        node.block()?;
        let receiver = node.receiver()?;
        let (parts, absolute) = constant_parts_and_absolute(&receiver)?;
        self.resolve_static_eval_namespace(&parts, absolute)
    }

    fn resolve_static_eval_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<Vec<RubyConstant>> {
        if parts.is_empty() {
            return None;
        }
        if absolute {
            let fqn = FullyQualifiedName::namespace(parts.to_vec());
            return self.known_namespaces.contains(&fqn).then(|| parts.to_vec());
        }

        let mut search = self.namespace_stack.clone();
        loop {
            let mut candidate = search.clone();
            candidate.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.known_namespaces.contains(&fqn) {
                return Some(candidate);
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.known_namespaces.contains(&fqn).then(|| parts.to_vec())
    }

    fn push_concern_class_methods_block(
        &mut self,
        node: &CallNode<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
            return None;
        }
        node.block()?;
        if self.namespace_stack.is_empty() {
            return None;
        }

        let class_methods = RubyConstant::new("ClassMethods").expect(
            "INVARIANT VIOLATED: static Concern ClassMethods constant is invalid. \
             This is a bug because `ClassMethods` is a valid Ruby constant. \
             Fix: inspect RubyConstant validation.",
        );
        let mut target_namespace = self.namespace_stack.clone();
        target_namespace.push(class_methods);
        let range = self.range(&node.location());
        self.push_namespace_facts(
            FullyQualifiedName::namespace(target_namespace),
            GraphNodeKind::Module,
            range,
        );
        self.push_edge(
            FullyQualifiedName::namespace(self.namespace_stack.clone()),
            &[class_methods],
            false,
            GraphEdgeKind::Extend,
            range,
        );

        Some(vec![class_methods])
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
            ScopeKind::Instance => NamespaceKind::Instance,
            ScopeKind::Singleton => NamespaceKind::Singleton,
        };
        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                owner_kind = NamespaceKind::Singleton;
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
            owner_kind = NamespaceKind::Singleton;
        }

        let fqn = FullyQualifiedName::method(self.namespace_stack.clone(), method);
        let owner =
            FullyQualifiedName::namespace_with_kind(self.namespace_stack.clone(), owner_kind);
        let range = self.range(&node.location());
        let params = method_param_facts(node);
        self.facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.facts.methods.push(
            MethodFact::with_param_facts(fqn.clone(), owner, range, params.clone())
                .with_visibility(self.current_visibility()),
        );
        if node.receiver().is_none()
            && owner_kind == NamespaceKind::Instance
            && self
                .module_function_mode_stack
                .last()
                .copied()
                .unwrap_or(false)
        {
            let owner = FullyQualifiedName::namespace_with_kind(
                self.namespace_stack.clone(),
                NamespaceKind::Singleton,
            );
            self.facts.methods.push(
                MethodFact::with_param_facts(fqn.clone(), owner, range, params)
                    .with_visibility(self.current_visibility()),
            );
        }
        if let Some(return_type) = method_body_literal_type(node) {
            self.facts.types.push(TypeFact::new(
                TypeSubject::MethodReturn(fqn.clone()),
                return_type,
                range,
                TypeProvenance::Inferred,
            ));
        }

        self.method_context_stack.push((method, owner_kind));
        visit_def_node(self, node);
        self.method_context_stack.pop().expect(
            "INVARIANT VIOLATED: analysis indexer method context stack underflow. \
             This is a bug because each pushed method context must pop after visiting the method body. \
             Fix: keep visit_def_node method context push/pop balanced.",
        );
    }

    fn visit_alias_method_node(&mut self, node: &AliasMethodNode<'_>) {
        let Some((new_name, old_name)) = alias_method_names(node) else {
            visit_alias_method_node(self, node);
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            visit_alias_method_node(self, node);
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            visit_alias_method_node(self, node);
            return;
        };

        let owner_kind = match self.current_scope_kind() {
            ScopeKind::Instance => crate::core::NamespaceKind::Instance,
            ScopeKind::Singleton => crate::core::NamespaceKind::Singleton,
        };
        let range = self.range(&node.location());
        self.push_method_fact(self.namespace_stack.clone(), owner_kind, new_method, range);

        let old_fqn = FullyQualifiedName::method(self.namespace_stack.clone(), old_method);
        let new_fqn = FullyQualifiedName::method(
            self.namespace_stack.clone(),
            RubyMethod::new(&new_name).expect(
                "INVARIANT VIOLATED: alias new method became invalid after validation. \
                 This is a bug because the same string was already accepted. \
                 Fix: keep alias method validation single-sourced.",
            ),
        );
        if let Some(old_type) = self
            .facts
            .types
            .iter()
            .find(|fact| fact.subject == TypeSubject::MethodReturn(old_fqn.clone()))
            .cloned()
        {
            self.facts.types.push(TypeFact::new(
                TypeSubject::MethodReturn(new_fqn),
                old_type.ruby_type,
                range,
                old_type.provenance,
            ));
        }

        visit_alias_method_node(self, node);
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
        if let Some(eval_namespace) = self.static_eval_block_namespace(node) {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                let old_namespace = std::mem::replace(&mut self.namespace_stack, eval_namespace);
                self.scope_stack.push(ScopeKind::Instance);
                self.visit(&block);
                self.scope_stack.pop();
                self.namespace_stack = old_namespace;
            }
            return;
        }
        if let Some(class_methods_namespace) = self.push_concern_class_methods_block(node) {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.namespace_stack
                    .extend(class_methods_namespace.iter().cloned());
                self.module_function_mode_stack.push(false);
                self.visibility_stack.push(MethodVisibility::Public);
                self.scope_stack.push(ScopeKind::Instance);
                self.visit(&block);
                self.scope_stack.pop();
                self.pop_namespace_parts(&class_methods_namespace);
            }
            return;
        }

        self.push_included_hook_mixin_edges(node);

        match node.name().as_slice() {
            b"class_attribute" => self.push_class_attribute_method_facts(node),
            _ => {}
        }

        if node.receiver().is_none() {
            match node.name().as_slice() {
                b"attr_reader" => self.push_attr_method_facts(node, true, false),
                b"attr_writer" => self.push_attr_method_facts(node, false, true),
                b"attr_accessor" => self.push_attr_method_facts(node, true, true),
                b"module_function" => self.push_module_function_facts(node),
                b"private" => self.push_visibility_modifier(node, MethodVisibility::Private),
                b"protected" => self.push_visibility_modifier(node, MethodVisibility::Protected),
                b"public" => self.push_visibility_modifier(node, MethodVisibility::Public),
                b"alias_method" => self.push_alias_method_call_fact(node),
                b"define_method" => self.push_define_method_fact(node),
                b"delegate" => self.push_delegate_method_facts(node),
                b"def_delegator" | b"def_delegators" => {
                    self.push_forwardable_delegate_method_facts(node)
                }
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
                let in_singleton = self.current_scope_kind() == ScopeKind::Singleton;
                let source_for_edge = if in_singleton {
                    source.to_singleton_namespace().expect(
                        "INVARIANT VIOLATED: singleton class mixin source could not convert to singleton namespace. \
                         This is a bug because class << self can only appear inside a namespace. \
                         Fix: guard singleton mixin indexing to namespace scopes.",
                    )
                } else {
                    source.clone()
                };
                let range = self.range(&node.location());
                for arg in arguments.arguments().iter() {
                    let mixin_ref = if arg.as_self_node().is_some() {
                        Some((source.namespace_parts(), true))
                            .filter(|(parts, _)| !parts.is_empty())
                    } else {
                        constant_parts_and_absolute(&arg)
                    };
                    if let Some((parts, absolute)) = mixin_ref {
                        self.push_edge(source_for_edge.clone(), &parts, absolute, kind, range);
                        if kind == GraphEdgeKind::Extend && !in_singleton {
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
        } else {
            self.push_send_define_method_fact(node);
        }

        visit_call_node(self, node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode<'_>) {
        self.scope_stack.push(ScopeKind::Singleton);
        self.visibility_stack.push(MethodVisibility::Public);
        visit_singleton_class_node(self, node);
        self.visibility_stack.pop().expect(
            "INVARIANT VIOLATED: analysis indexer visibility stack underflow on singleton exit. \
             This is a bug because every singleton class visit must pop exactly one visibility flag. \
             Fix: keep singleton visitor enter/exit balanced.",
        );
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

fn included_hook_mixin_call_kind(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(GraphEdgeKind, usize)> {
    match node.name().as_slice() {
        b"include" => Some((GraphEdgeKind::Include, 0)),
        b"extend" => Some((GraphEdgeKind::Extend, 0)),
        b"send" | b"public_send" | b"__send__" => {
            let arguments = node.arguments()?;
            let first = arguments.arguments().iter().next()?;
            let (selector, _) = attr_name_and_range(&first, file_id)?;
            match selector.as_str() {
                "include" => Some((GraphEdgeKind::Include, 1)),
                "extend" => Some((GraphEdgeKind::Extend, 1)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn define_method_name_and_range(
    node: &CallNode<'_>,
    file_id: SourceFileId,
    name_index: usize,
) -> Option<(String, TextRange)> {
    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().nth(name_index)?;
    if let Some(symbol) = arg.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &location),
        ));
    }
    attr_name_and_range(&arg, file_id)
}

fn call_two_symbol_or_string_args(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(String, String)> {
    let arguments = node.arguments()?;
    let args = arguments.arguments();
    let mut iter = args.iter();
    let (new_name, _) = attr_name_and_range(&iter.next()?, file_id)?;
    let (old_name, _) = attr_name_and_range(&iter.next()?, file_id)?;
    Some((new_name, old_name))
}

fn delegate_methods_and_receiver(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(Vec<String>, String)> {
    let arguments = node.arguments()?;
    let mut methods = Vec::new();
    let mut receiver = None;
    for arg in arguments.arguments().iter() {
        if let Some(keyword_hash) = arg.as_keyword_hash_node() {
            for element in keyword_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some((key, _)) = attr_name_and_range(&assoc.key(), file_id) else {
                    continue;
                };
                if key.trim_end_matches(':') == "to" {
                    receiver = attr_name_and_range(&assoc.value(), file_id).map(|(name, _)| name);
                }
            }
        } else if let Some((name, _range)) = attr_name_and_range(&arg, file_id) {
            methods.push(name);
        }
    }

    let receiver = receiver?;
    (!methods.is_empty()).then_some((methods, receiver))
}

fn forwardable_delegates_and_receiver(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(String, Vec<(String, String)>)> {
    let arguments = node.arguments()?;
    let mut args = arguments.arguments().iter();
    let (receiver, _) = attr_name_and_range(&args.next()?, file_id)?;
    let mut methods = Vec::new();

    match node.name().as_slice() {
        b"def_delegators" => {
            for arg in args {
                let Some((name, _)) = attr_name_and_range(&arg, file_id) else {
                    continue;
                };
                methods.push((name.clone(), name));
            }
        }
        b"def_delegator" => {
            let (target_name, _) = attr_name_and_range(&args.next()?, file_id)?;
            let defined_name = args
                .next()
                .and_then(|arg| attr_name_and_range(&arg, file_id).map(|(name, _)| name))
                .unwrap_or_else(|| target_name.clone());
            methods.push((defined_name, target_name));
        }
        _ => return None,
    }

    (!methods.is_empty()).then_some((receiver, methods))
}

fn symbol_name_and_range(node: &Node<'_>, file_id: SourceFileId) -> Option<(String, TextRange)> {
    node.as_symbol_node().map(|symbol| {
        (
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &symbol.location()),
        )
    })
}

fn alias_method_names(node: &AliasMethodNode<'_>) -> Option<(String, String)> {
    let new_name = symbol_name(&node.new_name())?;
    let old_name = symbol_name(&node.old_name())?;
    Some((new_name, old_name))
}

fn symbol_name(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
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
    fn indexes_bare_module_function_following_methods() {
        let index = AnalysisIndexer::new(file())
            .index_source("module Utils\n  module_function\n  def helper\n  end\nend\n");

        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "Utils#helper"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "Utils#helper"
                && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
        }));
    }

    #[test]
    fn indexes_class_attribute_methods() {
        let index = AnalysisIndexer::new(file())
            .index_source("class Worker\n  class_attribute :queue_config\nend\n");

        for kind in [
            crate::core::NamespaceKind::Instance,
            crate::core::NamespaceKind::Singleton,
        ] {
            assert!(index.methods.iter().any(|fact| {
                fact.fqn.to_string() == "Worker#queue_config"
                    && fact.owner.namespace_kind() == Some(kind)
            }));
            assert!(index.methods.iter().any(|fact| {
                fact.fqn.to_string() == "Worker#queue_config="
                    && fact.owner.namespace_kind() == Some(kind)
            }));
        }
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
