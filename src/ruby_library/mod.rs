pub mod version;
pub mod version_detector;
pub mod version_managers;
pub mod path_discovery;

// Re-export main types for convenience
pub use version::RubyVersion;
pub use version_detector::RubyVersionDetector;
pub use version_managers::{VersionManager, VersionManagerRegistry};
pub use path_discovery::{PathDiscovery, DiscoveredPaths};