//! Hover target classification.

use crate::core::RubyConstant;
use crate::indexer::{Identifier, IdentifierType, LVScopeId, MethodReceiver};
use tower_lsp::lsp_types::Position;

/// Represents a Ruby construct at the hover position.
#[derive(Debug, Clone)]
pub enum HoverTarget {
    LocalVariable {
        name: String,
        position: Position,
        scope_id: LVScopeId,
    },
    Constant {
        path: Vec<RubyConstant>,
    },
    Method {
        name: String,
        position: Position,
        receiver: MethodReceiver,
        namespace: Vec<RubyConstant>,
        is_definition: bool,
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
    scope_id: LVScopeId,
    position: Position,
) -> HoverTarget {
    match identifier {
        Identifier::RubyLocalVariable { name, .. } => HoverTarget::LocalVariable {
            name,
            position,
            scope_id,
        },
        Identifier::RubyConstant { iden, .. } => HoverTarget::Constant { path: iden },
        Identifier::RubyMethod {
            iden,
            receiver,
            namespace: method_namespace,
        } => {
            let namespace = if method_namespace.is_empty() {
                namespace
            } else {
                method_namespace
            };
            HoverTarget::Method {
                name: iden.to_string(),
                position,
                receiver,
                namespace,
                is_definition: identifier_type == Some(IdentifierType::MethodDef),
            }
        }
        Identifier::RubyInstanceVariable { name, .. } => HoverTarget::InstanceVariable { name },
        Identifier::RubyClassVariable { name, .. } => HoverTarget::ClassVariable { name },
        Identifier::RubyGlobalVariable { name, .. } => HoverTarget::GlobalVariable { name },
        Identifier::YardType { type_name, .. } => HoverTarget::YardType { type_name },
    }
}
