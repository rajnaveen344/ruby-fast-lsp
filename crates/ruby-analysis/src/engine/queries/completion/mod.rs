//! Completion receiver probing: semantic query contract and receiver type resolution.

mod literal_text;
mod method_matches;
mod method_return;
mod shape_keys;
mod view;

use crate::invariant::ExpectInvariant;
pub use literal_text::{
    infer_constructor_assignment_type, infer_literal_type, infer_literal_type_from_expression,
    is_variable_name,
};
pub use method_matches::rbs_method_matches_for_type;
pub use shape_keys::{shape_key_completions_for_target, ShapeKeyCompletionResult};

use method_return::{infer_bare_method_return_type, infer_method_call_return_type};

use crate::core::MethodReceiver;
use crate::core::RubyType;
use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod};
use crate::indexer::{CompletionReceiverTarget, Identifier, RubyDocument};

pub trait CompletionSemanticQuery {
    fn constant_type_in_context(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType>;

    fn method_return_type_for_receiver(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<RubyType>;

    fn variable_type_before(
        &self,
        kind: CompletionVariableKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: crate::core::SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType>;

    fn implicit_receiver_at(
        &self,
        file_id: crate::core::SourceFileId,
        byte_offset: u32,
    ) -> Option<FullyQualifiedName>;

    fn exact_expression_type(
        &self,
        file_id: crate::core::SourceFileId,
        start_byte: u32,
        end_byte: u32,
    ) -> Option<RubyType>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionVariableKind {
    Instance,
    Class,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionMethodMatch {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<RubyType>,
}

pub fn receiver_type_from_context(
    query: &impl CompletionSemanticQuery,
    document: &RubyDocument,
    content: &str,
    byte_offset: u32,
    namespace_kind: NamespaceKind,
    identifier: &Option<Identifier>,
    receiver_target: Option<&CompletionReceiverTarget>,
) -> Option<RubyType> {
    if let Some(target) = receiver_target {
        if let Some(ruby_type) = query.exact_expression_type(
            document.analysis_file_id(),
            target.receiver_start,
            target.receiver_end,
        ) {
            return (ruby_type != RubyType::Unknown).then_some(ruby_type);
        }
    }

    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::Constant(recv_parts),
        namespace,
        ..
    }) = identifier
    {
        if let Some(ruby_type) = query.constant_type_in_context(recv_parts, namespace) {
            return Some(ruby_type);
        }
        let fqn = FullyQualifiedName::constant(recv_parts.clone());
        return Some(RubyType::ClassReference(fqn));
    }

    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::SelfReceiver,
        namespace,
        ..
    }) = identifier
    {
        if !namespace.is_empty() {
            let fqn = FullyQualifiedName::from(namespace.clone());
            return Some(RubyType::Class(fqn));
        }
    }

    if let Some(Identifier::RubyMethod {
        receiver:
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            },
        namespace,
        ..
    }) = identifier
    {
        let inner_type = resolve_method_receiver_type(
            query,
            document,
            content,
            byte_offset,
            inner_receiver,
            namespace,
            namespace_kind,
        );
        if let Some(inner_type) = inner_type {
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &inner_type {
                    return crate::inference::method::constructor::seed_constructor_type(fqn);
                }
            }

            if let Some(return_type) =
                infer_method_call_return_type(query, &inner_type, method_name)
            {
                return Some(return_type);
            }
        }
    }

    if let Some(Identifier::RubyMethod {
        receiver: MethodReceiver::Literal(ty),
        ..
    }) = identifier
    {
        return Some(ty.clone());
    }

    if let Some(Identifier::RubyMethod {
        receiver,
        namespace,
        ..
    }) = identifier
    {
        let var_type = match receiver {
            MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => lookup_variable_type(
                query,
                document,
                name,
                receiver,
                namespace,
                namespace_kind,
                byte_offset,
            ),
            MethodReceiver::None
            | MethodReceiver::SelfReceiver
            | MethodReceiver::Super
            | MethodReceiver::Constant(_)
            | MethodReceiver::LocalVariable(_)
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_)
            | MethodReceiver::Expression => None,
        };
        if let Some(ty) = var_type {
            return Some(ty);
        }
    }

    let cursor_offset = usize::try_from(byte_offset).expect_invariant(
        "completion byte offset cannot fit usize",
        "source buffers are indexed by usize",
        "widen the completion offset representation together with source indexing",
    );
    invariant!(
        cursor_offset <= content.len() && content.is_char_boundary(cursor_offset),
        what = "completion offset {cursor_offset} is not a UTF-8 boundary in a {}-byte source",
        why = "the adapter converts LSP positions through RubyDocument first",
        fix = "pass position_to_analysis_offset output for the same document generation",
        content.len(),
        cursor_offset = cursor_offset,
    );
    let line_start = content[..cursor_offset]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let before_cursor = &content[line_start..cursor_offset];

    let dot_pos = before_cursor.rfind('.')?;
    let before_dot = before_cursor[..dot_pos].trim_end();

    if let Some(literal_type) = infer_literal_type_from_expression(before_dot) {
        return Some(literal_type);
    }

    let receiver_text = before_dot
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '@' && c != '$')
        .next()
        .map(str::trim)
        .unwrap_or("")
        .trim();

    if receiver_text.is_empty() {
        return None;
    }

    if let Some(literal_type) = infer_literal_type(receiver_text) {
        return Some(literal_type);
    }

    if receiver_text
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        if let Ok(constant) = RubyConstant::new(receiver_text) {
            return Some(RubyType::ClassReference(FullyQualifiedName::constant(
                vec![constant],
            )));
        }
    }

    // Prism recovers `receiver.` without a message location, so the AST-based
    // completion target above can be absent at the exact moment completion is
    // triggered. Recover only the receiver's source range, then ask the same
    // engine-owned expression query. An explicit Unknown remains
    // authoritative and must not fall through to textual constructor guesses.
    let receiver_start_in_line = before_dot
        .len()
        .checked_sub(receiver_text.len())
        .expect_invariant(
            "completion receiver text is longer than the line prefix it was extracted from",
            "receiver extraction must return a suffix of that prefix",
            "keep receiver parsing and source-offset calculation coupled",
        );
    let receiver_offset = line_start
        .checked_add(receiver_start_in_line)
        .and_then(|offset| u32::try_from(offset).ok())
        .expect_invariant(
            "completion receiver offset overflowed its source coordinates",
            "receiver text was sliced from the same bounded line prefix",
            "keep receiver extraction and byte-offset calculation coupled",
        );
    let receiver_end = receiver_offset
        .checked_add(u32::try_from(receiver_text.len()).expect_invariant(
            "completion receiver length exceeded u32",
            "the receiver was sliced from a u32-addressed source document",
            "reject oversized documents before completion",
        ))
        .expect_invariant(
            "completion receiver end overflowed u32 source coordinates",
            "the receiver range came from one bounded document",
            "reject oversized documents before completion",
        );
    let file_id = document.analysis_file_id();
    match query.exact_expression_type(file_id, receiver_offset, receiver_end) {
        Some(RubyType::Unknown) => return None,
        Some(ruby_type) => return Some(ruby_type),
        None => {}
    }

    if is_variable_name(receiver_text) {
        if let Some(scope_id) = document
            .variable_scopes
            .find_scope_for_variable_at(receiver_text, file_id, receiver_offset)
            .or_else(|| {
                document
                    .variable_scopes
                    .scope_at_position(file_id, receiver_offset)
            })
        {
            match document.variable_scopes.get_type_at_position(
                receiver_text,
                scope_id,
                file_id,
                receiver_offset,
            ) {
                Some(RubyType::Unknown) => return None,
                Some(ruby_type) => return Some(ruby_type.clone()),
                None => {}
            }
        }

        if let Some(ty) = infer_constructor_assignment_type(content, receiver_text) {
            return Some(ty);
        }

        if let Ok(method) = RubyMethod::new(receiver_text) {
            if let Some(owner) = query.implicit_receiver_at(file_id, receiver_offset) {
                if let Some(return_type) = query.method_return_type_for_receiver(&owner, &method) {
                    return Some(return_type);
                }
            }
        }
    }

    infer_bare_method_return_type(query, receiver_text, identifier)
}

fn resolve_method_receiver_type(
    query: &impl CompletionSemanticQuery,
    document: &RubyDocument,
    content: &str,
    byte_offset: u32,
    receiver: &MethodReceiver,
    current_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
) -> Option<RubyType> {
    match receiver {
        MethodReceiver::Constant(parts) => query
            .constant_type_in_context(parts, current_namespace)
            .or_else(|| {
                let fqn = FullyQualifiedName::constant(parts.clone());
                Some(RubyType::ClassReference(fqn))
            }),
        MethodReceiver::LocalVariable(name) => {
            let file_id = document.analysis_file_id();
            if let Some(scope_id) = document
                .variable_scopes
                .find_scope_for_variable_at(name, file_id, byte_offset)
                .or_else(|| {
                    document
                        .variable_scopes
                        .scope_at_position(file_id, byte_offset)
                })
            {
                match document.variable_scopes.get_type_at_position(
                    name,
                    scope_id,
                    file_id,
                    byte_offset,
                ) {
                    Some(RubyType::Unknown) => return None,
                    Some(ruby_type) => return Some(ruby_type.clone()),
                    None => {}
                }
            }
            infer_constructor_assignment_type(content, name)
        }
        MethodReceiver::SelfReceiver | MethodReceiver::Super => None,
        MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => lookup_variable_type(
            query,
            document,
            name,
            receiver,
            current_namespace,
            namespace_kind,
            byte_offset,
        ),
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            let inner_type = resolve_method_receiver_type(
                query,
                document,
                content,
                byte_offset,
                inner_receiver,
                current_namespace,
                namespace_kind,
            )?;
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &inner_type {
                    return crate::inference::method::constructor::seed_constructor_type(fqn);
                }
            }
            infer_method_call_return_type(query, &inner_type, method_name)
        }
        MethodReceiver::Literal(ty) => Some(ty.clone()),
        MethodReceiver::None | MethodReceiver::Expression => None,
    }
}

fn lookup_variable_type(
    query: &impl CompletionSemanticQuery,
    document: &RubyDocument,
    name: &str,
    receiver: &MethodReceiver,
    current_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    byte_offset: u32,
) -> Option<RubyType> {
    let kind = match receiver {
        MethodReceiver::InstanceVariable(_) => CompletionVariableKind::Instance,
        MethodReceiver::ClassVariable(_) => CompletionVariableKind::Class,
        MethodReceiver::GlobalVariable(_) => CompletionVariableKind::Global,
        MethodReceiver::None
        | MethodReceiver::SelfReceiver
        | MethodReceiver::Super
        | MethodReceiver::Constant(_)
        | MethodReceiver::LocalVariable(_)
        | MethodReceiver::MethodCall { .. }
        | MethodReceiver::Literal(_)
        | MethodReceiver::Expression => return None,
    };

    let file_id = document.analysis_file_id();
    let owner = query
        .implicit_receiver_at(file_id, byte_offset)
        .unwrap_or_else(|| {
            FullyQualifiedName::namespace_with_kind(current_namespace.to_vec(), namespace_kind)
        });
    query.variable_type_before(kind, name, &owner, file_id, byte_offset)
}

#[cfg(test)]
mod tests;
