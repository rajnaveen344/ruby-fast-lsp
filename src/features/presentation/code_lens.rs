//! Code lenses: for each `module` definition, the mixin usages (include,
//! prepend, extend) and classes that include it, plus extension lenses.

use crate::features::cursor::analysis_location::location_for_range;
use crate::features::cursor::{Cursor, EngineQuery};
use crate::invariant::ExpectInvariant;
use crate::server::Server;
use crate::utils::lsp::lsp_position;
use log::{debug, warn};
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::engine::{MixinUsageKind, View};
use ruby_analysis::indexer::module_definitions_for_lens;
use std::collections::HashMap;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

/// Handle `textDocument/codeLens`: module usage lenses plus extension lenses.
pub async fn handle(
    lang_server: &Server,
    params: CodeLensParams,
) -> LspResult<Option<Vec<CodeLens>>> {
    let uri = &params.text_document.uri;

    // 1. Config check (server concern).
    let modules_enabled = {
        let config = lang_server.config.lock();
        config.code_lens_modules_enabled.unwrap_or(true)
    };
    if !modules_enabled {
        return Ok(Some(Vec::new()));
    }

    // 2. Get document content and Arc.
    let (content, doc_arc) = {
        let doc_arc = match lang_server.open_document(uri) {
            Some(arc) => arc,
            None => {
                debug!("Document not found for URI: {}", uri);
                return Ok(Some(Vec::new()));
            }
        };
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // 3. Read module lenses under one cursor. The guards are released before
    // awaiting governed extension work, so the LSP future remains Send.
    let mut lenses: Vec<CodeLens> =
        EngineQuery::with_doc_and_project(doc_arc, lang_server.project_for_uri(uri))
            .with_view(|cursor| code_lenses_at(cursor, uri))
            .into_iter()
            .map(to_lsp_code_lens)
            .collect();
    let project_root = lang_server
        .analysis_workspace_for_uri(uri)
        .map(|workspace| workspace.root_path);
    match lang_server
        .extensions
        .registry()
        .code_lenses_governed(
            lang_server.indexing.resources().clone(),
            project_root,
            uri.as_str().to_string(),
            content,
            lang_server.extension_project_context_for_document(uri),
        )
        .await
    {
        Ok(extension_lenses) => lenses.extend(extension_lenses),
        Err(error) => warn!(
            "Extension code-lens request failed for {}: {error:#}",
            uri.path()
        ),
    }
    Ok(Some(lenses))
}

/// Convert a `CodeLensData` into an LSP `CodeLens`.
fn to_lsp_code_lens(data: CodeLensData) -> CodeLens {
    CodeLens {
        range: data.range,
        command: Some(Command {
            title: data.title,
            command: data.command,
            arguments: Some(vec![
                serde_json::to_value(data.uri.as_str()).unwrap(),
                serde_json::to_value(data.target_position).unwrap(),
                serde_json::to_value(data.locations).unwrap(),
            ]),
        }),
        data: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MixinType {
    Include,
    Prepend,
    Extend,
}

// ============================================================================
// Public data type
// ============================================================================

/// Domain result for a single code lens item.
///
/// LSP-agnostic: holds the data needed to build a `CodeLens`, but the final
/// `Command` construction happens in `to_lsp_code_lens`.
pub struct CodeLensData {
    /// LSP range covering the `module` keyword through the constant name.
    pub range: Range,
    /// Human-readable title, e.g. "2 include", "1 class".
    pub title: String,
    /// VS Code command id, e.g. "ruby-fast-lsp.showReferences".
    pub command: String,
    /// Document URI (needed for command arguments).
    pub uri: Url,
    /// Position for the showReferences command.
    pub target_position: Position,
    /// Reference locations to display.
    pub locations: Vec<Location>,
}

// ============================================================================
// Query over one cursor
// ============================================================================

/// Code lenses for every `module` definition in the cursor's document: one
/// `CodeLensData` per mixin-type bucket and one for classes, for every module
/// that has at least one usage.
pub fn code_lenses_at(cursor: Cursor<'_>, uri: &Url) -> Vec<CodeLensData> {
    let document = cursor.document.expect_invariant(
        "code lenses were read without a document",
        "the handler builds its cursor from the open document",
        "construct EngineQuery with with_doc_and_project()",
    );
    // Parse the document's Ruby analysis projection. For ERB this preserves
    // template byte offsets while masking host-language text.
    let modules = module_definitions_for_lens(document.analysis_content());

    if modules.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    for module in &modules {
        let usages = mixin_usages_from_analysis(cursor.view, &module.fqn);
        let class_locations = class_definition_locations_from_analysis(cursor.view, &module.fqn);

        if usages.is_empty() && class_locations.is_empty() {
            debug!("No usages or classes found for module: {:?}", module.fqn);
            continue;
        }

        // Convert byte offsets to LSP positions.
        let start_position = lsp_position(document.offset_to_position(module.start_offset));
        let end_position = lsp_position(document.offset_to_position(module.end_offset));
        let range = Range {
            start: start_position,
            end: end_position,
        };

        // Group mixin usages by type.
        let mut usages_by_type: HashMap<MixinType, Vec<Location>> = HashMap::new();
        for (mixin_type, location) in usages {
            usages_by_type.entry(mixin_type).or_default().push(location);
        }

        // One CodeLensData per mixin type.
        let mixin_types = [
            (MixinType::Include, "include"),
            (MixinType::Prepend, "prepend"),
            (MixinType::Extend, "extend"),
        ];

        for (mixin_type, type_name) in &mixin_types {
            if let Some(locations) = usages_by_type.get(mixin_type) {
                results.push(CodeLensData {
                    range,
                    title: format!("{} {}", locations.len(), type_name),
                    command: "ruby-fast-lsp.showReferences".to_string(),
                    uri: uri.clone(),
                    target_position: start_position,
                    locations: locations.clone(),
                });
            }
        }

        // One CodeLensData for classes.
        if !class_locations.is_empty() {
            let count = class_locations.len();
            results.push(CodeLensData {
                range,
                title: format!("{} {}", count, if count == 1 { "class" } else { "classes" }),
                command: "ruby-fast-lsp.showReferences".to_string(),
                uri: uri.clone(),
                target_position: start_position,
                locations: class_locations,
            });
        }
    }

    results
}

fn mixin_usages_from_analysis(
    view: &View<'_>,
    module_fqn: &FullyQualifiedName,
) -> Vec<(MixinType, Location)> {
    let mut usages = view
        .module_mixin_usages(module_fqn)
        .into_iter()
        .filter_map(|usage| {
            let mixin_type = mixin_type_from_usage_kind(usage.kind);
            let location = location_for_range(view, usage.range)?;
            Some((mixin_type, location))
        })
        .collect::<Vec<_>>();
    usages.sort_by_key(|(mixin_type, location)| {
        (
            mixin_type_sort_key(*mixin_type),
            location.uri.to_string(),
            location.range.start.line,
            location.range.start.character,
        )
    });
    usages
}

fn class_definition_locations_from_analysis(
    view: &View<'_>,
    module_fqn: &FullyQualifiedName,
) -> Vec<Location> {
    let mut result = view
        .module_including_class_definition_ranges(module_fqn)
        .into_iter()
        .filter_map(|range| location_for_range(view, range))
        .collect::<Vec<_>>();
    result.sort_by_key(|location| {
        (
            location.uri.to_string(),
            location.range.start.line,
            location.range.start.character,
        )
    });
    result.dedup_by(|left, right| {
        left.uri == right.uri
            && left.range.start == right.range.start
            && left.range.end == right.range.end
    });
    result
}

fn mixin_type_from_usage_kind(kind: MixinUsageKind) -> MixinType {
    match kind {
        MixinUsageKind::Include => MixinType::Include,
        MixinUsageKind::Prepend => MixinType::Prepend,
        MixinUsageKind::Extend => MixinType::Extend,
    }
}

fn mixin_type_sort_key(mixin_type: MixinType) -> u8 {
    match mixin_type {
        MixinType::Include => 0,
        MixinType::Prepend => 1,
        MixinType::Extend => 2,
    }
}
