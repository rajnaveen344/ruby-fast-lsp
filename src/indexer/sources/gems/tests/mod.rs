//! Gem source discovery and indexing tests, split by behaviour.

use super::java::discover_locked_java_gem_roots;
use super::lockfile::{compare_versions, parse_gemfile_gem_statement, parse_locked_gems};
use super::products::GEM_PRODUCT_TRANSIENT_MEMORY_BYTES;
use super::vendor_cache::{
    cached_gem_project_digest, cached_gem_project_extraction_root, cached_gem_project_identity,
    CACHED_GEM_PROJECT_DIGEST_MARKER, CACHED_GEM_PROJECT_DIGEST_PREFIX_CHARS,
};
use super::*;
use crate::indexer::scheduling::resources::IndexingResourcePriority;
use crate::indexer::scheduling::resources::IndexingWorkSpec;
use crate::server::RubyLanguageServer;
use flate2::write::GzEncoder;
use flate2::Compression;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyMethod, RubyType, SourceKind};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, FileFacts, ResolveMode};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tar::{Builder, Header};
use tempfile::TempDir;

mod exact_sources;
mod java_platform;
mod locked_requirements;
mod runtime_discovery;
mod shared_products;

fn create_test_indexer() -> IndexerGem {
    let temp_dir = TempDir::new().unwrap();
    IndexerGem::new(Some(temp_dir.path().to_path_buf()))
}
