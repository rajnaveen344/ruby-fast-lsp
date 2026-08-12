use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind, RubyConstant, RubyType,
};
use crate::mixin_ref_from_node;
use crate::LocalScopeKind as LVScopeKind;
use log::error;
use ruby_prism::ClassNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) -> bool {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());
        let lexical_context = self.scope_tracker.get_ns_stack();
        let mut syntactic_scope = self.scope_tracker.clone();
        if syntactic_scope
            .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
            .is_err()
        {
            error!("Error creating namespace for class");
            return false;
        }
        let syntactic_fqn = FullyQualifiedName::namespace(syntactic_scope.get_ns_stack());
        let has_explicit_superclass = node.superclass().is_some();
        let superclass = node.superclass().and_then(|superclass| {
            let reference = mixin_ref_from_node(&superclass)?;
            let super_range = self.direct_range(&superclass.location());
            let target = self.direct_resolve_namespace_from(
                &reference.parts,
                reference.absolute,
                &lexical_context,
            );
            Some((reference, super_range, target))
        });
        let mut reopened_target = mixin_ref_from_node(&node.constant_path())
            .and_then(|reference| {
                self.resolve_declaration_constant_value_type_from(
                    &reference.parts,
                    reference.absolute,
                    &lexical_context,
                )
            })
            .and_then(|(_constant, ruby_type)| match ruby_type {
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
        if has_explicit_superclass
            && superclass
                .as_ref()
                .and_then(|(_, _, target)| target.as_ref())
                == reopened_target.as_ref()
        {
            reopened_target = None;
        }
        // A prior index of this same `class Name` leaves a ClassReference for
        // `Name`. That must not be treated as an alias reopen: skipping the
        // graph node would let replace_facts delete the only class identity.
        if reopened_target.as_ref() == Some(&syntactic_fqn) {
            reopened_target = None;
        }

        // Handle namespace setup
        if let Some(target) = &reopened_target {
            self.scope_tracker
                .push_absolute_ns_scopes(target.namespace_parts().to_vec());
        } else {
            if self
                .scope_tracker
                .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
                .is_err()
            {
                error!("Error creating namespace for class");
                return false;
            }
        }

        let fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
        let range = self.direct_range(&node.location());
        let name_range = self
            .direct_terminal_name_range(&node.constant_path().location(), node.name().as_slice());
        if reopened_target.is_none() {
            assert_eq!(
                fqn, syntactic_fqn,
                "INVARIANT VIOLATED: syntactic class scope differs from the scope used for its declaration. This is a bug because both scopes were derived from the same Prism constant path. Fix: keep class declaration scope construction single-sourced."
            );
            self.direct_push_namespace_facts(fqn.clone(), GraphNodeKind::Class, range, name_range);
        }
        if let Some((superclass_ref, super_range, target)) = superclass {
            if let Some(target) = target {
                self.direct_push_resolved_edge(
                    fqn.clone(),
                    target.clone(),
                    GraphEdgeKind::Superclass,
                    super_range,
                );
                if let (Some(source_singleton), Some(target_singleton)) = (
                    fqn.to_singleton_namespace(),
                    target.to_singleton_namespace(),
                ) {
                    self.direct_push_resolved_edge(
                        source_singleton,
                        target_singleton,
                        GraphEdgeKind::Superclass,
                        super_range,
                    );
                }
            } else {
                self.direct_facts.unresolved_graph_edges.push(
                    crate::core::UnresolvedGraphEdgeFact::new(
                        fqn.clone(),
                        superclass_ref.parts,
                        superclass_ref.absolute,
                        FullyQualifiedName::namespace(lexical_context),
                        GraphEdgeKind::Superclass,
                        super_range,
                    ),
                );
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
            self.direct_push_edge_with_provenance(
                fqn.clone(),
                &[object],
                true,
                GraphEdgeKind::Superclass,
                GraphEdgeProvenance::ImplicitObject,
                range,
            );
        }

        // Setup local variable scope
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        // Get class name for scope tree
        let class_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        self.document.variable_scopes_mut().enter_scope(
            LVScopeKind::Constant,
            body_range,
            Some(class_name),
        );
        true
    }

    pub fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }
}

fn class_implicitly_inherits_object(fqn: &FullyQualifiedName) -> bool {
    let parts = fqn.namespace_parts();
    !matches!(
        parts.as_slice(),
        [name] if name.as_str() == "Object" || name.as_str() == "BasicObject"
    )
}
