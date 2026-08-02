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

/// Stable library-section identity for Java Projects–style presentation.
///
/// Editors map these ids to labels (`Ruby Standard Library`, `Gems`, …).
/// Keep ids stable; do not encode display copy here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySectionId {
    /// Core stubs, runtime stdlib, signatures, and other runtime/external decls
    /// (JRE System Library analogue).
    Runtime,
    /// Bundler / RubyGems dependency sources (Maven Dependencies analogue).
    Gems,
    /// Policy-excluded workspace sources that are not project truth.
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryPackageTree {
    pub name: String,
    pub version: String,
    pub modules: Vec<NamespaceNode>,
    pub classes: Vec<NamespaceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryNamespaceTree {
    pub id: LibrarySectionId,
    /// Ungrouped roots (non-gem library sections, or gem files without package id).
    pub modules: Vec<NamespaceNode>,
    pub classes: Vec<NamespaceNode>,
    /// Maven-style per-gem folders under the Gems section.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<LibraryPackageTree>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceTreeResponse {
    pub modules: Vec<NamespaceNode>,
    pub classes: Vec<NamespaceNode>,
    /// Flat union of all library sections. Retained for callers that do not yet
    /// consume [`Self::libraries`]; prefer `libraries` for JRE/gems grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_modules: Vec<NamespaceNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_classes: Vec<NamespaceNode>,
    /// Per-project library sections (runtime / gems / excluded), ordered for
    /// Java Projects–like presentation. Empty when external types are hidden.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<LibraryNamespaceTree>,
}
