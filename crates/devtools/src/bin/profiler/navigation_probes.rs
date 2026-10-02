//! Live and post-indexing navigation and diagnostic probes at workspace positions.

use ruby_fast_lsp::features::cursor::EngineQuery;
use ruby_fast_lsp::features::navigation::{definition, references};
use ruby_fast_lsp::loader::scheduling::status;
use ruby_fast_lsp::lsp::lifecycle::indexing;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams,
    Position, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
};

use crate::cli::ReferenceProbe;

#[derive(Clone, Debug)]
pub(crate) struct PreparedDefinitionProbe {
    pub(crate) relative_path: PathBuf,
    pub(crate) uri: Url,
    position: Position,
}

pub(crate) async fn prepare_live_definition_probes(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<Vec<PreparedDefinitionProbe>> {
    let workspace_path = std::fs::canonicalize(workspace_path)?;
    let mut prepared = Vec::with_capacity(probes.len());
    let mut opened = std::collections::HashSet::new();
    for probe in probes {
        anyhow::ensure!(
            probe.path.is_relative(),
            "--definition-at must be workspace-relative: {}",
            probe.path.display()
        );
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(&workspace_path),
            "--definition-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid definition file path: {}", path.display()))?;
        if opened.insert(uri.clone()) {
            let content = fs::read_to_string(&path)?;
            indexing::handle_did_open(
                server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.clone(),
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: content,
                    },
                },
            )
            .await;
        }
        prepared.push(PreparedDefinitionProbe {
            relative_path: probe.path.clone(),
            uri,
            position: Position {
                line: probe.line,
                character: probe.character,
            },
        });
    }
    Ok(prepared)
}

pub(crate) async fn observe_first_live_definition(
    server: &RubyLanguageServer,
    probe: PreparedDefinitionProbe,
    indexing_started: Instant,
) -> serde_json::Value {
    let workspace = server.workspace_for_uri(&probe.uri).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "live definition probe {} has no owning project",
            why = "live navigation evidence requires one isolated engine",
            fix = "choose a project-owned file",
            probe.relative_path.display(),
        )
    });
    loop {
        let query_started = Instant::now();
        let locations = match definition::handle(
            server,
            GotoDefinitionParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier {
                        uri: probe.uri.clone(),
                    },
                    position: probe.position,
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
        {
            Ok(Some(GotoDefinitionResponse::Array(locations))) => locations,
            Ok(Some(GotoDefinitionResponse::Scalar(location))) => vec![location],
            Ok(Some(GotoDefinitionResponse::Link(links))) => links
                .into_iter()
                .map(|link| tower_lsp::lsp_types::Location {
                    uri: link.target_uri,
                    range: link.target_selection_range,
                })
                .collect(),
            Ok(None) | Err(_) => Vec::new(),
        };
        let query_elapsed = query_started.elapsed();
        let status = workspace.indexing_status.snapshot();
        if !locations.is_empty() {
            let engine = workspace.analysis_engine.read();
            let target_source_kinds = locations
                .iter()
                .map(|location| {
                    location
                        .uri
                        .to_file_path()
                        .ok()
                        .and_then(|path| engine.view().file_id(path))
                        .and_then(|file_id| engine.view().file(file_id))
                        .map(|file| format!("{:?}", file.kind))
                        .unwrap_or_else(|| "Unknown".to_string())
                })
                .collect::<Vec<_>>();
            invariant!(
                target_source_kinds.iter().all(|kind| kind != "Unknown"),
                what = "definition probe {} resolved outside its originating project engine",
                why = "staged navigation must keep semantic ownership",
                fix = "keep the originating engine for external locations",
                probe.relative_path.display(),
            );
            return serde_json::json!({
                "file": probe.relative_path,
                "line": probe.position.line,
                "character": probe.position.character,
                "first_success_elapsed_ms": u64::try_from(indexing_started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "query_elapsed_ns": u64::try_from(query_elapsed.as_nanos()).unwrap_or(u64::MAX),
                "project_root": workspace.root_path,
                "generation": status.generation,
                "sequence": status.sequence,
                "phase": status.phase,
                "target_source_kinds": target_source_kinds,
                "locations": locations,
            });
        }
        if matches!(
            status.phase,
            status::IndexingPhase::Ready
                | status::IndexingPhase::Failed
                | status::IndexingPhase::Cancelled
        ) {
            unreachable_invariant!(
                what = "definition probe {}:{}:{} unresolved when project {} reached phase {:?} (failure: {:?})",
                why = "readiness without a successful query is not evidence",
                fix = "repair the probe or the staged indexing lifecycle",
                probe.relative_path.display(),
                probe.position.line,
                probe.position.character,
                workspace.root_path.display(),
                status.phase,
                status.failure,
            );
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

pub(crate) async fn sample_references(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<()> {
    for probe in probes {
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--references-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid references file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let position = Position {
            line: probe.line,
            character: probe.character,
        };
        let started = Instant::now();
        let locations = references::find_references_at_position(server, &uri, position)
            .await
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "references_file": probe.path,
                "position": position,
                "elapsed_ns": u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                "count": locations.len(),
                "locations": locations,
            }))?
        );
    }
    Ok(())
}

pub(crate) async fn sample_definitions(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    probes: &[ReferenceProbe],
) -> anyhow::Result<()> {
    for probe in probes {
        let path = std::fs::canonicalize(workspace_path.join(&probe.path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--definition-at escapes the workspace: {}",
            probe.path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid definition file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let position = Position {
            line: probe.line,
            character: probe.character,
        };
        let started = Instant::now();
        let locations = definition::find_definition_at_position(server, uri.clone(), position)
            .await
            .map(definition::definition_locations)
            .unwrap_or_default();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "definition_file": probe.path,
                "position": position,
                "elapsed_ns": u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                "count": locations.len(),
                "locations": locations,
            }))?
        );
    }
    Ok(())
}

pub(crate) async fn sample_open_file_diagnostics(
    server: &RubyLanguageServer,
    workspace_path: &std::path::Path,
    relative_paths: &[PathBuf],
) -> anyhow::Result<()> {
    for relative_path in relative_paths {
        anyhow::ensure!(
            relative_path.is_relative(),
            "--diagnostics-file must be workspace-relative: {}",
            relative_path.display()
        );
        let path = std::fs::canonicalize(workspace_path.join(relative_path))?;
        anyhow::ensure!(
            path.starts_with(workspace_path),
            "--diagnostics-file escapes the workspace: {}",
            relative_path.display()
        );
        let uri = Url::from_file_path(&path)
            .map_err(|()| anyhow::anyhow!("invalid diagnostics file path: {}", path.display()))?;
        let content = fs::read_to_string(&path)?;
        indexing::handle_did_open(
            server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: content,
                },
            },
        )
        .await;
        let diagnostics = EngineQuery::with_engine(server.analysis_engine_for_uri(&uri))
            .get_unresolved_diagnostics(&uri);

        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "diagnostics_file": relative_path,
                "semantic_diagnostics": diagnostics,
            }))?
        );
    }
    Ok(())
}
