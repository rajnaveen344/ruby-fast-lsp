//! Visitor to convert tree-sitter AST to our RBS types.

use tree_sitter::Node;

use crate::types::*;

/// Visitor that converts tree-sitter nodes to our AST types
pub struct Visitor<'a> {
    source: &'a str,
    pub declarations: Vec<Declaration>,
    current_visibility: Visibility,
}

impl<'a> Visitor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            declarations: Vec::new(),
            current_visibility: Visibility::Public,
        }
    }

    /// Get the text content of a node
    fn node_text(&self, node: &Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// Get location from a tree-sitter node
    fn node_location(&self, node: &Node) -> Location {
        let start = node.start_position();
        let end = node.end_position();
        Location::new(start.row, start.column, end.row, end.column)
    }

    /// Visit the root program node
    pub fn visit_program(&mut self, node: Node) -> Result<(), ParseError> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            self.visit_declaration(child)?;
        }

        Ok(())
    }

    /// Visit a top-level declaration
    fn visit_declaration(&mut self, node: Node) -> Result<(), ParseError> {
        match node.kind() {
            // Handle wrapper nodes
            "decl" => {
                // decl wraps the actual declaration
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_declaration(child)?;
                }
            }
            "class_declaration" | "class_decl" => {
                let decl = self.visit_class_declaration(node)?;
                self.declarations.push(Declaration::Class(decl));
            }
            "module_declaration" | "module_decl" => {
                let decl = self.visit_module_declaration(node)?;
                self.declarations.push(Declaration::Module(decl));
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
                "class_name" => {
                    // class_name contains a constant child
                    class.name = self.extract_name(&child);
                }
                "namespace" => {
                    class.name = self.node_text(&child).to_string();
                }
                "type_parameters" | "module_type_parameters" => {
                    class.type_params = self.visit_type_parameters(child)?;
                }
                "superclass" | "class_super" => {
                    if let Some(type_node) = child.child(0) {
                        class.superclass = Some(self.visit_type(type_node)?);
                    }
                }
                "members" | "class_body" => {
                    self.visit_members(child, &mut class.members, &mut class.methods)?;
                }
                _ => {}
            }
        }

        Ok(class)
    }

    /// Extract name from a node that may contain nested constant/identifier
    fn extract_name(&self, node: &Node) -> String {
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
                    module.name = self.node_text(&child).to_string();
                }
                "type_parameters" => {
                    module.type_params = self.visit_type_parameters(child)?;
                }
                "module_self_types" => {
                    module.self_types = self.visit_self_types(child)?;
                }
                "members" | "module_body" => {
                    self.visit_members(child, &mut module.members, &mut module.methods)?;
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
                    interface.name = self.node_text(&child).to_string();
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
                    alias.name = self.node_text(&child).to_string();
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
                "constant" | "constant_name" => {
                    constant.name = self.node_text(&child).to_string();
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
    fn visit_type_parameters(&self, node: Node) -> Result<Vec<TypeParam>, ParseError> {
        let mut params = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "type_parameter" || child.kind() == "type_variable" {
                let mut param = TypeParam::new(self.node_text(&child));

                // Check for variance
                let text = self.node_text(&child);
                if text.starts_with("out ") {
                    param.variance = Variance::Covariant;
                    param.name = text.trim_start_matches("out ").to_string();
                } else if text.starts_with("in ") {
                    param.variance = Variance::Contravariant;
                    param.name = text.trim_start_matches("in ").to_string();
                }

                params.push(param);
            } else if child.is_named() {
                // Try to extract name from nested structure
                let text = self.node_text(&child).trim();
                if !text.is_empty()
                    && !text.starts_with('[')
                    && !text.starts_with(',')
                    && !text.starts_with(']')
                {
                    params.push(TypeParam::new(text));
                }
            }
        }

        Ok(params)
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

    /// Visit class/module members
    fn visit_members(
        &mut self,
        node: Node,
        members: &mut Vec<Member>,
        methods: &mut Vec<MethodDecl>,
    ) -> Result<(), ParseError> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // Handle wrapper "member" node
                "member" => {
                    self.visit_members(child, members, methods)?;
                }
                "method_definition" | "method_member" | "method" => {
                    let method = self.visit_method_definition(child)?;
                    methods.push(method);
                }
                "singleton_method_definition" | "singleton_method" => {
                    let mut method = self.visit_method_definition(child)?;
                    method.kind = MethodKind::Singleton;
                    methods.push(method);
                }
                "include_member" | "include" => {
                    if let Some(type_node) = child.child(1).or_else(|| child.child(0)) {
                        if let Ok(t) = self.visit_type(type_node) {
                            members.push(Member::Include(t));
                        }
                    }
                }
                "extend_member" | "extend" => {
                    if let Some(type_node) = child.child(1).or_else(|| child.child(0)) {
                        if let Ok(t) = self.visit_type(type_node) {
                            members.push(Member::Extend(t));
                        }
                    }
                }
                "prepend_member" | "prepend" => {
                    if let Some(type_node) = child.child(1).or_else(|| child.child(0)) {
                        if let Ok(t) = self.visit_type(type_node) {
                            members.push(Member::Prepend(t));
                        }
                    }
                }
                "attr_reader" | "attr_writer" | "attr_accessor" | "attribute_member" => {
                    if let Ok(attr) = self.visit_attribute(child) {
                        members.push(Member::Attr(attr));
                    }
                }
                "alias_member" | "alias" => {
                    if let Ok(alias) = self.visit_alias(child) {
                        members.push(Member::Alias(alias));
                    }
                }
                "visibility_member" => {
                    let text = self.node_text(&child);
                    if text.contains("private") {
                        self.current_visibility = Visibility::Private;
                        members.push(Member::Private);
                    } else if text.contains("public") {
                        self.current_visibility = Visibility::Public;
                        members.push(Member::Public);
                    }
                }
                "public" => {
                    self.current_visibility = Visibility::Public;
                    members.push(Member::Public);
                }
                "private" => {
                    self.current_visibility = Visibility::Private;
                    members.push(Member::Private);
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Visit a method definition
    fn visit_method_definition(&mut self, node: Node) -> Result<MethodDecl, ParseError> {
        let mut method = MethodDecl::new("");
        method.visibility = self.current_visibility;
        method.location = Some(self.node_location(&node));

        let mut method_suffix = String::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "method_name" => {
                    // method_name contains identifier child
                    method.name = self.extract_name(&child);
                }
                "name" | "operator" | "identifier" => {
                    let name = self.node_text(&child);
                    if !name.is_empty() && method.name.is_empty() {
                        method.name = name.to_string();
                    }
                }
                // tree-sitter-rbs parses ! and ? as ERROR nodes for method names like upcase!, empty?
                "ERROR" => {
                    let text = self.node_text(&child);
                    if text == "!" || text == "?" {
                        method_suffix = text.to_string();
                    }
                }
                "method_type" | "method_types" | "overload" => {
                    self.visit_method_types(&child, &mut method.overloads)?;
                }
                "self" | "self." => {
                    method.kind = MethodKind::Singleton;
                }
                _ => {}
            }
        }

        // Append ! or ? suffix if present (tree-sitter-rbs parses these as ERROR nodes)
        if !method_suffix.is_empty() {
            method.name.push_str(&method_suffix);
        }

        // If no method types were found, try to parse the whole node text
        if method.overloads.is_empty() {
            let text = self.node_text(&node);
            // Extract return type from simple patterns like "def foo: () -> Type"
            if let Some(arrow_pos) = text.rfind("->") {
                let return_type_str = text[arrow_pos + 2..].trim();
                if !return_type_str.is_empty() {
                    let return_type = self.parse_simple_type(return_type_str);
                    method.overloads.push(MethodType::new(return_type));
                }
            }
        }

        Ok(method)
    }

    /// Visit method types (handles both single and multiple overloads)
    fn visit_method_types(
        &self,
        node: &Node,
        overloads: &mut Vec<MethodType>,
    ) -> Result<(), ParseError> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "method_type" => {
                    if let Ok(mt) = self.visit_method_type(child) {
                        overloads.push(mt);
                    }
                }
                "method_type_body" => {
                    if let Ok(mt) = self.visit_method_type_body(child) {
                        overloads.push(mt);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Visit a method type body (parameters -> return_type)
    fn visit_method_type_body(&self, node: Node) -> Result<MethodType, ParseError> {
        let mut method_type = MethodType::new(RbsType::Void);
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_parameters" => {
                    method_type.type_params = self.visit_type_parameters(child)?;
                }
                "parameters" => {
                    method_type.params = self.visit_parameters(child)?;
                }
                "type" => {
                    method_type.return_type = self.visit_type(child)?;
                }
                "block" | "block_type" => {
                    method_type.block = Some(self.visit_block(child)?);
                }
                _ => {}
            }
        }

        Ok(method_type)
    }

    /// Visit a method type signature
    fn visit_method_type(&self, node: Node) -> Result<MethodType, ParseError> {
        let mut method_type = MethodType::new(RbsType::Void);
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // Delegate to method_type_body if present
                "method_type_body" => {
                    return self.visit_method_type_body(child);
                }
                "type_parameters" => {
                    method_type.type_params = self.visit_type_parameters(child)?;
                }
                "parameters" | "method_parameters" | "params" => {
                    method_type.params = self.visit_parameters(child)?;
                }
                "return_type" | "type" => {
                    method_type.return_type = self.visit_type(child)?;
                }
                "block" | "block_type" => {
                    method_type.block = Some(self.visit_block(child)?);
                }
                _ => {}
            }
        }

        Ok(method_type)
    }

    /// Visit method parameters
    fn visit_parameters(&self, node: Node) -> Result<Vec<MethodParam>, ParseError> {
        let mut params = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "required_parameter" | "parameter" => {
                    let param = self.visit_parameter(child, ParamKind::Required)?;
                    params.push(param);
                }
                "optional_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::Optional)?;
                    params.push(param);
                }
                "rest_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::Rest)?;
                    params.push(param);
                }
                "keyword_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::Keyword)?;
                    params.push(param);
                }
                "optional_keyword_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::KeywordOpt)?;
                    params.push(param);
                }
                "keyword_rest_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::KeywordRest)?;
                    params.push(param);
                }
                "block_parameter" => {
                    let param = self.visit_parameter(child, ParamKind::Block)?;
                    params.push(param);
                }
                _ => {
                    // Try to parse as a type directly
                    if let Ok(t) = self.visit_type(child) {
                        params.push(MethodParam::required(t));
                    }
                }
            }
        }

        Ok(params)
    }

    /// Visit a single parameter
    fn visit_parameter(&self, node: Node, kind: ParamKind) -> Result<MethodParam, ParseError> {
        let mut param = MethodParam {
            name: None,
            r#type: RbsType::Untyped,
            kind,
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameter_name" | "name" | "identifier" => {
                    param.name = Some(self.node_text(&child).to_string());
                }
                _ => {
                    if let Ok(t) = self.visit_type(child) {
                        param.r#type = t;
                    }
                }
            }
        }

        // If no type was found, try the whole node text
        if param.r#type == RbsType::Untyped {
            let text = self.node_text(&node);
            // Handle patterns like "Integer index" or "?Integer length"
            let parts: Vec<&str> = text.split_whitespace().collect();
            if !parts.is_empty() {
                let type_str = parts[0].trim_start_matches('?').trim_start_matches('*');
                param.r#type = self.parse_simple_type(type_str);
                if parts.len() > 1 {
                    param.name = Some(parts[1].to_string());
                }
            }
        }

        Ok(param)
    }

    /// Visit a block type
    fn visit_block(&self, node: Node) -> Result<Block, ParseError> {
        let mut block = Block {
            params: Vec::new(),
            return_type: RbsType::Void,
            required: false,
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameters" | "block_parameters" => {
                    block.params = self.visit_parameters(child)?;
                }
                "return_type" | "type" => {
                    block.return_type = self.visit_type(child)?;
                }
                "?" => {
                    block.required = false;
                }
                _ => {}
            }
        }

        Ok(block)
    }

    /// Visit an attribute declaration
    fn visit_attribute(&self, node: Node) -> Result<AttrDecl, ParseError> {
        let text = self.node_text(&node);
        let kind = if text.contains("attr_accessor") {
            AttrKind::Accessor
        } else if text.contains("attr_writer") {
            AttrKind::Writer
        } else {
            AttrKind::Reader
        };

        let mut attr = AttrDecl {
            name: String::new(),
            kind,
            r#type: RbsType::Untyped,
            is_singleton: text.contains("self."),
            location: Some(self.node_location(&node)),
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "attr_name" | "name" | "identifier" => {
                    attr.name = self.node_text(&child).to_string();
                }
                _ => {
                    if let Ok(t) = self.visit_type(child) {
                        attr.r#type = t;
                    }
                }
            }
        }

        Ok(attr)
    }

    /// Visit an alias declaration
    fn visit_alias(&self, node: Node) -> Result<AliasDecl, ParseError> {
        let mut alias = AliasDecl {
            new_name: String::new(),
            old_name: String::new(),
            is_singleton: false,
            location: Some(self.node_location(&node)),
        };

        let text = self.node_text(&node);
        alias.is_singleton = text.contains("self.");

        let mut cursor = node.walk();
        let mut names: Vec<String> = Vec::new();

        for child in node.children(&mut cursor) {
            if child.kind() == "method_name" || child.kind() == "name" {
                names.push(self.node_text(&child).to_string());
            }
        }

        if names.len() >= 2 {
            alias.new_name = names[0].clone();
            alias.old_name = names[1].clone();
        }

        Ok(alias)
    }

    /// Visit a type node
    pub fn visit_type(&self, node: Node) -> Result<RbsType, ParseError> {
        let kind = node.kind();
        let text = self.node_text(&node).trim();

        match kind {
            // Simple types
            "class_type" | "class_name" | "namespace" | "constant" => {
                Ok(self.parse_class_type(node))
            }

            "interface_type" | "interface_name" => Ok(RbsType::Interface(text.to_string())),

            "type_variable" | "type_var" => Ok(RbsType::TypeVar(text.to_string())),

            // Union type
            "union_type" => {
                let mut types = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        types.push(t);
                    }
                }
                Ok(RbsType::union(types))
            }

            // Intersection type
            "intersection_type" => {
                let mut types = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        types.push(t);
                    }
                }
                Ok(RbsType::Intersection(types))
            }

            // Optional type
            "optional_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        return Ok(RbsType::optional(t));
                    }
                }
                Ok(RbsType::optional(RbsType::Untyped))
            }

            // Tuple type
            "tuple_type" => {
                let mut types = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        types.push(t);
                    }
                }
                Ok(RbsType::Tuple(types))
            }

            // Record type
            "record_type" => {
                let mut fields = Vec::new();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "record_field" {
                        if let Ok((name, t)) = self.visit_record_field(child) {
                            fields.push((name, t));
                        }
                    }
                }
                Ok(RbsType::Record(fields))
            }

            // Proc type
            "proc_type" => {
                let method_type = self.visit_method_type(node)?;
                Ok(RbsType::Proc(Box::new(method_type)))
            }

            // Literal types
            "string_literal" => {
                let s = text.trim_matches('"').trim_matches('\'');
                Ok(RbsType::Literal(Literal::String(s.to_string())))
            }
            "integer_literal" => {
                let n = text.parse().unwrap_or(0);
                Ok(RbsType::Literal(Literal::Integer(n)))
            }
            "symbol_literal" => {
                let s = text.trim_start_matches(':');
                Ok(RbsType::Literal(Literal::Symbol(s.to_string())))
            }
            "true_literal" | "true" => Ok(RbsType::Literal(Literal::True)),
            "false_literal" | "false" => Ok(RbsType::Literal(Literal::False)),

            // Special types
            "self" | "self_type" => Ok(RbsType::SelfType),
            "instance" | "instance_type" => Ok(RbsType::Instance),
            "class" | "class_singleton_type" => Ok(RbsType::ClassType),
            "void" => Ok(RbsType::Void),
            "nil" | "nil_type" => Ok(RbsType::Nil),
            "bool" | "bool_type" => Ok(RbsType::Bool),
            "untyped" => Ok(RbsType::Untyped),
            "top" => Ok(RbsType::Top),
            "bot" => Ok(RbsType::Bot),

            // Generic type application
            "generic_type" | "type_application" => {
                let mut name = String::new();
                let mut args = Vec::new();
                let mut cursor = node.walk();

                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "class_name" | "namespace" | "constant" => {
                            name = self.node_text(&child).to_string();
                        }
                        "type_arguments" | "type_args" => {
                            let mut args_cursor = child.walk();
                            for arg in child.children(&mut args_cursor) {
                                if let Ok(t) = self.visit_type(arg) {
                                    args.push(t);
                                }
                            }
                        }
                        _ => {
                            if let Ok(t) = self.visit_type(child) {
                                args.push(t);
                            }
                        }
                    }
                }

                if args.is_empty() {
                    Ok(RbsType::Class(name))
                } else {
                    Ok(RbsType::generic(name, args))
                }
            }

            // Parenthesized type
            "parenthesized_type" | "grouped_type" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        return Ok(t);
                    }
                }
                Err(ParseError::new("Empty parenthesized type"))
            }

            // Default: try to parse as simple type
            _ => Ok(self.parse_simple_type(text)),
        }
    }

    /// Parse a class type node which may have type arguments
    fn parse_class_type(&self, node: Node) -> RbsType {
        let text = self.node_text(&node);
        let mut cursor = node.walk();

        // Look for type arguments
        let mut args = Vec::new();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_arguments" || child.kind() == "type_args" {
                let mut args_cursor = child.walk();
                for arg in child.children(&mut args_cursor) {
                    if let Ok(t) = self.visit_type(arg) {
                        args.push(t);
                    }
                }
            }
        }

        // Extract the class name (remove type arguments from text if present)
        let name = if let Some(bracket_pos) = text.find('[') {
            text[..bracket_pos].trim().to_string()
        } else {
            text.to_string()
        };

        if args.is_empty() {
            RbsType::Class(name)
        } else {
            RbsType::generic(name, args)
        }
    }

    /// Visit a record field
    fn visit_record_field(&self, node: Node) -> Result<(String, RbsType), ParseError> {
        let mut name = String::new();
        let mut field_type = RbsType::Untyped;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "record_key" | "key" | "symbol" | "identifier" => {
                    name = self.node_text(&child).trim_start_matches(':').to_string();
                }
                _ => {
                    if let Ok(t) = self.visit_type(child) {
                        field_type = t;
                    }
                }
            }
        }

        Ok((name, field_type))
    }

    /// Parse a simple type from text
    fn parse_simple_type(&self, text: &str) -> RbsType {
        let text = text.trim();

        // Handle optional types
        if text.ends_with('?') {
            let inner = self.parse_simple_type(&text[..text.len() - 1]);
            return RbsType::optional(inner);
        }

        // Handle special keywords
        match text {
            "self" => RbsType::SelfType,
            "instance" => RbsType::Instance,
            "class" => RbsType::ClassType,
            "void" => RbsType::Void,
            "nil" => RbsType::Nil,
            "bool" => RbsType::Bool,
            "untyped" => RbsType::Untyped,
            "top" => RbsType::Top,
            "bot" => RbsType::Bot,
            "true" => RbsType::Literal(Literal::True),
            "false" => RbsType::Literal(Literal::False),
            _ => {
                // Check for generic types like Array[String]
                if let Some(bracket_pos) = text.find('[') {
                    if let Some(close_bracket) = text.rfind(']') {
                        if close_bracket > bracket_pos {
                            let name = text[..bracket_pos].trim();
                            let args_str = &text[bracket_pos + 1..close_bracket];
                            let args: Vec<RbsType> = args_str
                                .split(',')
                                .map(|s| self.parse_simple_type(s.trim()))
                                .collect();
                            return RbsType::generic(name, args);
                        }
                    }
                    // Incomplete generic type, just use the name part
                    let name = text[..bracket_pos].trim();
                    RbsType::Class(name.to_string())
                } else {
                    RbsType::Class(text.to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    #[test]
    fn test_visit_simple_class() {
        let mut parser = Parser::new();
        let source = "class Foo end";
        let result = parser.parse(source);
        assert!(result.is_ok());

        let decls = result.unwrap();
        assert_eq!(decls.len(), 1);

        if let Declaration::Class(class) = &decls[0] {
            assert_eq!(class.name, "Foo");
        } else {
            panic!("Expected class");
        }
    }

    #[test]
    fn test_visit_class_with_superclass() {
        let mut parser = Parser::new();
        let source = "class Foo < Bar end";
        let result = parser.parse(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_simple_type() {
        let visitor = Visitor::new("");

        assert_eq!(
            visitor.parse_simple_type("String"),
            RbsType::Class("String".to_string())
        );
        assert_eq!(visitor.parse_simple_type("void"), RbsType::Void);
        assert_eq!(visitor.parse_simple_type("nil"), RbsType::Nil);
        assert_eq!(visitor.parse_simple_type("bool"), RbsType::Bool);
        assert_eq!(visitor.parse_simple_type("self"), RbsType::SelfType);
    }

    #[test]
    fn test_parse_optional_type() {
        let visitor = Visitor::new("");

        let result = visitor.parse_simple_type("String?");
        assert_eq!(
            result,
            RbsType::optional(RbsType::Class("String".to_string()))
        );
    }

    #[test]
    fn test_parse_generic_type() {
        let visitor = Visitor::new("");

        let result = visitor.parse_simple_type("Array[String]");
        assert_eq!(
            result,
            RbsType::generic("Array", vec![RbsType::Class("String".to_string())])
        );
    }
}
