use crate::core::{FullyQualifiedName, GraphEdgeKind, GraphNodeKind, RubyConstant};
use crate::mixin_ref_from_node;
use crate::LocalScopeKind as LVScopeKind;
use log::error;
use ruby_prism::ClassNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());

        // Handle namespace setup
        if self
            .scope_tracker
            .push_namespace_from_constant_path(&node.constant_path(), node.name().as_slice())
            .is_err()
        {
            error!("Error creating namespace for class");
            return;
        }

        let fqn = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
        let range = self.direct_range(&node.location());
        let name_range = self
            .direct_terminal_name_range(&node.constant_path().location(), node.name().as_slice());
        self.direct_push_namespace_facts(fqn.clone(), GraphNodeKind::Class, range, name_range);
        if let Some(superclass) = node.superclass() {
            if let Some(superclass_ref) = mixin_ref_from_node(&superclass) {
                let super_range = self.direct_range(&superclass.location());
                self.direct_push_edge(
                    fqn.clone(),
                    &superclass_ref.parts,
                    superclass_ref.absolute,
                    GraphEdgeKind::Superclass,
                    super_range,
                );
                if let Some(source_singleton) = fqn.to_singleton_namespace() {
                    if let Some(target) = self
                        .direct_resolve_namespace(&superclass_ref.parts, superclass_ref.absolute)
                        .and_then(|target| target.to_singleton_namespace())
                    {
                        self.direct_facts
                            .graph_edges
                            .push(crate::core::GraphEdgeFact::new(
                                source_singleton,
                                target,
                                GraphEdgeKind::Superclass,
                                super_range,
                            ));
                    }
                }
            }
        } else if class_implicitly_inherits_object(&fqn) {
            let object = RubyConstant::new("Object").expect(
                "INVARIANT VIOLATED: Object is not a valid Ruby constant. \
                 This is a bug because Ruby's implicit class superclass must be representable. \
                 Fix: update RubyConstant validation or implicit superclass construction.",
            );
            self.direct_push_edge(
                fqn.clone(),
                &[object],
                true,
                GraphEdgeKind::Superclass,
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
