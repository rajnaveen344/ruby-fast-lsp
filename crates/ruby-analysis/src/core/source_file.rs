/// Origin of a source file known to the analysis engine.
///
/// Adapters decide this from their own discovery mechanism; the engine only
/// stores the fact for deterministic filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// Workspace-owned source selected by the project file policy.
    Project,
    /// Workspace source outside the project file policy, analyzed only while open.
    Excluded,
    /// Bundled language/runtime declarations.
    Stub,
    /// Ruby standard-library source.
    Stdlib,
    /// Dependency source discovered through Bundler or RubyGems.
    Gem,
}

impl SourceKind {
    pub fn is_workspace_owned(self) -> bool {
        matches!(self, SourceKind::Project)
    }

    pub fn is_editable(self) -> bool {
        self.is_workspace_owned()
    }

    pub fn contributes_project_diagnostics(self) -> bool {
        self.is_workspace_owned()
    }

    pub fn contributes_references(self) -> bool {
        matches!(self, SourceKind::Project | SourceKind::Excluded)
    }

    pub fn is_external(self) -> bool {
        !self.is_workspace_owned()
    }

    pub fn is_dependency_source(self) -> bool {
        matches!(
            self,
            SourceKind::Stub | SourceKind::Stdlib | SourceKind::Gem
        )
    }
}
