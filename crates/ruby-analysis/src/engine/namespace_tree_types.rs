use serde::{Deserialize, Serialize};

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
