//! Method definitions, overloads, parameters, and block signatures.

use tree_sitter::Node;

use super::Visitor;
use crate::types::{
    Block, MethodDecl, MethodKind, MethodParam, MethodType, ParamKind, ParseError, RbsType,
};

impl<'a> Visitor<'a> {
    /// Visit a method definition
    pub(super) fn visit_method_definition(&mut self, node: Node) -> Result<MethodDecl, ParseError> {
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
                    let return_type = Self::parse_simple_type(return_type_str);
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
                "type_parameters" | "method_type_parameters" => {
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
    pub(super) fn visit_method_type(&self, node: Node) -> Result<MethodType, ParseError> {
        let mut method_type = MethodType::new(RbsType::Void);
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // Delegate to method_type_body if present
                "method_type_body" => {
                    let mut body = self.visit_method_type_body(child)?;
                    body.type_params = method_type.type_params;
                    method_type = body;
                }
                "type_parameters" | "method_type_parameters" => {
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
            if !child.is_named() {
                continue;
            }
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
                "required_positionals" | "trailing_positionals" => {
                    params.extend(self.visit_parameter_group(child, ParamKind::Required)?);
                }
                "optional_positionals" => {
                    params.extend(self.visit_parameter_group(child, ParamKind::Optional)?);
                }
                "rest_positional" => {
                    params.extend(self.visit_parameter_group(child, ParamKind::Rest)?);
                }
                "keywords" => params.extend(self.visit_parameters(child)?),
                "required_keywords" => {
                    params.extend(self.visit_keyword_group(child, ParamKind::Keyword)?);
                }
                "optional_keywords" => {
                    params.extend(self.visit_keyword_group(child, ParamKind::KeywordOpt)?);
                }
                "splat_keyword" => {
                    params.extend(self.visit_parameter_group(child, ParamKind::KeywordRest)?);
                }
                "unnamed_parameter" => {}
                _ => {}
            }
        }

        Ok(params)
    }

    fn visit_parameter_group(
        &self,
        node: Node,
        kind: ParamKind,
    ) -> Result<Vec<MethodParam>, ParseError> {
        let mut params = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() && child.kind() == "parameter" {
                params.push(self.visit_parameter(child, kind.clone())?);
            }
        }
        Ok(params)
    }

    fn visit_keyword_group(
        &self,
        node: Node,
        kind: ParamKind,
    ) -> Result<Vec<MethodParam>, ParseError> {
        let mut params = Vec::new();
        let mut name = None;
        let mut parameter_type = None;
        let mut nested = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            match child.kind() {
                "keyword" => name = Some(self.node_text(&child).to_string()),
                "type" if parameter_type.is_none() => {
                    parameter_type = Some(self.visit_type(child)?);
                }
                "keywords" => nested.extend(self.visit_parameters(child)?),
                _ => {}
            }
        }
        if let (Some(name), Some(parameter_type)) = (name, parameter_type) {
            params.push(MethodParam {
                name: Some(name),
                r#type: parameter_type,
                kind,
            });
        }
        params.extend(nested);
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
                "parameter_name" | "name" | "identifier" | "var_name" => {
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
                param.r#type = Self::parse_simple_type(type_str);
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
            required: true,
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
}
