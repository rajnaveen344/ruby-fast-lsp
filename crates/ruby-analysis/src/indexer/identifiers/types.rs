use std::fmt;

use crate::core::{FullyQualifiedName, MethodReceiver, RubyConstant, RubyMethod};
use ustr::Ustr;

/// Enum to represent different types of identifiers at a specific position
#[derive(Debug, Clone)]
pub enum Identifier {
    /// Ruby constant with namespace context and identifier path
    RubyConstant {
        namespace: Vec<RubyConstant>,
        iden: Vec<RubyConstant>,
    },

    /// Ruby method with comprehensive context
    RubyMethod {
        namespace: Vec<RubyConstant>,
        receiver: MethodReceiver,
        iden: RubyMethod,
    },

    /// Ruby local variable with namespace context
    RubyLocalVariable {
        namespace: Vec<RubyConstant>,
        name: String,
    },

    /// Ruby instance variable
    RubyInstanceVariable {
        namespace: Vec<RubyConstant>,
        name: String,
    },

    /// Ruby class variable
    RubyClassVariable {
        namespace: Vec<RubyConstant>,
        name: String,
    },

    /// Ruby global variable
    RubyGlobalVariable {
        namespace: Vec<RubyConstant>,
        name: String,
    },

    /// YARD type reference in documentation comment
    YardType {
        type_name: String,
        namespace: Vec<RubyConstant>,
    },
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::RubyConstant { namespace: _, iden } => {
                let iden_str: Vec<String> = iden.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", iden_str.join("::"))
            }
            Identifier::RubyMethod { iden, .. } => write!(f, "{}", iden),
            Identifier::RubyLocalVariable { name, .. } => write!(f, "{}", name),
            Identifier::RubyInstanceVariable { name, .. } => write!(f, "{}", name),
            Identifier::RubyClassVariable { name, .. } => write!(f, "{}", name),
            Identifier::RubyGlobalVariable { name, .. } => write!(f, "{}", name),
            Identifier::YardType { type_name, .. } => write!(f, "{}", type_name),
        }
    }
}

impl From<Identifier> for FullyQualifiedName {
    fn from(value: Identifier) -> Self {
        match value {
            Identifier::RubyConstant { namespace: _, iden } => FullyQualifiedName::constant(iden),
            Identifier::RubyMethod {
                namespace, iden, ..
            } => FullyQualifiedName::method(namespace, iden),
            Identifier::RubyLocalVariable { name, .. } => {
                FullyQualifiedName::LocalVariable(Ustr::from(&name))
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                FullyQualifiedName::InstanceVariable(Ustr::from(&name))
            }
            Identifier::RubyClassVariable { name, .. } => {
                FullyQualifiedName::ClassVariable(Ustr::from(&name))
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                FullyQualifiedName::GlobalVariable(Ustr::from(&name))
            }
            Identifier::YardType { type_name, .. } => {
                let namespace: Vec<RubyConstant> = type_name
                    .split("::")
                    .filter_map(|part| RubyConstant::try_from(part.trim()).ok())
                    .collect();
                FullyQualifiedName::constant(namespace)
            }
        }
    }
}
