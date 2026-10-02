//! `class` and `module` declarations: the name under the cursor, and the
//! namespace frame the body opens, following constant aliases this file
//! declares.

use ruby_prism::{ClassNode, Location, ModuleNode, Node};

use crate::core::{FullyQualifiedName, GraphNodeKind, RubyConstant, RubyType};
use crate::indexer::documents::scope_rules::{alias_reopen_target, lexical_candidates};
use crate::indexer::{mixin_ref_from_node, queries::syntax, Identifier, LVScopeKind};

use crate::indexer::identifiers::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_class_node_entry(&mut self, node: &ClassNode) {
        let superclass = node.superclass().and_then(|superclass| {
            match self.file_constant_value(&superclass)? {
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
            }
        });
        self.enter_namespace_declaration(
            &node.constant_path(),
            node.name().as_slice(),
            GraphNodeKind::Class,
            superclass.as_ref(),
        );
    }

    pub fn process_class_node_exit(&mut self, node: &ClassNode) {
        self.exit_namespace_declaration(node.body().map(|body| body.location()), &node.location());
    }

    pub fn process_module_node_entry(&mut self, node: &ModuleNode) {
        self.enter_namespace_declaration(
            &node.constant_path(),
            node.name().as_slice(),
            GraphNodeKind::Module,
            None,
        );
    }

    pub fn process_module_node_exit(&mut self, node: &ModuleNode) {
        self.exit_namespace_declaration(node.body().map(|body| body.location()), &node.location());
    }

    /// Every declaration opens its frame, including those away from the
    /// cursor, so constant writes record their lexical owner.
    fn enter_namespace_declaration(
        &mut self,
        constant_path: &Node<'_>,
        name: &[u8],
        kind: GraphNodeKind,
        superclass: Option<&FullyQualifiedName>,
    ) {
        if self.is_result_set() {
            return;
        }
        if self.is_position_in_location(&constant_path.location()) {
            self.set_declaration_name_result(constant_path, kind);
            return;
        }
        let lexical_context = self.scope_tracker.get_ns_stack();
        let reopened = alias_reopen_target(
            constant_path,
            kind,
            superclass,
            &lexical_context,
            |candidates| {
                candidates
                    .iter()
                    .find_map(|candidate| self.file_constant_types.get(candidate).cloned())
            },
        );
        if let Some(target) = reopened {
            self.scope_tracker
                .push_absolute_ns_scopes(target.namespace_parts());
        } else if self
            .scope_tracker
            .push_namespace_from_constant_path(constant_path, name)
            .is_ok()
        {
            let declared = self.scope_tracker.get_ns_stack();
            let value = FullyQualifiedName::constant(declared.clone());
            self.file_constant_types.insert(
                declared,
                match kind {
                    GraphNodeKind::Class => RubyType::ClassReference(value),
                    GraphNodeKind::Module => RubyType::ModuleReference(value),
                },
            );
        } else {
            return;
        }
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);
    }

    fn exit_namespace_declaration(&mut self, body: Option<Location<'_>>, location: &Location<'_>) {
        if self.is_result_set() {
            return;
        }
        let (body_start, body_end) = syntax::get_body_offsets(body, location);
        if !self.is_position_in_offsets(body_start, body_end) {
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_scope_kind();
        }
    }

    fn set_declaration_name_result(&mut self, constant_path: &Node<'_>, kind: GraphNodeKind) {
        let iden = if let Some(path) = constant_path.as_constant_path_node() {
            let mut namespaces = Vec::new();
            syntax::collect_namespaces(&path, &mut namespaces);
            namespaces
        } else if let Some(read) = constant_path.as_constant_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            match RubyConstant::new(name.as_ref()) {
                Ok(constant) => vec![constant],
                Err(_) => return,
            }
        } else {
            return;
        };
        let identifier_type = match kind {
            GraphNodeKind::Class => IdentifierType::ClassDef,
            GraphNodeKind::Module => IdentifierType::ModuleDef,
        };
        self.set_result(
            Some(Identifier::RubyConstant {
                namespace: self.scope_tracker.get_ns_stack(),
                iden,
            }),
            Some(identifier_type),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
    }

    /// The class or module value a constant read names through declarations
    /// and constant writes earlier in this file.
    pub(in crate::indexer::identifiers) fn file_constant_value(
        &self,
        node: &Node<'_>,
    ) -> Option<RubyType> {
        let reference = mixin_ref_from_node(node)?;
        let lexical_context = self.scope_tracker.get_ns_stack();
        let value = lexical_candidates(&reference.parts, reference.absolute, &lexical_context)
            .find_map(|candidate| self.file_constant_types.get(&candidate).cloned());
        value
    }
}
