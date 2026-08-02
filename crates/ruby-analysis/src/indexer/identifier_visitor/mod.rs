use crate::core::{ExecutionContextFact, ExecutionScopeMode, NamespaceKind, RubyConstant};

use crate::{Identifier, LVScopeId, RubyDocument, ScopeTracker};

use ruby_prism::*;
use tower_lsp::lsp_types::Position;

mod alias_method_node;
mod back_reference_read_node;
mod block_node;
mod call_node;
mod class_node;
mod constant_path_node;
mod constant_write_node;
mod def_node;
mod local_variable_read_node;
mod module_node;
mod numbered_reference_read_node;
mod parameters_node;
mod super_node;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierType {
    ModuleDef,
    ClassDef,
    ConstantDef,
    MethodDef,
    MethodCall,
    LVarDef,
    LVarRead,
    CVarDef,
    CVarRead,
    IVarDef,
    IVarRead,
    GVarDef,
    GVarRead,
}

/// Visitor for finding identifiers at a specific position
pub struct IdentifierVisitor {
    document: RubyDocument,
    position: Position,
    scope_tracker: ScopeTracker,
    execution_context: Option<ExecutionContextFact>,

    // Output
    pub ns_stack_at_pos: Vec<RubyConstant>,
    namespace_kind_at_pos: Option<NamespaceKind>,
    pub lv_scope_id_at_pos: Option<LVScopeId>,
    pub identifier: Option<Identifier>,
    pub identifier_type: Option<IdentifierType>,
}

impl IdentifierVisitor {
    pub fn new(document: RubyDocument, position: Position) -> Self {
        Self::new_with_execution_context(document, position, None)
    }

    pub fn new_with_execution_context(
        document: RubyDocument,
        position: Position,
        execution_context: Option<ExecutionContextFact>,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();

        Self {
            document,
            position,
            scope_tracker,
            execution_context,
            ns_stack_at_pos: Vec::new(),
            namespace_kind_at_pos: None,
            lv_scope_id_at_pos: None,
            identifier: None,
            identifier_type: None,
        }
    }

    fn execution_context_for_call(&self, node: &CallNode) -> Option<&ExecutionContextFact> {
        let context = self.execution_context.as_ref()?;
        let block = node.block()?;
        let location = block.location();
        (context.range.start_byte == u32::try_from(location.start_offset()).expect(
            "INVARIANT VIOLATED: Prism block start exceeds u32. This is a bug because TextRange stores u32 byte offsets. Fix: widen TextRange before parsing files larger than u32::MAX bytes.",
        ) && context.range.end_byte == u32::try_from(location.end_offset()).expect(
            "INVARIANT VIOLATED: Prism block end exceeds u32. This is a bug because TextRange stores u32 byte offsets. Fix: widen TextRange before parsing files larger than u32::MAX bytes.",
        ))
        .then_some(context)
    }

    pub fn is_position_in_location(&self, location: &Location) -> bool {
        let position_offset = self.document.position_to_offset(self.position);

        let start_offset = location.start_offset();
        let end_offset = location.end_offset();

        // Include the end position for completion support (when cursor is right after an identifier)
        position_offset >= start_offset && position_offset <= end_offset
    }

    pub fn set_result(
        &mut self,
        identifier: Option<Identifier>,
        identifier_type: Option<IdentifierType>,
        ns_stack_at_pos: Vec<RubyConstant>,
        lv_scope_id_at_pos: Option<LVScopeId>,
    ) {
        let implicit_context = self.scope_tracker.implicit_receiver_context();
        let definition_context = self.scope_tracker.method_definition_context();
        self.namespace_kind_at_pos = Some(if ns_stack_at_pos == implicit_context.0 {
            implicit_context.1
        } else if ns_stack_at_pos == definition_context.0 {
            definition_context.1
        } else {
            self.scope_tracker.current_method_context()
        });
        self.identifier = identifier;
        self.identifier_type = identifier_type;
        self.ns_stack_at_pos = ns_stack_at_pos;
        self.lv_scope_id_at_pos = lv_scope_id_at_pos;
    }

    pub fn is_result_set(&self) -> bool {
        self.identifier.is_some() && self.identifier_type.is_some()
    }

    pub fn get_result(
        &self,
    ) -> (
        Option<Identifier>,
        Option<IdentifierType>,
        Vec<RubyConstant>,
        LVScopeId,
        NamespaceKind,
    ) {
        let ns_stack = match self.ns_stack_at_pos.len() {
            // If ns_stack_at_pos is empty because no identifier was found,
            // use the scope tracker's ns_stack
            0 => self.scope_tracker.get_ns_stack(),
            _ => self.ns_stack_at_pos.clone(),
        };

        let lv_scope_id = self.lv_scope_id_at_pos.unwrap_or(0);

        // Determine namespace kind from current method context
        let namespace_kind = self
            .namespace_kind_at_pos
            .unwrap_or_else(|| self.scope_tracker.current_method_context());

        (
            self.identifier.clone(),
            self.identifier_type,
            ns_stack,
            lv_scope_id,
            namespace_kind,
        )
    }
}

fn static_receiver_namespace(
    node: &Node<'_>,
    scope_tracker: &ScopeTracker,
) -> Option<Vec<RubyConstant>> {
    if node.as_self_node().is_some() {
        let (namespace, receiver_kind) = scope_tracker.implicit_receiver_context();
        return (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
            .then_some(namespace);
    }
    if let Some(receiver) = crate::mixin_ref_from_node(node) {
        return Some(receiver.parts);
    }
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"const_get" {
        return None;
    }
    let mut namespace = static_receiver_namespace(&call.receiver()?, scope_tracker)?;
    let arguments = call.arguments()?;
    let argument = arguments.arguments().iter().next()?;
    let name = if let Some(symbol) = argument.as_symbol_node() {
        String::from_utf8_lossy(symbol.unescaped()).to_string()
    } else if let Some(string) = argument.as_string_node() {
        String::from_utf8_lossy(string.unescaped()).to_string()
    } else {
        return None;
    };
    namespace.push(RubyConstant::new(&name).ok()?);
    Some(namespace)
}

fn static_eval_block_context(
    node: &CallNode,
    scope_tracker: &ScopeTracker,
) -> Option<(Vec<RubyConstant>, NamespaceKind, NamespaceKind)> {
    let (implicit_receiver_kind, method_definition_kind) = match node.name().as_slice() {
        b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => {
            (NamespaceKind::Singleton, NamespaceKind::Instance)
        }
        b"instance_eval" | b"instance_exec" => (NamespaceKind::Singleton, NamespaceKind::Singleton),
        _ => return None,
    };
    node.block()?;
    let eval_namespace = match node.receiver() {
        None => {
            let (namespace, receiver_kind) = scope_tracker.implicit_receiver_context();
            (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                .then_some(namespace)?
        }
        Some(receiver) if receiver.as_self_node().is_some() => {
            let (namespace, receiver_kind) = scope_tracker.implicit_receiver_context();
            (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                .then_some(namespace)?
        }
        Some(receiver) => static_receiver_namespace(&receiver, scope_tracker)?,
    };
    Some((
        eval_namespace,
        implicit_receiver_kind,
        method_definition_kind,
    ))
}

fn static_dynamic_definition_block_context(
    node: &CallNode,
    scope_tracker: &ScopeTracker,
) -> Option<(
    Vec<RubyConstant>,
    NamespaceKind,
    Vec<RubyConstant>,
    NamespaceKind,
)> {
    node.block()?;
    let (definition_namespace, definition_kind) = scope_tracker.method_definition_context();
    let (implicit_namespace, implicit_kind) = match node.receiver() {
        None => {
            let target_kind = match node.name().as_slice() {
                b"define_method"
                    if !scope_tracker.execution_context_active()
                        && scope_tracker.in_singleton() =>
                {
                    NamespaceKind::Singleton
                }
                b"define_method" => NamespaceKind::Instance,
                b"define_singleton_method" => NamespaceKind::Singleton,
                _ => return None,
            };
            let (namespace, receiver_kind) = scope_tracker.implicit_receiver_context();
            if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
                return None;
            }
            (namespace, target_kind)
        }
        Some(receiver) if node.name().as_slice() == b"define_singleton_method" => (
            static_receiver_namespace(&receiver, scope_tracker)?,
            NamespaceKind::Singleton,
        ),
        Some(receiver)
            if matches!(
                node.name().as_slice(),
                b"send" | b"public_send" | b"__send__"
            ) =>
        {
            let arguments = node.arguments()?;
            let selector = arguments.arguments().iter().next()?;
            let target_kind = if let Some(symbol) = selector.as_symbol_node() {
                match symbol.unescaped() {
                    b"define_method" => NamespaceKind::Instance,
                    b"define_singleton_method" => NamespaceKind::Singleton,
                    _ => return None,
                }
            } else if let Some(string) = selector.as_string_node() {
                match string.unescaped() {
                    b"define_method" => NamespaceKind::Instance,
                    b"define_singleton_method" => NamespaceKind::Singleton,
                    _ => return None,
                }
            } else {
                return None;
            };
            if node.name().as_slice() == b"public_send" && target_kind == NamespaceKind::Instance {
                return None;
            }
            (
                static_receiver_namespace(&receiver, scope_tracker)?,
                target_kind,
            )
        }
        Some(_) => return None,
    };
    Some((
        implicit_namespace,
        implicit_kind,
        definition_namespace,
        definition_kind,
    ))
}

fn concern_class_methods_block_namespace(node: &CallNode) -> Option<Vec<RubyConstant>> {
    if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
        return None;
    }
    node.block()?;
    Some(vec![RubyConstant::new("ClassMethods").expect(
        "INVARIANT VIOLATED: static Concern ClassMethods constant is invalid. \
         This is a bug because `ClassMethods` is a valid Ruby constant. \
         Fix: inspect RubyConstant validation.",
    )])
}

impl Visit<'_> for IdentifierVisitor {
    fn visit_class_node(&mut self, node: &ClassNode) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
        visit_singleton_class_node(self, node);
        self.scope_tracker.exit_singleton();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_alias_method_node(&mut self, node: &AliasMethodNode) {
        self.process_alias_method_node_entry(node);
        visit_alias_method_node(self, node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_parameters_node(&mut self, node: &ParametersNode) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }

    fn visit_constant_write_node(&mut self, node: &ruby_prism::ConstantWriteNode<'_>) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
        self.process_constant_write_node_exit(node);
    }

    fn visit_constant_or_write_node(&mut self, node: &ruby_prism::ConstantOrWriteNode<'_>) {
        self.process_constant_or_write_node_entry(node);
        visit_constant_or_write_node(self, node);
        self.process_constant_or_write_node_exit(node);
    }

    fn visit_constant_and_write_node(&mut self, node: &ruby_prism::ConstantAndWriteNode<'_>) {
        self.process_constant_and_write_node_entry(node);
        visit_constant_and_write_node(self, node);
        self.process_constant_and_write_node_exit(node);
    }

    fn visit_constant_operator_write_node(
        &mut self,
        node: &ruby_prism::ConstantOperatorWriteNode<'_>,
    ) {
        self.process_constant_operator_write_node_entry(node);
        visit_constant_operator_write_node(self, node);
        self.process_constant_operator_write_node_exit(node);
    }

    fn visit_constant_target_node(&mut self, node: &ruby_prism::ConstantTargetNode<'_>) {
        self.process_constant_target_node_entry(node);
        visit_constant_target_node(self, node);
        self.process_constant_target_node_exit(node);
    }

    fn visit_multi_write_node(&mut self, node: &ruby_prism::MultiWriteNode<'_>) {
        // Match FactCollector: visit RHS first so nested identifiers under the
        // value resolve before LHS targets claim the cursor position.
        self.visit(&node.value());
        for target in node.lefts().iter() {
            self.visit(&target);
        }
        if let Some(rest) = node.rest() {
            self.visit(&rest);
        }
        for target in node.rights().iter() {
            self.visit(&target);
        }
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        if let Some(context) = self.execution_context_for_call(node).cloned() {
            assert_eq!(
                context.lexical_scope,
                ExecutionScopeMode::Preserve,
                "INVARIANT VIOLATED: unsupported lexical execution-scope mode reached IdentifierVisitor. This is a bug because only implemented scope modes may enter engine facts. Fix: validate modes at the extension boundary and add traversal support before extending the enum."
            );
            assert_eq!(
                context.local_scope,
                ExecutionScopeMode::Preserve,
                "INVARIANT VIOLATED: unsupported local execution-scope mode reached IdentifierVisitor. This is a bug because only implemented scope modes may enter engine facts. Fix: validate modes at the extension boundary and add traversal support before extending the enum."
            );
            assert_eq!(
                self.scope_tracker.get_ns_stack(),
                context.lexical_namespace.namespace_parts(),
                "INVARIANT VIOLATED: persisted execution context lexical namespace differs from current AST traversal. This is a bug because stale context facts must be removed on every file replacement. Fix: keep extension contexts in the ordinary per-file replacement lifecycle."
            );
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let implicit_kind = context.implicit_receiver.namespace_kind().expect(
                "INVARIANT VIOLATED: execution implicit receiver is not a namespace. This is a bug because engine ingestion validates execution targets. Fix: keep ExecutionContextFact namespace validation in replace_facts.",
            );
            let definition_kind = context.method_definition_owner.namespace_kind().expect(
                "INVARIANT VIOLATED: execution method owner is not a namespace. This is a bug because engine ingestion validates execution targets. Fix: keep ExecutionContextFact namespace validation in replace_facts.",
            );
            self.scope_tracker.push_execution_context(
                context.implicit_receiver.namespace_parts(),
                implicit_kind,
                context.method_definition_owner.namespace_parts(),
                definition_kind,
            );
            self.visit(&node.block().expect(
                "INVARIANT VIOLATED: matching execution context call lost its block. This is a bug because execution_context_for_call matched that block immediately before traversal. Fix: keep the Prism call node immutable during visitor dispatch.",
            ));
            self.scope_tracker.pop_execution_context();
        } else if let Some((
            implicit_namespace,
            implicit_kind,
            definition_namespace,
            definition_kind,
        )) = static_dynamic_definition_block_context(node, &self.scope_tracker)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect(
                "INVARIANT VIOLATED: dynamic-definition identifier context lost its block. This is a bug because static_dynamic_definition_block_context required the same immutable Prism call to have a block. Fix: keep identifier traversal and context matching atomic.",
            );
            self.scope_tracker.push_execution_context(
                implicit_namespace,
                implicit_kind,
                definition_namespace,
                definition_kind,
            );
            self.visit(&block);
            self.scope_tracker.pop_execution_context();
        } else if let Some((eval_namespace, implicit_kind, definition_kind)) =
            static_eval_block_context(node, &self.scope_tracker)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_execution_context(
                    eval_namespace.clone(),
                    implicit_kind,
                    eval_namespace,
                    definition_kind,
                );
                self.visit(&block);
                self.scope_tracker.pop_execution_context();
            }
        } else if let Some(class_methods_namespace) = concern_class_methods_block_namespace(node) {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_ns_scopes(class_methods_namespace);
                self.visit(&block);
                self.scope_tracker.pop_ns_scope();
            }
        } else if crate::is_framework_instance_block_call_name(node.name().as_slice())
            && node.receiver().is_none()
            && node.block().is_some()
        {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker
                    .push_scope_kind(crate::LocalScopeKind::FrameworkInstanceBlock);
                self.visit(&block);
                self.scope_tracker.pop_scope_kind();
            }
        } else {
            visit_call_node(self, node);
        }
        self.process_call_node_exit(node);
    }

    fn visit_forwarding_super_node(&mut self, node: &ForwardingSuperNode) {
        self.process_forwarding_super_node_entry(node);
        visit_forwarding_super_node(self, node);
    }

    fn visit_super_node(&mut self, node: &SuperNode) {
        self.process_super_node_entry(node);
        visit_super_node(self, node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write_node_entry(node);
        visit_local_variable_write_node(self, node);
        self.process_local_variable_write_node_exit(node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_target_node_entry(node);
        visit_local_variable_target_node(self, node);
        self.process_local_variable_target_node_exit(node);
    }

    fn visit_class_variable_read_node(&mut self, node: &ClassVariableReadNode) {
        self.process_class_variable_read_node_entry(node);
        visit_class_variable_read_node(self, node);
        self.process_class_variable_read_node_exit(node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write_node_entry(node);
        visit_class_variable_write_node(self, node);
        self.process_class_variable_write_node_exit(node);
    }

    fn visit_instance_variable_read_node(&mut self, node: &InstanceVariableReadNode) {
        self.process_instance_variable_read_node_entry(node);
        visit_instance_variable_read_node(self, node);
        self.process_instance_variable_read_node_exit(node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write_node_entry(node);
        visit_instance_variable_write_node(self, node);
        self.process_instance_variable_write_node_exit(node);
    }

    fn visit_global_variable_read_node(&mut self, node: &GlobalVariableReadNode) {
        self.process_global_variable_read_node_entry(node);
        visit_global_variable_read_node(self, node);
        self.process_global_variable_read_node_exit(node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write_node_entry(node);
        visit_global_variable_write_node(self, node);
        self.process_global_variable_write_node_exit(node);
    }

    fn visit_numbered_reference_read_node(&mut self, node: &NumberedReferenceReadNode) {
        self.process_numbered_reference_read_node_entry(node);
        visit_numbered_reference_read_node(self, node);
        self.process_numbered_reference_read_node_exit(node);
    }

    fn visit_back_reference_read_node(&mut self, node: &BackReferenceReadNode) {
        self.process_back_reference_read_node_entry(node);
        visit_back_reference_read_node(self, node);
        self.process_back_reference_read_node_exit(node);
    }

    fn visit_hash_node(&mut self, node: &HashNode) {
        // Hash nodes don't need special processing - just recursively visit children
        // This ensures constants inside hash values are properly visited
        visit_hash_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use crate::MethodReceiver;

    use super::*;
    use tower_lsp::lsp_types::{Position, Url};

    // Helper function to test the full visitor behavior
    fn test_visitor(code: &str, position: Position, expected_parts: Vec<&str>) {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, position);
        let parse_result = ruby_prism::parse(code.as_bytes());

        // Use the full visitor pattern
        visitor.visit(&parse_result.node());

        // If expected_parts is empty and we're on a scope resolution operator,
        // we expect identifier to be None
        if expected_parts.is_empty() {
            assert!(
                visitor.is_result_set(),
                "Expected identifier to be None at position {:?}",
                position
            );
            return;
        }

        // Otherwise, check the identifier was found
        assert!(
            visitor.is_result_set(),
            "Expected to find an identifier at position {:?}",
            position
        );

        // Get the identifier for further processing
        let identifier = visitor.identifier.as_ref().unwrap();

        // Special case for root constants
        if code.starts_with("::") {
            if let Identifier::RubyConstant { namespace: _, iden } = identifier {
                // For root constants, we expect the identifier path to match expected_parts
                assert_eq!(
                    iden.len(),
                    expected_parts.len(),
                    "Identifier parts count mismatch for root constant path"
                );
                for (i, expected_part) in expected_parts.iter().enumerate() {
                    assert_eq!(
                        iden[i].to_string(),
                        *expected_part,
                        "Identifier part at index {} mismatch",
                        i
                    );
                }
                return;
            }
        }

        // Get the parts from the identifier - could be either a namespace or a constant
        let parts = match identifier {
            Identifier::RubyConstant { namespace: _, iden } => iden.clone(),
            // This line is no longer needed with the combined RubyConstant type
            _ => panic!("Expected a Namespace or Constant FQN"),
        };

        // Verify the parts match
        assert_eq!(
            parts.len(),
            expected_parts.len(),
            "Namespace parts count mismatch"
        );
        for (i, expected_part) in expected_parts.iter().enumerate() {
            assert_eq!(
                parts[i].to_string(),
                *expected_part,
                "Namespace part at index {} mismatch",
                i
            );
        }
    }

    #[test]
    fn test_simple_constant_path() {
        // Test case: Foo::Bar with cursor at Bar
        test_visitor("Foo::Bar", Position::new(0, 6), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_nested_constant_path_at_middle() {
        // Test case: Foo::Bar::Baz with cursor at Bar
        test_visitor("Foo::Bar::Baz", Position::new(0, 6), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_nested_constant_path_at_first() {
        // Test case: Foo::Bar::Baz with cursor at Foo
        test_visitor("Foo::Bar::Baz", Position::new(0, 1), vec!["Foo"]);
    }

    #[test]
    fn test_nested_constant_path_at_last() {
        // Test case: Foo::Bar::Baz with cursor at Baz
        test_visitor(
            "Foo::Bar::Baz",
            Position::new(0, 11),
            vec!["Foo", "Bar", "Baz"],
        );
    }

    #[test]
    fn test_nested_constant_path_at_scope_resolution() {
        // Test case: Foo::Bar::Baz with cursor at first "::"
        // When cursor is at "::", we only get the constant path up to that point
        test_visitor("Foo::Bar::Baz", Position::new(0, 3), vec!["Foo"]);
    }

    #[test]
    fn test_nested_constant_path_at_scope_resolution_2() {
        // Test case: Foo::Bar::Baz with cursor at second "::"
        // When cursor is at "::", we only get the constant path up to that point
        test_visitor("Foo::Bar::Baz", Position::new(0, 8), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_root_constant_read_node() {
        test_visitor(
            "::GLOBAL_CONSTANT",
            Position::new(0, 2),
            vec!["GLOBAL_CONSTANT"],
        );
    }

    #[test]
    fn test_root_constant_path_node() {
        test_visitor(
            "::Foo::Bar::GLOBAL_CONSTANT",
            Position::new(0, 12),
            vec!["Foo", "Bar", "GLOBAL_CONSTANT"],
        );
    }

    #[test]
    fn test_constant_in_method_call() {
        // Test case: Foo.bar with cursor at Foo
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "Foo.bar".to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 1));
        let parse_result = ruby_prism::parse("Foo.bar".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].to_string(), "Foo");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_path_in_method_call() {
        // Test case: Foo::Bar.baz with cursor at Bar
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "Foo::Bar.baz".to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 6));
        let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_module_method_call() {
        // Test case: Foo::Bar.baz with cursor at baz (module method call)
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "Foo::Bar.baz".to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 10));
        let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyMethod {
                namespace: parts,
                receiver,
                iden: method,
            } => {
                // The namespace should be empty at top-level (no artificial prefix)
                assert_eq!(parts.len(), 0);
                // The receiver should be Constant with [Foo, Bar]
                match receiver {
                    MethodReceiver::Constant(receiver_parts) => {
                        assert_eq!(receiver_parts.len(), 2);
                        assert_eq!(receiver_parts[0].to_string(), "Foo");
                        assert_eq!(receiver_parts[1].to_string(), "Bar");
                    }
                    _ => panic!("Expected Constant receiver"),
                }
                assert_eq!(method.to_string(), "baz");
            }
            _ => panic!("Expected Method identifier, got {:?}", identifier),
        }
    }

    #[test]
    fn test_namespace_in_method_call() {
        // Test case: Foo::Bar::Baz.foo with cursor at Bar
        let mut visitor = {
            let uri = Url::parse("file:///test.rb").unwrap();
            let document = RubyDocument::new(uri, "Foo::Bar::Baz.foo".to_string(), 1);
            IdentifierVisitor::new(document, Position::new(0, 6))
        };
        let parse_result = ruby_prism::parse("Foo::Bar::Baz.foo".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_nested_expression() {
        // Test case: Foo::Bar::Baz::ABC with cursor at ABC
        let mut visitor = {
            let uri = Url::parse("file:///test.rb").unwrap();
            let document = RubyDocument::new(uri, "Foo::Bar::Baz::ABC".to_string(), 1);
            IdentifierVisitor::new(document, Position::new(0, 15))
        };
        let parse_result = ruby_prism::parse("Foo::Bar::Baz::ABC".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 4);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
                assert_eq!(parts[2].to_string(), "Baz");
                assert_eq!(parts[3].to_string(), "ABC");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_method_arguments() {
        // Test case: method(Foo::Bar) with cursor at Bar
        let mut visitor = {
            let uri = Url::parse("file:///test.rb").unwrap();
            let document = RubyDocument::new(uri, "method(Foo::Bar)".to_string(), 1);
            IdentifierVisitor::new(document, Position::new(0, 12))
        };
        let parse_result = ruby_prism::parse("method(Foo::Bar)".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
            }
            _ => panic!("Expected Namespace FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_nested_constant_in_method_arguments() {
        // Test case: method(A::B::C::D::CONST) with cursor at CONST
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "method(A::B::C::D::CONST)".to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 20));
        let parse_result = ruby_prism::parse("method(A::B::C::D::CONST)".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 5);
                assert_eq!(parts[0].to_string(), "A");
                assert_eq!(parts[1].to_string(), "B");
                assert_eq!(parts[2].to_string(), "C");
                assert_eq!(parts[3].to_string(), "D");
                assert_eq!(parts[4].to_string(), "CONST");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_deeply_nested_call_node() {
        // Test case: a.b.c.d.e with cursor at d
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "a.b.c.d.e".to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 0));
        let parse_result = ruby_prism::parse("a.b.c.d.e".as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyMethod { iden: method, .. } => {
                assert_eq!(method.to_string(), "a");
            }
            _ => panic!("Expected InstanceMethod FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_error_raising() {
        // Test case: raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)
        // with cursor at CONSTANT
        let code = "raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)";
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(0, 40));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Messages");
                assert_eq!(parts[2].to_string(), "CONSTANT");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_complex_error_raising() {
        // Test case with complex nested constant paths in error raising:
        // raise RubyLSP::Core::Errors::ValidationError.new(
        //       RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,
        //       RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR
        //     )
        let code = String::from("raise RubyLSP::Core::Errors::ValidationError.new(\n")
            + "          RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,\n"
            + "          RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR\n"
            + "        )";

        // Test with cursor on INVALID_SYNTAX
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(1, 60));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 5);
                assert_eq!(parts[0].to_string(), "RubyLSP");
                assert_eq!(parts[1].to_string(), "Core");
                assert_eq!(parts[2].to_string(), "Constants");
                assert_eq!(parts[3].to_string(), "ErrorMessages");
                assert_eq!(parts[4].to_string(), "INVALID_SYNTAX");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }

        // Test with cursor on PARSE_ERROR
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(2, 55));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 5);
                assert_eq!(parts[0].to_string(), "RubyLSP");
                assert_eq!(parts[1].to_string(), "Core");
                assert_eq!(parts[2].to_string(), "Constants");
                assert_eq!(parts[3].to_string(), "ErrorCodes");
                assert_eq!(parts[4].to_string(), "PARSE_ERROR");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }

    #[test]
    fn test_constant_in_block() {
        // Test case with constant paths in a block:
        // items.each do |item|
        //   raise Error::Type.new(
        //     Error::Messages::INVALID_ITEM,
        //     Error::Codes::ITEM_ERROR
        //   )
        // end
        let code = String::from("items.each do |item|\n")
            + "  raise Error::InvalidItem.new(\n"
            + "    Error::Messages::INVALID_ITEM,\n"
            + "    Error::Codes::ITEM_ERROR\n"
            + "  )\n"
            + "end";

        // Test with cursor on INVALID_ITEM
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(2, 25));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Messages");
                assert_eq!(parts[2].to_string(), "INVALID_ITEM");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }

        // Test with cursor on ITEM_ERROR
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(3, 20));
        let parse_result = ruby_prism::parse(code.as_bytes());
        visitor.visit(&parse_result.node());

        // Ensure we found an identifier
        let identifier = visitor.identifier.expect("Expected to find an identifier");

        match identifier {
            Identifier::RubyConstant {
                namespace: _,
                iden: parts,
            } => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Codes");
                assert_eq!(parts[2].to_string(), "ITEM_ERROR");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }
}
