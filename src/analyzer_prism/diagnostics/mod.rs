//! Analyzer modules — each diagnostic is its own module consuming
//! `&dyn SymbolTable` plus explicit context (document, namespace, …)
//! instead of being fused into a god-class visitor.

pub mod bad_splat;
pub mod raise_non_exception;
pub mod unresolved_method;

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
