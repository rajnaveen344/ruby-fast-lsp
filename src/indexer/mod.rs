//! Indexer Module
//!
//! This module provides the indexing infrastructure for the Ruby Language Server.
//! It handles parsing, storing, and querying Ruby code definitions and references.
//!
//! ## Architecture
//!
//! - **`index`**: The central `RubyIndex` data structure that stores all indexed information
//! - **`file_processor`**: Shared file processing logic (parsing, visitors, diagnostics)
//! - **`coordinator`**: Orchestrates the complete two-phase indexing process
//! - **`indexer_project`**: Handles project-specific file discovery and indexing
//! - **`indexer_stdlib`**: Handles Ruby standard library indexing
//! - **`indexer_gem`**: Handles gem discovery and indexing
//!
//! ## Supporting Modules
//!
//! - **`entry`**: Entry types and builders for storing indexed items
//! - **`inheritance_graph`**: Method resolution order, inheritance, and mixin handling
//! - **`prefix_tree`**: Fast prefix-based search for auto-completion
//! - **`version`**: Ruby version detection and management

pub mod coordinator;
pub mod entry;
pub mod file_processor;
pub mod graph;
pub mod index;
pub mod indexer_gem;
pub mod indexer_project;
pub mod indexer_stdlib;
pub mod interner;
pub mod prefix_tree;

pub mod version;
