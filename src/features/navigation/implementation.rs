//! Go to implementation: where methods and modules are concretely implemented.
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use crate::invariant::ExpectInvariant;
use log::{info, trace};
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::Identifier;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Url};

use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use crate::features::cursor::EngineQuery;
use crate::server::RubyLanguageServer;
use crate::utils::lsp::source_position;

/// Handle `textDocument/implementation`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let implementations = (|| {
        let (content, doc_arc) = {
            let doc_guard = server.documents.read();
            let doc_arc = doc_guard.get(&uri)?.clone();
            let doc = doc_arc.read();
            (doc.content.clone(), doc_arc.clone())
        };
        let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(&uri));
        query.find_implementations_at_position(&uri, position, &content)
    })();

    match implementations {
        Some(locations) => {
            trace!("Returning {} implementation locations", locations.len());
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
        None => {
            info!("No implementations found for position {:?}", position);
            Ok(None)
        }
    }
}

impl EngineQuery {
    /// Find implementations for the identifier at the given position.
    ///
    /// - Cursor on a method definition → find all overrides in descendants/includers
    /// - Cursor on a class/module name → find all classes that include/prepend/extend it,
    ///   plus all subclasses
    pub fn find_implementations_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        let analyzer = self.analyzer_at_position(uri, content, position);
        let (identifier, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier_at_position(source_position(position));

        let identifier = match identifier {
            Some(id) => id,
            None => {
                info!("No identifier found at position {:?}", position);
                return None;
            }
        };

        info!(
            "Looking for implementations of: {}->{}",
            FullyQualifiedName::from(ancestors.clone()),
            identifier,
        );

        match &identifier {
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => {
                // Resolve the owner class/module of this method
                let owner_fqn = self.resolve_receiver_to_namespace(
                    receiver,
                    &ancestors,
                    namespace_kind,
                    position,
                )?;
                self.method_implementations_from_analysis(&owner_fqn, iden)
            }
            Identifier::RubyConstant { namespace: _, iden } => {
                let fqn = self.resolve_constant_fqn(iden, &ancestors);
                self.namespace_implementations_from_analysis(&fqn)
            }
            _ => {
                info!(
                    "Implementation not supported for identifier type: {:?}",
                    identifier
                );
                None
            }
        }
    }

    fn method_implementations_from_analysis(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine_ref = self.analysis_engine().expect_invariant(
            "method implementation query requires an analysis engine",
            "LSP implementation should be a thin wrapper over AnalysisEngine",
            "construct EngineQuery with with_doc_and_engine()",
        );
        let engine = engine_ref.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.method_implementation_ranges(owner_fqn, method),
        ))
    }

    fn namespace_implementations_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine_ref = self.analysis_engine().expect_invariant(
            "namespace implementation query requires an analysis engine",
            "LSP implementation should be a thin wrapper over AnalysisEngine",
            "construct EngineQuery with with_doc_and_engine()",
        );
        let engine = engine_ref.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.namespace_implementation_ranges(fqn),
        ))
    }
}
