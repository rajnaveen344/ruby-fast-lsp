use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphEdgeProvenance, GraphNodeKind, RubyConstant,
};
use crate::indexer::documents::scope_rules::alias_reopen_target;
use crate::indexer::mixin_ref_from_node;
use crate::indexer::LocalScopeKind as LVScopeKind;
use crate::invariant::ExpectInvariant;
use log::error;
use ruby_prism::ClassNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_class_node_entry(
        &mut self,
        node: &ClassNode,
    ) -> bool {
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
        let reopened_target = alias_reopen_target(
            &node.constant_path(),
            GraphNodeKind::Class,
            superclass
                .as_ref()
                .and_then(|(_, _, target)| target.as_ref()),
            &lexical_context,
            |candidates| {
                self.first_constant_value_type(candidates.iter().cloned())
                    .map(|(_, ruby_type)| ruby_type)
            },
        );

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
            invariant_eq!(
                fqn,
                syntactic_fqn,
                what = "syntactic class scope differs from the scope used for its declaration",
                why = "both scopes were derived from the same Prism constant path",
                fix = "keep class declaration scope construction single-sourced",
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
                self.facts.analysis.unresolved_graph_edges.push(
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
            let object = RubyConstant::new("Object").expect_invariant(
                "Object is not a valid Ruby constant",
                "ruby's implicit class superclass must be representable",
                "update RubyConstant validation or implicit superclass construction",
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

        self.document
            .variable_scopes_mut()
            .enter_scope(LVScopeKind::Constant, body_range);
        true
    }

    pub(in crate::indexer::fact_collector) fn process_class_node_exit(
        &mut self,
        _node: &ClassNode,
    ) {
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
