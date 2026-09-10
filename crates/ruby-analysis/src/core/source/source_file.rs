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
    /// RBS declaration source used for navigation and type facts, never Ruby diagnostics.
    Signature,
    /// Read-only external implementation source used for navigation. Semantic
    /// identity may come from a bounded native/runtime metadata provider rather
    /// than parsing this presentation document.
    External,
    /// Bundled language/runtime declarations.
    Stub,
    /// Ruby standard-library source.
    Stdlib,
    /// Dependency source discovered through Bundler or RubyGems.
    Gem,
}

/// Locked gem package identity for library-tree grouping (Maven-style).
///
/// Attached to `SourceKind::Gem` files at bind time from Bundler/GemInfo — never
/// guessed from filesystem path layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LibraryPackageId {
    pub name: String,
    pub version: String,
}

impl LibraryPackageId {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        let name = name.into();
        let version = version.into();
        assert!(
            !name.is_empty(),
            "INVARIANT VIOLATED: library package name is empty. \
             This is a bug because gem package identity requires a Bundler/GemInfo name. \
             Fix: pass gem_info.name when constructing LibraryPackageId."
        );
        assert!(
            !version.is_empty(),
            "INVARIANT VIOLATED: library package version is empty. \
             This is a bug because gem package identity requires a locked version. \
             Fix: pass gem_info.locked_version when constructing LibraryPackageId."
        );
        Self { name, version }
    }
}

impl SourceKind {
    /// Lower values are preferred for Go to Definition. Implementations and
    /// ordinary dependency sources outrank bundled declaration stubs, while
    /// signatures remain the final declaration-only fallback.
    pub fn definition_precedence(self) -> u8 {
        match self {
            SourceKind::Project
            | SourceKind::Excluded
            | SourceKind::External
            | SourceKind::Stdlib
            | SourceKind::Gem => 0,
            SourceKind::Stub => 1,
            SourceKind::Signature => 2,
        }
    }

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
            SourceKind::Signature
                | SourceKind::External
                | SourceKind::Stub
                | SourceKind::Stdlib
                | SourceKind::Gem
        )
    }
}
