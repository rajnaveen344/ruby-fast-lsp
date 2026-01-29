//! Receiver Type Resolution
//!
//! Resolves types for method receivers (variables, constants, method chains).
//! This consolidates logic from query/method/type_inference.rs.

use crate::analyzer_prism::MethodReceiver;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_namespace::RubyConstant;
use crate::utils::position_to_offset;
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::lsp_types::Position;

/// Resolves types for method receivers
pub struct ReceiverResolver<'a> {
    index: &'a Index<Unlocked>,
    document: Option<&'a Arc<RwLock<RubyDocument>>>,
}

impl<'a> ReceiverResolver<'a> {
    /// Create a new ReceiverResolver
    pub fn new(
        index: &'a Index<Unlocked>,
        document: Option<&'a Arc<RwLock<RubyDocument>>>,
    ) -> Self {
        Self { index, document }
    }

    /// Resolve the type of a MethodReceiver enum
    pub fn resolve(
        &self,
        receiver: &MethodReceiver,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        match receiver {
            MethodReceiver::None => None,
            MethodReceiver::SelfReceiver => None, // Requires namespace context
            MethodReceiver::Constant(path) => Some(RubyType::ClassReference(
                FullyQualifiedName::Constant(path.clone()),
            )),
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                self.resolve_variable(name, position, content)
            }
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => self.resolve_method_chain(inner_receiver, method_name, position, content),
            MethodReceiver::Expression => None,
        }
    }

    /// Resolve a variable's type at the given position
    pub fn resolve_variable(
        &self,
        name: &str,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        let offset = self.calculate_variable_offset(name, position, content);

        // Try document snapshot first
        if let Some(doc_arc) = self.document {
            let doc = doc_arc.read();
            if let Some(ty) = doc.get_var_type(offset, name) {
                return Some(ty.clone());
            }
        }

        // Fallback to constructor pattern
        self.infer_from_constructor_pattern(content, name)
    }

    /// Resolve a method chain (e.g., a.b.c)
    pub fn resolve_method_chain(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        let inner_type = self.resolve(inner_receiver, position, content)?;
        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, &inner_type, method_name)
    }

    /// Calculate the byte offset for a variable
    fn calculate_variable_offset(
        &self,
        var_name: &str,
        position: Position,
        content: &str,
    ) -> usize {
        let line = content.lines().nth(position.line as usize).unwrap_or("");
        let before_cursor = &line[..std::cmp::min(position.character as usize, line.len())];

        if let Some(var_pos) = before_cursor.rfind(var_name) {
            position_to_offset(
                content,
                Position {
                    line: position.line,
                    character: var_pos as u32,
                },
            )
        } else {
            position_to_offset(content, position)
        }
    }

    /// Infer type from constructor assignment pattern (e.g., x = Foo.new)
    fn infer_from_constructor_pattern(&self, content: &str, var_name: &str) -> Option<RubyType> {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(var_name) {
                let next_char = rest.chars().next();
                if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                    continue;
                }
                let rest = rest.trim();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rhs = rest.trim();
                    if let Some(new_pos) = rhs.find(".new") {
                        let class_part = rhs[..new_pos].trim();
                        if !class_part
                            .chars()
                            .next()
                            .map(|c| c.is_uppercase())
                            .unwrap_or(false)
                        {
                            continue;
                        }

                        let parts: Vec<_> = class_part
                            .split("::")
                            .filter_map(|s| RubyConstant::new(s.trim()).ok())
                            .collect();
                        if parts.is_empty() {
                            continue;
                        }

                        let class_fqn = FullyQualifiedName::Constant(parts);
                        let mut current_type = RubyType::Class(class_fqn);

                        // Handle method chain after .new
                        let after_new = &rhs[new_pos + 4..];
                        let after_new = if after_new.starts_with('(') {
                            if let Some(close_paren) = after_new.find(')') {
                                &after_new[close_paren + 1..]
                            } else {
                                after_new
                            }
                        } else {
                            after_new
                        };

                        let index = self.index.lock();
                        for method_call in after_new.split('.') {
                            let method_name = method_call
                                .split(|c: char| c == '(' || c.is_whitespace())
                                .next()
                                .unwrap_or("")
                                .trim();

                            if method_name.is_empty() {
                                continue;
                            }

                            if let Some(return_type) = MethodResolver::resolve_method_return_type(
                                &index,
                                &current_type,
                                method_name,
                            ) {
                                current_type = return_type;
                            } else {
                                break;
                            }
                        }
                        return Some(current_type);
                    }
                }
            }
        }
        None
    }
}
