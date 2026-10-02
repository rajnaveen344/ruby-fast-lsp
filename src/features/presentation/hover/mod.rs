//! Hover: require paths, local variables, methods, constants, and YARD types.
//!
//! The cursor identifier becomes an analysis-domain `HoverTarget`, and the
//! generators build markdown for it.

pub mod generators;

pub use generators::HoverInfo;

use generators::HoverContext;
use ruby_analysis::indexer::{identifier_to_hover_target, HoverTarget};
use std::path::PathBuf;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Position, Url,
};

use crate::features::cursor::EngineQuery;
use crate::features::navigation::definition::require_string_lsp_range;
use crate::invariant::ExpectInvariant;
use crate::loader::require_paths::{
    find_require_string_at_offset, resolve_require_path, RequireKind,
};
use crate::server::RubyLanguageServer;
use crate::utils::lsp::source_position;
use crate::utils::parser::position_to_offset;

/// Handle `textDocument/hover`.
pub async fn handle(server: &RubyLanguageServer, params: HoverParams) -> LspResult<Option<Hover>> {
    Ok(hover(server, params))
}

fn hover(server: &RubyLanguageServer, params: HoverParams) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let (content, doc_arc, document, byte_offset) = {
        let docs = server.documents.read();
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
    uri: &Url,
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
    let feature_index = server.require_feature_index_for_uri(uri);
    let engine = server.analysis_engine_for_uri(uri);
    let engine_guard = engine.read();
    let resolved = resolve_require_path(
        target.kind,
        &target.argument,
        &current_file,
        &project_root,
        &load_paths,
        &feature_index,
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

impl EngineQuery {
    /// Get hover info for the symbol at position.
    ///
    /// This is the unified entry point for hover requests. It handles:
    /// - Local variables (with engine type facts, document lvars, type snapshots)
    /// - Instance/class/global variables
    /// - Constants (classes, modules)
    /// - Methods (with receiver type resolution and return type inference)
    /// - YARD type references
    pub fn get_hover_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<HoverInfo> {
        // Step 1: Get identifier at position using existing analyzer
        let analyzer = self.analyzer_at_position(uri, content, position);
        let byte_offset = u32::try_from(position_to_offset(content, position)).expect_invariant(
            "hover position exceeded u32 byte offsets",
            "analysis TextRange offsets are u32",
            "widen domain offsets before accepting larger source files",
        );
        let (identifier_opt, identifier_type, namespace, scope_id, namespace_kind) =
            analyzer.get_identifier(byte_offset);

        let identifier = identifier_opt?;

        // Step 2: Convert Identifier to HoverTarget (analysis-domain representation)
        let hover_namespace = namespace.clone();
        let target = identifier_to_hover_target(
            identifier,
            identifier_type,
            namespace,
            namespace_kind,
            scope_id,
            byte_offset,
        );

        // Step 3: Create context for generators
        let context = HoverContext {
            document: self.doc(),
            analysis_engine: self.analysis_engine(),
            current_namespace: &hover_namespace,
            namespace_kind,
            position,
            byte_offset,
        };

        // Step 4: Generate hover content based on node type
        match target {
            HoverTarget::LocalVariable { .. } => {
                generators::generate_local_variable_hover(&target, &context)
            }
            HoverTarget::Constant { .. } => generators::generate_constant_hover(&target, &context),
            HoverTarget::Method { .. } => generators::generate_method_hover(&target, &context),
            HoverTarget::InstanceVariable { .. }
            | HoverTarget::ClassVariable { .. }
            | HoverTarget::GlobalVariable { .. } => {
                generators::generate_variable_hover(&target, &context)
            }
            HoverTarget::YardType { .. } => generators::generate_yard_type_hover(&target),
        }
    }
}
