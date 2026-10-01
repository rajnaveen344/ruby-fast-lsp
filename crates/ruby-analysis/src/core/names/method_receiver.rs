use crate::core::RubyConstant;

/// Represents the receiver of a method call, combining type and data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodReceiver {
    /// No receiver, e.g., `method_a`
    None,
    /// Self receiver, e.g., `self.method_a`
    SelfReceiver,
    /// Super receiver, e.g., `super` inside an overriding method.
    Super,
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
    Literal(crate::core::RubyType),
    /// Complex expression receiver that can't be statically analyzed, e.g., `(a + b).method`
    Expression,
}

impl MethodReceiver {
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
}
