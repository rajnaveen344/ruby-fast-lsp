/// Origin of a source file known to the analysis engine.
///
/// Adapters decide this from their own discovery mechanism; the engine only
/// stores the fact for deterministic filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Project,
    Stub,
    Stdlib,
    Gem,
}

impl SourceKind {
    pub fn is_project(self) -> bool {
        matches!(self, SourceKind::Project)
    }

    pub fn is_external(self) -> bool {
        !self.is_project()
    }
}
