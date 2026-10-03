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
