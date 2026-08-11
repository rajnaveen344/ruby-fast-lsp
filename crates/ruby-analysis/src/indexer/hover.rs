//! Hover target classification.

use crate::core::{NamespaceKind, RubyConstant};
use crate::indexer::{Identifier, IdentifierType, LVScopeId, MethodReceiver};

/// Represents a Ruby construct at the hover position.
#[derive(Debug, Clone)]
pub enum HoverTarget {
    LocalVariable {
        name: String,
        byte_offset: u32,
        scope_id: LVScopeId,
    },
    Constant {
        path: Vec<RubyConstant>,
    },
    Method {
        name: String,
        byte_offset: u32,
        receiver: MethodReceiver,
        namespace: Vec<RubyConstant>,
        namespace_kind: NamespaceKind,
        is_definition: bool,
        has_call_result: bool,
    },
    InstanceVariable {
        name: String,
    },
    ClassVariable {
        name: String,
    },
    GlobalVariable {
        name: String,
    },
    YardType {
        type_name: String,
    },
}

pub fn identifier_to_hover_target(
    identifier: Identifier,
    identifier_type: Option<IdentifierType>,
    namespace: Vec<RubyConstant>,
    namespace_kind: NamespaceKind,
    scope_id: LVScopeId,
    byte_offset: u32,
) -> HoverTarget {
    match identifier {
        Identifier::RubyLocalVariable { name, .. } => HoverTarget::LocalVariable {
            name,
            byte_offset,
            scope_id,
        },
        Identifier::RubyConstant { iden, .. } => HoverTarget::Constant { path: iden },
        Identifier::RubyMethod {
            iden,
            receiver,
            namespace: method_namespace,
        } => {
            let is_definition = identifier_type == Some(IdentifierType::MethodDef);
            let has_call_result = identifier_type == Some(IdentifierType::MethodCall);
            let method_namespace_kind = if is_definition && receiver != MethodReceiver::None {
                NamespaceKind::Singleton
            } else {
                namespace_kind
            };
            let namespace = if method_namespace.is_empty() {
                namespace
            } else {
                method_namespace
            };
            HoverTarget::Method {
                name: iden.to_string(),
                byte_offset,
                receiver,
                namespace,
                namespace_kind: method_namespace_kind,
                is_definition,
                has_call_result,
            }
        }
        Identifier::RubyInstanceVariable { name, .. } => HoverTarget::InstanceVariable { name },
        Identifier::RubyClassVariable { name, .. } => HoverTarget::ClassVariable { name },
        Identifier::RubyGlobalVariable { name, .. } => HoverTarget::GlobalVariable { name },
        Identifier::YardType { type_name, .. } => HoverTarget::YardType { type_name },
    }
}
