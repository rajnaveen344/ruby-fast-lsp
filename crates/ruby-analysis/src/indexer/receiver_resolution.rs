//! Method receiver resolution.
//!
//! Converts parser-level receiver shapes into reusable semantic namespaces.

use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType};
use crate::engine::{AnalysisQuery, VariableTypeKind};
use crate::indexer::{MethodReceiver, RubyDocument};
use crate::inference::method::method_call_return_type;

pub struct ReceiverResolutionContext<'a, 'q> {
    pub query: Option<&'q AnalysisQuery<'a>>,
    pub document: Option<&'q RubyDocument>,
    pub current_namespace: &'q [RubyConstant],
    pub namespace_kind: NamespaceKind,
    pub byte_offset: u32,
}

pub fn resolve_receiver_to_namespace(
    receiver: &MethodReceiver,
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<FullyQualifiedName> {
    match receiver {
        MethodReceiver::Constant(path) => resolve_constant_receiver(path, context),
        MethodReceiver::None | MethodReceiver::SelfReceiver | MethodReceiver::Super => {
            Some(FullyQualifiedName::namespace_with_kind(
                context.current_namespace.to_vec(),
                context.namespace_kind,
            ))
        }
        MethodReceiver::LocalVariable(name) => {
            let var_type = variable_receiver_type(name, context)?;
            type_to_namespace(&var_type, context)
        }
        MethodReceiver::InstanceVariable(name) => {
            let var_type = variable_type_before(name, VariableTypeKind::Instance, context)?;
            type_to_namespace(&var_type, context)
        }
        MethodReceiver::ClassVariable(name) => {
            let var_type = variable_type_before(name, VariableTypeKind::Class, context)?;
            type_to_namespace(&var_type, context)
        }
        MethodReceiver::GlobalVariable(name) => {
            let var_type = variable_type_before(name, VariableTypeKind::Global, context)?;
            type_to_namespace(&var_type, context)
        }
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            let chain_type = method_call_receiver_type(inner_receiver, method_name, context)?;
            type_to_namespace(&chain_type, context)
        }
        MethodReceiver::Literal(ruby_type) => type_to_namespace(ruby_type, context),
        MethodReceiver::Expression => None,
    }
}

pub fn resolve_receiver_type(
    receiver: &MethodReceiver,
    context: &ReceiverResolutionContext<'_, '_>,
) -> RubyType {
    match receiver {
        MethodReceiver::None | MethodReceiver::SelfReceiver | MethodReceiver::Super => {
            if context.current_namespace.is_empty() {
                RubyType::class("Object")
            } else {
                let fqn = FullyQualifiedName::from(context.current_namespace.to_vec());
                if let Some(query) = context.query {
                    if let Some(ruby_type) = query.namespace_type(&fqn) {
                        return ruby_type;
                    }
                }
                RubyType::Class(fqn)
            }
        }
        MethodReceiver::Constant(path) => {
            if let Some(query) = context.query {
                if let Some(resolved) =
                    query.resolve_constant_in_context(path, context.current_namespace)
                {
                    let resolved_constant =
                        FullyQualifiedName::constant(resolved.namespace_parts().to_vec());
                    if let Some(ruby_type) = query.constant_value_type(&resolved_constant) {
                        return ruby_type;
                    }
                    if let Some(ruby_type) =
                        query.constant_reference_type(resolved.namespace_parts_slice())
                    {
                        return ruby_type;
                    }
                }
                if let Some(ruby_type) = query.constant_reference_type(path) {
                    return ruby_type;
                }
            }
            RubyType::ClassReference(FullyQualifiedName::constant(path.clone()))
        }
        MethodReceiver::LocalVariable(name) => {
            variable_receiver_type(name, context).unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::InstanceVariable(name) => {
            variable_type_before(name, VariableTypeKind::Instance, context)
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::ClassVariable(name) => {
            variable_type_before(name, VariableTypeKind::Class, context)
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::GlobalVariable(name) => {
            variable_type_before(name, VariableTypeKind::Global, context)
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => {
            let inner_type = resolve_receiver_type(inner_receiver, context);
            if inner_type == RubyType::Unknown {
                return RubyType::Unknown;
            }

            method_call_return_type(context.query, &inner_type, method_name)
                .unwrap_or(RubyType::Unknown)
        }
        MethodReceiver::Literal(ruby_type) => ruby_type.clone(),
        MethodReceiver::Expression => RubyType::Unknown,
    }
}

fn resolve_constant_receiver(
    path: &[RubyConstant],
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<FullyQualifiedName> {
    if let Some(query) = context.query {
        if let Some(resolved) = query.resolve_constant_in_context(path, context.current_namespace) {
            let resolved_constant =
                FullyQualifiedName::constant(resolved.namespace_parts().to_vec());
            if let Some(ruby_type) = query.constant_value_type(&resolved_constant) {
                return query.type_to_namespace(&ruby_type);
            }
        }
        return Some(query.resolve_constant_receiver(path, context.current_namespace));
    }

    Some(FullyQualifiedName::namespace_with_kind(
        path.to_vec(),
        NamespaceKind::Singleton,
    ))
}

fn variable_receiver_type(
    var_name: &str,
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<RubyType> {
    if let Some(document) = context.document {
        let file_id = document.analysis_file_id();
        if let Some(scope_id) = document
            .variable_scopes()
            .find_scope_for_variable_at(var_name, file_id, context.byte_offset)
            .or_else(|| {
                document
                    .variable_scopes()
                    .scope_at_position(file_id, context.byte_offset)
            })
        {
            if let Some(ruby_type) = document.variable_scopes().get_type_at_position(
                var_name,
                scope_id,
                file_id,
                context.byte_offset,
            ) {
                return Some(ruby_type.clone());
            }

            let query = context.query?;
            let scope_id = u32::try_from(scope_id).expect(
                "INVARIANT VIOLATED: local variable scope id exceeded u32. This is a bug because analysis TypeSubject stores scope ids as u32. Fix: widen TypeSubject scope ids before storing more than u32::MAX scopes.",
            );
            return query.local_variable_type_at(var_name, scope_id, file_id, context.byte_offset);
        }
    }
    None
}

fn variable_type_before(
    name: &str,
    kind: VariableTypeKind,
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<RubyType> {
    let document = context.document?;
    let query = context.query?;
    let owner = FullyQualifiedName::namespace_with_kind(
        context.current_namespace.to_vec(),
        context.namespace_kind,
    );
    query.variable_type_before_in_owner(
        kind,
        name,
        &owner,
        document.analysis_file_id(),
        context.byte_offset,
    )
}

fn method_call_receiver_type(
    inner_receiver: &MethodReceiver,
    method_name: &str,
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<RubyType> {
    let inner_namespace = resolve_receiver_to_namespace(inner_receiver, context)?;
    if method_name == "new" && inner_namespace.namespace_kind() == Some(NamespaceKind::Singleton) {
        return Some(RubyType::Class(FullyQualifiedName::constant(
            inner_namespace.namespace_parts(),
        )));
    }

    let method = RubyMethod::new(method_name).ok()?;
    let query = context.query?;
    query.method_return_type_for_receiver(&inner_namespace, &method)
}

fn type_to_namespace(
    ruby_type: &RubyType,
    context: &ReceiverResolutionContext<'_, '_>,
) -> Option<FullyQualifiedName> {
    if let Some(query) = context.query {
        return query.type_to_namespace(ruby_type);
    }

    fallback_type_to_namespace(ruby_type)
}

fn fallback_type_to_namespace(ruby_type: &RubyType) -> Option<FullyQualifiedName> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => Some(
            FullyQualifiedName::namespace_with_kind(fqn.namespace_parts(), NamespaceKind::Instance),
        ),
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            Some(FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Singleton,
            ))
        }
        RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
            vec![RubyConstant::new("Array").expect(
                "INVARIANT VIOLATED: built-in constant `Array` is invalid. \
                 This is a bug because Ruby built-in constants must be valid Ruby constants. \
                 Fix: correct the hard-coded built-in constant name.",
            )],
            NamespaceKind::Instance,
        )),
        RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
            vec![RubyConstant::new("Hash").expect(
                "INVARIANT VIOLATED: built-in constant `Hash` is invalid. \
                 This is a bug because Ruby built-in constants must be valid Ruby constants. \
                 Fix: correct the hard-coded built-in constant name.",
            )],
            NamespaceKind::Instance,
        )),
        RubyType::Union(_) | RubyType::Unknown => None,
    }
}
