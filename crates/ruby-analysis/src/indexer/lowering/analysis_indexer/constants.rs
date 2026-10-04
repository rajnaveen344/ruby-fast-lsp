//! Value-constant declarations from constant writes and assignment targets.

use crate::core::{FullyQualifiedName, RubyConstant, SymbolFact, SymbolKind, TypeSubject};
use crate::indexer::documents::scope_rules::{
    constant_path_write_target, implicit_singleton_namespace,
};
use ruby_prism::{Location, Node};

use super::syntax::terminal_name_range;
use super::AnalysisIndexer;

impl AnalysisIndexer<'_> {
    /// Declare `NAME` in the current lexical namespace.
    pub(super) fn push_constant_declaration(
        &mut self,
        name: &[u8],
        name_location: Location<'_>,
        full_location: Location<'_>,
        value: Option<&Node<'_>>,
    ) {
        let Ok(constant) = RubyConstant::new(&String::from_utf8_lossy(name)) else {
            return;
        };
        let mut parts = self.scope.get_ns_stack();
        parts.push(constant);
        let fqn = FullyQualifiedName::constant(parts);
        self.facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&full_location),
            )
            .with_name_range(self.range(&name_location)),
        );
        let value_type = value.and_then(|value| self.assignment_type(value));
        self.push_type_fact(TypeSubject::Constant(fqn), value_type, name_location);
    }

    /// Declare the constant a `Parent::NAME` write or target names.
    pub(super) fn push_constant_path_declaration(
        &mut self,
        parent: Option<&Node<'_>>,
        name: &[u8],
        path_location: Location<'_>,
        full_location: Location<'_>,
        value: Option<&Node<'_>>,
    ) {
        let Some(fqn) = constant_path_write_target(
            parent,
            name,
            implicit_singleton_namespace(&self.scope).as_deref(),
            &self.scope.get_ns_stack(),
            &|fqn| self.known_namespaces.contains(fqn),
        ) else {
            return;
        };
        self.facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                SymbolKind::Constant,
                self.range(&full_location),
            )
            .with_name_range(terminal_name_range(self.file_id, &path_location, name)),
        );
        let value_type = value.and_then(|value| self.assignment_type(value));
        self.push_type_fact(TypeSubject::Constant(fqn), value_type, path_location);
    }

    /// Declare an assignment target that is a constant, with the value Ruby
    /// assigns to it when syntax decides one. Returns false for other targets.
    pub(super) fn push_constant_target_declaration(
        &mut self,
        target: &Node<'_>,
        value: Option<&Node<'_>>,
    ) -> bool {
        if let Some(target) = target.as_constant_target_node() {
            self.push_constant_declaration(
                target.name().as_slice(),
                target.location(),
                target.location(),
                value,
            );
            return true;
        }
        if let Some(target) = target.as_constant_path_target_node() {
            let Some(name) = target.name() else {
                return true;
            };
            self.push_constant_path_declaration(
                target.parent().as_ref(),
                name.as_slice(),
                target.location(),
                target.location(),
                value,
            );
            return true;
        }
        false
    }
}
