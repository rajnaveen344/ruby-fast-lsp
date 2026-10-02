//! Prism traversal that dispatches declaration, call, and variable nodes to
//! the fact producers.

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind,
    MethodAvailability, MethodFact, MethodVisibility, NamespaceKind, RubyConstant, RubyMethod,
    SymbolFact, SymbolKind, TypeFact, TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{
    visit_alias_method_node, visit_block_node, visit_call_node, visit_class_node,
    visit_class_variable_and_write_node, visit_class_variable_operator_write_node,
    visit_class_variable_or_write_node, visit_class_variable_target_node,
    visit_class_variable_write_node, visit_constant_path_target_node,
    visit_constant_path_write_node, visit_constant_write_node, visit_def_node,
    visit_global_variable_and_write_node, visit_global_variable_operator_write_node,
    visit_global_variable_or_write_node, visit_global_variable_target_node,
    visit_global_variable_write_node, visit_instance_variable_and_write_node,
    visit_instance_variable_operator_write_node, visit_instance_variable_or_write_node,
    visit_instance_variable_target_node, visit_instance_variable_write_node,
    visit_local_variable_and_write_node, visit_local_variable_operator_write_node,
    visit_local_variable_or_write_node, visit_local_variable_target_node,
    visit_local_variable_write_node, visit_module_node, visit_singleton_class_node,
    AliasMethodNode, BlockNode, CallNode, ClassNode, ClassVariableAndWriteNode,
    ClassVariableOperatorWriteNode, ClassVariableOrWriteNode, ClassVariableTargetNode,
    ClassVariableWriteNode, ConstantPathTargetNode, ConstantPathWriteNode, ConstantTargetNode,
    ConstantWriteNode, DefNode, GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode,
    GlobalVariableOrWriteNode, GlobalVariableTargetNode, GlobalVariableWriteNode,
    InstanceVariableAndWriteNode, InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode,
    InstanceVariableTargetNode, InstanceVariableWriteNode, LocalVariableAndWriteNode,
    LocalVariableOperatorWriteNode, LocalVariableOrWriteNode, LocalVariableTargetNode,
    LocalVariableWriteNode, ModuleNode, MultiWriteNode, SingletonClassNode, Visit,
};

use super::syntax::{
    alias_method_names, class_implicitly_inherits_object, constant_parts_and_absolute,
    method_param_facts, terminal_name_range,
};
use super::types::{literal_type, method_body_literal_type};
use super::AnalysisIndexer;
use crate::indexer::documents::scope_rules::{
    alias_reopen_target, eval_block, method_declaration, multi_write_targets,
    namespace_is_proven_class, self_definition_namespace, DefinitionVisibility, MethodDeclaration,
};
use crate::indexer::yard::parser::YardParser;
use crate::indexer::yard::types::YardMethodDoc;
use crate::indexer::{is_framework_instance_block_call_name, LocalScopeKind};

impl AnalysisIndexer {
    /// Visit a call's receiver, arguments, and block. A receiverless
    /// framework block such as `included do … end` runs on the class.
    fn visit_call_children(&mut self, node: &CallNode<'_>) {
        if node.receiver().is_some()
            || !is_framework_instance_block_call_name(node.name().as_slice())
        {
            visit_call_node(self, node);
            return;
        }
        if let Some(arguments) = node.arguments() {
            self.visit_arguments_node(&arguments);
        }
        if let Some(block) = node.block() {
            self.scope
                .push_scope_kind(LocalScopeKind::FrameworkInstanceBlock);
            self.visit(&block);
            self.scope.pop_scope_kind();
        }
    }
}

impl Visit<'_> for AnalysisIndexer {
    fn visit_class_node(&mut self, node: &ClassNode<'_>) {
        let lexical_context = self.scope.get_ns_stack();
        let has_explicit_superclass = node.superclass().is_some();
        let superclass = node.superclass().and_then(|superclass| {
            let (parts, absolute) = constant_parts_and_absolute(&superclass)?;
            let super_range = self.range(&superclass.location());
            let target = self.resolve_namespace_from(&parts, absolute, &lexical_context);
            Some((parts, absolute, super_range, target))
        });
        let reopened_target = alias_reopen_target(
            &node.constant_path(),
            GraphNodeKind::Class,
            superclass
                .as_ref()
                .and_then(|(_, _, _, target)| target.as_ref()),
            &lexical_context,
            |candidates| self.first_constant_value_type(candidates),
        );
        if let Some(target) = &reopened_target {
            self.scope.push_absolute_ns_scopes(target.namespace_parts());
        } else if !self.enter_namespace_from_node(&node.constant_path()) {
            return;
        }

        let fqn = FullyQualifiedName::namespace(self.scope.get_ns_stack());
        let range = self.range(&node.location());
        let name_range = terminal_name_range(
            self.file_id,
            &node.constant_path().location(),
            node.name().as_slice(),
        );
        if reopened_target.is_none() {
            self.push_namespace_facts(fqn.clone(), GraphNodeKind::Class, range, name_range);
        }

        if let Some((parts, absolute, super_range, target)) = superclass {
            if let Some(target) = target {
                self.facts.graph_edges.push(GraphEdgeFact::new(
                    fqn.clone(),
                    target.clone(),
                    GraphEdgeKind::Superclass,
                    super_range,
                ));
                if let (Some(source_singleton), Some(target_singleton)) = (
                    fqn.to_singleton_namespace(),
                    target.to_singleton_namespace(),
                ) {
                    self.facts.graph_edges.push(GraphEdgeFact::new(
                        source_singleton,
                        target_singleton,
                        GraphEdgeKind::Superclass,
                        super_range,
                    ));
                }
            } else {
                self.facts
                    .unresolved_graph_edges
                    .push(UnresolvedGraphEdgeFact::new(
                        fqn.clone(),
                        parts,
                        absolute,
                        FullyQualifiedName::namespace(lexical_context),
                        GraphEdgeKind::Superclass,
                        super_range,
                    ));
            }
        } else if reopened_target.is_none()
            && !has_explicit_superclass
            && class_implicitly_inherits_object(&fqn)
        {
            let object = RubyConstant::new("Object").expect_invariant(
                "Object is not a valid Ruby constant",
                "ruby's implicit class superclass must be representable",
                "update RubyConstant validation or implicit superclass construction",
            );
            self.push_edge_with_provenance(
                fqn.clone(),
                &[object],
                true,
                GraphEdgeKind::Superclass,
                GraphEdgeProvenance::ImplicitObject,
                range,
            );
        }

        self.scope.push_scope_kind(LocalScopeKind::Constant);
        visit_class_node(self, node);
        self.scope.pop_scope_kind();
        self.scope.pop_ns_scope();
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        let reopened_target = alias_reopen_target(
            &node.constant_path(),
            GraphNodeKind::Module,
            None,
            &self.scope.get_ns_stack(),
            |candidates| self.first_constant_value_type(candidates),
        );
        if let Some(target) = &reopened_target {
            self.scope.push_absolute_ns_scopes(target.namespace_parts());
        } else if !self.enter_namespace_from_node(&node.constant_path()) {
            return;
        }

        let fqn = FullyQualifiedName::namespace(self.scope.get_ns_stack());
        let range = self.range(&node.location());
        let name_range = terminal_name_range(
            self.file_id,
            &node.constant_path().location(),
            node.name().as_slice(),
        );
        if reopened_target.is_none() {
            self.push_namespace_facts(fqn, GraphNodeKind::Module, range, name_range);
        }

        self.scope.push_scope_kind(LocalScopeKind::Constant);
        visit_module_node(self, node);
        self.scope.pop_scope_kind();
        self.scope.pop_ns_scope();
    }

    fn visit_def_node(&mut self, node: &DefNode<'_>) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let Ok(method) = RubyMethod::new(&method_name) else {
            visit_def_node(self, node);
            return;
        };

        let (definition_namespace, definition_kind) = self.scope.method_definition_context();
        let mut owner_kind = definition_kind;
        let mut owner_namespace = definition_namespace.clone();
        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                let Some(namespace) = self_definition_namespace(&self.scope) else {
                    visit_def_node(self, node);
                    return;
                };
                owner_namespace = namespace;
                owner_kind = NamespaceKind::Singleton;
            } else if let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) {
                owner_namespace = namespace;
                owner_kind = NamespaceKind::Singleton;
            } else {
                visit_def_node(self, node);
                return;
            }
        }
        // The seed knows no namespace kinds from other files, so only a
        // same-file class declaration proves a constructor.
        let definition_fqn = FullyQualifiedName::namespace(owner_namespace.clone());
        let proven_class = namespace_is_proven_class(
            self.facts
                .graph_nodes
                .iter()
                .filter(|fact| fact.fqn == definition_fqn)
                .map(|fact| fact.kind),
            || None,
        );
        // `self` in the body follows the receiver, even where the declaration
        // moves to another side, as `initialize` becomes the singleton `new`.
        let body_kind = owner_kind;
        let MethodDeclaration {
            method,
            kind: owner_kind,
            visibility,
            module_function_copy,
        } = method_declaration(
            method,
            node.receiver().is_none(),
            owner_kind,
            proven_class,
            DefinitionVisibility {
                default: self.scope.current_visibility(),
                module_function_mode: self.scope.module_function_mode_enabled(),
            },
        );

        let fqn = FullyQualifiedName::method(owner_namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(owner_namespace.clone(), owner_kind);
        let range = self.range(&node.location());
        let name_range = self.range(&node.name_loc());
        let yard_doc = self.source.as_deref().and_then(|source| {
            YardParser::extract_from_source(source, node.location().start_offset())
        });
        let params = method_param_facts(node)
            .into_iter()
            .map(|param| {
                let yard_param = yard_doc
                    .as_ref()
                    .and_then(|doc| doc.find_param(&param.name));
                param.with_signature_metadata(
                    yard_param.and_then(|param| param.format_type()),
                    yard_param.and_then(|param| param.description.clone()),
                )
            })
            .collect::<Vec<_>>();
        let forwarded_block_call =
            crate::indexer::lowering::forwarded_block::direct_forwarded_block_call(node);
        let direct_yield_call = crate::indexer::lowering::forwarded_block::direct_yield_call(node);
        let availability = match yard_doc.as_ref() {
            Some(doc) => match (&doc.unavailable, &doc.absent) {
                (Some(reason), None) => MethodAvailability::Unavailable {
                    reason: reason.clone(),
                },
                (None, Some(reason)) => MethodAvailability::Absent {
                    reason: reason.clone(),
                },
                (None, None) => MethodAvailability::Available,
                (Some(_), Some(_)) => unreachable_invariant!(
                    what = "method `{method}` is marked both @unavailable and @absent",
                    why = "a runtime API cannot simultaneously exist-but-fail and not exist",
                    fix = "retain exactly one availability annotation in the owning stub",
                    method = method,
                ),
            },
            None => MethodAvailability::Available,
        };
        self.facts.symbols.push(
            SymbolFact::new(fqn.clone(), SymbolKind::Method, range).with_name_range(name_range),
        );
        self.facts.methods.push(
            MethodFact::with_param_facts(fqn.clone(), owner, range, params.clone())
                .with_name_range(name_range)
                .with_signature_metadata(
                    yard_doc.as_ref().and_then(|doc| doc.description.clone()),
                    yard_doc
                        .as_ref()
                        .and_then(YardMethodDoc::format_return_type),
                )
                .with_availability(availability.clone())
                .with_visibility(visibility)
                .with_forwarded_block_call(forwarded_block_call.clone())
                .with_direct_yield_call(direct_yield_call.clone()),
        );
        if module_function_copy {
            let owner = FullyQualifiedName::namespace_with_kind(
                owner_namespace.clone(),
                NamespaceKind::Singleton,
            );
            self.facts.methods.push(
                MethodFact::with_param_facts(fqn.clone(), owner, range, params)
                    .with_name_range(name_range)
                    .with_signature_metadata(
                        yard_doc.as_ref().and_then(|doc| doc.description.clone()),
                        yard_doc
                            .as_ref()
                            .and_then(YardMethodDoc::format_return_type),
                    )
                    .with_availability(availability)
                    .with_visibility(MethodVisibility::Public)
                    .with_forwarded_block_call(forwarded_block_call)
                    .with_direct_yield_call(direct_yield_call),
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

        // `self` in the body is the receiver; a nested definition still
        // lands in the enclosing owner.
        self.scope.push_scope_kind(match body_kind {
            NamespaceKind::Singleton => LocalScopeKind::ClassMethod,
            NamespaceKind::Instance => LocalScopeKind::InstanceMethod,
        });
        self.scope.push_method_fqn(fqn, owner_kind);
        self.scope.push_method_execution_context(
            owner_namespace,
            body_kind,
            definition_namespace,
            definition_kind,
        );
        visit_def_node(self, node);
        self.scope.pop_execution_context();
        self.scope.pop_method_fqn();
        self.scope.pop_scope_kind();
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

        let owner_kind = self.owner_kind();
        let range = self.range(&node.location());
        self.push_method_fact_without_parameter_shape(
            self.owner_namespace(),
            owner_kind,
            new_method,
            range,
        );

        let old_fqn = FullyQualifiedName::method(self.owner_namespace(), old_method);
        let new_fqn = FullyQualifiedName::method(
            self.owner_namespace(),
            RubyMethod::new(&new_name).expect_invariant(
                "alias new method became invalid after validation",
                "the same string was already accepted",
                "keep alias method validation single-sourced",
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
        self.push_constant_declaration(
            node.name().as_slice(),
            node.name_loc(),
            node.location(),
            Some(&node.value()),
        );
        visit_constant_write_node(self, node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode<'_>) {
        let target = node.target();
        if let Some(name) = target.name() {
            self.push_constant_path_declaration(
                target.parent().as_ref(),
                name.as_slice(),
                target.location(),
                node.location(),
                Some(&node.value()),
            );
        }
        visit_constant_path_write_node(self, node);
    }

    fn visit_constant_target_node(&mut self, node: &ConstantTargetNode<'_>) {
        self.push_constant_target_declaration(&node.as_node(), None);
    }

    fn visit_constant_path_target_node(&mut self, node: &ConstantPathTargetNode<'_>) {
        self.push_constant_target_declaration(&node.as_node(), None);
        visit_constant_path_target_node(self, node);
    }

    fn visit_multi_write_node(&mut self, node: &MultiWriteNode<'_>) {
        for (target, value) in multi_write_targets(node) {
            if !self.push_constant_target_declaration(&target, value.as_ref()) {
                self.visit(&target);
            }
        }
        self.visit(&node.value());
    }

    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        let eval = eval_block(node, &self.scope, |receiver| {
            self.resolve_constant_receiver_namespace(receiver)
        });
        if let Some(execution) = eval {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                execution.enter(&mut self.scope);
                self.visit(&block);
                self.scope.pop_execution_context();
            }
            return;
        }
        if let Some(execution) = self.push_concern_class_methods_block(node) {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                execution.enter(&mut self.scope);
                self.visit(&block);
                self.scope.pop_execution_context();
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
                b"define_singleton_method" => self.push_define_singleton_method_fact(node),
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
                let source = FullyQualifiedName::namespace(self.owner_namespace());
                let in_singleton = self.owner_kind() == NamespaceKind::Singleton;
                let source_for_edge = if in_singleton {
                    source.to_singleton_namespace().expect_invariant(
                        "singleton class mixin source could not convert to singleton namespace",
                        "class << self can only appear inside a namespace",
                        "guard singleton mixin indexing to namespace scopes",
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
            self.push_send_dynamic_method_fact(node);
            self.push_receiver_define_singleton_method_fact(node);
        }

        self.visit_call_children(node);
    }

    fn visit_block_node(&mut self, node: &BlockNode<'_>) {
        self.scope.push_scope_kind(LocalScopeKind::Block);
        visit_block_node(self, node);
        self.scope.pop_scope_kind();
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode<'_>) {
        self.scope.enter_singleton();
        visit_singleton_class_node(self, node);
        self.scope.exit_singleton();
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
