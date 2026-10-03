mod calls;
mod constants;
mod declarations;
mod scope;
pub(in crate::indexer) mod types;
mod variables;

use crate::core::{
    ExecutionContextFact, ExecutionScopeMode, NamespaceKind, RubyConstant, RubyType,
};
use crate::invariant::ExpectInvariant;
use std::collections::HashMap;

use crate::indexer::documents::scope_rules;
use crate::indexer::{Identifier, LVScopeId, ScopeTracker};
use crate::inference::semantics::Semantics;

use ruby_prism::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierType {
    ModuleDef,
    ClassDef,
    ConstantDef,
    MethodDef,
    MethodCall,
    MethodReference,
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
pub struct IdentifierVisitor<'s> {
    content: String,
    byte_offset: u32,
    scope_tracker: ScopeTracker,
    execution_context: Option<ExecutionContextFact>,
    /// Class and module values of constants this file declares or assigns
    /// before the current node; they win over `semantics`.
    file_constant_types: HashMap<Vec<RubyConstant>, RubyType>,
    /// The owning project, when the query has one, for constant aliases and
    /// receivers declared in other files.
    semantics: Option<&'s dyn Semantics>,

    // Output
    pub ns_stack_at_pos: Vec<RubyConstant>,
    namespace_kind_at_pos: Option<NamespaceKind>,
    pub lv_scope_id_at_pos: Option<LVScopeId>,
    pub identifier: Option<Identifier>,
    pub identifier_type: Option<IdentifierType>,
}

impl<'s> IdentifierVisitor<'s> {
    #[cfg(test)]
    pub fn new(
        document: crate::indexer::RubyDocument,
        position: crate::core::SourcePosition,
    ) -> Self {
        let byte_offset = document.position_to_analysis_offset(position);
        Self::new_with_execution_context_at_offset(document.content, byte_offset, None, None)
    }

    pub fn new_with_execution_context_at_offset(
        content: String,
        byte_offset: u32,
        execution_context: Option<ExecutionContextFact>,
        semantics: Option<&'s dyn Semantics>,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();

        Self {
            content,
            byte_offset,
            scope_tracker,
            execution_context,
            file_constant_types: HashMap::new(),
            semantics,
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
        (context.range.start_byte
            == u32::try_from(location.start_offset()).expect_invariant(
                "Prism block start exceeds u32",
                "TextRange stores u32 byte offsets",
                "widen TextRange before parsing files larger than u32::MAX bytes",
            )
            && context.range.end_byte
                == u32::try_from(location.end_offset()).expect_invariant(
                    "Prism block end exceeds u32",
                    "TextRange stores u32 byte offsets",
                    "widen TextRange before parsing files larger than u32::MAX bytes",
                ))
        .then_some(context)
    }

    pub fn is_position_in_location(&self, location: &Location) -> bool {
        self.is_position_in_offsets(location.start_offset(), location.end_offset())
    }

    pub fn is_position_in_offsets(&self, start_offset: usize, end_offset: usize) -> bool {
        let position_offset = usize::try_from(self.byte_offset).expect_invariant(
            "u32 identifier offset could not fit usize",
            "supported targets must address u32 source offsets",
            "reject the unsupported target architecture",
        );
        // Include the end position for completion support (when cursor is right after an identifier)
        position_offset >= start_offset && position_offset <= end_offset
    }

    pub fn cursor_offset(&self) -> u32 {
        self.byte_offset
    }

    pub fn content(&self) -> &str {
        &self.content
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
        return scope_rules::implicit_singleton_namespace(scope_tracker);
    }
    if let Some(receiver) = crate::indexer::mixin_ref_from_node(node) {
        return Some(receiver.parts);
    }
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"const_get" {
        return None;
    }
    let mut namespace = static_receiver_namespace(&call.receiver()?, scope_tracker)?;
    let arguments = call.arguments()?;
    let name = scope_rules::static_name(&arguments.arguments().iter().next()?)?;
    namespace.push(RubyConstant::new(&name).ok()?);
    Some(namespace)
}

impl Visit<'_> for IdentifierVisitor<'_> {
    fn visit_class_node(&mut self, node: &ClassNode) {
        let opened = self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node, opened);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
        visit_singleton_class_node(self, node);
        self.scope_tracker.exit_singleton();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        let opened = self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node, opened);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        let entered = self.process_def_node_entry(node);
        visit_def_node(self, node);
        if entered {
            self.process_def_node_exit(node);
        }
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

    fn visit_constant_path_write_node(&mut self, node: &ruby_prism::ConstantPathWriteNode<'_>) {
        self.record_constant_path_write(node);
        visit_constant_path_write_node(self, node);
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
            invariant_eq!(
                context.lexical_scope,
                ExecutionScopeMode::Preserve,
                what = "unsupported lexical execution-scope mode reached IdentifierVisitor",
                why = "only implemented scope modes may enter engine facts",
                fix = "validate modes at the extension boundary; add traversal support first",
            );
            invariant_eq!(
                context.local_scope,
                ExecutionScopeMode::Preserve,
                what = "unsupported local execution-scope mode reached IdentifierVisitor",
                why = "only implemented scope modes may enter engine facts",
                fix = "validate modes at the extension boundary; add traversal support first",
            );
            invariant_eq!(
                self.scope_tracker.get_ns_stack(),
                context.lexical_namespace.namespace_parts(),
                what = "persisted execution context lexical namespace differs from current AST traversal",
                why = "stale context facts must be removed on every file replacement",
                fix = "keep extension contexts in the ordinary per-file replacement lifecycle",
            );
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let implicit_kind = context.implicit_receiver.namespace_kind().expect_invariant(
                "execution implicit receiver is not a namespace",
                "engine ingestion validates execution targets",
                "keep ExecutionContextFact namespace validation in update",
            );
            let definition_kind = context
                .method_definition_owner
                .namespace_kind()
                .expect_invariant(
                    "execution method owner is not a namespace",
                    "engine ingestion validates execution targets",
                    "keep ExecutionContextFact namespace validation in update",
                );
            self.scope_tracker.push_block_execution_context(
                context.implicit_receiver.namespace_parts(),
                implicit_kind,
                context.method_definition_owner.namespace_parts(),
                definition_kind,
            );
            self.visit(&node.block().expect_invariant(
                "matching execution context call lost its block",
                "execution_context_for_call matched that block immediately before traversal",
                "keep the Prism call node immutable during visitor dispatch",
            ));
            self.scope_tracker.pop_execution_context();
        } else if let Some(execution) =
            scope_rules::dynamic_definition_block(node, &self.scope_tracker, |receiver| {
                static_receiver_namespace(receiver, &self.scope_tracker)
            })
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect_invariant(
                "dynamic-definition identifier context lost its block",
                "context matching required the same Prism call to have a block",
                "keep identifier traversal and context matching atomic",
            );
            execution.enter(&mut self.scope_tracker);
            self.visit(&block);
            self.scope_tracker.pop_execution_context();
        } else if let Some(execution) =
            scope_rules::eval_block(node, &self.scope_tracker, |receiver| {
                static_receiver_namespace(receiver, &self.scope_tracker)
            })
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                execution.enter(&mut self.scope_tracker);
                self.visit(&block);
                self.scope_tracker.pop_execution_context();
            }
        } else if let Some(execution) = scope_rules::class_methods_block(
            node,
            scope_rules::implicit_singleton_namespace(&self.scope_tracker),
        ) {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                execution.enter(&mut self.scope_tracker);
                self.visit(&block);
                self.scope_tracker.pop_execution_context();
            }
        } else if crate::indexer::is_framework_instance_block_call_name(node.name().as_slice())
            && node.receiver().is_none()
            && node.block().is_some()
        {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker
                    .push_scope_kind(crate::indexer::LocalScopeKind::FrameworkInstanceBlock);
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
mod tests;
