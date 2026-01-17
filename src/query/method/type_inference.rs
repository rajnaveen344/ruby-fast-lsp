//! Type Inference for Method Receivers
//!
//! Handles type inference for variables and method chains to enable type-aware method resolution.

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
use tower_lsp::lsp_types::{Position, Url};

/// Type inferrer for method receivers
pub struct TypeInferrer<'a> {
    pub index: &'a Index<Unlocked>,
    pub doc: Option<&'a Arc<RwLock<RubyDocument>>>,
}

impl<'a> TypeInferrer<'a> {
    /// Infer type for a variable receiver (local, instance, class, global)
    pub fn infer_variable_type(
        &self,
        var_name: &str,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        // Calculate receiver offset for variable lookup
        let line = content.lines().nth(position.line as usize).unwrap_or("");
        let before_cursor = &line[..std::cmp::min(position.character as usize, line.len())];
        let receiver_offset = if let Some(var_pos) = before_cursor.rfind(var_name) {
            position_to_offset(
                content,
                Position {
                    line: position.line,
                    character: var_pos as u32,
                },
            )
        } else {
            position_to_offset(content, position)
        };

        // Try variable type from document snapshot
        if let Some(doc_arc) = &self.doc {
            let doc = doc_arc.read();
            if let Some(receiver_type) = doc.get_var_type(receiver_offset, var_name) {
                return Some(receiver_type.clone());
            }
        }

        // Fallback: Check for constructor assignment pattern
        self.infer_from_constructor_pattern(content, var_name)
    }

    /// Resolve the type of a method call chain (a.b.c)
    pub fn resolve_method_chain_type(
        &self,
        receiver: &MethodReceiver,
        method_name: &str,
        _uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<RubyType> {
        // Resolve the inner receiver's type first
        let inner_type = match receiver {
            MethodReceiver::None | MethodReceiver::SelfReceiver => return None,
            MethodReceiver::Constant(path) => {
                RubyType::ClassReference(FullyQualifiedName::Constant(path.clone()))
            }
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                let offset = position_to_offset(content, position);

                // Try variable type from document
                if let Some(doc_arc) = &self.doc {
                    let doc = doc_arc.read();
                    if let Some(ty) = doc.get_var_type(offset, name) {
                        ty.clone()
                    } else if let Some(ty) = self.infer_from_constructor_pattern(content, name) {
                        ty
                    } else {
                        return None;
                    }
                } else if let Some(ty) = self.infer_from_constructor_pattern(content, name) {
                    ty
                } else {
                    return None;
                }
            }
            MethodReceiver::MethodCall {
                inner_receiver: nested_receiver,
                method_name: nested_method,
            } => self.resolve_method_chain_type(
                nested_receiver,
                nested_method,
                _uri,
                position,
                content,
            )?,
            MethodReceiver::Expression => return None,
        };

        // Resolve the method's return type on the inner type
        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, &inner_type, method_name)
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
                        // Determine if it's a constant
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

                        // Check method chain after .new
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

                        // Resolve chained method calls (e.g., .foo.bar)
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
