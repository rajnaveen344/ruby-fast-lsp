//! Loader Module
//!
//! This module provides the indexing infrastructure for the Ruby Language Server.
//! It handles parsing, storing, and querying Ruby code definitions and references.
//!
//! ## Architecture
//!
//! - **analysis engine**: The central fact graph storing indexed information
//! - **`file_processor`**: Shared file processing logic (parsing, visitors, diagnostics)
//! - **`coordinator`**: Orchestrates complete fact collection and diagnostics
//! - **`sources::project`**: Handles project-specific file discovery and indexing
//! - **`sources::stdlib`**: Handles Ruby standard library indexing
//! - **`sources::gems`**: Handles gem discovery and indexing
//!
//! ## Supporting Modules
//!
//! - **`inheritance_graph`**: Method resolution order, inheritance, and mixin handling
//! - **`version`**: Ruby version detection and management
//! - **`scheduling`**: Work admission, the indexing queue, and progress status
//! - **`cache`**: Persisted dependency products and their producer identity

pub mod cache;
pub mod coordinator;
pub mod file_processor;
pub mod require_paths;
pub mod scheduling;
pub mod sources;
pub mod version;
