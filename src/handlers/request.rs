//! LSP Request Handlers
//!
//! This module contains handlers for LSP requests (messages that require a response).
//! Each handler delegates to the appropriate capability module for the actual logic.

use crate::capabilities::{
    call_hierarchy, code_actions, code_lens, completion, debug, definitions, document_highlights,
    document_symbols, folding_range, formatting, hover, implementation, inlay_hints,
    namespace_tree, references, rename, selection_ranges, semantic_tokens, signature_help,
    type_hierarchy, workspace_symbols,
};
use crate::extensions::{ExtensionStatusParams, ExtensionStatusResponse};
use crate::navigation_demand::{NavigationDemandOutcome, NavigationDemandStage};
use crate::server::RubyLanguageServer;
use log::{debug, info, trace};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tower_lsp::jsonrpc::{Error as LspError, ErrorCode, Result as LspResult};
use tower_lsp::lsp_types::*;

const PROJECT_NAVIGATION_DEMAND_WAIT: Duration = Duration::from_secs(5);
const DEPENDENCY_NAVIGATION_DEMAND_WAIT: Duration = Duration::from_secs(15);

pub async fn handle_goto_definition(
    lang_server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params
        .text_document_position_params
        .text_document
        .uri
        .clone();
    let position = params.text_document_position_params.position;

    let project = lang_server.analysis_workspace_for_uri(&uri);
    let mut definition =
        definitions::find_definition_at_position(lang_server, uri.clone(), position).await;
    let demand_keys = if definition.is_none() {
        definitions::navigation_demand_keys_at_position(lang_server, &uri, position)
    } else {
        None
    };
    if definition.is_none() {
        if let (Some(project), Some(demand_keys)) = (&project, &demand_keys) {
            let snapshot = project.indexing_status.snapshot();
            type DemandWait = Pin<
                Box<
                    dyn Future<
                            Output = (
                                NavigationDemandStage,
                                Result<NavigationDemandOutcome, tokio::time::error::Elapsed>,
                            ),
                        > + Send,
                >,
            >;
            let mut project_wait: Option<DemandWait> =
                if snapshot.generation > 0 && snapshot.phase.project_navigation_pending() {
                    demand_keys.project_key.as_deref().map(|key| {
                        let ticket = project.navigation_demands.request(
                            snapshot.generation,
                            NavigationDemandStage::Project,
                            key,
                        );
                        Box::pin(async move {
                            (
                                NavigationDemandStage::Project,
                                tokio::time::timeout(PROJECT_NAVIGATION_DEMAND_WAIT, ticket.wait())
                                    .await,
                            )
                        }) as DemandWait
                    })
                } else {
                    None
                };
            let mut dependency_wait: Option<DemandWait> = if snapshot.generation > 0
                && snapshot.phase.dependency_navigation_pending()
            {
                demand_keys.dependency_key.as_deref().map(|key| {
                    let ticket = project.navigation_demands.request(
                        snapshot.generation,
                        NavigationDemandStage::Dependency,
                        key,
                    );
                    Box::pin(async move {
                        (
                            NavigationDemandStage::Dependency,
                            tokio::time::timeout(DEPENDENCY_NAVIGATION_DEMAND_WAIT, ticket.wait())
                                .await,
                        )
                    }) as DemandWait
                })
            } else {
                None
            };
            let mut deferred_reason = None;
            if project_wait.is_some() || dependency_wait.is_some() {
                info!(
                    "Goto definition waiting for navigation demand (project_timeout={:?}, dependency_timeout={:?}) at {:?}",
                    PROJECT_NAVIGATION_DEMAND_WAIT,
                    DEPENDENCY_NAVIGATION_DEMAND_WAIT,
                    position
                );
            }
            while definition.is_none() && (project_wait.is_some() || dependency_wait.is_some()) {
                let (stage, outcome) = match (&mut project_wait, &mut dependency_wait) {
                    (Some(project_future), Some(dependency_future)) => {
                        tokio::select! {
                            outcome = project_future.as_mut() => {
                                project_wait = None;
                                outcome
                            }
                            outcome = dependency_future.as_mut() => {
                                dependency_wait = None;
                                outcome
                            }
                        }
                    }
                    (Some(project_future), None) => {
                        let outcome = project_future.as_mut().await;
                        project_wait = None;
                        outcome
                    }
                    (None, Some(dependency_future)) => {
                        let outcome = dependency_future.as_mut().await;
                        dependency_wait = None;
                        outcome
                    }
                    (None, None) => unreachable!(
                    "INVARIANT VIOLATED: navigation wait loop entered without a future. This is \
                     a bug because the loop predicate and exact branch observe the same local \
                     options. Fix: keep demand-future removal inside this match."
                ),
                };
                match outcome {
                    Ok(
                        NavigationDemandOutcome::TargetProcessed
                        | NavigationDemandOutcome::StageComplete,
                    ) => {
                        definition = definitions::find_definition_at_position(
                            lang_server,
                            uri.clone(),
                            position,
                        )
                        .await;
                    }
                    Ok(NavigationDemandOutcome::Superseded) => {
                        return Err(LspError::content_modified());
                    }
                    Ok(NavigationDemandOutcome::Cancelled) => {
                        return Err(LspError::request_cancelled());
                    }
                    Ok(NavigationDemandOutcome::Saturated) => {
                        deferred_reason = Some(match stage {
                            NavigationDemandStage::Project => {
                                "the bounded project-demand queue is full"
                            }
                            NavigationDemandStage::Dependency => {
                                "the bounded dependency-demand queue is full"
                            }
                        });
                    }
                    Err(_) => {
                        deferred_reason = Some(match stage {
                            NavigationDemandStage::Project => {
                                "the requested project input is still indexing"
                            }
                            NavigationDemandStage::Dependency => {
                                "the requested dependency input is still indexing"
                            }
                        });
                    }
                }
            }
            if definition.is_none() {
                let phase = project.indexing_status.snapshot().phase;
                if phase.project_navigation_pending() || phase.dependency_navigation_pending() {
                    let reason = deferred_reason.unwrap_or(
                        "the requested definition still depends on broader indexing",
                    );
                    info!(
                        "Goto definition deferred while indexing ({reason}) at {:?}",
                        position
                    );
                    return Err(indexing_in_progress_error(project, reason));
                }
            }
        }
    }

    match definition {
        Some(locations) => {
            if let Some(project) = &project {
                for location in &locations {
                    lang_server.retain_external_document_project(&location.uri, project);
                }
            }
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
        None => {
            info!("No definition found for position {:?}", position);
            Ok(None)
        }
    }
}

fn indexing_in_progress_error(project: &crate::server::Workspace, reason: &str) -> LspError {
    LspError {
        code: ErrorCode::ServerError(-32802),
        message: format!(
            "Ruby Fast LSP is still indexing {}: {reason}",
            project.root_path.display()
        )
        .into(),
        data: Some(serde_json::json!({ "retriggerRequest": true })),
    }
}

pub async fn handle_goto_implementation(
    lang_server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params
        .text_document_position_params
        .text_document
        .uri
        .clone();
    let position = params.text_document_position_params.position;

    let implementations =
        implementation::find_implementation_at_position(lang_server, uri, position).await;

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

pub async fn handle_references(
    lang_server: &RubyLanguageServer,
    params: ReferenceParams,
) -> LspResult<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;

    let references = references::find_references_at_position(lang_server, &uri, position).await;

    Ok(references)
}

pub async fn handle_document_highlight(
    lang_server: &RubyLanguageServer,
    params: DocumentHighlightParams,
) -> LspResult<Option<Vec<DocumentHighlight>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    Ok(document_highlights::find_document_highlights(lang_server, &uri, position).await)
}

pub async fn handle_selection_ranges(
    lang_server: &RubyLanguageServer,
    params: SelectionRangeParams,
) -> LspResult<Option<Vec<SelectionRange>>> {
    Ok(selection_ranges::handle_selection_ranges(lang_server, params).await)
}

pub async fn handle_signature_help(
    lang_server: &RubyLanguageServer,
    params: SignatureHelpParams,
) -> LspResult<Option<SignatureHelp>> {
    Ok(signature_help::handle_signature_help(lang_server, params).await)
}

pub async fn handle_code_actions(
    lang_server: &RubyLanguageServer,
    params: CodeActionParams,
) -> LspResult<Option<Vec<CodeActionOrCommand>>> {
    Ok(code_actions::handle_code_actions(lang_server, params).await)
}

pub async fn handle_semantic_tokens_full(
    lang_server: &RubyLanguageServer,
    params: SemanticTokensParams,
) -> LspResult<Option<SemanticTokensResult>> {
    Ok(Some(semantic_tokens::get_semantic_tokens_full(
        lang_server,
        params.text_document.uri,
    )))
}

pub async fn handle_inlay_hints(
    lang_server: &RubyLanguageServer,
    params: InlayHintParams,
) -> LspResult<Option<Vec<InlayHint>>> {
    Ok(Some(
        inlay_hints::handle_inlay_hints(lang_server, params).await,
    ))
}

pub async fn handle_completion(
    lang_server: &RubyLanguageServer,
    params: CompletionParams,
) -> LspResult<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri.clone();
    let position = params.text_document_position.position;

    debug!("Completion request received with params {:?}", params);

    Ok(Some(
        completion::find_completion_at_position(lang_server, uri, position, params.context).await,
    ))
}

pub async fn handle_completion_resolve(
    _lang_server: &RubyLanguageServer,
    params: CompletionItem,
) -> LspResult<CompletionItem> {
    info!(
        "Completion item resolve request received for {}",
        params.label
    );
    Ok(params)
}

pub async fn handle_document_symbols(
    lang_server: &RubyLanguageServer,
    params: DocumentSymbolParams,
) -> Option<DocumentSymbolResponse> {
    document_symbols::handle_document_symbols(lang_server, params).await
}

pub async fn handle_workspace_symbols(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> LspResult<Option<Vec<SymbolInformation>>> {
    Ok(workspace_symbols::handle_workspace_symbols(lang_server, params).await)
}

pub async fn handle_document_on_type_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentOnTypeFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_on_type_formatting(lang_server, params).await)
}

pub async fn handle_document_formatting(
    lang_server: &RubyLanguageServer,
    params: DocumentFormattingParams,
) -> LspResult<Option<Vec<TextEdit>>> {
    Ok(formatting::handle_document_formatting(lang_server, params).await)
}

pub async fn handle_folding_range(
    lang_server: &RubyLanguageServer,
    params: FoldingRangeParams,
) -> LspResult<Option<Vec<FoldingRange>>> {
    let uri = &params.text_document.uri;

    // Get the document from the language server
    match lang_server.get_doc(uri) {
        Some(document) => folding_range::handle_folding_range(&document, params).await,
        None => {
            debug!("Document not found for URI: {}", uri);
            Ok(None)
        }
    }
}

pub async fn handle_namespace_tree(
    lang_server: &RubyLanguageServer,
    params: namespace_tree::NamespaceTreeParams,
) -> LspResult<namespace_tree::NamespaceTreeResponse> {
    info!("Namespace tree request received");
    let start_time = std::time::Instant::now();
    let result = namespace_tree::handle_namespace_tree(lang_server, params).await;
    info!(
        "[PERF] Namespace tree completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

pub async fn handle_code_lens(
    lang_server: &RubyLanguageServer,
    params: CodeLensParams,
) -> LspResult<Option<Vec<CodeLens>>> {
    info!(
        "CodeLens request received for {:?}",
        params.text_document.uri.path()
    );
    let start_time = std::time::Instant::now();
    let result = code_lens::handle_code_lens(lang_server, params).await;
    info!("[PERF] CodeLens completed in {:?}", start_time.elapsed());
    Ok(result)
}

pub async fn handle_hover(
    lang_server: &RubyLanguageServer,
    params: HoverParams,
) -> LspResult<Option<Hover>> {
    Ok(hover::handle_hover(lang_server, params).await)
}

// ============================================================================
// Debug Handlers
// ============================================================================

pub async fn handle_list_commands(
    _lang_server: &RubyLanguageServer,
) -> LspResult<debug::ListCommandsResponse> {
    info!("List commands request received");
    Ok(debug::handle_list_commands())
}

pub async fn handle_debug_lookup(
    lang_server: &RubyLanguageServer,
    params: debug::LookupParams,
) -> LspResult<debug::LookupResponse> {
    info!("Debug lookup request received for: {}", params.fqn);
    Ok(debug::handle_lookup(lang_server, params))
}

pub async fn handle_debug_stats(
    lang_server: &RubyLanguageServer,
    params: debug::StatsParams,
) -> LspResult<debug::StatsResponse> {
    info!("Debug stats request received");
    Ok(debug::handle_stats(lang_server, params))
}

pub async fn handle_debug_ancestors(
    lang_server: &RubyLanguageServer,
    params: debug::AncestorsParams,
) -> LspResult<debug::AncestorsResponse> {
    info!("Debug ancestors request received for: {}", params.class);
    Ok(debug::handle_ancestors(lang_server, params))
}

pub async fn handle_debug_methods(
    lang_server: &RubyLanguageServer,
    params: debug::MethodsParams,
) -> LspResult<debug::MethodsResponse> {
    info!("Debug methods request received for: {}", params.class);
    Ok(debug::handle_methods(lang_server, params))
}

pub async fn handle_debug_inference_stats(
    lang_server: &RubyLanguageServer,
    params: debug::InferenceStatsParams,
) -> LspResult<debug::InferenceStatsResponse> {
    info!("Debug inference-stats request received");
    Ok(debug::handle_inference_stats(lang_server, params))
}

pub async fn handle_export_graph(
    lang_server: &RubyLanguageServer,
    params: debug::ExportGraphParams,
) -> LspResult<debug::ExportGraphResponse> {
    info!("Export graph request received");
    Ok(debug::handle_export_graph(lang_server, params))
}

pub async fn handle_extension_status(
    lang_server: &RubyLanguageServer,
    _params: ExtensionStatusParams,
) -> LspResult<ExtensionStatusResponse> {
    info!("Extension status request received");
    Ok(ExtensionStatusResponse {
        extensions: lang_server.extension_registry.status_reports(),
    })
}

// ============================================================================
// Type Hierarchy Handlers
// ============================================================================

pub async fn handle_prepare_type_hierarchy(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchyPrepareParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!(
        "Prepare type hierarchy request received for {:?}",
        params
            .text_document_position_params
            .text_document
            .uri
            .path()
    );
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_prepare_type_hierarchy(lang_server, params).await;
    info!(
        "[PERF] Prepare type hierarchy completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

pub async fn handle_supertypes(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchySupertypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Supertypes request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_supertypes(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Supertypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}

pub async fn handle_subtypes(
    lang_server: &RubyLanguageServer,
    params: TypeHierarchySubtypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Subtypes request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = type_hierarchy::handle_subtypes(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Subtypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}

pub async fn handle_prepare_call_hierarchy(
    lang_server: &RubyLanguageServer,
    params: CallHierarchyPrepareParams,
) -> LspResult<Option<Vec<CallHierarchyItem>>> {
    info!(
        "Prepare call hierarchy request received for {:?}",
        params
            .text_document_position_params
            .text_document
            .uri
            .path()
    );
    let start_time = std::time::Instant::now();
    let result = call_hierarchy::handle_prepare_call_hierarchy(lang_server, params).await;
    info!(
        "[PERF] Prepare call hierarchy completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

pub async fn handle_incoming_calls(
    lang_server: &RubyLanguageServer,
    params: CallHierarchyIncomingCallsParams,
) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
    info!("Incoming calls request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = call_hierarchy::handle_incoming_calls(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Incoming calls completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}

pub async fn handle_outgoing_calls(
    lang_server: &RubyLanguageServer,
    params: CallHierarchyOutgoingCallsParams,
) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
    info!("Outgoing calls request received for: {}", params.item.name);
    let start_time = std::time::Instant::now();
    let result = call_hierarchy::handle_outgoing_calls(lang_server, params).await;
    let count = result.as_ref().map(|v| v.len()).unwrap_or(0);
    info!(
        "[PERF] Outgoing calls completed in {:?}, returned {} items",
        start_time.elapsed(),
        count
    );
    Ok(result)
}

pub async fn handle_rename(
    lang_server: &RubyLanguageServer,
    params: RenameParams,
) -> LspResult<Option<WorkspaceEdit>> {
    info!(
        "Rename request received for: {:?}",
        params.text_document_position
    );
    let start_time = std::time::Instant::now();
    let result = rename::handle_rename(lang_server, params).await;
    info!("[PERF] Rename completed in {:?}", start_time.elapsed());
    Ok(result)
}

pub async fn handle_prepare_rename(
    lang_server: &RubyLanguageServer,
    params: TextDocumentPositionParams,
) -> LspResult<Option<PrepareRenameResponse>> {
    info!("Prepare rename request received for: {:?}", params);
    Ok(rename::handle_prepare_rename(lang_server, params).await)
}

#[cfg(test)]
mod navigation_demand_tests {
    use super::*;
    use crate::indexer::file_processor::FileProcessor;
    use crate::indexing_status::IndexingPhase;
    use crate::navigation_demand::NavigationDemandStage;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn early_definition_request_waits_for_its_exact_project_demand_and_retries() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("server");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();
        let caller_path = project.join("caller.rb");
        let caller_uri = Url::from_file_path(&caller_path).unwrap();
        let caller = "UserPmm.lookup\n";
        std::fs::write(&caller_path, caller).unwrap();

        let server = Arc::new(RubyLanguageServer::default());
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        let run = workspace.begin_indexing_run();
        workspace
            .indexing_status
            .transition(run.generation(), IndexingPhase::IndexingProject, None, None)
            .unwrap();
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: caller_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: caller.to_string(),
                },
            },
        )
        .await;

        let request_server = server.clone();
        let request_uri = caller_uri.clone();
        let request = tokio::spawn(async move {
            handle_goto_definition(
                &request_server,
                GotoDefinitionParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: request_uri },
                        position: Position::new(0, 2),
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                },
            )
            .await
        });

        let demanded_keys = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let keys = workspace
                    .navigation_demands
                    .drain(run.generation(), NavigationDemandStage::Project);
                if !keys.is_empty() {
                    break keys;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the early definition request must enqueue a bounded project demand");
        assert_eq!(demanded_keys, vec!["userpmm".to_string()]);

        let target_path = project.join("user_pmm.rb");
        let target_uri = Url::from_file_path(&target_path).unwrap();
        std::fs::write(&target_path, "class UserPmm\nend\n").unwrap();
        FileProcessor::with_extension_registry(server.extension_registry.clone())
            .process_file(&target_uri, "class UserPmm\nend\n", &server)
            .unwrap();
        workspace.navigation_demands.complete_keys(
            run.generation(),
            NavigationDemandStage::Project,
            &demanded_keys,
        );

        let response = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect("the request must retry immediately after exact demand completion")
            .unwrap()
            .unwrap()
            .expect("the retried definition must resolve");
        let GotoDefinitionResponse::Array(locations) = response else {
            panic!("expected an array definition response");
        };
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri, target_uri);
    }

    #[tokio::test]
    async fn dependency_demand_can_resolve_before_the_project_stage_completes() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("server");
        let gem_root = fixture.path().join("gems/bson-4.14.101-java");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(gem_root.join("lib/bson")).unwrap();
        std::fs::write(project.join("Gemfile"), "gem 'bson'\n").unwrap();
        let caller_path = project.join("caller.rb");
        let caller_uri = Url::from_file_path(&caller_path).unwrap();
        let caller = "BSON::ObjectId.new\n";
        std::fs::write(&caller_path, caller).unwrap();

        let server = Arc::new(RubyLanguageServer::default());
        let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
        let run = workspace.begin_indexing_run();
        workspace
            .indexing_status
            .transition(run.generation(), IndexingPhase::IndexingProject, None, None)
            .unwrap();
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: caller_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: caller.to_string(),
                },
            },
        )
        .await;

        let request_server = server.clone();
        let request_uri = caller_uri.clone();
        let request = tokio::spawn(async move {
            handle_goto_definition(
                &request_server,
                GotoDefinitionParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: request_uri },
                        position: Position::new(0, 7),
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                },
            )
            .await
        });

        let demanded_keys = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let keys = workspace
                    .navigation_demands
                    .drain(run.generation(), NavigationDemandStage::Dependency);
                if !keys.is_empty() {
                    break keys;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the early definition request must enqueue a bounded dependency demand");
        assert_eq!(demanded_keys, vec!["bson".to_string()]);

        let target_path = gem_root.join("lib/bson/object_id.rb");
        let target_uri = Url::from_file_path(&target_path).unwrap();
        let target = "module BSON\n  class ObjectId\n  end\nend\n";
        std::fs::write(&target_path, target).unwrap();
        FileProcessor::with_extension_registry(server.extension_registry.clone())
            .collect_file_facts_as_deferred_resolution_in_engine(
                &target_uri,
                target,
                workspace.analysis_engine.clone(),
                ruby_analysis::core::SourceKind::Gem,
            )
            .unwrap();
        workspace.analysis_engine.write().resolve();
        workspace.navigation_demands.complete_keys(
            run.generation(),
            NavigationDemandStage::Dependency,
            &demanded_keys,
        );

        let response = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .expect(
                "the request must use the completed dependency demand without waiting for the \
                 still-pending project demand",
            )
            .unwrap()
            .unwrap()
            .expect("the retried dependency definition must resolve");
        let GotoDefinitionResponse::Array(locations) = response else {
            panic!("expected an array definition response");
        };
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri, target_uri);
    }
}
