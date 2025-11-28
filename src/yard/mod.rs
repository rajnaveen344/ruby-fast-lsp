//! YARD Documentation Parser
//!
//! This module provides functionality to parse YARD documentation comments
//! and extract type information for methods, parameters, and return values.
//!
//! ## Supported YARD Tags
//!
//! ### Parameters
//! - `@param name [Type] description` - Parameter type annotation
//! - `@option hash_name [Type] :key (default) description` - Hash option annotation
//!
//! ### Return Types
//! - `@return [Type] description` - Return type annotation
//!
//! ### Block/Yield
//! - `@yieldparam name [Type] description` - Block parameter type annotation
//! - `@yieldreturn [Type] description` - Block return type annotation
//!
//! ### Other
//! - `@raise [ExceptionType] description` - Exception annotation
//! - `@deprecated reason` - Deprecation notice
//!
//! ## Type Syntax
//!
//! - Simple types: `String`, `Integer`, `Boolean`
//! - Union types: `String, nil` or `Integer, String`
//! - Arrays: `Array<String>` or `Array<String, Integer>`
//! - Hashes: `Hash{Symbol => String}` or `Hash<Symbol, String>`

pub mod parser;
pub mod types;

pub use parser::YardParser;
pub use types::{YardMethodDoc, YardOption, YardParam, YardReturn};
