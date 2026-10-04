//! Top-level class, module, interface, alias, constant, and global declarations.

use tree_sitter::Node;

use super::Visitor;
use crate::types::{
    ClassDecl, ConstantDecl, Declaration, GlobalDecl, InterfaceDecl, ModuleDecl, ParseError,
    RbsType, TypeAliasDecl, TypeParam, Variance,
};

impl<'a> Visitor<'a> {
    /// Visit a top-level declaration
    pub(super) fn visit_declaration(&mut self, node: Node) -> Result<(), ParseError> {
        match node.kind() {
            // Handle wrapper nodes
            "decl" => {
                // decl wraps the actual declaration
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_declaration(child)?;
                }
            }
            // Nested declarations are flattened after their enclosing one,
            // so a namespace always precedes its members.
            "class_declaration" | "class_decl" => {
                let position = self.declarations.len();
                let decl = self.visit_class_declaration(node)?;
                self.declarations.insert(position, Declaration::Class(decl));
            }
            "module_declaration" | "module_decl" => {
                let position = self.declarations.len();
                let decl = self.visit_module_declaration(node)?;
                self.declarations
                    .insert(position, Declaration::Module(decl));
            }
            "interface_declaration" | "interface_decl" => {
                let decl = self.visit_interface_declaration(node)?;
                self.declarations.push(Declaration::Interface(decl));
            }
            "type_alias_declaration" | "type_alias_decl" => {
                let decl = self.visit_type_alias_declaration(node)?;
                self.declarations.push(Declaration::TypeAlias(decl));
            }
            "constant_declaration" | "const_decl" => {
                let decl = self.visit_constant_declaration(node)?;
                self.declarations.push(Declaration::Constant(decl));
            }
            "global_declaration" | "global_decl" => {
                let decl = self.visit_global_declaration(node)?;
                self.declarations.push(Declaration::Global(decl));
            }
            // Skip comments and other non-declaration nodes
            _ => {}
        }

        Ok(())
    }

    /// Visit a class declaration
    fn visit_class_declaration(&mut self, node: Node) -> Result<ClassDecl, ParseError> {
        let mut class = ClassDecl::new("");
        class.location = Some(self.node_location(&node));

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "class_name" | "namespace" => {
                    class.name = self.qualified_name(&child);
                }
                "type_parameters" | "module_type_parameters" => {
                    class.type_params = self.visit_type_parameters(child)?;
                }
                "superclass" | "class_super" => {
                    class.superclass = Some(self.visit_member_target(child)?);
                }
                "members" | "class_body" => {
                    let outer = self.enter_namespace(&class.name);
                    let visited = self.visit_members(child, &mut class.members, &mut class.methods);
                    self.leave_namespace(outer);
                    visited?;
                }
                _ => {}
            }
        }

        Ok(class)
    }

    /// Extract name from a node that may contain nested constant/identifier
    pub(super) fn extract_name(&self, node: &Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "constant" | "identifier" => {
                    return self.node_text(&child).to_string();
                }
                _ => {}
            }
        }
        // Fallback to the node's own text
        self.node_text(node).to_string()
    }

    /// Visit a module declaration
    fn visit_module_declaration(&mut self, node: Node) -> Result<ModuleDecl, ParseError> {
        let mut module = ModuleDecl::new("");
        module.location = Some(self.node_location(&node));

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "module_name" | "namespace" => {
                    module.name = self.qualified_name(&child);
                }
                "type_parameters" | "module_type_parameters" => {
                    module.type_params = self.visit_type_parameters(child)?;
                }
                "module_self_types" => {
                    module.self_types = self.visit_self_types(child)?;
                }
                "members" | "module_body" => {
                    let outer = self.enter_namespace(&module.name);
                    let visited =
                        self.visit_members(child, &mut module.members, &mut module.methods);
                    self.leave_namespace(outer);
                    visited?;
                }
                _ => {}
            }
        }

        Ok(module)
    }

    /// Visit an interface declaration
    fn visit_interface_declaration(&mut self, node: Node) -> Result<InterfaceDecl, ParseError> {
        let mut interface = InterfaceDecl {
            name: String::new(),
            type_params: Vec::new(),
            methods: Vec::new(),
            location: Some(self.node_location(&node)),
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "interface_name" => {
                    interface.name = self.qualified_name(&child);
                }
                "type_parameters" => {
                    interface.type_params = self.visit_type_parameters(child)?;
                }
                "members" | "interface_body" => {
                    let mut members = Vec::new();
                    self.visit_members(child, &mut members, &mut interface.methods)?;
                }
                _ => {}
            }
        }

        Ok(interface)
    }

    /// Visit a type alias declaration
    fn visit_type_alias_declaration(&mut self, node: Node) -> Result<TypeAliasDecl, ParseError> {
        let mut alias = TypeAliasDecl {
            name: String::new(),
            type_params: Vec::new(),
            r#type: RbsType::Untyped,
            location: Some(self.node_location(&node)),
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "alias_name" | "type_alias_name" => {
                    alias.name = self.qualified_name(&child);
                }
                "type_parameters" => {
                    alias.type_params = self.visit_type_parameters(child)?;
                }
                _ => {
                    // Try to parse as type if it looks like one
                    if let Ok(t) = self.visit_type(child) {
                        alias.r#type = t;
                    }
                }
            }
        }

        Ok(alias)
    }

    /// Visit a constant declaration
    fn visit_constant_declaration(&mut self, node: Node) -> Result<ConstantDecl, ParseError> {
        let mut constant = ConstantDecl {
            name: String::new(),
            r#type: RbsType::Untyped,
            location: Some(self.node_location(&node)),
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "constant" | "constant_name" | "const_name" => {
                    constant.name = self.qualified_name(&child);
                }
                _ => {
                    if let Ok(t) = self.visit_type(child) {
                        constant.r#type = t;
                    }
                }
            }
        }

        Ok(constant)
    }

    /// Visit a global declaration
    fn visit_global_declaration(&mut self, node: Node) -> Result<GlobalDecl, ParseError> {
        let mut global = GlobalDecl {
            name: String::new(),
            r#type: RbsType::Untyped,
            location: Some(self.node_location(&node)),
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "global_name" | "global" => {
                    global.name = self.node_text(&child).to_string();
                }
                _ => {
                    if let Ok(t) = self.visit_type(child) {
                        global.r#type = t;
                    }
                }
            }
        }

        Ok(global)
    }

    /// Visit type parameters
    pub(super) fn visit_type_parameters(&self, node: Node) -> Result<Vec<TypeParam>, ParseError> {
        let mut params = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if matches!(
                child.kind(),
                "type_parameter"
                    | "module_type_parameter"
                    | "type_variable"
                    | "method_type_parameter"
            ) {
                params.push(self.visit_type_parameter(child)?);
            }
        }

        Ok(params)
    }

    fn visit_type_parameter(&self, node: Node) -> Result<TypeParam, ParseError> {
        let text = self.node_text(&node).trim();
        let mut variable_name = None;
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            if current.kind() == "type_variable" {
                let mut cursor = current.walk();
                variable_name = current
                    .named_children(&mut cursor)
                    .find(|child| matches!(child.kind(), "constant" | "identifier"))
                    .map(|child| self.node_text(&child).to_string())
                    .or_else(|| Some(self.node_text(&current).to_string()));
                break;
            }
            let mut cursor = current.walk();
            pending.extend(current.named_children(&mut cursor));
        }
        let name = variable_name.ok_or_else(|| {
            ParseError::with_location(
                "RBS type parameter has no type variable",
                self.node_location(&node),
            )
        })?;
        let mut parameter = TypeParam::new(name);
        let tokens = text.split_whitespace().collect::<Vec<_>>();
        parameter.variance = if tokens.contains(&"out") {
            Variance::Covariant
        } else if tokens.contains(&"in") {
            Variance::Contravariant
        } else {
            Variance::Invariant
        };
        Ok(parameter)
    }

    /// Visit module self types
    fn visit_self_types(&self, node: Node) -> Result<Vec<RbsType>, ParseError> {
        let mut types = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if let Ok(t) = self.visit_type(child) {
                types.push(t);
            }
        }

        Ok(types)
    }
}
