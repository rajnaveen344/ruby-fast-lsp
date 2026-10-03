//! RBS type expressions, including records, generics, and textual fallback.

use tree_sitter::Node;

use super::Visitor;
use crate::types::{Literal, ParseError, RbsType, RecordField};

impl<'a> Visitor<'a> {
    /// Visit a type node
    pub fn visit_type(&self, node: Node) -> Result<RbsType, ParseError> {
        let kind = node.kind();
        let text = self.node_text(&node).trim();

        match kind {
            // The grammar wraps every concrete type node in a `type` field.
            "type" => {
                let mut cursor = node.walk();
                let mut named = node.named_children(&mut cursor);
                let child = named.next().ok_or_else(|| {
                    ParseError::with_location(
                        "RBS type wrapper has no concrete type",
                        self.node_location(&node),
                    )
                })?;
                if named.next().is_some() {
                    return Err(ParseError::with_location(
                        "RBS type wrapper has multiple concrete types",
                        self.node_location(&node),
                    ));
                }
                self.visit_type(child)
            }

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
                for child in node.named_children(&mut cursor) {
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
                for child in node.named_children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        types.push(t);
                    }
                }
                Ok(RbsType::Intersection(types))
            }

            // Optional type
            "optional_type" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
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
                for child in node.named_children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        types.push(t);
                    }
                }
                Ok(RbsType::Tuple(types))
            }

            // Record type
            "record_type" => {
                let mut fields = Vec::new();
                let mut pending_key = None;
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "record_key" => {
                            invariant!(
                                pending_key.is_none(),
                                what = "an RBS record exposed two keys without an intervening value",
                                why = "the grammar emits paired key/value fields",
                                fix = "keep the visitor synchronized with tree-sitter-rbs record_type",
                            );
                            pending_key = Some((
                                self.node_text(&child).to_string(),
                                self.record_key_is_optional(&node, &child),
                            ));
                        }
                        "type" => {
                            let Some((name, optional)) = pending_key.take() else {
                                return Err(ParseError::with_location(
                                    "RBS record value has no preceding key",
                                    self.node_location(&child),
                                ));
                            };
                            fields.push(RecordField::new(name, self.visit_type(child)?, optional));
                        }
                        _ => {}
                    }
                }
                if pending_key.is_some() {
                    return Err(ParseError::with_location(
                        "RBS record key has no value type",
                        self.node_location(&node),
                    ));
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

                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "class_name" | "namespace" | "constant" => {
                            name = self.node_text(&child).to_string();
                        }
                        "type_arguments" | "type_args" => {
                            let mut args_cursor = child.walk();
                            for arg in child.named_children(&mut args_cursor) {
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
                for child in node.named_children(&mut cursor) {
                    if let Ok(t) = self.visit_type(child) {
                        return Ok(t);
                    }
                }
                Err(ParseError::new("Empty parenthesized type"))
            }

            // Default: try to parse as simple type
            _ => Ok(Self::parse_simple_type(text)),
        }
    }

    /// Parse a class type node which may have type arguments
    fn parse_class_type(&self, node: Node) -> RbsType {
        let text = self.node_text(&node);
        let mut cursor = node.walk();

        // Look for type arguments
        let mut args = Vec::new();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "type_arguments" || child.kind() == "type_args" {
                let mut args_cursor = child.walk();
                for arg in child.named_children(&mut args_cursor) {
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

    fn record_key_is_optional(&self, record: &Node<'_>, key: &Node<'_>) -> bool {
        let prefix = &self.source.as_bytes()[record.start_byte()..key.start_byte()];
        prefix
            .iter()
            .rev()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| *byte == b'?')
    }

    /// Parse a simple type from text
    pub(super) fn parse_simple_type(text: &str) -> RbsType {
        let text = text.trim();

        // Handle optional types
        if let Some(stripped) = text.strip_suffix('?') {
            let inner = Self::parse_simple_type(stripped);
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
                                .map(|s| Self::parse_simple_type(s.trim()))
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
