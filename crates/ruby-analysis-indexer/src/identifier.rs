use std::fmt;

use ruby_analysis_core::{FullyQualifiedName, RubyConstant, RubyMethod};
use ustr::Ustr;

/// Represents the receiver of a method call, combining type and data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodReceiver {
    /// No receiver, e.g., `method_a`
    None,
    /// Self receiver, e.g., `self.method_a`
    SelfReceiver,
    /// Constant receiver with path, e.g., `Foo::Bar` in `Foo::Bar.method`
    Constant(Vec<RubyConstant>),
    /// Local variable receiver, e.g., `a` in `a.method`
    LocalVariable(String),
    /// Instance variable receiver, e.g., `@name` in `@name.method`
    InstanceVariable(String),
    /// Class variable receiver, e.g., `@@count` in `@@count.method`
    ClassVariable(String),
    /// Global variable receiver, e.g., `$stdout` in `$stdout.method`
    GlobalVariable(String),
    /// Method call receiver, e.g., `user.name` in `user.name.upcase`
    MethodCall {
        /// The receiver of the inner method call (boxed to avoid infinite size)
        inner_receiver: Box<MethodReceiver>,
        /// The method name being called
        method_name: String,
    },
    /// Literal expression receiver with known type, e.g., `[1,2,3]` or `"hello"`
    Literal(ruby_analysis_inference::RubyType),
    /// Complex expression receiver that can't be statically analyzed, e.g., `(a + b).method`
    Expression,
}

impl MethodReceiver {
    /// Returns true if this is a constant receiver (class method call)
    pub fn is_constant(&self) -> bool {
        matches!(self, MethodReceiver::Constant(_))
    }

    /// Returns the variable name if this is a variable receiver
    pub fn variable_name(&self) -> Option<&str> {
        match self {
            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => Some(name),
            _ => None,
        }
    }

    /// Returns the constant path if this is a constant receiver
    pub fn constant_path(&self) -> Option<&[RubyConstant]> {
        match self {
            MethodReceiver::Constant(path) => Some(path),
            _ => None,
        }
    }

    /// Returns the method call info if this is a method call receiver
    pub fn method_call_info(&self) -> Option<(&MethodReceiver, &str)> {
        match self {
            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => Some((inner_receiver, method_name)),
            _ => None,
        }
    }
}

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
            Identifier::RubyConstant { namespace: _, iden } => FullyQualifiedName::Constant(iden),
            Identifier::RubyMethod {
                namespace, iden, ..
            } => FullyQualifiedName::Method(namespace, iden),
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
                let namespace = type_name
                    .split("::")
                    .filter_map(|part| RubyConstant::try_from(part.trim()).ok())
                    .collect();
                FullyQualifiedName::Constant(namespace)
            }
        }
    }
}
