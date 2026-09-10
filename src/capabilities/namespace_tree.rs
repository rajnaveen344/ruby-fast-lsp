use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;
use log::debug;
use std::hash::{Hash, Hasher};
use tower_lsp::lsp_types::Url;

// Re-export types for external consumers
pub use crate::query::namespace_tree::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeParams, NamespaceTreeResponse, ViaModuleInfo,
};

pub async fn handle_namespace_tree(
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
    let analysis_engine = request_uri
        .as_ref()
        .map(|uri| lang_server.analysis_engine_for_uri(uri))
        .or_else(|| {
            let workspaces = lang_server.list_workspaces();
            (workspaces.len() == 1).then(|| workspaces[0].analysis_engine.clone())
        })
        .unwrap_or_else(|| lang_server.orphan_engine().clone());
    let query = EngineQuery::with_engine(analysis_engine);
    let engine_hash = query.compute_namespace_tree_hash(params.show_external_types);
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
    let response = query.compute_namespace_tree(params.show_external_types);

    lang_server.cache_namespace_tree(combined_hash, response.clone());

    debug!("[NAMESPACE_TREE] Completed in {:?}", start_time.elapsed());
    response
}
