use crate::core::{FullyQualifiedName, LiteralKey, NamespaceKind, RubyConstant, RubyMethod};
use crate::indexer::{
    CompletionReceiverTarget, Identifier, MethodReceiver, RubyDocument, ShapeKeyCompletionTarget,
    ShapeKeySyntax,
};
use crate::inference::r#type::shape as shape_reads;
use crate::inference::RubyType;
use std::collections::BTreeSet;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeKeyCompletionResult {
    pub keys: Vec<LiteralKey>,
    pub replacement_start: u32,
    pub replacement_end: u32,
}

pub fn shape_key_completions_for_target(
    query: &impl CompletionSemanticQuery,
    document: &RubyDocument,
    target: &ShapeKeyCompletionTarget,
) -> ShapeKeyCompletionResult {
    let file_id = document.analysis_file_id();
    let receiver_type =
        match query.exact_expression_type(file_id, target.receiver_start, target.receiver_end) {
            Some(RubyType::Unknown) => None,
            Some(ruby_type) => Some(ruby_type),
            None => (|| {
                let name = target.receiver_local_name.as_ref()?;
                let scope_id = document
                    .variable_scopes
                    .find_scope_for_variable_at(name, file_id, target.receiver_start)
                    .or_else(|| {
                        document
                            .variable_scopes
                            .scope_at_position(file_id, target.receiver_start)
                    })?;
                document
                    .variable_scopes
                    .get_type_at_position(name, scope_id, file_id, target.receiver_start)
                    .cloned()
            })(),
        };
    let keys = receiver_type
        .map(|receiver_type| common_shape_keys(&receiver_type))
        .unwrap_or_default()
        .into_iter()
        .filter(|key| match (target.syntax, key) {
            (ShapeKeySyntax::Symbol, LiteralKey::Symbol(value))
            | (ShapeKeySyntax::String, LiteralKey::String(value)) => {
                value.starts_with(&target.partial)
            }
            (ShapeKeySyntax::Symbol, LiteralKey::String(_))
            | (ShapeKeySyntax::String, LiteralKey::Symbol(_)) => false,
        })
        .collect::<Vec<_>>();
    ShapeKeyCompletionResult {
        keys,
        replacement_start: target.replacement_start,
        replacement_end: target.replacement_end,
    }
}

fn common_shape_keys(receiver_type: &RubyType) -> Vec<LiteralKey> {
    let shapes = match receiver_type {
        RubyType::Shape(shape) => vec![shape.as_ref()],
        RubyType::Union(members) => {
            let mut shapes = Vec::with_capacity(members.len());
            for member in members {
                let RubyType::Shape(shape) = member else {
                    return Vec::new();
                };
                shapes.push(shape.as_ref());
            }
            shapes
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => return Vec::new(),
    };
    let Some((first, rest)) = shapes.split_first() else {
        return Vec::new();
    };
    let mut common = first
        .fields()
        .iter()
        .map(|field| field.key().clone())
        .collect::<BTreeSet<_>>();
    for shape in rest {
        let keys = shape
            .fields()
            .iter()
            .map(|field| field.key().clone())
            .collect::<BTreeSet<_>>();
        common = common.intersection(&keys).cloned().collect();
    }
    common.into_iter().collect()
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
                    return Some(RubyType::Class(fqn.clone()));
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

    let cursor_offset = usize::try_from(byte_offset).expect(
        "INVARIANT VIOLATED: completion byte offset cannot fit usize. This is a bug because source buffers are indexed by usize. Fix: widen the completion offset representation together with source indexing.",
    );
    assert!(
        cursor_offset <= content.len() && content.is_char_boundary(cursor_offset),
        "INVARIANT VIOLATED: completion byte offset {cursor_offset} is not a valid UTF-8 boundary in a {}-byte source. This is a bug because the root adapter must convert the current LSP position through RubyDocument before querying inference. Fix: pass position_to_analysis_offset output for the same document generation.",
        content.len(),
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
    let receiver_start_in_line = before_dot.len().checked_sub(receiver_text.len()).expect(
        "INVARIANT VIOLATED: completion receiver text is longer than the line prefix it was extracted from. This is a bug because receiver extraction must return a suffix of that prefix. Fix: keep receiver parsing and source-offset calculation coupled.",
    );
    let receiver_offset = line_start
        .checked_add(receiver_start_in_line)
        .and_then(|offset| u32::try_from(offset).ok())
        .expect(
            "INVARIANT VIOLATED: completion receiver offset overflowed its source coordinates. This is a bug because receiver text was sliced from the same bounded line prefix. Fix: keep receiver extraction and byte-offset calculation coupled.",
        );
    let receiver_end = receiver_offset
        .checked_add(u32::try_from(receiver_text.len()).expect(
            "INVARIANT VIOLATED: completion receiver length exceeded u32. This is a bug because the receiver was sliced from a u32-addressed source document. Fix: reject oversized documents before completion.",
        ))
        .expect(
            "INVARIANT VIOLATED: completion receiver end overflowed u32 source coordinates. This is a bug because the receiver range came from one bounded document. Fix: reject oversized documents before completion.",
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

pub fn rbs_method_matches_for_type(
    receiver_type: &RubyType,
    partial_method: &str,
    kind: NamespaceKind,
) -> Vec<CompletionMethodMatch> {
    if let RubyType::Union(types) = receiver_type {
        let Some((first, rest)) = types.split_first() else {
            return Vec::new();
        };
        let mut common = rbs_method_matches_for_type(first, partial_method, kind)
            .into_iter()
            .map(|candidate| (candidate.name.clone(), candidate))
            .collect::<std::collections::BTreeMap<_, _>>();

        for member in rest {
            let member_matches = rbs_method_matches_for_type(member, partial_method, kind)
                .into_iter()
                .map(|candidate| (candidate.name.clone(), candidate))
                .collect::<std::collections::BTreeMap<_, _>>();
            common.retain(|name, candidate| {
                let Some(member_candidate) = member_matches.get(name) else {
                    return false;
                };
                if candidate.params != member_candidate.params {
                    return false;
                }
                candidate.return_type = match (
                    candidate.return_type.take(),
                    member_candidate.return_type.clone(),
                ) {
                    (Some(left), Some(right)) => RubyType::union_from_proven([left, right], Some),
                    (Some(_), None) | (None, Some(_)) | (None, None) => None,
                };
                true
            });
        }

        return common.into_values().collect();
    }

    let mut matches = Vec::new();
    let mut seen_methods = std::collections::HashSet::new();
    let is_singleton = kind == NamespaceKind::Singleton;

    for class_name in class_names_for_type(receiver_type) {
        for method_info in crate::inference::rbs::get_rbs_class_methods(&class_name, is_singleton) {
            if !method_info.name.starts_with(partial_method) {
                continue;
            }
            if !seen_methods.insert(method_info.name.clone()) {
                continue;
            }
            matches.push(CompletionMethodMatch {
                name: method_info.name,
                params: method_info.params,
                return_type: method_info.return_type,
            });
        }
    }

    if is_singleton {
        for rbs_class in ["Class", "Module"] {
            for method_info in crate::inference::rbs::get_rbs_class_methods(rbs_class, false) {
                if !method_info.name.starts_with(partial_method) {
                    continue;
                }
                if !seen_methods.insert(method_info.name.clone()) {
                    continue;
                }
                matches.push(CompletionMethodMatch {
                    name: method_info.name,
                    params: method_info.params,
                    return_type: method_info.return_type,
                });
            }
        }
    }

    matches.sort_by(|left, right| left.name.cmp(&right.name));
    matches
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
                    return Some(RubyType::Class(fqn.clone()));
                }
            }
            infer_method_call_return_type(query, &inner_type, method_name)
        }
        MethodReceiver::Literal(ty) => Some(ty.clone()),
        MethodReceiver::None | MethodReceiver::Expression => None,
    }
}

fn infer_method_call_return_type(
    query: &impl CompletionSemanticQuery,
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    if let RubyType::Union(types) = receiver_type {
        return crate::inference::method::return_type::resolve_proven_union(types, |member| {
            infer_method_call_return_type(query, member, method_name)
        });
    }

    if method_name == "new" {
        if let RubyType::ClassReference(fqn) = receiver_type {
            return Some(RubyType::Class(fqn.clone()));
        }
    }

    if shape_reads::is_shape_only(receiver_type) {
        if let Some(outcome) = shape_reads::argument_free_method_return(receiver_type, method_name)
        {
            return outcome.ok();
        }
        if shape_reads::operation_requires_call_arguments(method_name) {
            return None;
        }
    }

    if let Some(return_type) = infer_generic_rbs_method_return_type(receiver_type, method_name) {
        return Some(return_type);
    }

    let method = RubyMethod::new(method_name).ok()?;
    for namespace in receiver_type_to_analysis_namespaces(receiver_type) {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }

    infer_rbs_method_return_type(receiver_type, method_name)
}

fn infer_generic_rbs_method_return_type(
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => {
            crate::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Array",
                method_name,
                false,
                element_types,
            )
        }
        RubyType::Hash(key_types, value_types) => {
            let type_args = vec![
                RubyType::union(key_types.clone()),
                RubyType::union(value_types.clone()),
            ];
            crate::inference::rbs::get_rbs_method_return_type_with_type_args(
                "Hash",
                method_name,
                false,
                &type_args,
            )
        }
        RubyType::Shape(shape) => {
            infer_generic_rbs_method_return_type(&shape.generic_hash_type(), method_name)
        }
        RubyType::Literal(_) => None,
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

fn infer_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, false)
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            rbs_method_return_for_fqn(fqn, method_name, true)
        }
        RubyType::Array(_) | RubyType::Hash(_, _) => {
            infer_generic_rbs_method_return_type(receiver_type, method_name)
        }
        RubyType::Shape(shape) => {
            infer_rbs_method_return_type(&shape.generic_hash_type(), method_name)
        }
        RubyType::Literal(value) => {
            infer_rbs_method_return_type(&value.widened_type(), method_name)
        }
        RubyType::Union(types) => {
            crate::inference::method::return_type::resolve_proven_union(types, |ty| {
                infer_method_call_return_type_fallback(ty, method_name)
            })
        }
        RubyType::Unknown => None,
    }
}

fn infer_method_call_return_type_fallback(
    receiver_type: &RubyType,
    method_name: &str,
) -> Option<RubyType> {
    infer_generic_rbs_method_return_type(receiver_type, method_name)
        .or_else(|| infer_rbs_method_return_type(receiver_type, method_name))
}

fn rbs_method_return_for_fqn(
    fqn: &FullyQualifiedName,
    method_name: &str,
    is_singleton: bool,
) -> Option<RubyType> {
    for class_name in class_names_for_fqn(fqn) {
        if let Some(return_type) = crate::inference::rbs::get_rbs_method_return_type_as_ruby_type(
            &class_name,
            method_name,
            is_singleton,
        ) {
            return Some(return_type);
        }
    }
    None
}

fn class_names_for_fqn(fqn: &FullyQualifiedName) -> Vec<String> {
    let parts = fqn.namespace_parts();
    let fqn_name = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let simple_name = parts.last().map(|part| part.to_string());

    let mut names = Vec::new();
    if !fqn_name.is_empty() {
        names.push(fqn_name);
    }
    if let Some(simple_name) = simple_name {
        if !names.contains(&simple_name) {
            names.push(simple_name);
        }
    }
    names
}

fn class_names_for_type(ruby_type: &RubyType) -> Vec<String> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => class_names_for_fqn(fqn),
        RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => fqn
            .namespace_parts()
            .last()
            .map(|constant| vec![constant.to_string()])
            .unwrap_or_default(),
        RubyType::Array(_) => vec!["Array".to_string()],
        RubyType::Hash(_, _) | RubyType::Shape(_) => vec!["Hash".to_string()],
        RubyType::Literal(value) => class_names_for_type(&value.widened_type()),
        RubyType::Union(types) => {
            let mut all_names = Vec::new();
            for ty in types {
                for name in class_names_for_type(ty) {
                    if !all_names.contains(&name) {
                        all_names.push(name);
                    }
                }
            }
            all_names
        }
        RubyType::Unknown => Vec::new(),
    }
}

fn receiver_type_to_analysis_namespaces(receiver_type: &RubyType) -> Vec<FullyQualifiedName> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Instance,
            )]
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Singleton,
            )]
        }
        RubyType::Union(types) => types
            .iter()
            .flat_map(receiver_type_to_analysis_namespaces)
            .collect(),
        RubyType::Literal(value) => receiver_type_to_analysis_namespaces(&value.widened_type()),
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) | RubyType::Unknown => {
            Vec::new()
        }
    }
}

fn infer_bare_method_return_type(
    query: &impl CompletionSemanticQuery,
    method_name: &str,
    identifier: &Option<Identifier>,
) -> Option<RubyType> {
    let method = RubyMethod::new(method_name).ok()?;
    let mut namespaces = Vec::new();
    if let Some(Identifier::RubyMethod { namespace, .. }) = identifier {
        namespaces.push(FullyQualifiedName::namespace_with_kind(
            namespace.clone(),
            NamespaceKind::Instance,
        ));
    }
    namespaces.push(FullyQualifiedName::namespace_with_kind(
        Vec::new(),
        NamespaceKind::Instance,
    ));

    for namespace in namespaces {
        if let Some(return_type) = query.method_return_type_for_receiver(&namespace, &method) {
            return Some(return_type);
        }
    }
    None
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

pub fn infer_constructor_assignment_type(content: &str, var_name: &str) -> Option<RubyType> {
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(var_name) else {
            continue;
        };

        let next_char = rest.chars().next();
        if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
            continue;
        }

        let rest = rest.trim();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rhs = rest.trim();
        if !(rhs.ends_with(".new") || rhs.contains(".new(") || rhs.contains(".new ")) {
            continue;
        }

        let new_pos = rhs.find(".new")?;
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

        if !parts.is_empty() {
            return Some(RubyType::Class(FullyQualifiedName::constant(parts)));
        }
    }

    None
}

pub fn infer_literal_type_from_expression(text: &str) -> Option<RubyType> {
    let trimmed = text.trim();

    if trimmed.ends_with('"') || trimmed.ends_with('\'') {
        return Some(RubyType::string());
    }

    if trimmed.ends_with(']') && trimmed.starts_with('[') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let element_types = infer_array_element_types(inner);
        return Some(RubyType::Array(element_types));
    }

    if trimmed.ends_with('}') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
    }

    if let Some(rest) = trimmed.rsplit_once(|c: char| c.is_whitespace() || c == '(' || c == ',') {
        if rest.1.starts_with(':') {
            return Some(RubyType::symbol());
        }
    } else if trimmed.starts_with(':') {
        return Some(RubyType::symbol());
    }

    None
}

pub fn infer_literal_type(text: &str) -> Option<RubyType> {
    if text.starts_with('"') || text.starts_with('\'') {
        return Some(RubyType::string());
    }

    if text.starts_with(':') {
        return Some(RubyType::symbol());
    }

    if text.starts_with('[') {
        return Some(RubyType::Array(vec![RubyType::Unknown]));
    }

    if text.starts_with('{') {
        return Some(RubyType::Hash(
            vec![RubyType::Unknown],
            vec![RubyType::Unknown],
        ));
    }

    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return Some(RubyType::integer());
    }

    if text.contains('.')
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
    {
        return Some(RubyType::float());
    }

    None
}

pub fn is_variable_name(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let first_char = text.chars().next().expect(
        "INVARIANT VIOLATED: non-empty variable text has no first char. \
         This is a bug because Rust str chars must yield at least one char for non-empty valid UTF-8. \
         Fix: check caller input encoding.",
    );
    if !first_char.is_lowercase() && first_char != '_' {
        return false;
    }

    text.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn infer_array_element_types(inner: &str) -> Vec<RubyType> {
    let mut types = Vec::new();
    for element in inner.split(',') {
        let el = element.trim();
        if el.is_empty() {
            continue;
        }
        let ty = if el.starts_with('"') || el.starts_with('\'') {
            RubyType::string()
        } else if el.starts_with(':') {
            RubyType::symbol()
        } else if el.parse::<i64>().is_ok() {
            RubyType::integer()
        } else if el.parse::<f64>().is_ok() {
            RubyType::float()
        } else if el == "true" || el == "false" {
            RubyType::true_class()
        } else if el == "nil" {
            RubyType::nil_class()
        } else {
            RubyType::Unknown
        };
        if !types.contains(&ty) {
            types.push(ty);
        }
    }
    RubyType::canonical_union_members(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rbs_method_matches_include_string_methods() {
        let matches = rbs_method_matches_for_type(&RubyType::string(), "", NamespaceKind::Instance);
        let names = matches
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"length"));
        assert!(names.contains(&"upcase"));
    }

    #[test]
    fn rbs_method_matches_filter_by_partial() {
        let matches =
            rbs_method_matches_for_type(&RubyType::string(), "up", NamespaceKind::Instance);

        assert!(
            matches
                .iter()
                .all(|candidate| candidate.name.starts_with("up")),
            "INVARIANT VIOLATED: RBS method completion returned a method outside the requested prefix. \
             This is a bug because completion filtering must be deterministic before LSP mapping. \
             Fix: apply the partial filter before returning completion candidates."
        );
    }

    #[test]
    fn rbs_method_matches_for_union_require_every_member() {
        let ty = RubyType::union(vec![RubyType::string(), RubyType::integer()]);
        let matches = rbs_method_matches_for_type(&ty, "", NamespaceKind::Instance);
        let names = matches
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"to_s"));
        assert!(!names.contains(&"upcase"));
        assert!(!names.contains(&"abs"));
    }

    #[test]
    fn unresolved_array_literal_does_not_drop_unknown_elements() {
        assert_eq!(
            infer_literal_type_from_expression("[1, dynamic_value]"),
            Some(RubyType::Array(vec![RubyType::Unknown]))
        );
    }

    #[test]
    fn rbs_method_match_carries_return_type() {
        let matches =
            rbs_method_matches_for_type(&RubyType::string(), "length", NamespaceKind::Instance);
        let length = matches
            .iter()
            .find(|candidate| candidate.name == "length")
            .expect(
                "INVARIANT VIOLATED: String#length missing from RBS completion candidates. \
                 This is a bug because bundled RBS must expose String#length. \
                 Fix: check RBS loading and completion class-name mapping.",
            );

        assert!(length.return_type.is_some());
    }
}
