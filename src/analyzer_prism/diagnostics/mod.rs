//! Analyzer diagnostic helpers used by fact collection and engine diagnostics.

/// Information about the receiver of a method call, used by the unresolved-method diagnostic.
#[derive(Debug, Clone)]
pub enum ReceiverInfo {
    /// No receiver (e.g., `method_name`)
    NoReceiver,
    /// Self receiver (e.g., `self.method_name`)
    SelfReceiver,
    /// Constant receiver (e.g., `Foo.method` or `Foo::Bar.method`)
    ConstantReceiver(String),
    /// Expression receiver (e.g., `variable.method`)
    ExpressionReceiver,
    /// Invalid constant path (contains non-constant nodes)
    InvalidConstantPath,
}
