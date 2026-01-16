//! Hover tests for call nodes (method calls).
//!
//! Two categories:
//! - with_receiver: obj.method, Class.new, chained calls
//! - without_receiver: implicit self calls, method parameters

pub mod with_receiver;
pub mod without_receiver;
