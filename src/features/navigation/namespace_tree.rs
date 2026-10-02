//! Namespace tree: the class and module tree of one project engine, cached by
//! the engine's namespace hash.

use log::{debug, info};
use ruby_analysis::engine::NamespaceTreeResponse;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;

use crate::server::RubyLanguageServer;

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeParams {
    #[serde(default, rename = "uri", alias = "workspace_uri")]
    pub workspace_uri: Option<String>,
    #[serde(default)]
    pub show_external_types: bool,
}

/// Handle the custom `ruby/namespaceTree` request.
pub async fn handle(
    lang_server: &RubyLanguageServer,
    params: NamespaceTreeParams,
) -> LspResult<NamespaceTreeResponse> {
    info!("Namespace tree request received");
    let start_time = Instant::now();
    let response = namespace_tree(lang_server, params);
    info!(
        "[PERF] Namespace tree completed in {:?}",
        start_time.elapsed()
    );
    Ok(response)
}

fn namespace_tree(
    lang_server: &RubyLanguageServer,
    params: NamespaceTreeParams,
) -> NamespaceTreeResponse {
    debug!(
        "[NAMESPACE_TREE] Request received (show_external_types={})",
        params.show_external_types
    );
    let start_time = std::time::Instant::now();

    let request_uri = params
        .workspace_uri
        .as_deref()
        .filter(|uri| !uri.is_empty())
        .and_then(|uri| Url::parse(uri).ok());
    let project = request_uri
        .as_ref()
        .map(|uri| lang_server.project_for_uri(uri))
        .or_else(|| {
            let workspaces = lang_server.list_workspaces();
            (workspaces.len() == 1).then(|| workspaces[0].handle().clone())
        })
        .unwrap_or_else(|| lang_server.orphan_project().clone());
    let engine_hash = project.view(|view| view.namespace_tree_hash(params.show_external_types));
    let mut cache_hasher = std::collections::hash_map::DefaultHasher::new();
    request_uri
        .as_ref()
        .map(Url::as_str)
        .hash(&mut cache_hasher);
    engine_hash.hash(&mut cache_hasher);
    let combined_hash = cache_hasher.finish();

    if let Some(response) = lang_server.cached_namespace_tree(combined_hash) {
        debug!("[NAMESPACE_TREE] Cache hit in {:?}", start_time.elapsed());
        return response;
    }

    debug!("[NAMESPACE_TREE] Cache miss, computing namespace tree");
    let response = project.view(|view| view.namespace_tree(params.show_external_types));

    lang_server.cache_namespace_tree(combined_hash, response.clone());

    debug!("[NAMESPACE_TREE] Completed in {:?}", start_time.elapsed());
    response
}
