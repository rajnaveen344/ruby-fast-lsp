//! Shared imports for coordinator integration tests, split by topic.

use super::jruby::{
    build_cached_project_java_catalog, build_jruby_import_provider, read_jdk_feature,
};
use super::priority::{
    active_document_constant_priority_keys, prioritize_demanded_gem_names,
    prioritize_locked_gem_names, ActiveDocumentPriorityKeys,
};
use super::resources::MIB;
use super::runtime::runtime_stdlib_paths_for_project;
use super::*;
use crate::environment::config::runtime::{
    ProjectJrubyConfig, ProjectRuntimeSelection, RuntimeMode, RuntimeSelection,
    RuntimeSelectionConfig, SelectedRuntimeDescriptor,
};
use crate::environment::runtime::catalog::RuntimeDiscoverySource;
use crate::environment::runtime::jruby::java_catalog::JavaArtifactProductCache;
use crate::server::Server;
use crate::utils::persistent_cache::PersistentDerivedProductCache;
use ruby_analysis::core::{FullyQualifiedName, RubyType, TypeSubject};
use ruby_analysis::engine::SourceFileInput;
use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;
use tempfile::TempDir;
use tower_lsp::lsp_types::Position;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, TextDocumentContentChangeEvent,
    TextDocumentItem, VersionedTextDocumentIdentifier,
};
use zip::write::SimpleFileOptions;

mod jruby;
mod scheduling;
mod workflow;

/// Select the project runtime and Ruby version through the production
/// indexing sequence: exact runtime resolution, then version selection.
async fn select_ruby_version(
    coordinator: &mut IndexingCoordinator,
    server: &Server,
) -> Option<RubyVersion> {
    coordinator
        .resolve_effective_runtime(&server.load_context_for_project(coordinator.workspace_root()))
        .await
        .expect("fixture runtime selection must resolve");
    coordinator
        .detect_ruby_version_off_reactor()
        .await
        .expect("fixture Ruby version selection must succeed")
}
