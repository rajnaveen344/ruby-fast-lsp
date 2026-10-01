//! Source-family indexers.
//!
//! Each indexer discovers one family of Ruby inputs for an owning project and
//! delegates per-file fact collection to `FileProcessor`:
//!
//! - **`project`**: project files, project-root discovery, and dependency scan
//! - **`gems`**: locked and explicitly included gem discovery and binding
//! - **`stdlib`**: bundled core stubs and exact runtime standard library

pub mod gems;
pub mod project;
pub mod stdlib;
