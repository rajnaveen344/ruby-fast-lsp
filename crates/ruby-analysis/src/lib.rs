//! Unified Ruby analysis API.
//!
//! This crate is the public analysis boundary. Internal modules are currently
//! backed by compatibility crates during the collapse from many small analysis
//! crates into one cohesive analysis package.

pub mod core {
    pub use ruby_analysis_core::*;
}

pub mod engine {
    pub use ruby_analysis_engine::*;
}

pub mod inference {
    pub use ruby_analysis_inference::*;
}

pub mod indexer {
    pub use ruby_analysis_indexer::*;
}

pub use core::*;
