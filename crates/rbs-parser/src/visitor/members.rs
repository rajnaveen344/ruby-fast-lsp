//! Class and module body members, visibility sections, attributes, and aliases.

use tree_sitter::Node;

use super::Visitor;
use crate::types::{
    AliasDecl, AttrDecl, AttrKind, Member, MethodDecl, MethodKind, ParseError, RbsType, Visibility,
};

impl<'a> Visitor<'a> {
    /// Visit class/module members
    pub(super) fn visit_members(
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
                "class_decl"
                | "module_decl"
                | "interface_decl"
                | "type_alias_decl"
                | "const_decl"
                | "class_declaration"
                | "module_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
                | "constant_declaration" => {
                    self.visit_declaration(child)?;
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
                    members.push(Member::Include(self.visit_member_target(child)?));
                }
                "extend_member" | "extend" => {
                    members.push(Member::Extend(self.visit_member_target(child)?));
                }
                "prepend_member" | "prepend" => {
                    members.push(Member::Prepend(self.visit_member_target(child)?));
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

    pub(super) fn visit_member_target(&self, node: Node) -> Result<RbsType, ParseError> {
        let mut name = None;
        let mut arguments = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "class_name" | "namespace" | "constant" => {
                    name = Some(self.node_text(&child).to_string());
                }
                "type_arguments" | "type_args" => {
                    let mut argument_cursor = child.walk();
                    for argument in child.named_children(&mut argument_cursor) {
                        arguments.push(self.visit_type(argument)?);
                    }
                }
                _ => {}
            }
        }
        let name = name.ok_or_else(|| {
            ParseError::with_location(
                "RBS mixin member has no target name",
                self.node_location(&node),
            )
        })?;
        if arguments.is_empty() {
            Ok(RbsType::Class(name))
        } else {
            Ok(RbsType::generic(name, arguments))
        }
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
}
