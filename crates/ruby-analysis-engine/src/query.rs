use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::Path;

use ruby_analysis_core::{
    DiagnosticFact, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
    MethodCalleeResolution, MethodFact, ReferenceFact, ResolvedMethodCallee, RubyConstant,
    RubyMethod, SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact, TypeResolution,
    TypeSubject,
};

use crate::{AnalysisEngine, SourceFile};
use serde::{Deserialize, Serialize};

pub struct AnalysisQuery<'a> {
    engine: &'a AnalysisEngine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceSymbolMatch {
    pub name: String,
    pub kind: SymbolKind,
    pub range: TextRange,
    pub container_name: Option<String>,
    pub relevance: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyMethod {
    pub fqn: FullyQualifiedName,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingCall {
    pub from: CallHierarchyMethod,
    pub from_ranges: Vec<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingCall {
    pub to: CallHierarchyMethod,
    pub from_ranges: Vec<TextRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeHierarchyRelation {
    Superclass,
    Include,
    Prepend,
    Extend,
    Subclass,
    IncludedBy,
    PrependedBy,
    ExtendedBy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeHierarchyEntry {
    pub fqn: FullyQualifiedName,
    pub node_kind: Option<GraphNodeKind>,
    pub relation: TypeHierarchyRelation,
    pub range: TextRange,
    pub edge_file_id: Option<SourceFileId>,
    pub unresolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceNode {
    pub name: String,
    pub fqn: String,
    pub kind: String,
    pub locations: Vec<LocationInfo>,
    pub superclass: Option<MixinInfo>,
    pub includes: Vec<MixinInfo>,
    pub prepends: Vec<MixinInfo>,
    pub singleton_class: Option<Box<NamespaceNode>>,
    pub included_by: Vec<IncluderInfo>,
    pub modules: Vec<NamespaceNode>,
    pub classes: Vec<NamespaceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationInfo {
    pub uri: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MixinInfo {
    pub name: String,
    pub locations: Vec<LocationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViaModuleInfo {
    pub name: String,
    pub call_location: Option<LocationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncluderInfo {
    pub name: String,
    pub locations: Vec<LocationInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via_modules: Vec<ViaModuleInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceTreeResponse {
    pub modules: Vec<NamespaceNode>,
    pub classes: Vec<NamespaceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupEntry {
    pub fqn: String,
    pub kind: String,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupResponse {
    pub found: bool,
    pub entries: Vec<LookupEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_definitions: usize,
    pub total_entries: usize,
    pub classes: usize,
    pub modules: usize,
    pub methods: usize,
    pub constants: usize,
    pub instance_variables: usize,
    pub files_indexed: usize,
    pub indexing_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AncestorEntry {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AncestorsResponse {
    pub class: String,
    pub ancestors: Vec<AncestorEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodEntry {
    pub name: String,
    pub kind: String,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodsResponse {
    pub class: String,
    pub methods: Vec<MethodEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceStatsResponse {
    pub total_methods: usize,
    pub methods_with_return_type: usize,
    pub methods_without_return_type: usize,
    pub inference_coverage_percent: f64,
    pub top_files_by_method_count: Vec<FileMethodCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMethodCount {
    pub file: String,
    pub method_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNodeSnapshot {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superclass: Option<String>,
    pub includes: Vec<String>,
    pub prepends: Vec<String>,
    pub included_by: Vec<String>,
    pub prepended_by: Vec<String>,
    pub children: Vec<String>,
    pub included_by_classes: Vec<String>,
    pub mro: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportGraphResponse {
    pub node_count: usize,
    pub nodes: HashMap<String, GraphNodeSnapshot>,
}

struct NamespaceTreeResult {
    modules: Vec<NamespaceNode>,
    classes: Vec<NamespaceNode>,
}

impl<'a> AnalysisQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine) -> Self {
        Self { engine }
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.engine.file_id(path)
    }

    pub fn file(&self, file_id: SourceFileId) -> Option<&'a SourceFile> {
        self.engine.file(file_id)
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.engine.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.engine.type_store().facts_in_file(file_id)
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.engine.symbol_store().facts_in_file(file_id)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.engine.all_symbol_facts()
    }

    pub fn has_symbols(&self) -> bool {
        !self.engine.all_symbol_facts().is_empty()
    }

    pub fn top_level_symbols(&self, limit: usize) -> Vec<WorkspaceSymbolMatch> {
        let mut symbols = Vec::new();

        for fact in self.engine.all_symbol_facts() {
            if symbols.len() >= limit {
                break;
            }
            match &fact.fqn {
                FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
                    if parts.len() == 1 {
                        if let Some(symbol) = workspace_symbol_match(fact, 0.1) {
                            symbols.push(symbol);
                        }
                    }
                }
                FullyQualifiedName::Method(_, _)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => {}
            }
        }

        symbols
    }

    pub fn search_workspace_symbols(&self, query: &str, limit: usize) -> Vec<WorkspaceSymbolMatch> {
        let matcher = SymbolMatcher::new();
        let mut results = Vec::new();

        for fact in self.engine.all_symbol_facts() {
            let name = display_name(&fact.fqn);
            let match_name = match &fact.fqn {
                FullyQualifiedName::Method(_, method) => method.get_name(),
                FullyQualifiedName::Namespace(_, _)
                | FullyQualifiedName::Constant(_)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => name.clone(),
            };
            if let Some(relevance) = matcher.calculate_relevance(&match_name, query) {
                if let Some(symbol) = workspace_symbol_match(fact, relevance) {
                    results.push(symbol);
                }
            }
        }

        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    pub fn symbols_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [SymbolFact] {
        self.engine.symbol_facts_for(fqn)
    }

    pub fn references_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [ReferenceFact] {
        self.engine.reference_facts_for(fqn)
    }

    pub fn methods_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [MethodFact] {
        self.engine.method_facts_for(fqn)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.engine.method_store().facts_in_file(file_id)
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.engine.all_method_facts()
    }

    pub fn references_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        self.engine.reference_store().facts_in_file(file_id)
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> &'a [GraphNodeFact] {
        self.engine.graph_nodes_for(fqn)
    }

    pub fn graph_edges_from(&self, fqn: &FullyQualifiedName) -> &'a [GraphEdgeFact] {
        self.engine.graph_edges_from(fqn)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.engine.all_graph_edges()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.engine.diagnostic_facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.engine.all_diagnostic_facts()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.engine.graph_store().nodes_in_file(file_id)
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.engine.graph_store().edges_in_file(file_id)
    }

    pub fn resolve_method_callees(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        let fqns_to_search = if is_module_instance_namespace(self.engine, namespace_fqn) {
            let includers = module_includers(self.engine, namespace_fqn);
            if includers.is_empty() {
                vec![namespace_fqn.clone()]
            } else {
                includers
            }
        } else {
            vec![namespace_fqn.clone()]
        };

        let mut callees = Vec::new();
        for fqn in &fqns_to_search {
            let ancestor_chain = method_lookup_chain(self.engine, fqn);
            if let Some(callee) = method_callee_in_chain(
                self.engine,
                &ancestor_chain,
                method,
                MethodCalleeResolution::Exact,
            ) {
                callees.push(callee);
            }
        }

        if callees.is_empty() {
            return Some(
                fqns_to_search
                    .into_iter()
                    .map(|fqn| ResolvedMethodCallee {
                        owner: fqn,
                        method: *method,
                        resolution: MethodCalleeResolution::ReceiverOnly,
                        definition_ranges: Vec::new(),
                    })
                    .collect(),
            );
        }

        Some(callees)
    }

    pub fn parse_method_fqn(&self, fqn: &str) -> Option<FullyQualifiedName> {
        parse_method_fqn_string(fqn)
    }

    pub fn call_hierarchy_method(
        &self,
        method_fqn: &FullyQualifiedName,
    ) -> Option<CallHierarchyMethod> {
        let fact = self.engine.method_facts_for(method_fqn).first()?;
        Some(CallHierarchyMethod {
            fqn: method_fqn.clone(),
            range: fact.range,
        })
    }

    pub fn incoming_calls(&self, method_fqn: &FullyQualifiedName) -> Vec<IncomingCall> {
        let mut grouped: Vec<(FullyQualifiedName, Vec<TextRange>)> = Vec::new();
        for fact in self.engine.reference_facts_for(method_fqn) {
            let Some(caller) = &fact.caller else {
                continue;
            };
            push_grouped_text_range(&mut grouped, caller.clone(), fact.range);
        }

        grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
        grouped
            .into_iter()
            .filter_map(|(caller_fqn, from_ranges)| {
                Some(IncomingCall {
                    from: self.call_hierarchy_method(&caller_fqn)?,
                    from_ranges,
                })
            })
            .collect()
    }

    pub fn outgoing_calls(&self, method_fqn: &FullyQualifiedName) -> Vec<OutgoingCall> {
        let mut grouped: Vec<(FullyQualifiedName, Vec<TextRange>)> = Vec::new();
        for fact in self.engine.reference_store().all_facts() {
            if fact.caller.as_ref() != Some(method_fqn) {
                continue;
            }
            push_grouped_text_range(&mut grouped, fact.target, fact.range);
        }

        grouped.sort_by(|(left, _), (right, _)| left.to_string().cmp(&right.to_string()));
        grouped
            .into_iter()
            .filter_map(|(callee_fqn, from_ranges)| {
                Some(OutgoingCall {
                    to: self.call_hierarchy_method(&callee_fqn)?,
                    from_ranges,
                })
            })
            .collect()
    }

    pub fn method_reference_targets(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<FullyQualifiedName> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut targets = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for namespace in related_namespaces_for_method_references(self.engine, namespace_fqn) {
            for ancestor in method_lookup_chain(self.engine, &namespace) {
                let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
                if seen.insert(method_fqn.clone()) {
                    targets.push(method_fqn);
                }
            }
        }
        targets
    }

    pub fn method_completion_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return Vec::new();
        }

        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            for fact in self.engine.all_method_facts() {
                if fact.owner.namespace_parts() != ancestor.namespace_parts()
                    || fact.owner.namespace_kind() != ancestor.namespace_kind()
                {
                    continue;
                }
                let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                    continue;
                };
                let method_name = method.get_name();
                if !method_name.starts_with(partial) {
                    continue;
                }
                if seen.insert(method_name) {
                    facts.push(fact);
                }
            }
        }
        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    pub fn method_fact_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<MethodFact> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let mut facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .cloned()
                .collect::<Vec<_>>();

            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();

            match facts.len() {
                0 => continue,
                1 => return facts.pop(),
                _ => return None,
            }
        }

        None
    }

    pub fn method_return_type(&self, fact: &MethodFact) -> Option<ruby_analysis_core::RubyType> {
        match self.engine.type_at(
            &TypeSubject::MethodReturn(fact.fqn.clone()),
            fact.range.file_id,
            fact.range.end_byte,
        ) {
            TypeResolution::Resolved(type_fact) => Some(type_fact.ruby_type),
            TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
        }
    }

    pub fn method_return_type_for_receiver(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<ruby_analysis_core::RubyType> {
        if !namespace_target_exists(self.engine, namespace_fqn) {
            return None;
        }

        for ancestor in method_lookup_chain(self.engine, namespace_fqn) {
            let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
            let facts = self
                .engine
                .method_facts_for(&method_fqn)
                .iter()
                .filter(|fact| {
                    fact.owner.namespace_parts() == ancestor.namespace_parts()
                        && fact.owner.namespace_kind() == ancestor.namespace_kind()
                })
                .collect::<Vec<_>>();

            if facts.is_empty() {
                continue;
            }

            let mut return_types = facts
                .into_iter()
                .filter_map(|fact| self.method_return_type(fact))
                .collect::<Vec<_>>();

            if return_types.is_empty() {
                return None;
            }

            return_types.sort_by_key(|ruby_type| ruby_type.to_string());
            return_types.dedup();
            return match return_types.len() {
                1 => return_types.pop(),
                _ => Some(ruby_analysis_core::RubyType::union(return_types)),
            };
        }

        None
    }

    pub fn top_level_method_completion_facts(&self, partial: &str) -> Vec<MethodFact> {
        let mut facts = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for fact in self.engine.all_method_facts() {
            if !fact.owner.namespace_parts().is_empty() {
                continue;
            }

            let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                continue;
            };
            let method_name = method.get_name();
            if !method_name.starts_with(partial) {
                continue;
            }

            if seen.insert(method_name) {
                facts.push(fact);
            }
        }

        facts.sort_by_key(|fact| fact.fqn.to_string());
        facts
    }

    pub fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        let current_fqn = FullyQualifiedName::namespace_with_kind(
            current_namespace.to_vec(),
            ruby_analysis_core::NamespaceKind::Instance,
        );
        let resolved = resolve_constant_fqn(self.engine, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::constant(path.to_vec()));

        FullyQualifiedName::namespace_with_kind(
            resolved.namespace_parts(),
            ruby_analysis_core::NamespaceKind::Singleton,
        )
    }

    pub fn parse_namespace_fqn(&self, fqn: &str) -> Option<FullyQualifiedName> {
        parse_namespace_fqn_string(fqn)
    }

    pub fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let context_fqn = FullyQualifiedName::namespace(context.to_vec());
        resolve_constant_fqn(self.engine, parts, false, &context_fqn)
    }

    pub fn type_hierarchy_node(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<(GraphNodeKind, TextRange)> {
        let node = self.engine.graph_nodes_for(fqn).first()?;
        Some((node.kind, node.range))
    }

    pub fn supertypes(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyEntry> {
        let primary_file_id = match self.engine.graph_nodes_for(fqn).first() {
            Some(node) => node.range.file_id,
            None => return Vec::new(),
        };

        let edges = self.engine.graph_edges_from(fqn);
        let mut supertypes = Vec::new();
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Prepend,
            TypeHierarchyRelation::Prepend,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Include,
            TypeHierarchyRelation::Include,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Superclass,
            TypeHierarchyRelation::Superclass,
            &mut supertypes,
        );
        push_supertype_entries(
            self.engine,
            edges,
            GraphEdgeKind::Extend,
            TypeHierarchyRelation::Extend,
            &mut supertypes,
        );
        push_unresolved_supertype_entries(self.engine, fqn, &mut supertypes);

        for entry in &mut supertypes {
            if entry.edge_file_id == Some(primary_file_id) {
                entry.edge_file_id = None;
            }
        }

        supertypes
    }

    pub fn subtypes(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyEntry> {
        if self.engine.graph_nodes_for(fqn).is_empty() {
            return Vec::new();
        }

        let mut subclass_edges = Vec::new();
        let mut included_by_edges = Vec::new();
        let mut prepended_by_edges = Vec::new();
        let mut extended_by_edges = Vec::new();

        for edge in self.engine.all_graph_edges() {
            if &edge.target != fqn {
                continue;
            }
            match edge.kind {
                GraphEdgeKind::Superclass => subclass_edges.push(edge),
                GraphEdgeKind::Include
                    if edge.source.namespace_kind()
                        == Some(ruby_analysis_core::NamespaceKind::Instance) =>
                {
                    included_by_edges.push(edge)
                }
                GraphEdgeKind::Prepend
                    if edge.source.namespace_kind()
                        == Some(ruby_analysis_core::NamespaceKind::Instance) =>
                {
                    prepended_by_edges.push(edge)
                }
                GraphEdgeKind::Include | GraphEdgeKind::Prepend => {}
                GraphEdgeKind::Extend => extended_by_edges.push(edge),
            }
        }

        let mut subtypes = Vec::new();
        push_subtype_entries(
            self.engine,
            &mut subclass_edges,
            TypeHierarchyRelation::Subclass,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut included_by_edges,
            TypeHierarchyRelation::IncludedBy,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut prepended_by_edges,
            TypeHierarchyRelation::PrependedBy,
            &mut subtypes,
        );
        push_subtype_entries(
            self.engine,
            &mut extended_by_edges,
            TypeHierarchyRelation::ExtendedBy,
            &mut subtypes,
        );
        subtypes
    }

    pub fn implementor_namespaces(
        &self,
        origin_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        collect_all_implementors(self.engine, origin_fqn)
    }

    pub fn method_implementation_ranges(
        &self,
        owner_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<TextRange> {
        let namespaces_to_check = collect_all_implementors(self.engine, owner_fqn);
        let mut ranges = Vec::new();

        for ns_fqn in &namespaces_to_check {
            let method_fqn = FullyQualifiedName::method(ns_fqn.namespace_parts(), *method);
            for fact in self.engine.method_facts_for(&method_fqn) {
                if fact.owner.namespace_parts() == ns_fqn.namespace_parts()
                    && fact.owner.namespace_kind() == ns_fqn.namespace_kind()
                {
                    ranges.push(fact.range);
                }
            }
        }

        ranges
    }

    pub fn namespace_implementation_ranges(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        collect_all_implementors(self.engine, fqn)
            .iter()
            .filter_map(|impl_fqn| self.engine.graph_nodes_for(impl_fqn).first())
            .map(|fact| fact.range)
            .collect()
    }

    pub fn namespace_tree_hash(&self, show_external_types: bool) -> u64 {
        compute_namespace_tree_hash(self.engine, show_external_types)
    }

    pub fn namespace_tree(&self, show_external_types: bool) -> NamespaceTreeResponse {
        compute_namespace_tree(self.engine, show_external_types)
    }

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
            ruby_analysis_core::NamespaceKind::Instance,
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
                        .unwrap_or(ruby_analysis_core::NamespaceKind::Instance)
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
                ruby_analysis_core::NamespaceKind::Instance,
            ),
            FullyQualifiedName::namespace_with_kind(
                parts.clone(),
                ruby_analysis_core::NamespaceKind::Singleton,
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
                .unwrap_or(ruby_analysis_core::NamespaceKind::Instance)
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

fn non_unknown_type_string(ruby_type: ruby_analysis_core::RubyType) -> Option<String> {
    if ruby_type == ruby_analysis_core::RubyType::Unknown {
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
        FullyQualifiedName::Namespace(parts, ruby_analysis_core::NamespaceKind::Instance) => parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        FullyQualifiedName::Namespace(parts, ruby_analysis_core::NamespaceKind::Singleton) => {
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
        if node.fqn.namespace_kind() == Some(ruby_analysis_core::NamespaceKind::Singleton) {
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

fn analysis_location_info(engine: &AnalysisEngine, range: TextRange) -> Option<LocationInfo> {
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

fn workspace_symbol_match(fact: SymbolFact, relevance: f64) -> Option<WorkspaceSymbolMatch> {
    if matches!(fact.kind, SymbolKind::LocalVariable) {
        return None;
    }

    Some(WorkspaceSymbolMatch {
        name: display_name(&fact.fqn),
        kind: fact.kind,
        range: fact.range,
        container_name: container_name(&fact.fqn),
        relevance,
    })
}

fn display_name(fqn: &FullyQualifiedName) -> String {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => parts
            .last()
            .map(|part| part.to_string())
            .unwrap_or_default(),
        FullyQualifiedName::Method(_, method) => method.get_name(),
        FullyQualifiedName::LocalVariable(name)
        | FullyQualifiedName::InstanceVariable(name)
        | FullyQualifiedName::ClassVariable(name)
        | FullyQualifiedName::GlobalVariable(name) => name.to_string(),
    }
}

fn container_name(fqn: &FullyQualifiedName) -> Option<String> {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
            if parts.len() <= 1 {
                return None;
            }
            Some(
                parts[..parts.len() - 1]
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        FullyQualifiedName::Method(namespace, _) => {
            if namespace.is_empty() {
                return None;
            }
            Some(
                namespace
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        FullyQualifiedName::LocalVariable(_)
        | FullyQualifiedName::InstanceVariable(_)
        | FullyQualifiedName::ClassVariable(_)
        | FullyQualifiedName::GlobalVariable(_) => None,
    }
}

fn parse_method_fqn_string(fqn_str: &str) -> Option<FullyQualifiedName> {
    let (namespace_str, method_str) = fqn_str.rsplit_once('#')?;
    let method = RubyMethod::new(method_str).ok()?;
    let namespace = if namespace_str.is_empty() {
        Vec::new()
    } else {
        namespace_str
            .split("::")
            .map(RubyConstant::new)
            .collect::<Result<Vec<_>, _>>()
            .ok()?
    };
    Some(FullyQualifiedName::method(namespace, method))
}

fn parse_namespace_fqn_string(fqn: &str) -> Option<FullyQualifiedName> {
    let namespace = fqn
        .split("::")
        .map(RubyConstant::new)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if namespace.is_empty() {
        return None;
    }
    Some(FullyQualifiedName::namespace(namespace))
}

fn push_grouped_text_range(
    grouped: &mut Vec<(FullyQualifiedName, Vec<TextRange>)>,
    fqn: FullyQualifiedName,
    range: TextRange,
) {
    if let Some((_, ranges)) = grouped.iter_mut().find(|(existing, _)| *existing == fqn) {
        ranges.push(range);
        return;
    }
    grouped.push((fqn, vec![range]));
}

fn push_subtype_entries(
    engine: &crate::AnalysisEngine,
    edges: &mut [GraphEdgeFact],
    relation: TypeHierarchyRelation,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    edges.sort_by(|left, right| left.source.to_string().cmp(&right.source.to_string()));
    for edge in edges {
        if let Some(entry) = hierarchy_entry_for_node(engine, &edge.source, relation, None, false) {
            entries.push(entry);
        }
    }
}

fn push_supertype_entries(
    engine: &crate::AnalysisEngine,
    edges: &[GraphEdgeFact],
    kind: GraphEdgeKind,
    relation: TypeHierarchyRelation,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    edges
        .iter()
        .filter(|edge| edge.kind == kind)
        .rev()
        .filter_map(|edge| {
            hierarchy_entry_for_node(
                engine,
                &edge.target,
                relation,
                Some(edge.range.file_id),
                false,
            )
        })
        .for_each(|entry| entries.push(entry));
}

fn push_unresolved_supertype_entries(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    entries: &mut Vec<TypeHierarchyEntry>,
) {
    for edge in engine.unresolved_graph_edges() {
        if edge.source != *fqn {
            continue;
        }
        let relation = match edge.kind {
            GraphEdgeKind::Superclass => TypeHierarchyRelation::Superclass,
            GraphEdgeKind::Include => TypeHierarchyRelation::Include,
            GraphEdgeKind::Prepend => TypeHierarchyRelation::Prepend,
            GraphEdgeKind::Extend => TypeHierarchyRelation::Extend,
        };
        entries.push(TypeHierarchyEntry {
            fqn: FullyQualifiedName::constant(edge.target_parts.clone()),
            node_kind: None,
            relation,
            range: edge.range,
            edge_file_id: Some(edge.range.file_id),
            unresolved: true,
        });
    }
}

fn hierarchy_entry_for_node(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    relation: TypeHierarchyRelation,
    edge_file_id: Option<SourceFileId>,
    unresolved: bool,
) -> Option<TypeHierarchyEntry> {
    let node = engine.graph_nodes_for(fqn).first()?;
    Some(TypeHierarchyEntry {
        fqn: fqn.clone(),
        node_kind: Some(node.kind),
        relation,
        range: node.range,
        edge_file_id,
        unresolved,
    })
}

fn collect_all_implementors(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = vec![origin_fqn.clone()];

    visited.insert(origin_fqn.clone());

    while let Some(current) = queue.pop() {
        for descendant in descendants(engine, &current) {
            if visited.insert(descendant.clone()) {
                result.push(descendant);
            }
        }

        for mixer in mixers(engine, &current) {
            if visited.insert(mixer.clone()) {
                result.push(mixer.clone());
                queue.push(mixer);
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn mixers(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut mixers = engine
        .all_graph_edges()
        .into_iter()
        .filter(|edge| {
            edge.target == *origin_fqn
                && matches!(
                    edge.kind,
                    GraphEdgeKind::Include | GraphEdgeKind::Prepend | GraphEdgeKind::Extend
                )
                && (matches!(edge.kind, GraphEdgeKind::Extend)
                    || edge.source.namespace_kind()
                        == Some(ruby_analysis_core::NamespaceKind::Instance))
        })
        .map(|edge| edge.source)
        .collect::<Vec<_>>();
    mixers.sort_by_key(|fqn| fqn.to_string());
    mixers.dedup();
    mixers
}

struct SymbolMatcher;

impl SymbolMatcher {
    fn new() -> Self {
        Self
    }

    fn calculate_relevance(&self, symbol_name: &str, pattern: &str) -> Option<f64> {
        if pattern.is_empty() {
            return Some(0.1);
        }

        let symbol_lower = symbol_name.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        if symbol_name == pattern {
            return Some(1.0);
        }
        if symbol_lower == pattern_lower {
            return Some(0.9);
        }
        if symbol_lower.starts_with(&pattern_lower) {
            return Some(0.8);
        }
        if let Some(score) = self.camel_case_match(symbol_name, pattern) {
            return Some(score);
        }
        if let Some(score) = self.fuzzy_match(&symbol_lower, &pattern_lower) {
            return Some(score);
        }
        if self.word_boundary_match(&symbol_lower, &pattern_lower) {
            return Some(0.6);
        }
        if symbol_lower.contains(&pattern_lower) {
            return Some(0.4);
        }

        None
    }

    fn camel_case_match(&self, symbol_name: &str, pattern: &str) -> Option<f64> {
        let symbol_caps: String = symbol_name.chars().filter(|c| c.is_uppercase()).collect();
        let pattern_caps: String = pattern.chars().filter(|c| c.is_uppercase()).collect();

        if !pattern_caps.is_empty() && symbol_caps.starts_with(&pattern_caps) {
            Some(0.7)
        } else {
            None
        }
    }

    fn word_boundary_match(&self, symbol_lower: &str, pattern_lower: &str) -> bool {
        symbol_lower
            .split('_')
            .any(|word| word.starts_with(pattern_lower))
    }

    fn fuzzy_match(&self, symbol: &str, pattern: &str) -> Option<f64> {
        let symbol_chars: Vec<char> = symbol.chars().collect();
        let pattern_chars: Vec<char> = pattern.chars().collect();

        if pattern_chars.is_empty() {
            return Some(0.1);
        }
        if pattern_chars.len() > symbol_chars.len() {
            return None;
        }

        let mut pattern_idx = 0;
        let mut symbol_idx = 0;
        let mut matches = Vec::new();

        while pattern_idx < pattern_chars.len() && symbol_idx < symbol_chars.len() {
            if pattern_chars[pattern_idx] == symbol_chars[symbol_idx] {
                matches.push(symbol_idx);
                pattern_idx += 1;
            }
            symbol_idx += 1;
        }

        if pattern_idx < pattern_chars.len() {
            return None;
        }

        let score = self.calculate_fuzzy_score(&matches, symbol_chars.len(), pattern_chars.len());
        if score > 0.2 {
            Some(score)
        } else {
            None
        }
    }

    fn calculate_fuzzy_score(
        &self,
        matches: &[usize],
        symbol_len: usize,
        pattern_len: usize,
    ) -> f64 {
        if matches.is_empty() {
            return 0.0;
        }

        let coverage_score = pattern_len as f64 / symbol_len as f64;
        let mut consecutive_bonus = 0.0;
        let mut consecutive_count = 1;

        for i in 1..matches.len() {
            if matches[i] == matches[i - 1] + 1 {
                consecutive_count += 1;
            } else {
                if consecutive_count > 1 {
                    consecutive_bonus += (consecutive_count as f64 - 1.0) * 0.1;
                }
                consecutive_count = 1;
            }
        }

        if consecutive_count > 1 {
            consecutive_bonus += (consecutive_count as f64 - 1.0) * 0.1;
        }

        let early_match_bonus = if matches[0] == 0 { 0.2 } else { 0.0 };
        let mut gap_penalty = 0.0;
        for i in 1..matches.len() {
            let gap = matches[i] - matches[i - 1] - 1;
            gap_penalty += gap as f64 * 0.01;
        }

        let raw_score = coverage_score + consecutive_bonus + early_match_bonus - gap_penalty;
        (raw_score * 0.45 + 0.3).clamp(0.3, 0.75)
    }
}

fn namespace_target_exists(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    let parts = fqn.namespace_parts();
    if parts.is_empty() {
        return true;
    }
    let instance_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        ruby_analysis_core::NamespaceKind::Instance,
    );
    let singleton_fqn = FullyQualifiedName::namespace_with_kind(
        parts.clone(),
        ruby_analysis_core::NamespaceKind::Singleton,
    );
    let constant_fqn = FullyQualifiedName::constant(parts);

    !engine.graph_nodes_for(&instance_fqn).is_empty()
        || !engine.graph_nodes_for(&singleton_fqn).is_empty()
        || !engine.symbol_facts_for(&constant_fqn).is_empty()
}

fn is_module_instance_namespace(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> bool {
    if fqn.namespace_kind() != Some(ruby_analysis_core::NamespaceKind::Instance) {
        return false;
    }
    engine
        .graph_nodes_for(fqn)
        .iter()
        .any(|fact| fact.kind == GraphNodeKind::Module)
}

fn module_includers(
    engine: &crate::AnalysisEngine,
    module_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    for edge in engine.all_graph_edges() {
        if &edge.target == module_fqn
            && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
            && visited.insert(edge.source.clone())
        {
            queue.push_back(edge.source);
        }
    }

    while let Some(current) = queue.pop_front() {
        if node_kind(engine, &current) == Some(GraphNodeKind::Class) {
            result.push(current);
            continue;
        }

        if node_kind(engine, &current) == Some(GraphNodeKind::Module) {
            for edge in engine.all_graph_edges() {
                if edge.target == current
                    && matches!(edge.kind, GraphEdgeKind::Include | GraphEdgeKind::Prepend)
                    && visited.insert(edge.source.clone())
                {
                    queue.push_back(edge.source);
                }
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn related_namespaces_for_method_references(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    if seen.insert(origin_fqn.clone()) {
        queue.push_back(origin_fqn.clone());
    }

    while let Some(current) = queue.pop_front() {
        result.push(current.clone());

        for descendant in descendants(engine, &current) {
            if seen.insert(descendant.clone()) {
                queue.push_back(descendant);
            }
        }

        for includer in module_includers(engine, &current) {
            if seen.insert(includer.clone()) {
                queue.push_back(includer);
            }
        }
    }

    result.sort_by_key(|fqn| fqn.to_string());
    result
}

fn descendants(
    engine: &crate::AnalysisEngine,
    origin_fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(origin_fqn.clone());
    seen.insert(origin_fqn.clone());

    while let Some(current) = queue.pop_front() {
        for edge in engine.all_graph_edges() {
            if edge.kind == GraphEdgeKind::Superclass
                && edge.target == current
                && seen.insert(edge.source.clone())
            {
                result.push(edge.source.clone());
                queue.push_back(edge.source);
            }
        }
    }

    result
}

fn method_lookup_chain(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    assert!(
        matches!(fqn, FullyQualifiedName::Namespace(_, _)),
        "INVARIANT VIOLATED: analysis method lookup requested for non-namespace FQN: {fqn}. \
         This is a bug because only namespaces have method lookup chains. \
         Fix: resolve receivers to Namespace FQNs before method lookup."
    );

    if engine.graph_nodes_for(fqn).is_empty() {
        if fqn.namespace_parts().is_empty() {
            let mut chain = Vec::new();
            let mut visited = std::collections::HashSet::new();
            build_mro(engine, fqn, &mut chain, &mut visited);
            if chain.is_empty() {
                chain.push(fqn.clone());
            }
            return chain;
        }

        return vec![
            fqn.clone(),
            FullyQualifiedName::namespace_with_kind(
                Vec::new(),
                ruby_analysis_core::NamespaceKind::Instance,
            ),
        ];
    }

    let mut chain = Vec::new();
    let mut visited = std::collections::HashSet::new();
    build_mro(engine, fqn, &mut chain, &mut visited);

    let root = FullyQualifiedName::namespace_with_kind(
        Vec::new(),
        ruby_analysis_core::NamespaceKind::Instance,
    );
    if !chain.contains(&root) && !fqn.namespace_parts().is_empty() {
        chain.push(root);
    }

    chain
}

fn build_mro(
    engine: &crate::AnalysisEngine,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut std::collections::HashSet<FullyQualifiedName>,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    let mut prepends = edges_from(engine, fqn, GraphEdgeKind::Prepend);
    for edge in prepends.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    chain.push(fqn.clone());

    let mut includes = edges_from(engine, fqn, GraphEdgeKind::Include);
    for edge in includes.iter_mut().rev() {
        build_mro(engine, &edge.target, chain, visited);
    }

    if let Some(superclass) = edges_from(engine, fqn, GraphEdgeKind::Superclass).first() {
        build_mro(engine, &superclass.target, chain, visited);
    }
}

fn edges_from(
    engine: &crate::AnalysisEngine,
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

fn method_callee_in_chain(
    engine: &crate::AnalysisEngine,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    resolution: MethodCalleeResolution,
) -> Option<ResolvedMethodCallee> {
    for ancestor in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(ancestor.namespace_parts(), *method);
        let definition_ranges = engine
            .method_facts_for(&method_fqn)
            .iter()
            .filter(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                })
            })
            .map(|fact| fact.range)
            .collect::<Vec<_>>();

        if !definition_ranges.is_empty() {
            return Some(ResolvedMethodCallee {
                owner: ancestor.clone(),
                method: *method,
                resolution,
                definition_ranges,
            });
        }
    }

    None
}

fn resolve_constant_fqn(
    engine: &crate::AnalysisEngine,
    parts: &[RubyConstant],
    absolute: bool,
    context_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search_namespaces = if absolute {
        Vec::new()
    } else {
        context_fqn.namespace_parts()
    };

    loop {
        let mut probe = search_namespaces.clone();
        probe.extend(parts.iter().cloned());

        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            probe.clone(),
            ruby_analysis_core::NamespaceKind::Instance,
        );
        if !engine.graph_nodes_for(&namespace_fqn).is_empty() {
            return Some(namespace_fqn);
        }

        let constant_fqn = FullyQualifiedName::constant(probe);
        if !engine.symbol_facts_for(&constant_fqn).is_empty() {
            return Some(constant_fqn);
        }

        if absolute || search_namespaces.is_empty() {
            break;
        }
        search_namespaces.pop();
    }

    None
}

fn node_kind(engine: &crate::AnalysisEngine, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
    engine.graph_nodes_for(fqn).first().map(|fact| fact.kind)
}

#[cfg(test)]
mod tests {
    use ruby_analysis_core::{RubyConstant, SourceFileId, SymbolKind};

    use super::*;

    fn query_with_symbols() -> (AnalysisEngine, SourceFileId) {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("/tmp/user.rb", source);
        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::namespace(vec![user.clone()]),
            SymbolKind::Class,
            TextRange::new(file_id, 6, 10),
        ));
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::method(
                vec![user],
                RubyMethod::new("name").expect("test method must be valid"),
            ),
            SymbolKind::Method,
            TextRange::new(file_id, 17, 21),
        ));
        (engine, file_id)
    }

    #[test]
    fn workspace_symbol_search_returns_domain_matches() {
        let (engine, file_id) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        let symbols = query.search_workspace_symbols("name", 100);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::Method);
        assert_eq!(symbols[0].range.file_id, file_id);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_return_only_top_level_namespaces() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        let symbols = query.top_level_symbols(50);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
    }

    #[test]
    fn symbol_matcher_relevance() {
        let matcher = SymbolMatcher::new();

        assert_eq!(matcher.calculate_relevance("test", "test"), Some(1.0));
        assert_eq!(matcher.calculate_relevance("Test", "test"), Some(0.9));
        assert_eq!(matcher.calculate_relevance("testing", "test"), Some(0.8));
        assert_eq!(matcher.calculate_relevance("foo", "bar"), None);
    }

    #[test]
    fn fuzzy_matching() {
        let matcher = SymbolMatcher::new();

        let result = matcher.calculate_relevance("showthemeshelper", "showthemehelper");
        assert!(result.is_some());
        assert!(result.unwrap() > 0.3);

        assert!(matcher
            .calculate_relevance("ApplicationController", "AppCtrl")
            .is_some());
        assert!(matcher
            .calculate_relevance("user_authentication", "userauth")
            .is_some());
        assert!(matcher
            .calculate_relevance("get_user_by_id", "getuid")
            .is_some());

        assert!(matcher
            .calculate_relevance("completely_different", "xyz")
            .is_none());
        assert!(matcher
            .calculate_relevance("short", "verylongpattern")
            .is_none());
    }

    #[test]
    fn fuzzy_match_scoring() {
        let matcher = SymbolMatcher::new();

        let consecutive = matcher.fuzzy_match("abcdef", "abc").unwrap();
        let scattered = matcher.fuzzy_match("azbycx", "abc").unwrap();
        assert!(consecutive > scattered);

        let early = matcher.fuzzy_match("abcxyz", "abc").unwrap();
        let late = matcher.fuzzy_match("xyzabc", "abc").unwrap();
        assert!(early > late);
    }

    #[test]
    fn parse_method_fqn_strings() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        assert_eq!(
            query.parse_method_fqn("Foo#bar").unwrap().to_string(),
            "Foo#bar"
        );
        assert_eq!(
            query.parse_method_fqn("Foo::Bar#baz").unwrap().to_string(),
            "Foo::Bar#baz"
        );
        assert_eq!(query.parse_method_fqn("#foo").unwrap().to_string(), "#foo");
        assert!(query.parse_method_fqn("Foo::bar").is_none());
    }

    #[test]
    fn parse_namespace_fqn_strings() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        assert_eq!(
            query
                .parse_namespace_fqn("Foo::Bar::Baz")
                .unwrap()
                .to_string(),
            "Foo::Bar::Baz"
        );
        assert_eq!(
            query.parse_namespace_fqn("User").unwrap().to_string(),
            "User"
        );
        assert!(query.parse_namespace_fqn("").is_none());
    }
}
