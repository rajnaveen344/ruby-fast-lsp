//! Inlay hints for method definitions.
//!
//! - return_type: -> Type hints after method signature
//! - parameter_type: Type hints for parameters
//! - implicit_return: "return" hints before implicit return values

pub mod implicit_return;
pub mod parameter_type;
pub mod return_type;
