//! Printed server statistics and stable per-project semantic manifests.

use log::info;
use ruby_analysis::core::{
    DiagnosticCandidate, DiagnosticFact, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    MethodFact, ReferenceCandidate, ReferenceFact, SourceKind, SymbolFact, TypeFact,
};
use ruby_analysis::engine::{AnalysisStat, ResolveStat};
use ruby_fast_lsp::server::RubyLanguageServer;
use sha2::{Digest, Sha256};

use crate::evidence::stable_fingerprint_hex;

pub(crate) fn print_semantic_export_manifest(server: &RubyLanguageServer) -> anyhow::Result<()> {
    let mut workspaces = server.list_workspaces();
    workspaces.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    for workspace in workspaces {
        let engine = workspace.analysis_engine.read();
        let result_fingerprints = engine
            .semantic_result_file_fingerprints()
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        let resolution_fingerprints = engine.semantic_resolution_file_fingerprints();
        let mut files = engine
            .files()
            .map(|file| {
                let path = if file.kind == SourceKind::Project {
                    file.path
                        .strip_prefix(&workspace.root_path)
                        .unwrap_or_else(|_| {
                            unreachable_invariant!(
                                what = "project semantic export source {} is outside owning root {}",
                                why = "project source ownership must remain workspace-contained",
                                fix = "route registration through the deepest owning project before exporting evidence",
                                file.path.display(),
                                workspace.root_path.display(),
                            )
                        })
                        .to_path_buf()
                } else {
                    file.path.clone()
                };
                let fingerprint = engine
                    .semantic_export_fingerprint(file.id)
                    .map(|fingerprint| stable_fingerprint_hex(fingerprint.stable_bytes()));
                let result_fingerprint = result_fingerprints.get(&file.id).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "semantic result manifest omitted registered file {}",
                        why = "result fingerprinting seeds one partition for every source",
                        fix = "keep source registration and semantic result partitioning aligned",
                        file.path.display(),
                    )
                });
                let [reference_fingerprint, context_fingerprint, local_read_fingerprint] =
                    resolution_fingerprints.get(&file.id).copied().unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "semantic resolution manifest omitted registered file {}",
                            why = "category fingerprinting seeds one partition for every source",
                            fix = "keep source registration and semantic resolution partitioning aligned",
                            file.path.display(),
                        )
                    });
                serde_json::json!({
                    "path": path,
                    "source_kind": format!("{:?}", file.kind),
                    "fingerprint_hex": fingerprint,
                    "result_fingerprint_hex": stable_fingerprint_hex(result_fingerprint.stable_bytes()),
                    "reference_fingerprint_hex": stable_fingerprint_hex(reference_fingerprint.stable_bytes()),
                    "context_fingerprint_hex": stable_fingerprint_hex(context_fingerprint.stable_bytes()),
                    "local_read_fingerprint_hex": stable_fingerprint_hex(local_read_fingerprint.stable_bytes()),
                })
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| {
            left["source_kind"]
                .as_str()
                .cmp(&right["source_kind"].as_str())
                .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
        });
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "semantic_export_manifest": {
                    "project": workspace.root_path,
                    "files": files,
                }
            }))?
        );
    }
    Ok(())
}

pub(crate) fn print_diagnostic_manifest(server: &RubyLanguageServer) -> anyhow::Result<()> {
    let mut workspaces = server.list_workspaces();
    workspaces.sort_by(|left, right| left.root_path.cmp(&right.root_path));
    for workspace in workspaces {
        let engine = workspace.analysis_engine.read();
        let query = engine.view();
        let mut diagnostics = query
            .all_diagnostic_facts()
            .into_iter()
            .map(|diagnostic| {
                let file = query.file(diagnostic.range.file_id).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "diagnostic {} references unknown file {:?}",
                        why = "diagnostic facts must belong to a registered source",
                        fix = "remove diagnostics through file replacement before unregistering",
                        diagnostic.code,
                        diagnostic.range.file_id,
                    )
                });
                let path = if file.kind == SourceKind::Project {
                    file.path
                        .strip_prefix(&workspace.root_path)
                        .unwrap_or_else(|_| {
                            unreachable_invariant!(
                                what = "project diagnostic source {} is outside owning root {}",
                                why = "project diagnostic ownership must remain workspace-contained",
                                fix = "route registration through the deepest owning project before exporting evidence",
                                file.path.display(),
                                workspace.root_path.display(),
                            )
                        })
                        .to_path_buf()
                } else {
                    file.path.clone()
                };
                let (start_line, start_character) = file
                    .byte_offset_to_line_character(diagnostic.range.start_byte)
                    .unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "diagnostic {} start byte {} is outside source {}",
                            why = "resolved facts must retain valid source ranges",
                            fix = "validate fact ranges before engine ingestion",
                            diagnostic.code,
                            diagnostic.range.start_byte,
                            file.path.display(),
                        )
                    });
                let (end_line, end_character) = file
                    .byte_offset_to_line_character(diagnostic.range.end_byte)
                    .unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "diagnostic {} end byte {} is outside source {}",
                            why = "resolved facts must retain valid source ranges",
                            fix = "validate fact ranges before engine ingestion",
                            diagnostic.code,
                            diagnostic.range.end_byte,
                            file.path.display(),
                        )
                    });
                serde_json::json!({
                    "path": path,
                    "source_kind": format!("{:?}", file.kind),
                    "start_byte": diagnostic.range.start_byte,
                    "end_byte": diagnostic.range.end_byte,
                    "start_line": start_line,
                    "start_character": start_character,
                    "end_line": end_line,
                    "end_character": end_character,
                    "severity": format!("{:?}", diagnostic.severity),
                    "code": diagnostic.code,
                    "message": diagnostic.message,
                })
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by(|left, right| {
            (
                left["source_kind"].as_str(),
                left["path"].as_str(),
                left["start_byte"].as_u64(),
                left["end_byte"].as_u64(),
                left["severity"].as_str(),
                left["code"].as_str(),
                left["message"].as_str(),
            )
                .cmp(&(
                    right["source_kind"].as_str(),
                    right["path"].as_str(),
                    right["start_byte"].as_u64(),
                    right["end_byte"].as_u64(),
                    right["severity"].as_str(),
                    right["code"].as_str(),
                    right["message"].as_str(),
                ))
        });
        let encoded = serde_json::to_vec(&diagnostics)?;
        let fingerprint_sha256 = format!("{:x}", Sha256::digest(&encoded));
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "diagnostic_manifest": {
                    "project": workspace.root_path,
                    "fingerprint_sha256": fingerprint_sha256,
                    "diagnostics": diagnostics,
                }
            }))?
        );
    }
    Ok(())
}

pub(crate) fn print_stats(server: &RubyLanguageServer) {
    info!("=== SHALLOW TYPE SIZES ===");
    info!(
        "FullyQualifiedName: {} bytes",
        std::mem::size_of::<FullyQualifiedName>()
    );
    info!("SymbolFact: {} bytes", std::mem::size_of::<SymbolFact>());
    info!("MethodFact: {} bytes", std::mem::size_of::<MethodFact>());
    info!(
        "ReferenceCandidate: {} bytes",
        std::mem::size_of::<ReferenceCandidate>()
    );
    for (name, bytes) in ruby_analysis::engine::reference_storage_sizes() {
        info!("{name}: {bytes} bytes");
    }
    info!(
        "ReferenceFact: {} bytes",
        std::mem::size_of::<ReferenceFact>()
    );
    info!("TypeFact: {} bytes", std::mem::size_of::<TypeFact>());
    info!(
        "DiagnosticCandidate: {} bytes",
        std::mem::size_of::<DiagnosticCandidate>()
    );
    info!(
        "DiagnosticFact: {} bytes",
        std::mem::size_of::<DiagnosticFact>()
    );
    info!(
        "GraphNodeFact: {} bytes",
        std::mem::size_of::<GraphNodeFact>()
    );
    info!(
        "GraphEdgeFact: {} bytes",
        std::mem::size_of::<GraphEdgeFact>()
    );

    for workspace in server.list_workspaces() {
        let engine = workspace.analysis_engine.read();
        let stats = engine.stats();
        info!("=== ANALYSIS STATS: {} ===", workspace.root_path.display());
        info!("Files: {}", stats.get(AnalysisStat::Files));
        info!(
            "Source bytes indexed: {}",
            stats.get(AnalysisStat::SourceBytes)
        );
        info!("Symbols: {}", stats.get(AnalysisStat::Symbols));
        info!("Methods: {}", stats.get(AnalysisStat::Methods));
        info!(
            "Reference candidates: {}",
            stats.get(AnalysisStat::ReferenceCandidates)
        );
        info!(
            "Reference candidates by kind: constants={}, methods={}, resolved={}",
            stats.get(AnalysisStat::ConstantReferenceCandidates),
            stats.get(AnalysisStat::MethodReferenceCandidates),
            stats.get(AnalysisStat::ResolvedReferenceCandidates)
        );
        let resolve_pass = engine.last_resolve_stats();
        info!(
            "Resolve pass ns: graph_retry={}, diagnostic_seed={}, constants={}, methods={}, sort_all={}, diagnostic_rebuild={}",
            resolve_pass.get(ResolveStat::GraphRetryNs),
            resolve_pass.get(ResolveStat::DiagnosticSeedNs),
            resolve_pass.get(ResolveStat::ConstantCandidatesNs),
            resolve_pass.get(ResolveStat::MethodCandidatesNs),
            resolve_pass.get(ResolveStat::SortAllNs),
            resolve_pass.get(ResolveStat::DiagnosticRebuildNs)
        );
        info!(
            "Resolve caches: constant hits/misses/unique={}/{}/{}, method hits/misses/unique={}/{}/{}, chain={}, namespace_exists={}, suggestion={}, incomplete_chain={}",
            resolve_pass.get(ResolveStat::ConstantCacheHits),
            resolve_pass.get(ResolveStat::ConstantCacheMisses),
            resolve_pass.get(ResolveStat::ConstantCacheUniqueKeys),
            resolve_pass.get(ResolveStat::MethodCacheHits),
            resolve_pass.get(ResolveStat::MethodCacheMisses),
            resolve_pass.get(ResolveStat::MethodCacheUniqueKeys),
            resolve_pass.get(ResolveStat::MethodLookupChainCacheEntries),
            resolve_pass.get(ResolveStat::MethodNamespaceExistsCacheEntries),
            resolve_pass.get(ResolveStat::MethodSuggestionCacheEntries),
            resolve_pass.get(ResolveStat::IncompleteMethodChainCacheEntries)
        );
        info!(
            "Deferred call receivers: candidates={}, proven={}, unknown={}",
            resolve_pass.get(ResolveStat::DeferredReceiverCandidates),
            resolve_pass.get(ResolveStat::DeferredReceiverProven),
            resolve_pass.get(ResolveStat::DeferredReceiverUnknown)
        );
        info!(
            "Call outcome caches: return hits/misses/entries={}/{}/{}, visibility hits/misses/entries={}/{}/{}, ambiguous return hits/misses/entries={}/{}/{}",
            resolve_pass.get(ResolveStat::MethodReturnCacheHits),
            resolve_pass.get(ResolveStat::MethodReturnCacheMisses),
            resolve_pass.get(ResolveStat::MethodReturnCacheEntries),
            resolve_pass.get(ResolveStat::MethodVisibilityCacheHits),
            resolve_pass.get(ResolveStat::MethodVisibilityCacheMisses),
            resolve_pass.get(ResolveStat::MethodVisibilityCacheEntries),
            resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheHits),
            resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheMisses),
            resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheEntries)
        );
        info!(
            "Resolved references: {}",
            stats.get(AnalysisStat::References)
        );
        info!("Type facts: {}", stats.get(AnalysisStat::Types));
        let inference = engine.inference_telemetry();
        info!(
            "Shape proof telemetry: occurrences={}, fields_total={}, fields_max={}, depth_max={}, unions={}, union_variants_total={}, union_variants_max={}, aliases_max={}, invalidated_unknowns={}, bound_unknowns={}",
            inference.retained_shape_occurrences,
            inference.retained_shape_fields,
            inference.max_retained_shape_fields,
            inference.max_retained_shape_depth,
            inference.retained_shape_unions,
            inference.retained_shape_union_variants,
            inference.max_retained_shape_union_variants,
            inference.max_live_shape_aliases,
            inference.shape_invalidated_outcomes,
            inference.shape_bound_exceeded_outcomes,
        );
        info!(
            "Diagnostic candidates: {}",
            stats.get(AnalysisStat::DiagnosticCandidates)
        );
        info!("Diagnostics: {}", stats.get(AnalysisStat::Diagnostics));
        info!("Graph nodes: {}", stats.get(AnalysisStat::GraphNodes));
        info!("Graph edges: {}", stats.get(AnalysisStat::GraphEdges));
        info!(
            "Unresolved graph edges: {}",
            stats.get(AnalysisStat::UnresolvedGraphEdges)
        );

        let memory = engine.estimated_memory_stats();
        let total = memory.total();
        info!(
            "=== ESTIMATED ENGINE HEAP: {} ===",
            workspace.root_path.display()
        );
        info!("Estimated total: {:.1} MB", bytes_to_mb(total));
        log_memory_bucket("names", memory.names, total);
        log_memory_bucket("files", memory.files, total);
        log_memory_bucket("symbols", memory.symbols, total);
        log_memory_bucket("methods", memory.methods, total);
        log_memory_bucket("types", memory.types, total);
        log_memory_bucket("reference candidates", memory.reference_candidates, total);
        log_memory_bucket("references", memory.references, total);
        log_memory_bucket("diagnostics", memory.diagnostics, total);
        log_memory_bucket("diagnostic candidates", memory.diagnostic_candidates, total);
        log_memory_bucket("graph", memory.graph, total);
        log_memory_bucket(
            "unresolved graph edges",
            memory.unresolved_graph_edges,
            total,
        );
    }
}

fn log_memory_bucket(name: &str, bytes: usize, total: usize) {
    let percent = if total == 0 {
        0.0
    } else {
        bytes as f64 * 100.0 / total as f64
    };
    info!("{name}: {:.1} MB ({percent:.1}%)", bytes_to_mb(bytes));
}

pub(crate) fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}
