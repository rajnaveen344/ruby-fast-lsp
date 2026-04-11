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

    // Build a tree from every workspace index plus the orphan index, then
    // merge into a single response. We compute the cache key by XOR'ing the
    // per-index hashes so changes in any workspace invalidate the cache.
    let indices = lang_server.all_indices();
    let mut combined_hash: u64 = 0;
    for idx in &indices {
        let q = IndexQuery::new(idx.clone());
        combined_hash ^= q.compute_namespace_tree_hash(params.show_external_types);
    }

    // Check cache
    {
        let cache = lang_server.namespace_tree_cache.lock();
        if let Some((cached_hash, cached_response)) = cache.as_ref() {
            if *cached_hash == combined_hash {
                debug!("[NAMESPACE_TREE] Cache hit in {:?}", start_time.elapsed());
                return cached_response.clone();
            }
        }
    }

    debug!("[NAMESPACE_TREE] Cache miss, computing namespace tree");
    let mut response = NamespaceTreeResponse {
        modules: Vec::new(),
        classes: Vec::new(),
    };
    for idx in indices {
        let q = IndexQuery::new(idx);
        let part = q.compute_namespace_tree(params.show_external_types);
        response.modules.extend(part.modules);
        response.classes.extend(part.classes);
    }

    // Store in cache
    {
        let mut cache = lang_server.namespace_tree_cache.lock();
        *cache = Some((combined_hash, response.clone()));
    }

    debug!("[NAMESPACE_TREE] Completed in {:?}", start_time.elapsed());
    response
}
