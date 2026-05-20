//! Indexer Module
//!
//! This module provides the indexing infrastructure for the Ruby Language Server.
//! It handles parsing, storing, and querying Ruby code definitions and references.
//!
//! ## Architecture
//!
//! - **analysis engine**: The central fact graph storing indexed information
//! - **`file_processor`**: Shared file processing logic (parsing, visitors, diagnostics)
//! - **`coordinator`**: Orchestrates complete fact collection and diagnostics
//! - **`indexer_project`**: Handles project-specific file discovery and indexing
//! - **`indexer_stdlib`**: Handles Ruby standard library indexing
//! - **`indexer_gem`**: Handles gem discovery and indexing
//!
//! ## Supporting Modules
//!
//! - **`inheritance_graph`**: Method resolution order, inheritance, and mixin handling
//! - **`version`**: Ruby version detection and management

pub mod coordinator;
pub mod file_processor;
pub mod indexer_gem;
pub mod indexer_project;
pub mod indexer_stdlib;
pub mod interner;

pub mod version;
