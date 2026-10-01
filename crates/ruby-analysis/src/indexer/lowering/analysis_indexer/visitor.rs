//! Prism traversal that dispatches declaration, call, and variable nodes to
//! the fact producers.

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind,
    MethodAvailability, MethodFact, MethodVisibility, NamespaceKind, RubyConstant, RubyMethod,
    RubyType, SymbolFact, SymbolKind, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
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
    ClassVariableWriteNode, ConstantPathWriteNode, ConstantWriteNode, DefNode,
    GlobalVariableAndWriteNode, GlobalVariableOperatorWriteNode, GlobalVariableOrWriteNode,
    GlobalVariableTargetNode, GlobalVariableWriteNode, InstanceVariableAndWriteNode,
    InstanceVariableOperatorWriteNode, InstanceVariableOrWriteNode, InstanceVariableTargetNode,
    InstanceVariableWriteNode, LocalVariableAndWriteNode, LocalVariableOperatorWriteNode,
    LocalVariableOrWriteNode, LocalVariableTargetNode, LocalVariableWriteNode, ModuleNode,
    SingletonClassNode, Visit,
};

use super::syntax::{
    alias_method_names, class_implicitly_inherits_object, constant_parts_and_absolute,
    constant_path_parts, method_param_facts, terminal_name_range,
};
use super::types::{literal_type, method_body_literal_type};
use super::{AnalysisIndexer, ScopeKind};
use crate::indexer::yard::{YardMethodDoc, YardParser};

impl Visit<'_> for AnalysisIndexer {
    fn visit_class_node(&mut self, node: &ClassNode<'_>) {
        let lexical_context = self.namespace_stack.clone();
        let syntactic_fqn = syntactic_namespace(&node.constant_path(), &lexical_context);
        let mut reopened_target = constant_parts_and_absolute(&node.constant_path())
            .and_then(|(parts, absolute)| {
                self.resolve_declaration_constant_value_type_from(
                    &parts,
                    absolute,
                    &lexical_context,
                )
            })
            .and_then(|ruby_type| match ruby_type {
                RubyType::ClassReference(target) => target.to_instance_namespace(),
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::ModuleReference(_)
                | RubyType::Literal(_)
                | RubyType::Array(_)
                | RubyType::Hash(_, _)
                | RubyType::Shape(_)
                | RubyType::Union(_)
                | RubyType::Unknown => None,
            });
        if syntactic_fqn.as_ref() == reopened_target.as_ref() {
            reopened_target = None;
        }
        let (parts, previous_namespace) = if let Some(target) = &reopened_target {
            let parts = target.namespace_parts().to_vec();
            assert!(
                !parts.is_empty(),
                "INVARIANT VIOLATED: a resolved class alias target has an empty namespace. \
                 This is a bug because a Ruby class object must have a constant identity. \
                 Fix: reject root namespace values before class alias reopening."
            );
            let previous = std::mem::replace(&mut self.namespace_stack, parts.clone());
            self.module_function_mode_stack.push(false);
            self.visibility_stack.push(MethodVisibility::Public);
            (parts, Some(previous))
        } else {
            let Some(parts) = self.push_namespace_from_node(&node.constant_path()) else {
                return;
            };
            (parts, None)
        };

        let fqn = FullyQualifiedName::namespace(self.namespace_stack.clone());
        let range = self.range(&node.location());
        let name_range = terminal_name_range(
            self.file_id,
            &node.constant_path().location(),
            node.name().as_slice(),
        );
        let has_explicit_superclass = node.superclass().is_some();
        let superclass = node.superclass().and_then(|superclass| {
            let (parts, absolute) = constant_parts_and_absolute(&superclass)?;
            let super_range = self.range(&superclass.location());
            let target = self.resolve_namespace_from(&parts, absolute, &lexical_context);
            Some((parts, absolute, super_range, target))
        });
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
            let object = RubyConstant::new("Object").expect(
                "INVARIANT VIOLATED: Object is not a valid Ruby constant. \
                 This is a bug because Ruby's implicit class superclass must be representable. \
                 Fix: update RubyConstant validation or implicit superclass construction.",
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

        self.scope_stack.push(ScopeKind::Instance);
        visit_class_node(self, node);
        self.scope_stack.pop();
        if let Some(previous) = previous_namespace {
            self.module_function_mode_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer module_function mode stack underflow after an aliased class reopening. \
                 This is a bug because every aliased namespace frame owns one module_function flag. \
                 Fix: keep aliased class visitor enter/exit balanced.",
            );
            self.visibility_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer visibility stack underflow after an aliased class reopening. \
                 This is a bug because every aliased namespace frame owns one visibility flag. \
                 Fix: keep aliased class visitor enter/exit balanced.",
            );
            self.namespace_stack = previous;
        } else {
            self.pop_namespace_parts(&parts);
        }
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        let lexical_context = self.namespace_stack.clone();
        let syntactic_fqn = syntactic_namespace(&node.constant_path(), &lexical_context);
        let mut reopened_target = constant_parts_and_absolute(&node.constant_path())
            .and_then(|(parts, absolute)| {
                self.resolve_declaration_constant_value_type_from(
                    &parts,
                    absolute,
                    &lexical_context,
                )
            })
            .and_then(|ruby_type| match ruby_type {
                RubyType::ModuleReference(target) => target.to_instance_namespace(),
                RubyType::Class(_)
                | RubyType::ClassReference(_)
                | RubyType::Module(_)
                | RubyType::Literal(_)
                | RubyType::Array(_)
                | RubyType::Hash(_, _)
                | RubyType::Shape(_)
                | RubyType::Union(_)
                | RubyType::Unknown => None,
            });
        if syntactic_fqn.as_ref() == reopened_target.as_ref() {
            reopened_target = None;
        }
        let (parts, previous_namespace) = if let Some(target) = &reopened_target {
            let parts = target.namespace_parts().to_vec();
            assert!(
                !parts.is_empty(),
                "INVARIANT VIOLATED: a resolved module alias target has an empty namespace. \
                 This is a bug because a Ruby module object must have a constant identity. \
                 Fix: reject root namespace values before module alias reopening."
            );
            let previous = std::mem::replace(&mut self.namespace_stack, parts.clone());
            self.module_function_mode_stack.push(false);
            self.visibility_stack.push(MethodVisibility::Public);
            (parts, Some(previous))
        } else {
            let Some(parts) = self.push_namespace_from_node(&node.constant_path()) else {
                return;
            };
            (parts, None)
        };

        let fqn = FullyQualifiedName::namespace(self.namespace_stack.clone());
        let range = self.range(&node.location());
        let name_range = terminal_name_range(
            self.file_id,
            &node.constant_path().location(),
            node.name().as_slice(),
        );
        if reopened_target.is_none() {
            self.push_namespace_facts(fqn, GraphNodeKind::Module, range, name_range);
        }

        self.scope_stack.push(ScopeKind::Instance);
        visit_module_node(self, node);
        self.scope_stack.pop();
        if let Some(previous) = previous_namespace {
            self.module_function_mode_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer module_function mode stack underflow after an aliased module reopening. \
                 This is a bug because every aliased namespace frame owns one module_function flag. \
                 Fix: keep aliased module visitor enter/exit balanced.",
            );
            self.visibility_stack.pop().expect(
                "INVARIANT VIOLATED: analysis indexer visibility stack underflow after an aliased module reopening. \
                 This is a bug because every aliased namespace frame owns one visibility flag. \
                 Fix: keep aliased module visitor enter/exit balanced.",
            );
            self.namespace_stack = previous;
        } else {
            self.pop_namespace_parts(&parts);
        }
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
                (Some(_), Some(_)) => panic!(
                    "INVARIANT VIOLATED: method `{method}` is marked both @unavailable and @absent. \
                     This is a bug because a runtime API cannot simultaneously exist-but-fail and not exist. \
                     Fix: retain exactly one availability annotation in the owning stub."
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
                .with_visibility(self.current_visibility())
                .with_forwarded_block_call(forwarded_block_call.clone())
                .with_direct_yield_call(direct_yield_call.clone()),
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
                    .with_name_range(name_range)
                    .with_signature_metadata(
                        yard_doc.as_ref().and_then(|doc| doc.description.clone()),
                        yard_doc
                            .as_ref()
                            .and_then(YardMethodDoc::format_return_type),
                    )
                    .with_availability(availability)
                    .with_visibility(self.current_visibility())
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
        self.push_method_fact_without_parameter_shape(
            self.namespace_stack.clone(),
            owner_kind,
            new_method,
            range,
        );

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
            self.facts.symbols.push(
                SymbolFact::new(
                    fqn.clone(),
                    SymbolKind::Constant,
                    self.range(&node.location()),
                )
                .with_name_range(self.range(&node.name_loc())),
            );
            self.push_type_fact(
                TypeSubject::Constant(fqn),
                self.assignment_type(&node.value()),
                node.name_loc(),
            );
        }
        visit_constant_write_node(self, node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode<'_>) {
        let target = node.target();
        if let Some(parts) = constant_path_parts(&target) {
            let fqn = FullyQualifiedName::constant(parts);
            let name = target.name().expect(
                "INVARIANT VIOLATED: constant path write target has no terminal name. \
                 This is a bug because constant_path_parts accepted the same target. \
                 Fix: keep constant path extraction and name range derivation aligned.",
            );
            self.facts.symbols.push(
                SymbolFact::new(
                    fqn.clone(),
                    SymbolKind::Constant,
                    self.range(&node.location()),
                )
                .with_name_range(terminal_name_range(
                    self.file_id,
                    &target.location(),
                    name.as_slice(),
                )),
            );
            self.push_type_fact(
                TypeSubject::Constant(fqn),
                self.assignment_type(&node.value()),
                target.location(),
            );
        }
        visit_constant_path_write_node(self, node);
    }

    fn visit_call_node(&mut self, node: &CallNode<'_>) {
        if let Some((eval_namespace, definition_scope)) = self.static_eval_block_context(node) {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                let old_namespace = std::mem::replace(&mut self.namespace_stack, eval_namespace);
                self.eval_context_depths.push((
                    self.scope_stack.len().checked_add(1).expect(
                        "INVARIANT VIOLATED: analysis indexer scope depth overflowed while entering an eval block. This is a bug because source nesting cannot exceed usize address space. Fix: reject impossibly deep source before traversal.",
                    ),
                    self.method_context_stack.len(),
                ));
                self.scope_stack.push(definition_scope);
                self.visit(&block);
                self.scope_stack.pop();
                self.eval_context_depths.pop().expect(
                    "INVARIANT VIOLATED: analysis indexer eval-context stack underflow. This is a bug because every static eval block context must be popped exactly once. Fix: keep AnalysisIndexer::visit_call_node eval traversal balanced.",
                );
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
            self.push_send_dynamic_method_fact(node);
            self.push_receiver_define_singleton_method_fact(node);
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

/// The namespace a class or module declaration names before any alias is
/// followed. A constant whose value is this same namespace is an ordinary
/// reopening, not an alias, so the declaration still owns its facts.
fn syntactic_namespace(
    path: &ruby_prism::Node<'_>,
    lexical_context: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    constant_parts_and_absolute(path).map(|(parts, absolute)| {
        if absolute {
            FullyQualifiedName::namespace(parts)
        } else {
            let mut probe = lexical_context.to_vec();
            probe.extend(parts);
            FullyQualifiedName::namespace(probe)
        }
    })
}
