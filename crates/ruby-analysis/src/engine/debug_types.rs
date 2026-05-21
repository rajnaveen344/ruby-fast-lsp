use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
