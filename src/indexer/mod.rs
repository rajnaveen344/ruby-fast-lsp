//! Indexer Module
//!
//! This module provides the indexing infrastructure for the Ruby Language Server.
//! It handles parsing, storing, and querying Ruby code definitions and references.
//!
//! ## Architecture
//!
//! - **`index`**: The central `RubyIndex` data structure that stores all indexed information
//! - **`indexer_core`**: Core indexing functionality shared across all indexing phases
//! - **`coordinator`**: Orchestrates the complete two-phase indexing process
//! - **`indexer_project`**: Handles project-specific indexing
//! - **`indexer_stdlib`**: Handles Ruby standard library indexing
//! - **`indexer_gem`**: Handles gem discovery and indexing
//!
//! ## Supporting Modules
//!
//! - **`entry`**: Entry types and builders for storing indexed items
//! - **`ancestor_chain`**: Method resolution order and mixin handling
//! - **`prefix_tree`**: Fast prefix-based search for auto-completion
//! - **`dependency_tracker`**: Tracks project dependencies (stdlib, gems)
//! - **`version`**: Ruby version detection and management
//! - **`utils`**: Shared utility functions

pub mod ancestor_chain;
pub mod coordinator;
pub mod dependency_tracker;
pub mod entry;
pub mod index;
pub mod indexer_core;
pub mod indexer_gem;
pub mod indexer_project;
pub mod indexer_stdlib;
pub mod prefix_tree;

pub mod version;
