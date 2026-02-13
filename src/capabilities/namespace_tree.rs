use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use log::debug;

// Re-export types for external consumers
pub use crate::query::namespace_tree::{
    IncluderInfo, LocationInfo, MixinInfo, NamespaceNode, NamespaceTreeParams,
    NamespaceTreeResponse, ViaModuleInfo,
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

    let query = IndexQuery::new(lang_server.index.clone());

    // Compute hash for cache check
    let index_hash = query.compute_namespace_tree_hash(params.show_external_types);

    // Check cache
    {
        let cache = lang_server.namespace_tree_cache.lock();
        if let Some((cached_hash, cached_response)) = cache.as_ref() {
            if *cached_hash == index_hash {
                debug!(
                    "[NAMESPACE_TREE] Cache hit in {:?}",
                    start_time.elapsed()
                );
                return cached_response.clone();
            }
        }
    }

    debug!("[NAMESPACE_TREE] Cache miss, computing namespace tree");
    let response = query.compute_namespace_tree(params.show_external_types);

    // Store in cache
    {
        let mut cache = lang_server.namespace_tree_cache.lock();
        *cache = Some((index_hash, response.clone()));
    }

    debug!("[NAMESPACE_TREE] Completed in {:?}", start_time.elapsed());
    response
}
