//! # Truth Ledgers
//!
//! Expectations tracking for the Graph Growth strategy.
//! Ledgers record what we expect to find during verification.

use std::collections::HashMap;

/// Tracks expected variable/method types for completion and inlay hint verification.
///
/// ## Example
///
/// When generating `var_42 = "hello"`, we record:
/// `type_ledger.insert("var_42", "String")`
///
/// During verification, we ask the LSP for inlay hints and check if
/// the hint at `var_42` contains "String".
#[derive(Debug, Clone, Default)]
pub struct TypeLedger {
    /// Variable name -> expected type (e.g., "var_42" -> "String")
    pub var_types: HashMap<String, String>,
    /// Method name -> expected return type
    pub method_returns: HashMap<String, String>,
}

/// Tracks reference anchors for go-to-definition verification.
///
/// ## Example
///
/// When generating `_ = Class_0.new # <REF:42>`, we record:
/// `ref_ledger.insert("REF:42", "Class_0")`
///
/// During verification:
/// 1. Find position of `# <REF:42>` anchor
/// 2. Ask LSP for "Definition" at that position
/// 3. Verify it jumps to where `Class_0` is defined
#[derive(Debug, Clone, Default)]
pub struct ReferenceLedger {
    /// Anchor ID -> target definition name (e.g., "REF:42" -> "Class_0")
    pub anchors: HashMap<String, String>,
}

/// Tracks expected inlay hints.
///
/// ## Example
///
/// When generating `x = Class_0.new`, we record:
/// `hint_ledger.insert("x", "Class_0")`
#[derive(Debug, Clone, Default)]
pub struct HintLedger {
    /// Variable/expression name -> expected hint text
    pub hints: HashMap<String, String>,
}

/// Tracks expected diagnostic errors (the "Saboteur" pattern).
///
/// ## Example
///
/// When generating `1 + "string" # <ERR:99>`, we record:
/// `error_ledger.insert("ERR:99", "TypeError")`
#[derive(Debug, Clone, Default)]
pub struct ErrorLedger {
    /// Anchor ID -> expected error type
    pub errors: HashMap<String, String>,
}

/// Tracks expected completion items for a given position.
///
/// ## Example
///
/// When generating a class with methods, we track what completions should appear:
/// `completion_ledger.insert("COMP:1", vec!["method_0", "method_1"])`
#[derive(Debug, Clone, Default)]
pub struct CompletionLedger {
    /// Anchor ID -> expected completion items
    pub expected_completions: HashMap<String, Vec<String>>,
}
