//! Control Flow Graph (CFG) for type narrowing analysis.
//!
//! This module provides CFG construction and dataflow analysis for
//! type narrowing in Ruby code. It enables precise type inference
//! within conditional branches.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
//! │ Ruby AST    │────►│ CFG Builder │────►│ Control Flow     │
//! │ (Prism)     │     │             │     │ Graph            │
//! └─────────────┘     └─────────────┘     └────────┬─────────┘
//!                                                   │
//!                                                   ▼
//!                                         ┌──────────────────┐
//!                                         │ Dataflow         │
//!                                         │ Analyzer         │
//!                                         └────────┬─────────┘
//!                                                   │
//!                                                   ▼
//!                                         ┌──────────────────┐
//!                                         │ Type State       │
//!                                         │ per Block        │
//!                                         └──────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use type_inference::cfg::TypeNarrowingEngine;
//!
//! let engine = TypeNarrowingEngine::new();
//!
//! // Track file opens/closes
//! engine.on_file_open(&uri, &content);
//!
//! // Get narrowed type at a position
//! let narrowed_type = engine.get_narrowed_type(&uri, "x", offset);
//! ```

mod builder;
mod dataflow;
mod engine;
mod graph;
mod guards;

pub use builder::CfgBuilder;
pub use dataflow::{DataflowAnalyzer, DataflowResults, TypeState};
pub use engine::{FileCfgState, MethodCfgState, TypeNarrowingEngine};
pub use graph::{
    BasicBlock, BlockId, BlockLocation, CfgEdge, ControlFlowGraph, EdgeKind, Statement,
    StatementKind,
};
pub use guards::TypeGuard;
