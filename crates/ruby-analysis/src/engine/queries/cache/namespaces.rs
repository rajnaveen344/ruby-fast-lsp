//! Namespace, constant, and constant hover queries.

use crate::invariant::ExpectInvariant;
use std::collections::HashSet;

use crate::core::{FullyQualifiedName, GraphNodeKind, RubyConstant, RubyType, TypeSubject};
use crate::engine::queries::lookup::types::{ConstantHover, ConstantHoverKind};
use crate::engine::queries::View;

impl<'a> View<'a> {
    pub fn namespace_node_kind(&self, namespace_fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.latest_graph_node_kind(namespace_fqn)
    }

    pub fn namespace_exists(&self, namespace_fqn: &FullyQualifiedName) -> bool {
        self.namespace_node_kind(namespace_fqn).is_some()
    }

    pub fn namespace_type(&self, namespace_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.namespace_node_kind(namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::Class(namespace_fqn.clone())),
            GraphNodeKind::Module => Some(RubyType::Module(namespace_fqn.clone())),
        }
    }

    pub fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        match self.namespace_node_kind(&namespace_fqn)? {
            GraphNodeKind::Class => Some(RubyType::ClassReference(constant_fqn)),
            GraphNodeKind::Module => Some(RubyType::ModuleReference(constant_fqn)),
        }
    }

    pub fn type_to_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Instance,
                ))
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    crate::core::NamespaceKind::Singleton,
                ))
            }
            RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Array").expect_invariant(
                    "built-in constant `Array` is invalid",
                    "ruby built-in constants must be valid Ruby constants",
                    "correct the hard-coded built-in constant name",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Hash").expect_invariant(
                    "built-in constant `Hash` is invalid",
                    "ruby built-in constants must be valid Ruby constants",
                    "correct the hard-coded built-in constant name",
                )],
                crate::core::NamespaceKind::Instance,
            )),
            RubyType::Shape(_) => self.type_to_namespace(&RubyType::Hash(
                vec![RubyType::Unknown],
                vec![RubyType::Unknown],
            )),
            RubyType::Literal(value) => self.type_to_namespace(&value.widened_type()),
            RubyType::Union(_) | RubyType::Unknown => None,
        }
    }

    pub(crate) fn constant_dependency_type(
        &self,
        dependency: &crate::core::ConstantTypeDependency,
    ) -> Option<RubyType> {
        crate::engine::state::resolve_constant_dependency_type(self, dependency)
    }

    pub fn constant_value_type(&self, constant_fqn: &FullyQualifiedName) -> Option<RubyType> {
        self.engine
            .type_store()
            .facts_for(&TypeSubject::Constant(constant_fqn.clone()))
            .iter()
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.ruby_type.clone())
    }

    pub fn constant_hover(&self, path: &[RubyConstant]) -> Option<ConstantHover> {
        let namespace_fqn = FullyQualifiedName::namespace(path.to_vec());
        let constant_fqn = FullyQualifiedName::constant(path.to_vec());
        let name = path
            .iter()
            .map(|constant| constant.to_string())
            .collect::<Vec<_>>()
            .join("::");

        match self.namespace_node_kind(&namespace_fqn) {
            Some(GraphNodeKind::Class) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Class,
                });
            }
            Some(GraphNodeKind::Module) => {
                return Some(ConstantHover {
                    name,
                    kind: ConstantHoverKind::Module,
                });
            }
            None => {}
        }

        self.constant_value_type(&constant_fqn)
            .map(|ruby_type| ConstantHover {
                name,
                kind: ConstantHoverKind::Value(ruby_type),
            })
    }

    pub fn known_namespace_fqns(&self) -> HashSet<FullyQualifiedName> {
        self.engine.decls.known_namespace_fqns(&self.engine.names)
    }
}
