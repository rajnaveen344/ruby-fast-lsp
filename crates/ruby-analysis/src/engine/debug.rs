use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeKind, MethodFact, RubyConstant,
    RubyMethod, SymbolFact, SymbolKind, TextRange,
};
use crate::engine::namespace_tree::analysis_location_info;
use crate::engine::query::AnalysisQuery;
use crate::engine::query_types::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
use crate::engine::resolution::{method_lookup_chain, node_kind};
use crate::AnalysisEngine;

impl<'a> AnalysisQuery<'a> {
    pub fn debug_lookup(&self, fqn: &str) -> LookupResponse {
        let Some(fqn) = parse_debug_fqn(fqn) else {
            return LookupResponse {
                found: false,
                entries: Vec::new(),
            };
        };

        let mut entries = Vec::new();
        if let FullyQualifiedName::Method(_, _) = &fqn {
            entries.extend(
                self.engine
                    .method_facts_for(&fqn)
                    .iter()
                    .map(|fact| lookup_entry_from_method_fact(self.engine, fact)),
            );
        }

        if entries.is_empty() {
            for candidate in lookup_candidates(&fqn) {
                entries.extend(
                    self.engine
                        .symbol_facts_for(&candidate)
                        .iter()
                        .map(|fact| lookup_entry_from_symbol_fact(self.engine, fact)),
                );
            }
        }

        entries.sort_by_key(|entry| {
            (
                entry.fqn.clone(),
                entry.kind.clone(),
                entry.location.clone(),
            )
        });
        LookupResponse {
            found: !entries.is_empty(),
            entries,
        }
    }

    pub fn debug_stats(&self, indexing_complete: bool) -> StatsResponse {
        let symbols = self.engine.all_symbol_facts();
        let unique_definitions = symbols
            .iter()
            .map(|fact| fact.fqn.clone())
            .collect::<HashSet<_>>()
            .len();

        StatsResponse {
            total_definitions: unique_definitions,
            total_entries: symbols.len(),
            classes: count_symbols(&symbols, SymbolKind::Class),
            modules: count_symbols(&symbols, SymbolKind::Module),
            methods: count_symbols(&symbols, SymbolKind::Method),
            constants: count_symbols(&symbols, SymbolKind::Constant),
            instance_variables: count_symbols(&symbols, SymbolKind::InstanceVariable),
            files_indexed: self.engine.file_count(),
            indexing_complete,
        }
    }

    pub fn debug_ancestors(&self, class_name: &str) -> AncestorsResponse {
        let Some(namespace) = parse_namespace(class_name) else {
            return AncestorsResponse {
                class: class_name.to_string(),
                ancestors: Vec::new(),
            };
        };
        let fqn = FullyQualifiedName::namespace_with_kind(
            namespace,
            crate::core::NamespaceKind::Instance,
        );
        let ancestors = self
            .engine
            .graph_edges_from(&fqn)
            .iter()
            .map(|edge| AncestorEntry {
                name: fqn_to_key(&edge.target),
                kind: graph_edge_kind_label(edge.kind).to_string(),
            })
            .collect();

        AncestorsResponse {
            class: class_name.to_string(),
            ancestors,
        }
    }

    pub fn debug_methods(&self, class_name: &str) -> MethodsResponse {
        let Some(namespace) = parse_namespace(class_name) else {
            return MethodsResponse {
                class: class_name.to_string(),
                methods: Vec::new(),
            };
        };
        let mut methods = self
            .engine
            .all_method_facts()
            .into_iter()
            .filter(|fact| fact.owner.namespace_parts() == namespace)
            .map(|fact| MethodEntry {
                name: method_name(&fact),
                kind: format!(
                    "{:?}",
                    fact.owner
                        .namespace_kind()
                        .unwrap_or(crate::core::NamespaceKind::Instance)
                ),
                visibility: "Public".to_string(),
                return_type: self
                    .method_return_type(&fact)
                    .and_then(non_unknown_type_string),
            })
            .collect::<Vec<_>>();
        methods.sort_by_key(|method| (method.kind.clone(), method.name.clone()));

        MethodsResponse {
            class: class_name.to_string(),
            methods,
        }
    }

    pub fn debug_inference_stats(&self) -> InferenceStatsResponse {
        let methods = self.engine.all_method_facts();
        let total_methods = methods.len();
        let mut methods_with_return_type = 0usize;
        let mut file_method_counts: HashMap<String, usize> = HashMap::new();

        for fact in &methods {
            if self
                .method_return_type(fact)
                .and_then(non_unknown_type_string)
                .is_some()
            {
                methods_with_return_type += 1;
            }
            let file_name = self
                .engine
                .file(fact.range.file_id)
                .and_then(|file| {
                    file.path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "unknown".to_string());
            *file_method_counts.entry(file_name).or_insert(0) += 1;
        }

        let methods_without_return_type = total_methods - methods_with_return_type;
        let inference_coverage_percent = if total_methods > 0 {
            (methods_with_return_type as f64 / total_methods as f64) * 100.0
        } else {
            0.0
        };

        let mut file_counts = file_method_counts.into_iter().collect::<Vec<_>>();
        file_counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        let top_files_by_method_count = file_counts
            .into_iter()
            .take(10)
            .map(|(file, method_count)| FileMethodCount { file, method_count })
            .collect();

        InferenceStatsResponse {
            total_methods,
            methods_with_return_type,
            methods_without_return_type,
            inference_coverage_percent,
            top_files_by_method_count,
        }
    }

    pub fn debug_export_graph(&self) -> ExportGraphResponse {
        let nodes_by_fqn = graph_nodes_by_fqn(self.engine);
        let mut nodes = HashMap::new();

        for (fqn, kind) in &nodes_by_fqn {
            let outgoing = self.engine.graph_edges_from(fqn);
            let superclass = outgoing
                .iter()
                .find(|edge| edge.kind == GraphEdgeKind::Superclass)
                .map(|edge| fqn_to_key(&edge.target));
            let includes = edge_targets(outgoing, GraphEdgeKind::Include);
            let prepends = edge_targets(outgoing, GraphEdgeKind::Prepend);
            let included_by = reverse_edge_sources(self.engine, fqn, GraphEdgeKind::Include);
            let prepended_by = reverse_edge_sources(self.engine, fqn, GraphEdgeKind::Prepend);
            let children = reverse_edge_sources(self.engine, fqn, GraphEdgeKind::Superclass);
            let included_by_classes = if *kind == GraphNodeKind::Module {
                included_by_classes(self.engine, fqn)
            } else {
                Vec::new()
            };
            let mro = method_resolution_order(self.engine, fqn);

            nodes.insert(
                fqn_to_key(fqn),
                GraphNodeSnapshot {
                    kind: format!("{:?}", kind),
                    superclass,
                    includes,
                    prepends,
                    included_by,
                    prepended_by,
                    children,
                    included_by_classes,
                    mro,
                },
            );
        }

        ExportGraphResponse {
            node_count: nodes.len(),
            nodes,
        }
    }
}

fn lookup_candidates(fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName> {
    match fqn {
        FullyQualifiedName::Constant(parts) => vec![
            fqn.clone(),
            FullyQualifiedName::namespace_with_kind(
                parts.clone(),
                crate::core::NamespaceKind::Instance,
            ),
            FullyQualifiedName::namespace_with_kind(
                parts.clone(),
                crate::core::NamespaceKind::Singleton,
            ),
        ],
        _ => vec![fqn.clone()],
    }
}

fn lookup_entry_from_symbol_fact(engine: &AnalysisEngine, fact: &SymbolFact) -> LookupEntry {
    LookupEntry {
        fqn: fact.fqn.to_string(),
        kind: format!("{:?}", fact.kind),
        location: location_string(engine, fact.range),
        visibility: None,
        return_type: None,
        parameters: None,
    }
}

fn lookup_entry_from_method_fact(engine: &AnalysisEngine, fact: &MethodFact) -> LookupEntry {
    let query = AnalysisQuery::new(engine);
    LookupEntry {
        fqn: fact.fqn.to_string(),
        kind: format!(
            "Method({:?})",
            fact.owner
                .namespace_kind()
                .unwrap_or(crate::core::NamespaceKind::Instance)
        ),
        location: location_string(engine, fact.range),
        visibility: Some("Public".to_string()),
        return_type: query
            .method_return_type(fact)
            .and_then(non_unknown_type_string),
        parameters: if fact.params.is_empty() {
            None
        } else {
            Some(fact.params.clone())
        },
    }
}

fn count_symbols(symbols: &[SymbolFact], kind: SymbolKind) -> usize {
    symbols.iter().filter(|fact| fact.kind == kind).count()
}

fn non_unknown_type_string(ruby_type: crate::core::RubyType) -> Option<String> {
    if ruby_type == crate::core::RubyType::Unknown {
        None
    } else {
        Some(ruby_type.to_string())
    }
}

fn method_name(fact: &MethodFact) -> String {
    let FullyQualifiedName::Method(_, method) = &fact.fqn else {
        panic!(
            "INVARIANT VIOLATED: MethodFact FQN is not a method: {}. \
             This is a bug because MethodStore must only contain method FQNs. \
             Fix: validate MethodFact before insertion.",
            fact.fqn
        );
    };
    method.get_name().to_string()
}

fn location_string(engine: &AnalysisEngine, range: TextRange) -> String {
    analysis_location_info(engine, range)
        .map(|location| format!("{}:{}:{}", location.uri, location.line, location.character))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_debug_fqn(fqn_str: &str) -> Option<FullyQualifiedName> {
    if let Some(hash_pos) = fqn_str.find('#') {
        let namespace_str = &fqn_str[..hash_pos];
        let method_name = &fqn_str[hash_pos + 1..];
        let namespace = parse_namespace(namespace_str)?;
        let method = RubyMethod::new(method_name).ok()?;
        Some(FullyQualifiedName::method(namespace, method))
    } else if let Some(dot_pos) = fqn_str.rfind('.') {
        let before_dot = &fqn_str[..dot_pos];
        let after_dot = &fqn_str[dot_pos + 1..];
        if before_dot.contains("::")
            || before_dot
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
        {
            let namespace = parse_namespace(before_dot)?;
            let method = RubyMethod::new(after_dot).ok()?;
            Some(FullyQualifiedName::method(namespace, method))
        } else {
            Some(FullyQualifiedName::constant(parse_namespace(fqn_str)?))
        }
    } else {
        Some(FullyQualifiedName::constant(parse_namespace(fqn_str)?))
    }
}

fn parse_namespace(namespace_str: &str) -> Option<Vec<RubyConstant>> {
    let parts = namespace_str.split("::").collect::<Vec<_>>();
    let namespace = parts
        .iter()
        .filter_map(|part| RubyConstant::new(part.trim()).ok())
        .collect::<Vec<_>>();

    if namespace.len() == parts.len() {
        Some(namespace)
    } else {
        None
    }
}

fn fqn_to_key(fqn: &FullyQualifiedName) -> String {
    match fqn {
        FullyQualifiedName::Namespace(parts, crate::core::NamespaceKind::Instance) => parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        FullyQualifiedName::Namespace(parts, crate::core::NamespaceKind::Singleton) => {
            let name = parts
                .iter()
                .map(|part| part.to_string())
                .collect::<Vec<_>>()
                .join("::");
            format!("#<Class:{}>", name)
        }
        other => other.to_string(),
    }
}

fn graph_nodes_by_fqn(engine: &AnalysisEngine) -> HashMap<FullyQualifiedName, GraphNodeKind> {
    let mut nodes = HashMap::new();
    for node in engine.graph_store().all_nodes() {
        nodes.entry(node.fqn).or_insert(node.kind);
    }
    nodes
}

fn edge_targets(edges: &[GraphEdgeFact], kind: GraphEdgeKind) -> Vec<String> {
    edges
        .iter()
        .filter(|edge| edge.kind == kind)
        .map(|edge| fqn_to_key(&edge.target))
        .collect()
}

fn reverse_edge_sources(
    engine: &AnalysisEngine,
    target: &FullyQualifiedName,
    kind: GraphEdgeKind,
) -> Vec<String> {
    let mut result = engine
        .all_graph_edges()
        .into_iter()
        .filter(|edge| edge.kind == kind && edge.target == *target)
        .map(|edge| fqn_to_key(&edge.source))
        .collect::<Vec<_>>();
    result.sort();
    result
}

fn included_by_classes(engine: &AnalysisEngine, module_fqn: &FullyQualifiedName) -> Vec<String> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    for edge in engine.all_graph_edges() {
        if edge.target == *module_fqn
            && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
            && visited.insert(edge.source.clone())
        {
            queue.push_back(edge.source);
        }
    }

    while let Some(current) = queue.pop_front() {
        match node_kind(engine, &current) {
            Some(GraphNodeKind::Class) => result.push(fqn_to_key(&current)),
            Some(GraphNodeKind::Module) => {
                for edge in engine.all_graph_edges() {
                    if edge.target == current
                        && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                        && visited.insert(edge.source.clone())
                    {
                        queue.push_back(edge.source);
                    }
                }
            }
            None => {}
        }
    }

    result.sort();
    result
}

fn method_resolution_order(engine: &AnalysisEngine, fqn: &FullyQualifiedName) -> Vec<String> {
    method_lookup_chain(engine, fqn)
        .into_iter()
        .map(|item| fqn_to_key(&item))
        .collect()
}

fn graph_edge_kind_label(kind: GraphEdgeKind) -> &'static str {
    match kind {
        GraphEdgeKind::Superclass => "superclass",
        GraphEdgeKind::Include => "include",
        GraphEdgeKind::Extend => "extend",
        GraphEdgeKind::Prepend => "prepend",
    }
}
