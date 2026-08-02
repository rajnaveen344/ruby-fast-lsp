//! Code Lens capability — thin adapter over the query layer.
//!
//! Handles server concerns (config check, document lookup) and converts
//! `CodeLensData` from the query layer into LSP `CodeLens` items.

use log::{debug, warn};
use tower_lsp::lsp_types::*;

use crate::query::{CodeLensData, EngineQuery};
use crate::server::RubyLanguageServer;

/// Handle CodeLens request for a document.
pub async fn handle_code_lens(
    lang_server: &RubyLanguageServer,
    params: CodeLensParams,
) -> Option<Vec<CodeLens>> {
    let uri = &params.text_document.uri;

    // 1. Config check (server concern).
    let modules_enabled = {
        let config = lang_server.config.lock();
        config.code_lens_modules_enabled.unwrap_or(true)
    };
    if !modules_enabled {
        return Some(Vec::new());
    }

    // 2. Get document content and Arc.
    let (content, doc_arc) = {
        let docs = lang_server.docs.lock();
        let doc_arc = match docs.get(uri) {
            Some(arc) => arc.clone(),
            None => {
                debug!("Document not found for URI: {}", uri);
                return Some(Vec::new());
            }
        };
        let doc = doc_arc.read();
        (doc.content.clone(), doc_arc.clone())
    };

    // 3. Create query with document context.
    let mut lenses: Vec<CodeLens> = {
        // EngineQuery owns read-side query context. Destroy it before awaiting
        // governed extension work so the LSP future remains Send.
        let query =
            EngineQuery::with_doc_and_engine(doc_arc, lang_server.analysis_engine_for_uri(uri));
        query
            .get_code_lenses(uri)
            .into_iter()
            .map(to_lsp_code_lens)
            .collect()
    };
    let project_root = lang_server
        .analysis_workspace_for_uri(uri)
        .map(|workspace| workspace.root_path);
    match lang_server
        .extension_registry
        .code_lenses_governed(
            lang_server.indexing_resources.clone(),
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
    Some(lenses)
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test(flavor = "current_thread")]
    async fn request_time_extension_code_lenses_wait_for_admission_without_blocking_reactor() {
        let uri = Url::parse("file:///tmp/governed_code_lenses.rb").expect("test URI must parse");
        let mut server = RubyLanguageServer::default();
        server.indexing_resources = crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                256 * 1024 * 1024,
                1,
            ),
        );
        server
            .extension_registry
            .configure_from_config(&crate::config::RubyFastLspConfig {
                extension_packages: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("extensions/rspec-ruby")
                    .to_string_lossy()
                    .into_owned()],
                ..crate::config::RubyFastLspConfig::default()
            });
        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: "class GovernedCodeLens\nend\n".to_string(),
                },
            },
        )
        .await;

        let holder_release = Arc::new(tokio::sync::Notify::new());
        let holder_release_task = holder_release.clone();
        let holder_governor = server.indexing_resources.clone();
        let holder = tokio::spawn(async move {
            holder_governor
                .run_async_with_resources(
                    "code lens contention holder",
                    crate::indexing_resources::IndexingWorkSpec::new(
                        None,
                        crate::indexing_resources::IndexingResourcePriority::Background,
                        1,
                        256 * 1024 * 1024,
                        1,
                    ),
                    None,
                    async move {
                        holder_release_task.notified().await;
                    },
                )
                .await
                .unwrap();
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().active_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("resource holder must be admitted before code-lens request");

        let request_server = server.clone();
        let request_uri = uri.clone();
        let request = tokio::spawn(async move {
            handle_code_lens(
                &request_server,
                CodeLensParams {
                    text_document: TextDocumentIdentifier { uri: request_uri },
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                },
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while server.indexing_resources.snapshot().queued_tasks != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("code-lens request must queue behind the complete weighted claim");
        tokio::time::timeout(
            Duration::from_millis(50),
            tokio::time::sleep(Duration::from_millis(1)),
        )
        .await
        .expect("queued code-lens request must not block the current-thread Tokio reactor");
        assert!(
            !request.is_finished(),
            "code-lens request must not bypass weighted admission"
        );

        holder_release.notify_one();
        holder.await.unwrap();
        request
            .await
            .unwrap()
            .expect("open document must return a code-lens response");
        let complete = server.indexing_resources.snapshot();
        assert_eq!(complete.active_tasks, 0);
        assert_eq!(complete.queued_tasks, 0);
        assert_eq!(complete.completed_tasks, 3);
    }
}
