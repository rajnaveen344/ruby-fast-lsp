//! Semantic query memoization and the `View` type queries that use it.

mod bindings;
mod expressions;
mod memo;
mod method_returns;
mod namespaces;
mod thread_memo;

pub use memo::AnalysisQueryCache;
pub(in crate::engine) use memo::{memoizes, MethodMemoKey};

#[cfg(test)]
mod tests;
