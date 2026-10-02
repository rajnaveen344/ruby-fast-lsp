//! Workspace symbols: symbol search across every project engine, or the
//! top-level symbols for an empty query.

use crate::invariant::ExpectInvariant;
use log::info;
use ruby_analysis::core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis::engine::AnalysisQuery;
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{SymbolInformation, SymbolKind, WorkspaceSymbolParams};

use crate::features::cursor::analysis_location::location_for_range;
use crate::features::cursor::EngineQuery;
use crate::server::RubyLanguageServer;

/// Handle workspace symbol requests.
///
/// `workspace/symbol` has no anchor URI, so we query every registered
/// workspace index plus the orphan index and merge the results. Multi-root
/// workspaces see symbols from every folder, with the per-workspace indices
/// remaining isolated for all other queries.
pub async fn handle(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> LspResult<Option<Vec<SymbolInformation>>> {
    let query_text = params.query;
    info!("Workspace symbols request for query: '{}'", query_text);

    let start_time = Instant::now();
    let mut symbols = Vec::new();
    for analysis_engine in lang_server.analysis_engines() {
        let engine_query = EngineQuery::with_engine(analysis_engine);
        if query_text.is_empty() {
            symbols.extend(engine_query.get_top_level_symbols());
        } else {
            symbols.extend(engine_query.search_workspace_symbols(&query_text));
        }
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

impl EngineQuery {
    pub fn get_top_level_symbols(&self) -> Vec<SymbolInformation> {
        let engine_ref = self.analysis_engine().expect_invariant(
            "workspace symbols query requires an analysis engine",
            "LSP workspace/symbol should be a thin wrapper over AnalysisEngine",
            "construct EngineQuery with with_engine()",
        );
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine)
            .top_level_symbols(50)
            .into_iter()
            .filter_map(|symbol| symbol_information_from_engine_symbol(&engine, symbol))
            .collect()
    }

    pub fn search_workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let engine_ref = self.analysis_engine().expect_invariant(
            "workspace symbol search requires an analysis engine",
            "LSP workspace/symbol should be a thin wrapper over AnalysisEngine",
            "construct EngineQuery with with_engine()",
        );
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine)
            .search_workspace_symbols(query, 100)
            .into_iter()
            .filter_map(|symbol| symbol_information_from_engine_symbol(&engine, symbol))
            .collect()
    }
}

fn symbol_information_from_engine_symbol(
    engine: &ruby_analysis::engine::AnalysisEngine,
    symbol: ruby_analysis::engine::WorkspaceSymbolMatch,
) -> Option<SymbolInformation> {
    Some(SymbolInformation {
        name: symbol.name,
        kind: analysis_symbol_kind_to_lsp_kind(symbol.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: Some(false),
        location: location_for_range(engine, symbol.range)?,
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
    use std::sync::Arc;

    use parking_lot::RwLock;
    use ruby_analysis::core::{
        FileAnalysis, FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SourceKind,
        SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange,
    };
    use ruby_analysis::engine::{AnalysisEngine, ResolveMode, SourceFileInput};

    use super::*;

    fn query_with_analysis_symbols() -> EngineQuery {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: crate::test::harness::fixture_path("/tmp/user.rb"),
            content: source.into(),
            kind: SourceKind::Project,
        });
        invariant_eq!(
            file_id,
            SourceFileId(0),
            what = "first test analysis file id changed",
            why = "this test assumes a fresh AnalysisEngine",
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

        EngineQuery::with_engine(Arc::new(RwLock::new(engine)))
    }

    #[test]
    fn workspace_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.search_workspace_symbols("name");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::METHOD);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.get_top_level_symbols();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
    }
}
