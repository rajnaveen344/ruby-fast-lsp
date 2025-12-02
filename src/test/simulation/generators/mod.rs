//! # Generators
//!
//! Proptest strategies for generating Ruby content, valid edits,
//! positions, and transition sequences.
//!
//! ## Graph Growth Strategy
//!
//! This module implements the "Graph Growth" pattern for generating complex Ruby code:
//!
//! 1. **Grow, Don't Write**: Build a DAG of dependencies. New code only references old code.
//! 2. **Find, Don't Remember**: Store unique names, find positions dynamically.
//! 3. **Construction-Based Typing**: Decide types before generating code.
//! 4. **Anchor Everything**: Use anchor comments (`# <REF:42>`) for non-unique items.
//!
//! ## Key Components
//!
//! - `SourceLocator`: Finds positions dynamically by searching for unique names/anchors
//! - `GeneratorState`: Maintains pools of definitions and truth ledgers
//! - `TruthLedger`: Tracks expectations for verification (types, references, hints, errors)
//!
//! ## Module Structure
//!
//! - `source_locator`: Dynamic position finding
//! - `ledgers`: Truth ledgers for expectations tracking
//! - `state`: Generator state with pools and ledgers
//! - `tracked_v2`: Graph Growth based tracked code
//! - `graph_growth`: Pool-based code generators
//! - `comprehensive`: Comprehensive test generators
//! - `ruby_content`: Ruby content and transition generators
//! - `safe_edit`: Safe edit generators

mod comprehensive;
mod graph_growth;
mod ledgers;
mod ruby_content;
mod safe_edit;
mod source_locator;
mod state;
mod tracked_v2;

// Re-export everything
pub use comprehensive::*;
pub use graph_growth::*;
pub use ledgers::*;
pub use ruby_content::*;
pub use safe_edit::*;
pub use source_locator::*;
pub use state::*;
pub use tracked_v2::*;

use proptest::prelude::*;

/// Generate any tracked code scenario using Graph Growth strategy
///
/// Covers definitions, references, method calls, completion, and type inference.
/// All generators use unique identifiers and anchor comments for robust position tracking.
pub fn tracked_code() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // === BASIC GRAPH GROWTH GENERATORS ===
        // These use unique identifiers and anchor comments for robust position tracking
        3 => graph_class_hierarchy(),
        3 => graph_mixin_relationships(),
        2 => graph_type_inference(),
        2 => graph_class_references(),
        2 => graph_completion_test(),

        // === STRICT TYPE INFERENCE GENERATORS ===
        // These test method chains, array access, and complex type scenarios
        3 => graph_method_chain_types(),
        3 => graph_array_access_types(),
        2 => graph_class_method_types(),
        2 => graph_ivar_type_propagation(),
        2 => graph_completion_after_type(),
        1 => graph_type_edge_cases(),

        // === COMPREHENSIVE GENERATORS (test ALL verification points) ===
        // These generators create code that tests definitions, references, types, and completions
        4 => graph_comprehensive_mixin(),
        4 => graph_method_return_types(),
        3 => graph_comprehensive_inheritance(),
    ]
}

