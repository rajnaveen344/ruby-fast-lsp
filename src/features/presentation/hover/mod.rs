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
    Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Position, Range, Url,
};

use crate::features::cursor::{Cursor, EngineQuery};
use crate::features::navigation::definition::require_string_lsp_range;
use crate::invariant::ExpectInvariant;
use crate::loader::require_paths::{
    find_require_string_at_offset, resolve_require_path, RequireKind, RequireStringTarget,
};
use crate::server::Server;
use crate::utils::lsp::source_position;
use crate::utils::parser::position_to_offset;

/// Handle `textDocument/hover`.
pub async fn handle(server: &Server, params: HoverParams) -> LspResult<Option<Hover>> {
    Ok(hover(server, params))
}

fn hover(server: &Server, params: HoverParams) -> Option<Hover> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc_arc = server.open_document(&uri)?;
    let require = {
        let document = doc_arc.read();
        let byte_offset = document.position_to_analysis_offset(source_position(position));
        find_require_string_at_offset(&document.content, byte_offset as usize)
            .map(|target| (require_string_lsp_range(&document, &target), target))
    };
    if let Some(hover) =
        require.and_then(|(range, target)| require_path_hover(server, &uri, range, &target))
    {
        return Some(hover);
    }

    let hover_info = EngineQuery::with_doc_and_project(doc_arc, server.project_for_uri(&uri))
        .with_view(|cursor| {
            let content = &cursor.document?.content;
            hover_at(cursor, &uri, position, content)
        })?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_info.content,
        }),
        range: hover_info.range,
    })
}

fn require_path_hover(
    server: &Server,
    uri: &Url,
    range: Range,
    target: &RequireStringTarget,
) -> Option<Hover> {
    let current_file = uri.to_file_path().ok()?;
    let project_root = server
        .workspace_for_uri(uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| current_file.parent().map(PathBuf::from))?;
    let load_paths = server.with_configuration(|config| {
        config
            .indexing
            .load_paths
            .paths_for_project(&project_root)
            .to_vec()
    });
    let feature_index = server.require_feature_index_for_uri(uri);
    let resolved = server.project_for_uri(uri).view(|view| {
        resolve_require_path(
            target.kind,
            &target.argument,
            &current_file,
            &project_root,
            &load_paths,
            &feature_index,
            Some(view),
        )
    });
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

/// Hover info for the symbol at `position`: local, instance, class, and
/// global variables, constants, methods (receiver type and return type), and
/// YARD type references.
pub fn hover_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<HoverInfo> {
    let byte_offset = u32::try_from(position_to_offset(content, position)).expect_invariant(
        "hover position exceeded u32 byte offsets",
        "analysis TextRange offsets are u32",
        "widen domain offsets before accepting larger source files",
    );
    let (identifier, identifier_type, namespace, scope_id, namespace_kind) = cursor
        .analyzer(uri, content, position)
        .get_identifier(byte_offset);
    let target = identifier_to_hover_target(
        identifier?,
        identifier_type,
        namespace.clone(),
        namespace_kind,
        scope_id,
        byte_offset,
    );
    let context = HoverContext {
        cursor,
        current_namespace: &namespace,
        namespace_kind,
        position,
        byte_offset,
    };
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
