//! Workspace symbols: symbol search across every project engine, or the
//! top-level symbols for an empty query.

use log::info;
use ruby_analysis::core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis::engine::View;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{SymbolInformation, SymbolKind, WorkspaceSymbolParams};

use crate::features::cursor::analysis_location::location_for_range;
use crate::features::cursor::EngineQuery;
use crate::server::Server;

/// Handle workspace symbol requests.
///
/// `workspace/symbol` has no anchor URI, so we query every registered
/// workspace index plus the orphan index and merge the results. Multi-root
/// workspaces see symbols from every folder, with the per-workspace indices
/// remaining isolated for all other queries.
pub async fn handle(
    lang_server: &Server,
    params: WorkspaceSymbolParams,
) -> LspResult<Option<Vec<SymbolInformation>>> {
    let query_text = params.query;
    info!("Workspace symbols request for query: '{}'", query_text);

    let start_time = Instant::now();
    let mut symbols = Vec::new();
    for project in lang_server.projects() {
        symbols.extend(EngineQuery::with_project(project).with_view(|cursor| {
            if query_text.is_empty() {
                top_level_symbols(cursor.view)
            } else {
                search_symbols(cursor.view, &query_text)
            }
        }));
    }
    symbols.sort_by(|left, right| {
        (
            left.name.as_str(),
            left.location.uri.as_str(),
            left.location.range.start,
            left.location.range.end,
        )
            .cmp(&(
                right.name.as_str(),
                right.location.uri.as_str(),
                right.location.range.start,
                right.location.range.end,
            ))
    });
    symbols.dedup_by(|left, right| {
        left.name == right.name
            && left.kind == right.kind
            && left.location == right.location
            && left.container_name == right.container_name
    });

    info!(
        "Workspace symbols search completed in {:?} - found {} symbols",
        start_time.elapsed(),
        symbols.len()
    );

    Ok(Some(symbols))
}

/// The first top-level symbols of one project, for an empty query.
pub fn top_level_symbols(view: &View<'_>) -> Vec<SymbolInformation> {
    view.top_level_symbols(50)
        .into_iter()
        .filter_map(|symbol| symbol_information(view, symbol))
        .collect()
}

/// One project's symbols that match `query`.
pub fn search_symbols(view: &View<'_>, query: &str) -> Vec<SymbolInformation> {
    view.search_workspace_symbols(query, 100)
        .into_iter()
        .filter_map(|symbol| symbol_information(view, symbol))
        .collect()
}

fn symbol_information(
    view: &View<'_>,
    symbol: ruby_analysis::engine::WorkspaceSymbolMatch,
) -> Option<SymbolInformation> {
    Some(SymbolInformation {
        name: symbol.name,
        kind: analysis_symbol_kind_to_lsp_kind(symbol.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: Some(false),
        location: location_for_range(view, symbol.range)?,
        container_name: symbol.container_name,
    })
}

fn analysis_symbol_kind_to_lsp_kind(kind: AnalysisSymbolKind) -> SymbolKind {
    match kind {
        AnalysisSymbolKind::Class => SymbolKind::CLASS,
        AnalysisSymbolKind::Module => SymbolKind::MODULE,
        AnalysisSymbolKind::Method => SymbolKind::METHOD,
        AnalysisSymbolKind::Constant => SymbolKind::CONSTANT,
        AnalysisSymbolKind::LocalVariable
        | AnalysisSymbolKind::InstanceVariable
        | AnalysisSymbolKind::ClassVariable
        | AnalysisSymbolKind::GlobalVariable => SymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use ruby_analysis::core::{
        FileAnalysis, FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SourceKind,
        SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange,
    };
    use ruby_analysis::engine::{Project, ResolveMode, SourceFileInput};

    use super::*;

    fn engine_with_analysis_symbols() -> Project {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = Project::new();
        let file_id = engine.register_file(SourceFileInput {
            path: crate::test::harness::fixture_path("/tmp/user.rb"),
            content: source.into(),
            kind: SourceKind::Project,
        });
        invariant_eq!(
            file_id,
            SourceFileId(0),
            what = "first test analysis file id changed",
            why = "this test assumes a fresh Project",
            fix = "update the expected file id or avoid asserting it",
        );

        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.update(
            file_id,
            FileAnalysis {
                symbols: vec![
                    SymbolFact::new(
                        FullyQualifiedName::namespace(vec![user]),
                        AnalysisSymbolKind::Class,
                        TextRange::new(file_id, 6, 10),
                    ),
                    SymbolFact::new(
                        FullyQualifiedName::method(
                            vec![user],
                            RubyMethod::new("name").expect("test method must be valid"),
                        ),
                        AnalysisSymbolKind::Method,
                        TextRange::new(file_id, 17, 21),
                    ),
                ],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );

        engine
    }

    #[test]
    fn workspace_symbols_can_read_project_without_index_entries() {
        let engine = engine_with_analysis_symbols();

        let symbols = search_symbols(&engine.view(), "name");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::METHOD);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_can_read_project_without_index_entries() {
        let engine = engine_with_analysis_symbols();

        let symbols = top_level_symbols(&engine.view());

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
    }
}
