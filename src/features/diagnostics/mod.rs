//! Diagnostics: the engine's diagnostics for a document, the lifecycle entry
//! point that runs the external linter for one, and the linter and formatter
//! runner.
//!
//! The engine-to-LSP projection itself is server-owned (`server::diagnostics`)
//! so that publication and queries share one conversion. AST-only diagnostics
//! (syntax errors and warnings) live in
//! `loader/file_processor/syntax_diagnostics.rs`.

pub mod linter;

pub use crate::server::engine_diagnostics;

use crate::environment::config::LinterKind;
use crate::server::Server;
use linter::lint_document;
use ruby_analysis::core::SourceKind;
use std::path::Path;
use std::time::Duration;
use tower_lsp::lsp_types::Url;

/// Lint an editable document and retain the output for its exact current
/// source, where every later composition of its diagnostics reads it.
/// Lifecycle entry point for didOpen and didSave; publication then goes
/// through the server's one composition.
pub async fn run_linter(server: &Server, uri: &Url, content: &str) {
    server.clear_external_linter_diagnostics(uri);
    if !document_kind(server, uri).is_some_and(SourceKind::is_editable) {
        return;
    }
    if ruby_analysis::indexer::is_erb_path(uri.path()) {
        return;
    }
    let config = server.configuration_snapshot();
    if config.linter == LinterKind::None {
        return;
    }
    let Ok(file_path) = uri.to_file_path() else {
        log::warn!(
            "Skipping {} diagnostics for non-file URI {}",
            config.linter.data_name().unwrap_or("external linter"),
            uri
        );
        return;
    };
    let workspace_root = server
        .workspace_for_uri(uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| file_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    let Some(snapshot) = server.diagnostic_source_snapshot(uri) else {
        return;
    };

    match lint_document(
        &config,
        server.indexing.resources().clone(),
        &workspace_root,
        &file_path,
        content,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(linter_diagnostics) => {
            server.retain_external_linter_diagnostics(uri, snapshot, &linter_diagnostics);
        }
        Err(error) => log::warn!(
            "External linter diagnostics unavailable for {}: {error:#}. \
             Ensure the selected linter is available through the owning project's bundle.",
            file_path.display()
        ),
    }
}

fn document_kind(server: &Server, uri: &Url) -> Option<SourceKind> {
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| std::path::PathBuf::from(uri.to_string()));
    server.project_for_uri(uri).view(|view| {
        view.file_id(&path)
            .and_then(|file_id| view.file(file_id))
            .map(|file| file.kind)
    })
}
