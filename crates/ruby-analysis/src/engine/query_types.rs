use std::collections::HashMap;

use crate::core::{
    FullyQualifiedName, GraphNodeKind, RubyType, SourceFileId, SymbolKind, TextRange,
};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantCompletionRequest {
    pub partial_name: String,
    pub namespace_prefix: Option<FullyQualifiedName>,
    pub is_qualified: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantCompletionCandidate {
    pub fqn: FullyQualifiedName,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodCompletionCandidate {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<RubyType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MixinUsageKind {
    Include,
    Prepend,
    Extend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinUsage {
    pub kind: MixinUsageKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableTypeKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
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
