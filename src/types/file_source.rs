/// Represents the origin/source of a file in the index.
///
/// This enum tracks where a file comes from, which is essential for:
/// - Filtering the namespace tree to show only project types
/// - Distinguishing user code from external dependencies
/// - Making intelligent decisions about what to index or display
///
/// **IMPORTANT**: File source is determined by proper discovery from tools
/// (bundler, rubygems, ruby), not by guessing from path patterns.
/// The indexing order matters:
/// 1. Gem indexer discovers gem paths (via bundler/rubygems) → `FileSource::Gem`
/// 2. Stdlib indexer discovers stdlib path (via ruby) → `FileSource::Stdlib`
/// 3. Stub indexer indexes bundled stubs → `FileSource::Stub`
/// 4. Project indexer indexes remaining workspace files → `FileSource::Project`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileSource {
    /// User's project code (workspace files not in gem/stdlib paths)
    Project,
    /// Core Ruby stubs bundled with the extension (type signatures)
    Stub,
    /// Ruby standard library files (csv.rb, json.rb, etc.)
    Stdlib,
    /// Installed gems (discovered via bundler/rubygems, includes vendor/cache)
    Gem,
}

impl FileSource {
    /// Returns true if this is a project file (user's code)
    pub fn is_project(&self) -> bool {
        matches!(self, FileSource::Project)
    }

    /// Returns true if this is an external type (stub, stdlib, gem)
    pub fn is_external(&self) -> bool {
        !self.is_project()
    }
}
