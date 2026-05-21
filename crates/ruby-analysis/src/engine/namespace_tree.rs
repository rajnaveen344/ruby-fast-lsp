use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, TextRange,
};
use crate::engine::namespace_tree_types::{
    IncluderInfo, LocationInfo, MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};
use crate::engine::query::AnalysisQuery;
use crate::AnalysisEngine;

struct NamespaceTreeResult {
    modules: Vec<NamespaceNode>,
    classes: Vec<NamespaceNode>,
}

impl<'a> AnalysisQuery<'a> {
    pub fn namespace_tree_hash(&self, show_external_types: bool) -> u64 {
        compute_namespace_tree_hash(self.engine, show_external_types)
    }

    pub fn namespace_tree(&self, show_external_types: bool) -> NamespaceTreeResponse {
        compute_namespace_tree(self.engine, show_external_types)
    }
}

fn compute_namespace_tree_hash(engine: &AnalysisEngine, show_external_types: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    show_external_types.hash(&mut hasher);

    let mut node_keys = engine
        .graph_store()
        .all_nodes()
        .into_iter()
        .filter(|node| show_external_types || analysis_range_is_project(engine, node.range))
        .map(|node| {
            (
                node.fqn.to_string(),
                node.kind,
                node.range.file_id,
                node.range.start_byte,
                node.range.end_byte,
            )
        })
        .collect::<Vec<_>>();
    node_keys.sort();
    node_keys.hash(&mut hasher);

    let mut edge_keys = engine
        .all_graph_edges()
        .into_iter()
        .filter(|edge| show_external_types || analysis_range_is_project(engine, edge.range))
        .map(|edge| {
            (
                edge.source.to_string(),
                edge.target.to_string(),
                edge.kind,
                edge.range.file_id,
                edge.range.start_byte,
                edge.range.end_byte,
            )
        })
        .collect::<Vec<_>>();
    edge_keys.sort();
    edge_keys.hash(&mut hasher);

    hasher.finish()
}

fn compute_namespace_tree(
    engine: &AnalysisEngine,
    show_external_types: bool,
) -> NamespaceTreeResponse {
    let mut nodes_by_fqn: HashMap<FullyQualifiedName, Vec<GraphNodeFact>> = HashMap::new();

    for node in engine.graph_store().all_nodes() {
        if node.fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton) {
            continue;
        }
        if !show_external_types && !analysis_range_is_project(engine, node.range) {
            continue;
        }
        nodes_by_fqn.entry(node.fqn.clone()).or_default().push(node);
    }

    let mut namespace_map = HashMap::new();
    for (fqn, mut nodes) in nodes_by_fqn {
        nodes.sort_by_key(|node| (node.kind, node.range.file_id, node.range.start_byte));
        let first_node = nodes.first().expect(
            "INVARIANT VIOLATED: namespace node bucket is empty. \
             This is a bug because only non-empty buckets are inserted. \
             Fix: keep namespace node grouping and iteration coupled.",
        );

        let fqn_string = fqn.to_string();
        let kind = match first_node.kind {
            GraphNodeKind::Class => "Class".to_string(),
            GraphNodeKind::Module => "Module".to_string(),
        };
        let locations = nodes
            .iter()
            .filter_map(|node| analysis_location_info(engine, node.range))
            .collect::<Vec<_>>();

        let superclass = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Superclass),
            show_external_types,
        )
        .into_iter()
        .next();
        let includes = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Include),
            show_external_types,
        );
        let prepends = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Prepend),
            show_external_types,
        );
        let extends = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Extend),
            show_external_types,
        );
        let singleton_class = if extends.is_empty() {
            None
        } else {
            let singleton_fqn = format!("#<Class:{}>", fqn_string);
            Some(Box::new(NamespaceNode {
                name: singleton_fqn.clone(),
                fqn: singleton_fqn,
                kind: "Singleton".to_string(),
                locations: Vec::new(),
                superclass: None,
                includes: extends,
                prepends: Vec::new(),
                singleton_class: None,
                included_by: Vec::new(),
                modules: Vec::new(),
                classes: Vec::new(),
            }))
        };

        let included_by = if first_node.kind == GraphNodeKind::Module {
            analysis_find_includers(engine, &fqn, show_external_types)
        } else {
            Vec::new()
        };

        namespace_map.insert(
            fqn_string.clone(),
            NamespaceNode {
                name: fqn.name().to_string(),
                fqn: fqn_string,
                kind,
                locations,
                superclass,
                includes,
                prepends,
                singleton_class,
                included_by,
                modules: Vec::new(),
                classes: Vec::new(),
            },
        );
    }

    let tree_result = build_namespace_tree(namespace_map);
    NamespaceTreeResponse {
        modules: tree_result.modules,
        classes: tree_result.classes,
    }
}

fn analysis_edges_from(
    engine: &AnalysisEngine,
    fqn: &FullyQualifiedName,
    kind: GraphEdgeKind,
) -> Vec<GraphEdgeFact> {
    engine
        .graph_edges_from(fqn)
        .iter()
        .filter(|edge| edge.kind == kind)
        .cloned()
        .collect()
}

fn analysis_edges_to_mixins(
    engine: &AnalysisEngine,
    edges: &[GraphEdgeFact],
    show_external_types: bool,
) -> Vec<MixinInfo> {
    let mut grouped: HashMap<String, Vec<LocationInfo>> = HashMap::new();

    for edge in edges {
        if !show_external_types && !analysis_namespace_is_project(engine, &edge.target) {
            continue;
        }
        grouped
            .entry(edge.target.to_string())
            .or_default()
            .extend(analysis_location_info(engine, edge.range));
    }

    let mut result = grouped
        .into_iter()
        .map(|(name, locations)| MixinInfo { name, locations })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn analysis_find_includers(
    engine: &AnalysisEngine,
    module_fqn: &FullyQualifiedName,
    show_external_types: bool,
) -> Vec<IncluderInfo> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((module_fqn.clone(), Vec::<ViaModuleInfo>::new()));

    while let Some((target, via_modules)) = queue.pop_front() {
        for edge in engine.all_graph_edges() {
            if edge.target != target {
                continue;
            }
            if !matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend) {
                continue;
            }
            if !visited.insert((edge.source.clone(), target.clone())) {
                continue;
            }

            match analysis_node_kind(engine, &edge.source) {
                Some(GraphNodeKind::Class) => {
                    if !show_external_types && !analysis_namespace_is_project(engine, &edge.source)
                    {
                        continue;
                    }
                    result.push(IncluderInfo {
                        name: edge.source.to_string(),
                        locations: analysis_namespace_locations(engine, &edge.source),
                        via_modules: via_modules.clone(),
                    });
                }
                Some(GraphNodeKind::Module) => {
                    let mut next_via_modules = via_modules.clone();
                    next_via_modules.push(ViaModuleInfo {
                        name: edge.source.to_string(),
                        call_location: analysis_location_info(engine, edge.range),
                    });
                    queue.push_back((edge.source, next_via_modules));
                }
                None => {}
            }
        }
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

fn analysis_node_kind(engine: &AnalysisEngine, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|node| node.kind)
}

fn analysis_namespace_is_project(engine: &AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    engine
        .graph_nodes_for(fqn)
        .iter()
        .any(|node| analysis_range_is_project(engine, node.range))
}

fn analysis_range_is_project(engine: &AnalysisEngine, range: TextRange) -> bool {
    engine
        .file(range.file_id)
        .is_some_and(|file| file.kind.is_project())
}

fn analysis_namespace_locations(
    engine: &AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<LocationInfo> {
    engine
        .graph_nodes_for(fqn)
        .iter()
        .filter_map(|node| analysis_location_info(engine, node.range))
        .collect()
}

pub(super) fn analysis_location_info(
    engine: &AnalysisEngine,
    range: TextRange,
) -> Option<LocationInfo> {
    let file = engine.file(range.file_id)?;
    let offset = usize::try_from(range.start_byte).ok()?;
    let prefix = file.source.get(..offset)?;
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let character = prefix[line_start..].chars().count() as u32;
    Some(LocationInfo {
        uri: file.path.to_string_lossy().to_string(),
        line,
        character,
    })
}

fn build_namespace_tree(namespace_map: HashMap<String, NamespaceNode>) -> NamespaceTreeResult {
    if namespace_map.is_empty() {
        return NamespaceTreeResult {
            modules: Vec::new(),
            classes: Vec::new(),
        };
    }

    let mut all_nodes: Vec<NamespaceNode> = namespace_map.into_values().collect();
    all_nodes.sort_by(|a, b| a.fqn.cmp(&b.fqn));

    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut node_lookup: HashMap<String, NamespaceNode> = HashMap::new();

    for node in all_nodes {
        let fqn = node.fqn.clone();
        if let Some(last_sep) = fqn.rfind("::") {
            let parent_fqn = fqn[..last_sep].to_string();
            children_map
                .entry(parent_fqn)
                .or_default()
                .push(fqn.clone());
        }
        node_lookup.insert(fqn, node);
    }

    let mut processed = HashSet::new();
    let mut root_modules = Vec::new();
    let mut root_classes = Vec::new();
    let all_fqns: Vec<String> = node_lookup.keys().cloned().collect();

    for fqn in all_fqns {
        if processed.contains(&fqn) {
            continue;
        }

        let is_root = if let Some(last_sep) = fqn.rfind("::") {
            let parent_fqn = fqn[..last_sep].to_string();
            !node_lookup.contains_key(&parent_fqn)
        } else {
            true
        };

        if is_root {
            if let Some(mut node) = node_lookup.remove(&fqn) {
                build_children_iterative(
                    &fqn,
                    &mut node,
                    &children_map,
                    &mut node_lookup,
                    &mut processed,
                );
                if node.kind == "Module" {
                    root_modules.push(node);
                } else {
                    root_classes.push(node);
                }
            }
        }
    }

    root_modules.sort_by(|a, b| a.name.cmp(&b.name));
    root_classes.sort_by(|a, b| a.name.cmp(&b.name));

    NamespaceTreeResult {
        modules: root_modules,
        classes: root_classes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
        RubyConstant, SourceKind, TextRange,
    };

    fn constant(name: &str) -> RubyConstant {
        RubyConstant::new(name).unwrap()
    }

    #[test]
    fn namespace_tree_filters_external_mixins() {
        let mut engine = AnalysisEngine::new();
        let user_file = engine.open_or_update_file_with_kind(
            "/tmp/project/user.rb",
            "class User; include Auth; end",
            SourceKind::Project,
        );
        let auth_file = engine.open_or_update_file_with_kind(
            "/tmp/gems/auth.rb",
            "module Auth; end",
            SourceKind::Gem,
        );
        let user = FullyQualifiedName::namespace(vec![constant("User")]);
        let auth = FullyQualifiedName::namespace(vec![constant("Auth")]);
        engine.add_graph_node_fact(GraphNodeFact::new(
            user.clone(),
            GraphNodeKind::Class,
            TextRange::new(user_file, 0, 10),
        ));
        engine.add_graph_node_fact(GraphNodeFact::new(
            auth.clone(),
            GraphNodeKind::Module,
            TextRange::new(auth_file, 0, 11),
        ));
        engine.add_graph_edge_fact(GraphEdgeFact::new(
            user,
            auth,
            GraphEdgeKind::Include,
            TextRange::new(user_file, 12, 24),
        ));

        let query = AnalysisQuery::new(&engine);
        let project_only = query.namespace_tree(false);
        assert_eq!(project_only.modules.len(), 0);
        assert_eq!(project_only.classes.len(), 1);
        assert_eq!(project_only.classes[0].fqn, "User");
        assert_eq!(project_only.classes[0].includes.len(), 0);

        let with_external = query.namespace_tree(true);
        assert_eq!(with_external.modules.len(), 1);
        assert_eq!(with_external.modules[0].fqn, "Auth");
        assert_eq!(with_external.classes[0].includes[0].name, "Auth");
    }
}

fn build_children_iterative(
    parent_fqn: &str,
    parent_node: &mut NamespaceNode,
    children_map: &HashMap<String, Vec<String>>,
    node_map: &mut HashMap<String, NamespaceNode>,
    processed: &mut HashSet<String>,
) {
    processed.insert(parent_fqn.to_string());

    if let Some(child_fqns) = children_map.get(parent_fqn) {
        let mut modules = Vec::new();
        let mut classes = Vec::new();

        for child_fqn in child_fqns {
            if let Some(mut child_node) = node_map.remove(child_fqn) {
                if !processed.contains(child_fqn) {
                    build_children_iterative(
                        child_fqn,
                        &mut child_node,
                        children_map,
                        node_map,
                        processed,
                    );
                }
                if child_node.kind == "Module" {
                    modules.push(child_node);
                } else {
                    classes.push(child_node);
                }
            }
        }

        modules.sort_by(|a, b| a.name.cmp(&b.name));
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        parent_node.modules = modules;
        parent_node.classes = classes;
    }
}
