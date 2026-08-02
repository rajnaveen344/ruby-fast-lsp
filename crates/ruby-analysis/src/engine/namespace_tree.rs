use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    LibraryPackageId, SourceKind, TextRange,
};
use crate::engine::namespace_tree_types::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
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
        .all_graph_nodes()
        .into_iter()
        .filter(|node| {
            !node.fqn.has_generated_owner()
                && (show_external_types || analysis_range_is_project(engine, node.range))
        })
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
        .filter(|edge| {
            !edge.source.has_generated_owner()
                && !edge.target.has_generated_owner()
                && (show_external_types || analysis_range_is_project(engine, edge.range))
        })
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
    if !show_external_types {
        let project_tree = build_namespace_tree(collect_project_namespace_map(engine, false));
        return NamespaceTreeResponse {
            modules: project_tree.modules,
            classes: project_tree.classes,
            external_modules: Vec::new(),
            external_classes: Vec::new(),
            libraries: Vec::new(),
        };
    }

    let partitioned = partition_namespace_nodes(engine);
    // Project types keep Included By. Library/gem sections skip that BFS — it
    // dominated namespaceTree with show_external_types on large lockfiles.
    let project_tree = build_namespace_tree(build_namespace_map_from_grouped_nodes(
        engine,
        partitioned.project,
        true,
        true,
    ));
    let mut libraries = Vec::new();
    let runtime = build_namespace_tree(build_namespace_map_from_grouped_nodes(
        engine,
        partitioned.runtime,
        true,
        false,
    ));
    if !runtime.modules.is_empty() || !runtime.classes.is_empty() {
        libraries.push(LibraryNamespaceTree {
            id: LibrarySectionId::Runtime,
            modules: runtime.modules,
            classes: runtime.classes,
            packages: Vec::new(),
        });
    }

    let mut packages = partitioned
        .gem_packages
        .into_iter()
        .filter_map(|(package, nodes)| {
            let tree = build_namespace_tree(build_namespace_map_from_grouped_nodes(
                engine, nodes, true, false,
            ));
            if tree.modules.is_empty() && tree.classes.is_empty() {
                return None;
            }
            Some(LibraryPackageTree {
                name: package.name,
                version: package.version,
                modules: tree.modules,
                classes: tree.classes,
            })
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });
    let ungrouped = build_namespace_tree(build_namespace_map_from_grouped_nodes(
        engine,
        partitioned.gem_ungrouped,
        true,
        false,
    ));
    if !packages.is_empty() || !ungrouped.modules.is_empty() || !ungrouped.classes.is_empty() {
        libraries.push(LibraryNamespaceTree {
            id: LibrarySectionId::Gems,
            modules: ungrouped.modules,
            classes: ungrouped.classes,
            packages,
        });
    }

    let excluded = build_namespace_tree(build_namespace_map_from_grouped_nodes(
        engine,
        partitioned.excluded,
        true,
        false,
    ));
    if !excluded.modules.is_empty() || !excluded.classes.is_empty() {
        libraries.push(LibraryNamespaceTree {
            id: LibrarySectionId::Excluded,
            modules: excluded.modules,
            classes: excluded.classes,
            packages: Vec::new(),
        });
    }

    let (external_modules, external_classes) = flatten_library_namespaces(&libraries);
    NamespaceTreeResponse {
        modules: project_tree.modules,
        classes: project_tree.classes,
        external_modules,
        external_classes,
        libraries,
    }
}

struct PartitionedNamespaceNodes {
    project: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    runtime: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    excluded: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    gem_ungrouped: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    gem_packages: HashMap<LibraryPackageId, HashMap<FullyQualifiedName, Vec<GraphNodeFact>>>,
}

fn partition_namespace_nodes(engine: &AnalysisEngine) -> PartitionedNamespaceNodes {
    let mut partitioned = PartitionedNamespaceNodes {
        project: HashMap::new(),
        runtime: HashMap::new(),
        excluded: HashMap::new(),
        gem_ungrouped: HashMap::new(),
        gem_packages: HashMap::new(),
    };

    for node in engine.all_graph_nodes() {
        if node.fqn.has_generated_owner() {
            continue;
        }
        if node.fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton) {
            continue;
        }
        if analysis_range_is_project(engine, node.range) {
            partitioned
                .project
                .entry(node.fqn.clone())
                .or_default()
                .push(node);
            continue;
        }
        let Some(file) = engine.file(node.range.file_id) else {
            continue;
        };
        match source_kind_library_section(file.kind) {
            Some(LibrarySectionId::Runtime) => {
                partitioned
                    .runtime
                    .entry(node.fqn.clone())
                    .or_default()
                    .push(node);
            }
            Some(LibrarySectionId::Excluded) => {
                partitioned
                    .excluded
                    .entry(node.fqn.clone())
                    .or_default()
                    .push(node);
            }
            Some(LibrarySectionId::Gems) => match file.library_package.clone() {
                Some(package) => {
                    partitioned
                        .gem_packages
                        .entry(package)
                        .or_default()
                        .entry(node.fqn.clone())
                        .or_default()
                        .push(node);
                }
                None => {
                    partitioned
                        .gem_ungrouped
                        .entry(node.fqn.clone())
                        .or_default()
                        .push(node);
                }
            },
            None => {}
        }
    }

    partitioned
}

fn flatten_library_namespaces(
    libraries: &[LibraryNamespaceTree],
) -> (Vec<NamespaceNode>, Vec<NamespaceNode>) {
    let mut modules = Vec::new();
    let mut classes = Vec::new();
    for section in libraries {
        modules.extend(section.modules.iter().cloned());
        classes.extend(section.classes.iter().cloned());
        for package in &section.packages {
            modules.extend(package.modules.iter().cloned());
            classes.extend(package.classes.iter().cloned());
        }
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    (modules, classes)
}

fn source_kind_library_section(kind: SourceKind) -> Option<LibrarySectionId> {
    match kind {
        SourceKind::Project => None,
        SourceKind::Gem => Some(LibrarySectionId::Gems),
        SourceKind::Stub
        | SourceKind::Stdlib
        | SourceKind::External
        | SourceKind::Signature => Some(LibrarySectionId::Runtime),
        SourceKind::Excluded => Some(LibrarySectionId::Excluded),
    }
}

fn collect_project_namespace_map(
    engine: &AnalysisEngine,
    show_external_mixins: bool,
) -> HashMap<String, NamespaceNode> {
    let mut nodes_by_fqn: HashMap<FullyQualifiedName, Vec<GraphNodeFact>> = HashMap::new();

    for node in engine.all_graph_nodes() {
        if node.fqn.has_generated_owner() {
            continue;
        }
        if node.fqn.namespace_kind() == Some(crate::core::NamespaceKind::Singleton) {
            continue;
        }
        if !analysis_range_is_project(engine, node.range) {
            continue;
        }
        nodes_by_fqn.entry(node.fqn.clone()).or_default().push(node);
    }

    build_namespace_map_from_grouped_nodes(engine, nodes_by_fqn, show_external_mixins, true)
}

fn build_namespace_map_from_grouped_nodes(
    engine: &AnalysisEngine,
    nodes_by_fqn: HashMap<FullyQualifiedName, Vec<GraphNodeFact>>,
    show_external_mixins: bool,
    compute_included_by: bool,
) -> HashMap<String, NamespaceNode> {
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
            show_external_mixins,
        )
        .into_iter()
        .next();
        let includes = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Include),
            show_external_mixins,
        );
        let prepends = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Prepend),
            show_external_mixins,
        );
        let extends = analysis_edges_to_mixins(
            engine,
            &analysis_edges_from(engine, &fqn, GraphEdgeKind::Extend),
            show_external_mixins,
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

        let included_by = if compute_included_by && first_node.kind == GraphNodeKind::Module {
            analysis_find_includers(engine, &fqn, show_external_mixins)
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

    namespace_map
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
        for edge in engine.graph_edges_to(&target) {
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
                    queue.push_back((edge.source.clone(), next_via_modules));
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
        .is_some_and(|file| file.kind.is_workspace_owned())
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
    let (line, character) = file.byte_offset_to_line_character(range.start_byte)?;
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
        FullyQualifiedName, GeneratedOwnerId, GraphEdgeFact, GraphEdgeKind, GraphNodeFact,
        GraphNodeKind, LibraryPackageId, RubyConstant, SourceKind, TextRange,
    };
    use crate::engine::namespace_tree_types::LibrarySectionId;
    use crate::{FileFacts, ResolveMode, SourceFileInput};

    fn constant(name: &str) -> RubyConstant {
        RubyConstant::new(name).unwrap()
    }

    #[test]
    fn namespace_tree_filters_external_mixins() {
        let mut engine = AnalysisEngine::new();
        let user_file = engine.register_file(SourceFileInput {
            path: "/tmp/project/user.rb".into(),
            content: "class User; include Auth; end".into(),
            kind: SourceKind::Project,
        });
        let auth_file = engine.register_file(SourceFileInput {
            path: "/tmp/gems/auth.rb".into(),
            content: "module Auth; end".into(),
            kind: SourceKind::Gem,
        });
        let user = FullyQualifiedName::namespace(vec![constant("User")]);
        let auth = FullyQualifiedName::namespace(vec![constant("Auth")]);
        engine.replace_facts(
            user_file,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    user.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(user_file, 0, 10),
                )],
                graph_edges: vec![GraphEdgeFact::new(
                    user,
                    auth.clone(),
                    GraphEdgeKind::Include,
                    TextRange::new(user_file, 12, 24),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine.replace_facts(
            auth_file,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    auth,
                    GraphNodeKind::Module,
                    TextRange::new(auth_file, 0, 11),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );

        let query = AnalysisQuery::new(&engine);
        let project_only = query.namespace_tree(false);
        assert_eq!(project_only.modules.len(), 0);
        assert_eq!(project_only.classes.len(), 1);
        assert_eq!(project_only.classes[0].fqn, "User");
        assert_eq!(project_only.classes[0].includes.len(), 0);
        assert!(project_only.external_modules.is_empty());
        assert!(project_only.external_classes.is_empty());

        let with_external = query.namespace_tree(true);
        assert_eq!(with_external.modules.len(), 0);
        assert_eq!(with_external.classes.len(), 1);
        assert_eq!(with_external.classes[0].fqn, "User");
        assert_eq!(with_external.classes[0].includes[0].name, "Auth");
        assert_eq!(with_external.external_modules.len(), 1);
        assert_eq!(with_external.external_modules[0].fqn, "Auth");
        assert!(with_external.external_classes.is_empty());
        assert_eq!(with_external.libraries.len(), 1);
        assert_eq!(with_external.libraries[0].id, LibrarySectionId::Gems);
        assert_eq!(with_external.libraries[0].modules[0].fqn, "Auth");
        assert!(with_external.libraries[0].packages.is_empty());
    }

    #[test]
    fn namespace_tree_splits_runtime_and_gem_libraries() {
        let mut engine = AnalysisEngine::new();
        let user_file = engine.register_file(SourceFileInput {
            path: "/tmp/project/user.rb".into(),
            content: "class User; end".into(),
            kind: SourceKind::Project,
        });
        let string_file = engine.register_file(SourceFileInput {
            path: "/tmp/stubs/string.rb".into(),
            content: "class String; end".into(),
            kind: SourceKind::Stub,
        });
        let auth_file = engine.register_gem_file(
            SourceFileInput {
                path: "/tmp/gems/auth.rb".into(),
                content: "module Auth; end".into(),
                kind: SourceKind::Gem,
            },
            LibraryPackageId::new("auth", "1.0.0"),
        );
        let user = FullyQualifiedName::namespace(vec![constant("User")]);
        let string = FullyQualifiedName::namespace(vec![constant("String")]);
        let auth = FullyQualifiedName::namespace(vec![constant("Auth")]);
        engine.replace_facts(
            user_file,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    user,
                    GraphNodeKind::Class,
                    TextRange::new(user_file, 0, 10),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine.replace_facts(
            string_file,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    string,
                    GraphNodeKind::Class,
                    TextRange::new(string_file, 0, 12),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine.replace_facts(
            auth_file,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    auth,
                    GraphNodeKind::Module,
                    TextRange::new(auth_file, 0, 11),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );

        let tree = AnalysisQuery::new(&engine).namespace_tree(true);
        assert_eq!(tree.classes[0].fqn, "User");
        assert_eq!(tree.libraries.len(), 2);
        assert_eq!(tree.libraries[0].id, LibrarySectionId::Runtime);
        assert_eq!(tree.libraries[0].classes[0].fqn, "String");
        assert_eq!(tree.libraries[1].id, LibrarySectionId::Gems);
        assert!(tree.libraries[1].modules.is_empty());
        assert_eq!(tree.libraries[1].packages.len(), 1);
        assert_eq!(tree.libraries[1].packages[0].name, "auth");
        assert_eq!(tree.libraries[1].packages[0].version, "1.0.0");
        assert_eq!(tree.libraries[1].packages[0].modules[0].fqn, "Auth");
        assert_eq!(tree.external_classes[0].fqn, "String");
        assert_eq!(tree.external_modules[0].fqn, "Auth");
    }

    #[test]
    fn namespace_tree_shows_gem_reopen_of_stdlib_class_under_package() {
        let mut engine = AnalysisEngine::new();
        let stub_string = engine.register_file(SourceFileInput {
            path: "/tmp/stubs/string.rb".into(),
            content: "class String; end".into(),
            kind: SourceKind::Stub,
        });
        let as_string = engine.register_gem_file(
            SourceFileInput {
                path: "/tmp/gems/activesupport-7.1.0/lib/active_support/core_ext/string.rb"
                    .into(),
                content: "class String; def blank?; end; end".into(),
                kind: SourceKind::Gem,
            },
            LibraryPackageId::new("activesupport", "7.1.0"),
        );
        let string = FullyQualifiedName::namespace(vec![constant("String")]);
        engine.replace_facts(
            stub_string,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    string.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(stub_string, 0, 12),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine.replace_facts(
            as_string,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    string,
                    GraphNodeKind::Class,
                    TextRange::new(as_string, 0, 12),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );

        let tree = AnalysisQuery::new(&engine).namespace_tree(true);
        assert_eq!(tree.libraries.len(), 2);
        assert_eq!(tree.libraries[0].id, LibrarySectionId::Runtime);
        assert_eq!(tree.libraries[0].classes[0].fqn, "String");
        assert_eq!(tree.libraries[0].classes[0].locations.len(), 1);
        assert_eq!(tree.libraries[1].id, LibrarySectionId::Gems);
        assert_eq!(tree.libraries[1].packages.len(), 1);
        assert_eq!(tree.libraries[1].packages[0].name, "activesupport");
        assert_eq!(tree.libraries[1].packages[0].classes[0].fqn, "String");
        assert!(tree.libraries[1].packages[0].classes[0].locations[0]
            .uri
            .contains("activesupport"));
    }

    #[test]
    fn namespace_tree_nests_project_modules_by_fqn() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: "/tmp/project/platform.rb".into(),
            content: "module GoshPosh; module Platform; module API; end; end; end".into(),
            kind: SourceKind::Project,
        });
        let gosh = FullyQualifiedName::namespace(vec![constant("GoshPosh")]);
        let platform =
            FullyQualifiedName::namespace(vec![constant("GoshPosh"), constant("Platform")]);
        let api = FullyQualifiedName::namespace(vec![
            constant("GoshPosh"),
            constant("Platform"),
            constant("API"),
        ]);
        engine.replace_facts(
            file_id,
            FileFacts {
                graph_nodes: vec![
                    GraphNodeFact::new(
                        gosh,
                        GraphNodeKind::Module,
                        TextRange::new(file_id, 0, 8),
                    ),
                    GraphNodeFact::new(
                        platform,
                        GraphNodeKind::Module,
                        TextRange::new(file_id, 10, 18),
                    ),
                    GraphNodeFact::new(api, GraphNodeKind::Module, TextRange::new(file_id, 20, 23)),
                ],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );

        let tree = AnalysisQuery::new(&engine).namespace_tree(false);
        assert_eq!(tree.modules.len(), 1);
        assert_eq!(tree.modules[0].fqn, "GoshPosh");
        assert_eq!(tree.modules[0].modules.len(), 1);
        assert_eq!(tree.modules[0].modules[0].fqn, "GoshPosh::Platform");
        assert_eq!(tree.modules[0].modules[0].modules.len(), 1);
        assert_eq!(
            tree.modules[0].modules[0].modules[0].fqn,
            "GoshPosh::Platform::API"
        );
    }

    #[test]
    fn namespace_tree_hides_generated_semantic_owners() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: "/tmp/user_spec.rb".into(),
            content: "RSpec.describe User do; end".into(),
            kind: SourceKind::Project,
        });
        let generated = FullyQualifiedName::namespace(vec![RubyConstant::generated_owner(
            GeneratedOwnerId::new("rspec-ruby", "file:///tmp/user_spec.rb", "group:0:0")
                .expect("test generated owner identity must be valid"),
        )]);
        engine.replace_facts(
            file_id,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    generated,
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 10),
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        let query = AnalysisQuery::new(&engine);

        assert!(query.namespace_tree(false).classes.is_empty());
        assert!(query.namespace_tree(true).classes.is_empty());
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
