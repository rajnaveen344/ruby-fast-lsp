//! Gem Indexing
//!
//! This module handles gem discovery and indexing for the Ruby Language Server.
//! It supports both Bundler-based (Gemfile) and global gem discovery.

use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::loader::cache::dependency_product::{GemDependencyManifest, GemDependencyProduct};
use crate::loader::file_processor::FileProcessor;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

mod discovery;
mod java;
mod lockfile;
mod products;
mod vendor_cache;

pub use java::discover_locked_java_gem_roots;

// ============================================================================
// Types
// ============================================================================

/// Information about a discovered gem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemInfo {
    pub name: String,
    /// RubyGems semantic version without the platform suffix.
    pub version: String,
    /// RubyGems platform (`ruby`, `java`, `x86_64-linux`, and so on).
    pub platform: String,
    /// Exact version identity used by Gemfile.lock and cached archive names.
    pub locked_version: String,
    pub source: GemSource,
    pub path: PathBuf,
    pub lib_paths: Vec<PathBuf>,
    pub dependencies: Vec<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum GemSource {
    BundlerInstalled,
    GlobalInstalled,
    VendorGit,
    VendorArchive,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum LockedGemSource {
    Registry,
    Git,
    Path,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ActiveRubyEngine {
    JRuby,
    Other,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct LockedGemIdentity {
    name: String,
    locked_version: String,
    source: LockedGemSource,
    dependencies: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CachedGemMetadata {
    name: String,
    version: String,
    platform: String,
    locked_version: String,
    require_paths: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct GemDiscoveryRecord {
    name: String,
    version: String,
    platform: String,
    gem_dir: PathBuf,
    lib_dirs: Vec<PathBuf>,
    dependencies: Vec<String>,
    default_gem: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum GemDiscoveryStage {
    NotStarted,
    NavigationInputs,
    Complete,
}

// ============================================================================
// IndexerGem
// ============================================================================

/// Handles gem indexing for the Ruby Language Server.
/// Manages gem discovery, prioritization, and selective indexing.
pub struct IndexerGem {
    workspace_root: Option<PathBuf>,
    required_gems: HashSet<String>,
    explicitly_included_gems: HashSet<String>,
    excluded_gems: HashSet<String>,
    discovered_gems: HashMap<String, Vec<GemInfo>>,
    locked_gems: HashMap<String, LockedGemIdentity>,
    active_ruby_engine: ActiveRubyEngine,
    ruby_executable: Option<PathBuf>,
    java_home: Option<PathBuf>,
    cached_gem_root_override: Option<PathBuf>,
    file_processor: Option<FileProcessor>,
    /// The immutable core/runtime seed and its semantic identity, computed
    /// once because every gem manifest keys on it.
    dependency_seed: Option<(
        Arc<ruby_analysis::engine::Project>,
        ruby_analysis::engine::SemanticExportFingerprint,
    )>,
    runtime_provider_fingerprint: Option<String>,
    discovery_stage: GemDiscoveryStage,
}

pub(crate) struct LoadedGemDependencyProduct {
    manifest: GemDependencyManifest,
    product: Arc<GemDependencyProduct>,
}

impl IndexerGem {
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self {
            workspace_root,
            required_gems: HashSet::new(),
            explicitly_included_gems: HashSet::new(),
            excluded_gems: HashSet::new(),
            discovered_gems: HashMap::new(),
            locked_gems: HashMap::new(),
            active_ruby_engine: ActiveRubyEngine::Other,
            ruby_executable: None,
            java_home: None,
            cached_gem_root_override: None,
            file_processor: None,
            dependency_seed: None,
            runtime_provider_fingerprint: None,
            discovery_stage: GemDiscoveryStage::NotStarted,
        }
    }

    #[cfg(test)]
    fn set_cached_gem_root_for_test(&mut self, root: PathBuf) {
        self.cached_gem_root_override = Some(root);
    }

    /// Set the file processor for indexing
    pub fn set_file_processor(&mut self, file_processor: FileProcessor) {
        self.file_processor = Some(file_processor);
    }

    pub fn set_dependency_seed_engine(
        &mut self,
        dependency_seed_engine: Arc<ruby_analysis::engine::Project>,
    ) {
        let fingerprint = dependency_seed_engine.view().semantic_context_fingerprint();
        self.dependency_seed = Some((dependency_seed_engine, fingerprint));
    }

    pub fn set_runtime_provider_fingerprint(&mut self, fingerprint: Option<String>) {
        self.runtime_provider_fingerprint = fingerprint;
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Set the required gems for the project
    pub fn set_required_gems(&mut self, gems: HashSet<String>) {
        self.required_gems = gems;
        debug!(
            "Set {} required gems for indexing",
            self.required_gems.len()
        );
    }

    /// Allow explicitly configured gems to use the active Ruby's installed
    /// version even when the project lockfile does not contain that name.
    pub fn set_explicitly_included_gems(&mut self, gems: HashSet<String>) {
        self.explicitly_included_gems = gems;
    }

    /// Exclude gems even when they are required directly or transitively.
    pub fn set_excluded_gems(&mut self, gems: HashSet<String>) {
        self.excluded_gems = gems;
        debug!("Set {} excluded gems", self.excluded_gems.len());
    }

    pub fn set_selected_runtime(
        &mut self,
        executable: PathBuf,
        implementation: RuntimeImplementation,
        java_home: Option<PathBuf>,
    ) {
        invariant!(
            executable.is_absolute(),
            what = "selected Ruby executable is not absolute",
            why = "gem discovery must execute the exact runtime selected for one project",
            fix = "pass the validated canonical runtime descriptor executable",
        );
        self.ruby_executable = Some(executable);
        self.java_home = java_home;
        self.active_ruby_engine = match implementation {
            RuntimeImplementation::Jruby => ActiveRubyEngine::JRuby,
            RuntimeImplementation::Mri | RuntimeImplementation::Truffleruby => {
                ActiveRubyEngine::Other
            }
        };
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    pub fn get_gem(&self, name: &str) -> Option<&GemInfo> {
        self.discovered_gems
            .get(name)
            .and_then(|v| self.select_preferred_version(v))
    }

    pub fn has_gem(&self, name: &str) -> bool {
        self.get_gem(name).is_some()
    }

    pub fn gem_count(&self) -> usize {
        self.discovered_gems
            .values()
            .filter(|candidates| self.select_preferred_version(candidates).is_some())
            .count()
    }

    pub fn get_required_gems(&self) -> &HashSet<String> {
        &self.required_gems
    }

    pub(crate) fn ordered_required_gems_after_discovery(&self) -> Vec<String> {
        self.required_gems_with_dependencies()
    }

    pub fn get_gem_lib_paths(&self) -> Vec<PathBuf> {
        self.discovered_gems
            .values()
            .filter_map(|v| self.select_preferred_version(v))
            .flat_map(|g| g.lib_paths.iter().cloned())
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
