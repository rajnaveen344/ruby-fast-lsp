//! Diagnostics tests organized by the Ruby behaviour they check.

mod external_linter;
// Statement flow: unreachable code, inconsistent returns, and nil receivers.
mod flow;
// Method calls: arity, keywords, misspellings, and unresolved receivers.
mod method_calls;
// Values with a provably wrong type for their operator (splat, raise).
mod value_types;
