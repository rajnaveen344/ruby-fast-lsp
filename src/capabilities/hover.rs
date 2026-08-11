//! Hover capability for displaying type information.
//!
//! Provides hover information for:
//! - Require / require_relative string paths (contents only, no quotes)
//! - Local variables (shows inferred type)
//! - Methods (shows return type)
//! - Classes/Modules (shows class/module name)
//! - Constants (shows type or value info)
//!
//! Semantic hover logic is delegated to the query layer.

use std::path::PathBuf;

use tower_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, HoverProviderCapability, MarkupContent, MarkupKind,
};

use crate::capabilities::definitions::require_string_lsp_range;
use crate::indexer::require_paths::{
    find_require_string_at_offset, resolve_require_path, RequireKind,
};
use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;
use crate::utils::lsp::source_position;

/// Return the hover capability.
pub fn get_hover_capability() -> HoverProviderCapability {
    HoverProviderCapability::Simple(true)
}

/// Handle hover request using the unified EngineQuery layer.
pub async fn handle_hover(server: &RubyLanguageServer, params: HoverParams) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let (content, doc_arc, document, byte_offset) = {
        let docs = server.docs.lock();
        let doc_arc = docs.get(&uri)?.clone();
        let doc = doc_arc.read();
        let byte_offset = doc.position_to_analysis_offset(source_position(position));
        (
            doc.content.clone(),
            doc_arc.clone(),
            doc.clone(),
            byte_offset,
        )
    };

    if let Some(hover) = require_path_hover(server, &uri, &content, &document, byte_offset as usize)
    {
        return Some(hover);
    }

    let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(&uri));
    let hover_info = query.get_hover_at_position(&uri, position, &content)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_info.content,
        }),
        range: hover_info.range,
    })
}

fn require_path_hover(
    server: &RubyLanguageServer,
    uri: &tower_lsp::lsp_types::Url,
    content: &str,
    document: &ruby_analysis::indexer::RubyDocument,
    byte_offset: usize,
) -> Option<Hover> {
    let target = find_require_string_at_offset(content, byte_offset)?;
    let current_file = uri.to_file_path().ok()?;
    let project_root = server
        .workspace_for_uri(uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| current_file.parent().map(PathBuf::from))?;
    let load_paths = server
        .config
        .lock()
        .indexing
        .load_paths
        .paths_for_project(&project_root)
        .to_vec();
    let dependency_roots = server.dependency_require_paths_for_uri(uri);
    let engine = server.analysis_engine_for_uri(uri);
    let engine_guard = engine.read();
    let resolved = resolve_require_path(
        target.kind,
        &target.argument,
        &current_file,
        &project_root,
        &load_paths,
        &dependency_roots,
        Some(&engine_guard),
    );
    let range = require_string_lsp_range(document, &target);
    let kind = match target.kind {
        RequireKind::Require => "require",
        RequireKind::RequireRelative => "require_relative",
    };
    let value = match resolved {
        Some(path) => format!(
            "```ruby\n{kind} \"{}\"\n```\n\n`{}`",
            target.argument,
            path.display()
        ),
        None => format!(
            "```ruby\n{kind} \"{}\"\n```\n\nCannot resolve require path.",
            target.argument
        ),
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(range),
    })
}
