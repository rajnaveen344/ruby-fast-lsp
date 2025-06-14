use crate::analyzer_prism::utils;
use crate::analyzer_prism::Identifier;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::types::ruby_variable::{RubyVariable, RubyVariableType};
use crate::types::scope_kind::LVScopeKind;

use lsp_types::Position;
use ruby_prism::ParametersNode;
use ruby_prism::{
    visit_arguments_node, visit_block_node, visit_call_node, visit_class_node,
    visit_class_variable_read_node, visit_constant_path_node, visit_constant_write_node,
    visit_def_node, visit_global_variable_read_node, visit_instance_variable_read_node,
    visit_local_variable_and_write_node, visit_local_variable_operator_write_node,
    visit_local_variable_or_write_node, visit_local_variable_read_node,
    visit_local_variable_target_node, visit_local_variable_write_node, visit_module_node,
    BlockNode, CallNode, ClassNode, ConstantPathNode, ConstantReadNode, DefNode,
    GlobalVariableReadNode, LocalVariableAndWriteNode, LocalVariableOperatorWriteNode,
    LocalVariableOrWriteNode, LocalVariableReadNode, LocalVariableTargetNode,
    LocalVariableWriteNode, Location, ModuleNode, Visit,
};

pub enum IdentifierType {
    ModuleDef,
    ClassDef,
    ConstantDef,
    MethodDef,
    LVarDef,
    Call,
}

/// Visitor for finding identifiers at a specific position
pub struct IdentifierVisitor {
    document: RubyDocument,
    position: Position,

    /// Stack of namespaces for each scope
    /// To support module/class definitions with ConstantPathNode
    /// we store the namespace stack for each scope as Vec<Vec<RubyConstant>>
    /// Eg. module A; end
    /// namespace_stack = [[A]]
    /// Eg. module A::B::C; end;
    /// namespace_stack = [[A, B, C]]
    namespace_stack: Vec<Vec<RubyConstant>>,
    scope_stack: Vec<LVScopeKind>,
    current_method: Option<RubyMethod>,
    pub ancestors: Vec<RubyConstant>,
    pub identifier: Option<Identifier>,
    pub identifier_type: IdentifierType,
}

impl IdentifierVisitor {
    pub fn new(document: RubyDocument, position: Position) -> Self {
        Self {
            document,
            position,
            namespace_stack: Vec::new(),
            scope_stack: Vec::new(),
            current_method: None,
            ancestors: Vec::new(),
            identifier: None,
            identifier_type: IdentifierType::Call,
        }
    }

    pub fn is_position_in_location(&self, location: &Location) -> bool {
        let position_offset = self.document.position_to_offset(self.position);

        let start_offset = location.start_offset();
        let end_offset = location.end_offset();

        position_offset >= start_offset && position_offset < end_offset
    }

    fn push_ns_scope(&mut self, namespace: RubyConstant) {
        self.namespace_stack.push(vec![namespace]);
    }

    fn push_ns_scopes(&mut self, namespaces: Vec<RubyConstant>) {
        self.namespace_stack.push(namespaces);
    }

    fn pop_ns_scope(&mut self) -> Option<Vec<RubyConstant>> {
        self.namespace_stack.pop()
    }

    fn push_lv_scope(&mut self, kind: LVScopeKind) {
        self.scope_stack.push(kind);
    }

    fn pop_lv_scope(&mut self) -> Option<LVScopeKind> {
        self.scope_stack.pop()
    }
}

impl Visit<'_> for IdentifierVisitor {
    fn visit_class_node(&mut self, node: &ClassNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let constant_path = node.constant_path();
        let name_loc = constant_path.location();

        if self.is_position_in_location(&name_loc) {
            // Handle constant path node for class definition
            if let Some(constant_path_node) = constant_path.as_constant_path_node() {
                let mut namespaces = Vec::new();
                utils::collect_namespaces(&constant_path_node, &mut namespaces);
                self.identifier = Some(Identifier::RubyConstant(namespaces));
            } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
                let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
                let namespace = RubyConstant::new(&name.to_string()).unwrap();
                self.identifier = Some(Identifier::RubyConstant(vec![namespace]));
            }
            self.identifier_type = IdentifierType::ClassDef;
            // Flatten the namespace stack into a single vector of constants
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            return;
        }

        // Add the class name to the namespace stack
        if let Some(constant_path_node) = constant_path.as_constant_path_node() {
            let mut namespaces = Vec::new();
            utils::collect_namespaces(&constant_path_node, &mut namespaces);
            self.push_ns_scopes(namespaces);
            self.push_lv_scope(LVScopeKind::Constant);
        } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
            let namespace = RubyConstant::new(&name.to_string()).unwrap();
            self.push_ns_scope(namespace);
            self.push_lv_scope(LVScopeKind::Constant);
        }

        // Visit the class body
        visit_class_node(self, &node);

        // Remove the class name from the namespace stack
        self.pop_ns_scope();
        self.pop_lv_scope();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let constant_path = node.constant_path();
        let name_loc = constant_path.location();

        if self.is_position_in_location(&name_loc) {
            // Handle constant path node for module definition
            if let Some(constant_path_node) = constant_path.as_constant_path_node() {
                let mut namespaces = Vec::new();
                utils::collect_namespaces(&constant_path_node, &mut namespaces);
                self.identifier = Some(Identifier::RubyConstant(namespaces));
            } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
                let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
                let namespace = RubyConstant::new(&name.to_string()).unwrap();
                self.identifier = Some(Identifier::RubyConstant(vec![namespace]));
            }
            self.identifier_type = IdentifierType::ModuleDef;
            // Flatten the namespace stack into a single vector of constants
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            return;
        }

        // Add the module name to the namespace stack
        if let Some(constant_path_node) = constant_path.as_constant_path_node() {
            let mut namespaces = Vec::new();
            utils::collect_namespaces(&constant_path_node, &mut namespaces);
            self.push_ns_scopes(namespaces);
            self.push_lv_scope(LVScopeKind::Constant);
        } else if let Some(constant_read_node) = constant_path.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read_node.name().as_slice());
            let namespace = RubyConstant::new(&name.to_string()).unwrap();
            self.push_ns_scope(namespace);
            self.push_lv_scope(LVScopeKind::Constant);
        }

        // Visit the module body
        visit_module_node(self, &node);

        // Remove the module name from the namespace stack
        self.pop_ns_scope();
        self.pop_lv_scope();
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name = String::from_utf8_lossy(&node.name().as_slice()).to_string();
        let method = RubyMethod::from(name);
        self.current_method = Some(method.clone());
        self.push_lv_scope(LVScopeKind::Method);

        // Is position on method name
        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            self.identifier = Some(Identifier::RubyMethod(vec![], method));
            self.identifier_type = IdentifierType::MethodDef;
            self.ancestors = vec![];
        }

        visit_def_node(self, node);
        self.current_method = None;
        self.pop_lv_scope();
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.push_lv_scope(LVScopeKind::Block);
        visit_block_node(self, node);
        self.pop_lv_scope();
    }

    fn visit_parameters_node(&mut self, node: &ParametersNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        // Required parameters
        let requireds = node.requireds();
        for required in requireds.iter() {
            if let Some(param) = required.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                    let var_type = RubyVariableType::Local(
                        self.document.uri.clone(),
                        self.scope_stack.clone(),
                    );
                    let var = RubyVariable::new(&param_name, var_type).unwrap();
                    self.identifier =
                        Some(Identifier::RubyVariable(self.current_method.clone(), var));
                    self.identifier_type = IdentifierType::LVarDef;
                    self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
                }
            }
        }

        // Optional parameters
        let optionals = node.optionals();
        for optional in optionals.iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                    let var_type = RubyVariableType::Local(
                        self.document.uri.clone(),
                        self.scope_stack.clone(),
                    );
                    let var = RubyVariable::new(&param_name, var_type).unwrap();
                    self.identifier =
                        Some(Identifier::RubyVariable(self.current_method.clone(), var));
                    self.identifier_type = IdentifierType::LVarDef;
                    self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
                }
            }
        }

        // Rest parameters
        if let Some(rest) = node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    if self.is_position_in_location(&param.location()) {
                        let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                        let var_type = RubyVariableType::Local(
                            self.document.uri.clone(),
                            self.scope_stack.clone(),
                        );
                        let var = RubyVariable::new(&param_name, var_type).unwrap();
                        self.identifier =
                            Some(Identifier::RubyVariable(self.current_method.clone(), var));
                        self.identifier_type = IdentifierType::LVarDef;
                        self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
                    }
                }
            }
        }

        // Post parameters
        for post in node.posts().iter() {
            if let Some(param) = post.as_required_parameter_node() {
                if self.is_position_in_location(&param.location()) {
                    let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                    let var_type = RubyVariableType::Local(
                        self.document.uri.clone(),
                        self.scope_stack.clone(),
                    );
                    let var = RubyVariable::new(&param_name, var_type).unwrap();
                    self.identifier =
                        Some(Identifier::RubyVariable(self.current_method.clone(), var));
                    self.identifier_type = IdentifierType::LVarDef;
                    self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
                }
            }
        }

        // TODO: keywords, keyword_rest, block
    }

    fn visit_constant_write_node(&mut self, node: &ruby_prism::ConstantWriteNode<'_>) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).unwrap();

        let name_loc = node.name_loc();
        if self.is_position_in_location(&name_loc) {
            self.identifier = Some(Identifier::RubyConstant(vec![constant]));
            self.identifier_type = IdentifierType::ConstantDef;
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            return;
        }

        visit_constant_write_node(self, node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        // Based on a constant node target, a constant path node parent and a position, this method will find the exact
        // portion of the constant path that matches the requested position, for higher precision in hover and
        // definition. For example:
        //
        // ```ruby
        // Foo::Bar::BAZ
        //           ^ Going to definition here should go to Foo::Bar::BAZ
        //      ^ Going to definition here should go to Foo::Bar - Parent of ConstantPathNode BAZ
        // ^ Going to definition here should go to Foo - Parent of ConstantPathNode Bar
        // ```
        if let Some(parent_node) = node.parent() {
            if self.is_position_in_location(&parent_node.location()) {
                visit_constant_path_node(self, node);
                return;
            }
        }

        let mut namespaces = vec![];
        utils::collect_namespaces(node, &mut namespaces);

        // Check if first two char are ::
        let code = self.document.content.as_bytes();
        let start = node.location().start_offset();
        let end = start + 2;
        let target_str = String::from_utf8_lossy(&code[start..end]).to_string();
        let is_root_constant = target_str.starts_with("::");

        // Process the namespace
        if !namespaces.is_empty() {
            self.identifier = Some(Identifier::RubyConstant(namespaces));
        }

        // Set ancestors based on whether it's a root constant
        if is_root_constant {
            self.ancestors = vec![];
        } else {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
        }
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Create a RubyConstant from the constant name
        if let Ok(constant) = RubyConstant::new(&constant_name) {
            self.identifier = Some(Identifier::RubyConstant(vec![constant]));
        } else {
            self.identifier = Some(Identifier::RubyConstant(Vec::new()));
        }

        self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        if let Some(arguments) = node.arguments() {
            if self.is_position_in_location(&arguments.location()) {
                visit_arguments_node(self, &arguments);
                return;
            }
        }

        if let Some(receiver) = node.receiver() {
            if self.is_position_in_location(&receiver.location()) {
                self.visit(&receiver);
                return;
            }
        }

        if let Some(block) = node.block() {
            if self.is_position_in_location(&block.location()) {
                self.visit(&block);
                return;
            }
        }

        let method_name_bytes = node.name().as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes).to_string();

        if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
            // Get the namespace from the receiver if it exists
            let mut namespace = vec![];

            if let Some(receiver) = node.receiver() {
                // Eg. Foo::Bar.baz
                // Foo::Bar is ConstantPathNode, Foo is ConstantReadNode, baz is CallNode
                if let Some(constant_path) = receiver.as_constant_path_node() {
                    let mut namespaces = vec![];
                    utils::collect_namespaces(&constant_path, &mut namespaces);
                    namespace = namespaces;
                }

                // Eg. Foo.bar, Foo::bar
                // Foo is ConstantReadNode, bar is CallNode
                if let Some(constant_read) = receiver.as_constant_read_node() {
                    let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
                    if let Ok(ns) = RubyConstant::new(&name) {
                        namespace.push(ns);
                    }
                }
            }

            self.identifier = Some(Identifier::RubyMethod(namespace, method_name));
            self.ancestors = vec![];
        }

        visit_call_node(self, node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        // Continue visiting the node
        visit_local_variable_read_node(self, node);
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.name_loc()) {
            visit_local_variable_write_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier_type = IdentifierType::LVarDef;
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.name_loc()) {
            visit_local_variable_and_write_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier_type = IdentifierType::LVarDef;
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.name_loc()) {
            visit_local_variable_or_write_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier_type = IdentifierType::LVarDef;
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.name_loc()) {
            visit_local_variable_operator_write_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier_type = IdentifierType::LVarDef;
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        visit_local_variable_operator_write_node(self, node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            visit_local_variable_target_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(
            &variable_name,
            RubyVariableType::Local(self.document.uri.clone(), self.scope_stack.clone()),
        );
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier_type = IdentifierType::LVarDef;
            self.identifier = Some(Identifier::RubyVariable(
                self.current_method.clone(),
                variable,
            ));
        }

        visit_local_variable_target_node(self, node);
    }

    fn visit_class_variable_read_node(&mut self, node: &ruby_prism::ClassVariableReadNode<'_>) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(&variable_name, RubyVariableType::Class);
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier = Some(Identifier::RubyVariable(None, variable));
        }

        visit_class_variable_read_node(self, node);
    }

    fn visit_instance_variable_read_node(
        &mut self,
        node: &ruby_prism::InstanceVariableReadNode<'_>,
    ) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(&variable_name, RubyVariableType::Instance);
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier = Some(Identifier::RubyVariable(None, variable));
        }

        visit_instance_variable_read_node(self, node);
    }

    fn visit_global_variable_read_node(&mut self, node: &GlobalVariableReadNode) {
        if self.identifier.is_some() || !self.is_position_in_location(&node.location()) {
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let var = RubyVariable::new(&variable_name, RubyVariableType::Global);
        if let Ok(variable) = var {
            self.ancestors = self.namespace_stack.iter().flatten().cloned().collect();
            self.identifier = Some(Identifier::RubyVariable(None, variable));
        }

        visit_global_variable_read_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Url};

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
                visitor.identifier.is_none(),
                "Expected identifier to be None at position {:?}",
                position
            );
            return;
        }

        // Otherwise, check the identifier was found
        assert!(
            visitor.identifier.is_some(),
            "Expected to find an identifier at position {:?}",
            position
        );

        // Get the identifier for further processing
        let identifier = visitor.identifier.as_ref().unwrap();

        // Special case for root constants
        if code.starts_with("::") {
            match identifier {
                Identifier::RubyConstant(parts) => {
                    // For root constants, we expect an empty namespace vector
                    if expected_parts.len() == 1 {
                        // For direct root constants like ::GLOBAL_CONSTANT
                        assert_eq!(
                            parts.len(),
                            0,
                            "Expected empty namespace vector for root constant"
                        );
                        assert_eq!(
                            parts[0].to_string(),
                            expected_parts[0],
                            "Expected constant name to match"
                        );
                    } else {
                        // For nested root constants like ::Foo::Bar::CONSTANT
                        assert_eq!(
                            parts.len(),
                            expected_parts.len() - 1,
                            "Namespace parts count mismatch for root constant path"
                        );
                        for (i, expected_part) in expected_parts
                            .iter()
                            .take(expected_parts.len() - 1)
                            .enumerate()
                        {
                            assert_eq!(
                                parts[i].to_string(),
                                *expected_part,
                                "Namespace part at index {} mismatch",
                                i
                            );
                        }
                        assert_eq!(
                            parts.last().unwrap().to_string(),
                            expected_parts[expected_parts.len() - 1],
                            "Expected constant name to match"
                        );
                    }
                    return;
                }
                _ => {}
            }
        }

        // Get the parts from the identifier - could be either a namespace or a constant
        let parts = match identifier {
            Identifier::RubyConstant(parts) => parts.clone(),
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
        // Empty vector indicates we expect identifier to be None
        test_visitor("Foo::Bar::Baz", Position::new(0, 3), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_nested_constant_path_at_scope_resolution_2() {
        // Test case: Foo::Bar::Baz with cursor at second "::"
        // Empty vector indicates we expect identifier to be None
        test_visitor(
            "Foo::Bar::Baz",
            Position::new(0, 8),
            vec!["Foo", "Bar", "Baz"],
        );
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyMethod(parts, method) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].to_string(), "Foo");
                assert_eq!(parts[1].to_string(), "Bar");
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyMethod(_, method) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
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
            Identifier::RubyConstant(parts) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0].to_string(), "Error");
                assert_eq!(parts[1].to_string(), "Codes");
                assert_eq!(parts[2].to_string(), "ITEM_ERROR");
            }
            _ => panic!("Expected Constant FQN, got {:?}", identifier),
        }
    }
}
