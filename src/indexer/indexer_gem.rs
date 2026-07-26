//! Gem Indexing
//!
//! This module handles gem discovery and indexing for the Ruby Language Server.
//! It supports both Bundler-based (Gemfile) and global gem discovery.

use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::version::ruby_version::RubyImplementation;
use crate::server::RubyLanguageServer;
use crate::utils;
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use log::{debug, info, warn};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::EntryType;
use tower_lsp::lsp_types::Url;
use walkdir::WalkDir;

const MAX_CACHED_GEM_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CACHED_GEM_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHED_GEM_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CACHED_GEM_FILES: usize = 100_000;
const MAX_LOCKFILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_JAVA_GEM_SEARCH_ENTRIES: usize = 100_000;

pub fn discover_locked_java_gem_roots(
    project_root: &Path,
    jruby_executable: &Path,
    compatibility_version: &str,
) -> Result<Vec<PathBuf>> {
    let project_root = project_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize owning project root {}",
            project_root.display()
        )
    })?;
    if !project_root.is_dir() {
        return Err(anyhow!(
            "owning project root is not a directory: {}",
            project_root.display()
        ));
    }
    let jruby_executable = jruby_executable.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize selected JRuby executable {}",
            jruby_executable.display()
        )
    })?;
    if !jruby_executable.is_file() {
        return Err(anyhow!(
            "selected JRuby executable is not a file: {}",
            jruby_executable.display()
        ));
    }
    if compatibility_version.is_empty()
        || !compatibility_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return Err(anyhow!(
            "selected JRuby compatibility version `{compatibility_version}` is invalid"
        ));
    }

    let lockfile_path = project_root.join("Gemfile.lock");
    let metadata = match std::fs::metadata(&lockfile_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect owning project lockfile {}",
                    lockfile_path.display()
                )
            });
        }
    };
    if metadata.len() > MAX_LOCKFILE_BYTES {
        return Err(anyhow!(
            "owning project lockfile {} is {} bytes, exceeding the {}-byte limit",
            lockfile_path.display(),
            metadata.len(),
            MAX_LOCKFILE_BYTES
        ));
    }
    let lockfile = std::fs::read_to_string(&lockfile_path).with_context(|| {
        format!(
            "failed to read owning project lockfile {}",
            lockfile_path.display()
        )
    })?;

    let mut identities_by_name = HashMap::<String, Vec<LockedGemIdentity>>::new();
    for identity in parse_locked_gems(&lockfile)? {
        identities_by_name
            .entry(identity.name.clone())
            .or_default()
            .push(identity);
    }
    let mut selected = identities_by_name
        .into_iter()
        .map(|(name, identities)| {
            select_locked_identity_for_engine(&name, &identities, ActiveRubyEngine::JRuby)
        })
        .collect::<Result<Vec<_>>>()?;
    selected.retain(|identity| {
        identity.source == LockedGemSource::Registry && identity.locked_version.ends_with("-java")
    });
    selected.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.locked_version.cmp(&right.locked_version))
    });

    let jruby_home = jruby_executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            anyhow!(
                "selected JRuby executable {} has no runtime home",
                jruby_executable.display()
            )
        })?;
    let runtime_name = jruby_home
        .file_name()
        .ok_or_else(|| anyhow!("selected JRuby runtime home has no directory name"))?;
    let rvm_repository = jruby_home
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "rubies"))
        .and_then(Path::parent)
        .map(|rvm| rvm.join("gems").join(runtime_name).join("gems"));
    let runtime_repositories = [
        jruby_home.join("lib/ruby/gems/shared/gems"),
        jruby_home.join(format!("lib/ruby/gems/{compatibility_version}.0/gems")),
    ];

    let mut roots = Vec::new();
    for identity in selected {
        let exact_directory = format!("{}-{}", identity.name, identity.locked_version);
        let project_matches = find_project_java_gem_matches(&project_root, &exact_directory)?;
        if let Some(root) =
            select_unique_java_gem_match(&identity, "project vendor/bundle", project_matches)?
        {
            roots.push(root);
            continue;
        }

        let rvm_matches = rvm_repository
            .iter()
            .map(|repository| exact_java_gem_directory(repository, &exact_directory))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        if let Some(root) =
            select_unique_java_gem_match(&identity, "selected RVM runtime", rvm_matches)?
        {
            roots.push(root);
            continue;
        }

        let runtime_matches = runtime_repositories
            .iter()
            .map(|repository| exact_java_gem_directory(repository, &exact_directory))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        if let Some(root) =
            select_unique_java_gem_match(&identity, "selected JRuby runtime", runtime_matches)?
        {
            roots.push(root);
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn find_project_java_gem_matches(
    project_root: &Path,
    exact_directory: &str,
) -> Result<Vec<PathBuf>> {
    let vendor_bundle = project_root.join("vendor/bundle");
    if !vendor_bundle.is_dir() {
        return Ok(Vec::new());
    }
    let mut matches = Vec::new();
    let mut entries = 0usize;
    for entry in WalkDir::new(&vendor_bundle)
        .follow_links(false)
        .max_depth(8)
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect project-local gem repository {}",
                vendor_bundle.display()
            )
        })?;
        entries += 1;
        if entries > MAX_JAVA_GEM_SEARCH_ENTRIES {
            return Err(anyhow!(
                "project-local gem repository {} exceeds the {}-entry search limit",
                vendor_bundle.display(),
                MAX_JAVA_GEM_SEARCH_ENTRIES
            ));
        }
        if !entry.file_type().is_dir()
            || entry.file_name() != exact_directory
            || entry
                .path()
                .parent()
                .and_then(Path::file_name)
                .is_none_or(|name| name != "gems")
        {
            continue;
        }
        let canonical = entry.path().canonicalize().with_context(|| {
            format!(
                "failed to canonicalize project-local Java gem {}",
                entry.path().display()
            )
        })?;
        if !canonical.starts_with(project_root) {
            return Err(anyhow!(
                "project-local Java gem {} escapes owning project {}",
                canonical.display(),
                project_root.display()
            ));
        }
        matches.push(canonical);
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn exact_java_gem_directory(repository: &Path, exact_directory: &str) -> Result<Option<PathBuf>> {
    let candidate = repository.join(exact_directory);
    if !candidate.is_dir() {
        return Ok(None);
    }
    candidate
        .canonicalize()
        .map(Some)
        .with_context(|| format!("failed to canonicalize Java gem {}", candidate.display()))
}

fn select_unique_java_gem_match(
    identity: &LockedGemIdentity,
    tier: &str,
    mut matches: Vec<PathBuf>,
) -> Result<Option<PathBuf>> {
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(anyhow!(
            "locked Java gem `{}-{}` has ambiguous installations in {tier}: {matches:?}",
            identity.name,
            identity.locked_version
        )),
    }
}

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
    active_ruby_engine_override: Option<ActiveRubyEngine>,
    ruby_executable: Option<PathBuf>,
    java_home: Option<PathBuf>,
    cached_gem_root_override: Option<PathBuf>,
    gem_paths: Vec<PathBuf>,
    file_processor: Option<FileProcessor>,
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
            active_ruby_engine_override: None,
            ruby_executable: None,
            java_home: None,
            cached_gem_root_override: None,
            gem_paths: Vec::new(),
            file_processor: None,
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
        implementation: RubyImplementation,
        java_home: Option<PathBuf>,
    ) {
        assert!(
            executable.is_absolute(),
            "INVARIANT VIOLATED: selected Ruby executable is not absolute. This is a bug because \
             gem discovery must execute the exact runtime selected for one project. Fix: pass the \
             validated canonical runtime descriptor executable."
        );
        self.ruby_executable = Some(executable);
        self.java_home = java_home;
        self.active_ruby_engine_override = Some(match implementation {
            RubyImplementation::JRuby => ActiveRubyEngine::JRuby,
            RubyImplementation::Mri | RubyImplementation::TruffleRuby => ActiveRubyEngine::Other,
        });
    }

    /// Add a required gem to the project
    pub fn add_required_gem(&mut self, gem_name: String) {
        if self.required_gems.insert(gem_name.clone()) {
            debug!("Added required gem: {}", gem_name);
        }
    }

    // ========================================================================
    // Indexing
    // ========================================================================

    /// Index gems based on project requirements.
    /// If `selective` is true, only index required gems.
    /// If `selective` is false, index all discovered gems.
    pub async fn index_gems(
        &mut self,
        selective: bool,
        server: &RubyLanguageServer,
    ) -> Result<Vec<Url>> {
        info!("Starting gem indexing (selective: {})", selective);

        if selective && self.required_gems.is_empty() {
            info!("No required gems discovered; skipping gem discovery/indexing");
            return Ok(Vec::new());
        }

        self.discover_gems().await?;
        info!("Discovered {} gems", self.discovered_gems.len());
        let project_root = self.workspace_root.as_ref().expect(
            "INVARIANT VIOLATED: gem indexing has no Ruby project root. This is a bug because dependency facts must be owned by one isolated project engine. Fix: construct IndexerGem with the owning project root.",
        );
        let project_uri = Url::from_directory_path(project_root).map_err(|_| {
            anyhow!(
                "Ruby project root is not a valid file URI: {}",
                project_root.display()
            )
        })?;
        let analysis_engine = server.analysis_engine_for_uri(&project_uri);

        let indexed_files = if selective && !self.required_gems.is_empty() {
            self.index_required_gems(server, analysis_engine.clone())
                .await?
        } else {
            self.index_all_gems(server, analysis_engine.clone()).await?
        };

        if !indexed_files.is_empty() {
            analysis_engine.write().resolve();
        }

        info!("Indexed {} files from gems", indexed_files.len());
        Ok(indexed_files)
    }

    /// Index only the gems required by the project
    async fn index_required_gems(
        &self,
        server: &RubyLanguageServer,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<Vec<Url>> {
        let required_gems = self.required_gems_with_dependencies();
        let total = required_gems.len();
        let mut indexed_files = Vec::new();

        for (current, gem_name) in required_gems.iter().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Gems".to_string(),
                current + 1,
                total,
            )
            .await;

            if let Some(gem_versions) = self.discovered_gems.get(gem_name.as_str()) {
                if let Some(gem_info) = self.select_preferred_version(gem_versions) {
                    info!(
                        "Indexing required gem: {} v{} platform={} source={:?}",
                        gem_info.name, gem_info.version, gem_info.platform, gem_info.source
                    );
                    indexed_files.extend(self.index_gem_files(gem_info, analysis_engine.clone()));
                }
            } else {
                debug!("Required gem not found: {}", gem_name);
            }
        }

        Ok(indexed_files)
    }

    /// Index all discovered gems
    async fn index_all_gems(
        &self,
        server: &RubyLanguageServer,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<Vec<Url>> {
        let total = self.discovered_gems.len();
        let mut indexed_files = Vec::new();

        for (current, gem_versions) in self.discovered_gems.values().enumerate() {
            IndexingCoordinator::send_progress_report(
                server,
                "Indexing Gems".to_string(),
                current + 1,
                total,
            )
            .await;

            if let Some(gem_info) = self.select_preferred_version(gem_versions) {
                if self.excluded_gems.contains(&gem_info.name) {
                    continue;
                }
                info!(
                    "Indexing gem: {} v{} platform={} source={:?}",
                    gem_info.name, gem_info.version, gem_info.platform, gem_info.source
                );
                indexed_files.extend(self.index_gem_files(gem_info, analysis_engine.clone()));
            }
        }

        Ok(indexed_files)
    }

    /// Index all Ruby files from a gem's lib paths
    fn index_gem_files(
        &self,
        gem_info: &GemInfo,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Vec<Url> {
        let Some(processor) = &self.file_processor else {
            warn!(
                "No file processor set for gem indexer, skipping {}",
                gem_info.name
            );
            return Vec::new();
        };

        let mut indexed_files = Vec::new();

        for lib_path in &gem_info.lib_paths {
            if lib_path.exists() && lib_path.is_dir() {
                debug!("Indexing files from gem lib path: {:?}", lib_path);

                let ruby_files = utils::collect_ruby_files(lib_path);

                ruby_files.par_iter().for_each(|file_path| {
                    if let Ok(content) = std::fs::read_to_string(file_path) {
                        if let Ok(uri) = Url::from_file_path(file_path) {
                            if let Err(e) = processor
                                .collect_file_facts_as_deferred_resolution_in_engine(
                                    &uri,
                                    &content,
                                    analysis_engine.clone(),
                                    ruby_analysis::core::SourceKind::Gem,
                                )
                            {
                                warn!("Failed to index gem file {:?}: {}", file_path, e);
                            }
                        }
                    }
                });

                for file_path in &ruby_files {
                    if let Ok(uri) = Url::from_file_path(file_path) {
                        indexed_files.push(uri);
                    }
                }
            }
        }

        indexed_files
    }

    // ========================================================================
    // Discovery
    // ========================================================================

    /// Discover available gems in the system
    pub async fn discover_gems(&mut self) -> Result<usize> {
        debug!("Starting gem discovery process");

        self.discovered_gems.clear();
        self.locked_gems.clear();
        self.gem_paths.clear();

        self.detect_active_ruby_engine()?;
        self.load_locked_gems()?;
        self.discover_gem_paths()?;
        self.discover_installed_gems()?;
        self.discover_cached_git_gems()?;
        self.discover_cached_gem_archives()?;
        self.resolve_gem_lib_paths();

        info!("Discovered {} unique gems", self.discovered_gems.len());
        Ok(self.discovered_gems.len())
    }

    /// Get gem paths from Ruby's gem environment
    fn discover_gem_paths(&mut self) -> Result<()> {
        let output = self
            .ruby_command()
            .args(["-e", "require 'rubygems'; puts Gem.path.join('\n')"])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby command: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path.exists() && path.is_dir() {
                self.gem_paths.push(path.clone());
                debug!("Found gem path: {:?}", path);
            }
        }

        Ok(())
    }

    fn detect_active_ruby_engine(&mut self) -> Result<()> {
        if let Some(active) = self.active_ruby_engine_override {
            self.active_ruby_engine = active;
            return Ok(());
        }
        let output = self
            .ruby_command()
            .args(["-e", "print RUBY_ENGINE"])
            .output()
            .map_err(|error| anyhow!("Failed to detect active Ruby engine: {error}"))?;
        if !output.status.success() {
            return Err(anyhow!(
                "Active Ruby engine detection failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        self.active_ruby_engine = if String::from_utf8_lossy(&output.stdout).trim() == "jruby" {
            ActiveRubyEngine::JRuby
        } else {
            ActiveRubyEngine::Other
        };
        Ok(())
    }

    /// Discover all installed gems using the configured scope
    fn discover_installed_gems(&mut self) -> Result<()> {
        let scope = std::env::var("RUBY_LSP_GEM_SCOPE")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase();

        match scope.as_str() {
            "bundler" | "gemfile" => {
                info!("Gem indexing scope: Bundler/Gemfile only");
                self.discover_bundler_gems()
            }
            "global" => {
                info!("Gem indexing scope: Global gems only");
                self.discover_global_gems()
            }
            _ => {
                debug!("Gem indexing scope: Auto (Bundler with global fallback)");
                if self.discover_bundler_gems().is_ok() {
                    debug!("Using Bundler gems from Gemfile");
                    Ok(())
                } else {
                    debug!("Falling back to global gem discovery");
                    self.discover_global_gems()
                }
            }
        }
    }

    /// Discover gems using Bundler (Gemfile-based)
    fn discover_bundler_gems(&mut self) -> Result<()> {
        let gemfile = self.find_gemfile()?;

        let script = r#"
            require 'bundler'
            require 'json'
            begin
              Bundler.root
              gems = Bundler.load.specs.map do |spec|
                next if spec.name.nil? || spec.version.nil?
                {{
                  name: spec.name,
                  version: spec.version.to_s,
                  platform: spec.platform.to_s,
                  gem_dir: spec.gem_dir,
                  lib_dirs: spec.require_paths.map {{ |p| File.join(spec.gem_dir, p) }},
                  dependencies: spec.runtime_dependencies.map(&:name),
                  default_gem: spec.default_gem?
                }}
              end.compact
              puts JSON.generate(gems)
            rescue Bundler::GemfileNotFound
              exit 1
            end
        "#;

        let output = self
            .ruby_command()
            .env("BUNDLE_GEMFILE", &gemfile)
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow!("Failed to execute bundler gem discovery: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "No Gemfile found or bundler failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.process_gem_json(&output.stdout, "Bundler", GemSource::BundlerInstalled)
    }

    /// Discover all global gems
    fn discover_global_gems(&mut self) -> Result<()> {
        let script = r#"
            require 'rubygems'
            require 'json'
            gems = Gem::Specification.map do |spec|
              next if spec.name.nil? || spec.version.nil?
              {
                name: spec.name,
                version: spec.version.to_s,
                platform: spec.platform.to_s,
                gem_dir: spec.gem_dir,
                lib_dirs: spec.require_paths.map { |p| File.join(spec.gem_dir, p) },
                dependencies: spec.runtime_dependencies.map(&:name),
                default_gem: spec.default_gem?
              }
            end.compact
            puts JSON.generate(gems)
        "#;

        let output = self
            .ruby_command()
            .args(["-e", script])
            .output()
            .map_err(|e| anyhow!("Failed to execute ruby gem discovery: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "Ruby gem discovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        self.process_gem_json(&output.stdout, "Global", GemSource::GlobalInstalled)
    }

    fn load_locked_gems(&mut self) -> Result<()> {
        self.locked_gems.clear();
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let lockfile_path = root.join("Gemfile.lock");
        let content = match std::fs::read_to_string(&lockfile_path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read owning project lockfile {}",
                        lockfile_path.display()
                    )
                });
            }
        };

        let mut identities_by_name = HashMap::<String, Vec<LockedGemIdentity>>::new();
        for identity in parse_locked_gems(&content)? {
            identities_by_name
                .entry(identity.name.clone())
                .or_default()
                .push(identity);
        }
        for (name, identities) in identities_by_name {
            let selected =
                select_locked_identity_for_engine(&name, &identities, self.active_ruby_engine)?;
            self.locked_gems.insert(name, selected);
        }
        Ok(())
    }

    /// Add extracted Bundler Git caches using only lockfile metadata. Gemfiles
    /// and gemspecs are project code and must not be executed by this fallback.
    fn discover_cached_git_gems(&mut self) -> Result<()> {
        self.load_locked_gems()?;
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let lockfile_path = root.join("Gemfile.lock");
        let lockfile = match std::fs::read_to_string(&lockfile_path) {
            Ok(lockfile) => lockfile,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read owning project lockfile {}",
                        lockfile_path.display()
                    )
                });
            }
        };
        let cache_root = root.join("vendor/cache");
        if !cache_root.is_dir() {
            return Ok(());
        }

        let mut lines = lockfile.lines().peekable();
        while let Some(line) = lines.next() {
            if line != "GIT" {
                continue;
            }
            let mut remote = None;
            let mut revision = None;
            let mut specs = Vec::new();
            while let Some(section_line) = lines.peek().copied() {
                if !section_line.is_empty() && !section_line.starts_with(' ') {
                    break;
                }
                let section_line = lines.next().expect(
                    "INVARIANT VIOLATED: peeked lockfile line disappeared. This is a bug because iterator state must remain stable between peek and next. Fix: keep lockfile parsing single-threaded.",
                );
                if let Some(value) = section_line.strip_prefix("  remote: ") {
                    remote = Some(value.to_string());
                } else if let Some(value) = section_line.strip_prefix("  revision: ") {
                    revision = Some(value.to_string());
                } else if section_line.starts_with("    ") && !section_line.starts_with("      ") {
                    let spec = section_line.trim();
                    if let Some((name, version)) = spec.split_once(" (") {
                        if let Some(version) = version.strip_suffix(')') {
                            specs.push((name.to_string(), version.to_string()));
                        }
                    }
                }
            }

            let (Some(remote), Some(revision)) = (remote, revision) else {
                continue;
            };
            if revision.len() < 7 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                warn!(
                    "Skipping cached Git dependency with invalid lockfile revision: {}",
                    revision
                );
                continue;
            }
            let repository = remote
                .rsplit(['/', ':'])
                .next()
                .map(|name| name.strip_suffix(".git").unwrap_or(name))
                .filter(|name| !name.is_empty());
            let Some(repository) = repository else {
                continue;
            };
            let revision_prefix = revision.get(..revision.len().min(12)).expect(
                "INVARIANT VIOLATED: Git revision prefix is not a UTF-8 boundary. This is a bug because lockfile revisions must be ASCII hexadecimal. Fix: validate Bundler lockfile revision syntax before slicing.",
            );
            let cache_path = cache_root.join(format!("{repository}-{revision_prefix}"));
            let lib_path = cache_path.join("lib");
            if !cache_path.is_dir() || !lib_path.is_dir() {
                continue;
            }
            for (name, version) in specs {
                let locked = self.locked_gems.get(&name).ok_or_else(|| {
                    anyhow!(
                        "Gemfile.lock Git cache parser produced `{name}` without a locked identity"
                    )
                })?;
                if locked.source != LockedGemSource::Git || locked.locked_version != version {
                    return Err(anyhow!(
                        "Gemfile.lock Git cache identity for `{name}` disagrees with the unified lock parser"
                    ));
                }
                self.discovered_gems
                    .entry(name.clone())
                    .or_default()
                    .push(GemInfo {
                        name,
                        version: version.clone(),
                        platform: "ruby".to_string(),
                        locked_version: version,
                        source: GemSource::VendorGit,
                        path: cache_path.clone(),
                        lib_paths: vec![lib_path.clone()],
                        dependencies: locked.dependencies.clone(),
                        is_default: false,
                    });
            }
        }
        Ok(())
    }

    fn discover_cached_gem_archives(&mut self) -> Result<()> {
        self.load_locked_gems()?;
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let cache_root = root.join("vendor/cache");
        if !cache_root.is_dir() {
            return Ok(());
        }
        let extraction_root = self.cached_gem_extraction_root(root)?;

        let mut locked_registry_gems = self
            .locked_gems
            .values()
            .filter(|identity| identity.source == LockedGemSource::Registry)
            .cloned()
            .collect::<Vec<_>>();
        locked_registry_gems.sort_by(|left, right| left.name.cmp(&right.name));

        for locked in &locked_registry_gems {
            let exact_installed_source_exists =
                self.discovered_gems
                    .get(&locked.name)
                    .is_some_and(|candidates| {
                        candidates.iter().any(|candidate| {
                            matches!(
                                candidate.source,
                                GemSource::BundlerInstalled | GemSource::GlobalInstalled
                            ) && candidate.locked_version == locked.locked_version
                                && candidate.lib_paths.iter().any(|path| path.is_dir())
                        })
                    });
            if exact_installed_source_exists {
                continue;
            }

            let archive_name = format!("{}-{}.gem", locked.name, locked.locked_version);
            let archive_path = cache_root.join(&archive_name);
            if !archive_path.is_file() {
                continue;
            }

            match extract_cached_gem_archive(&extraction_root, &archive_path, &locked) {
                Ok(gem) => {
                    info!(
                        "Discovered locked vendor cache gem: {} v{}",
                        gem.name, gem.version
                    );
                    self.discovered_gems
                        .entry(gem.name.clone())
                        .or_default()
                        .push(gem);
                }
                Err(error) => {
                    warn!(
                        "Skipping invalid cached gem archive {}: {error:#}",
                        archive_path.display()
                    );
                }
            }
        }

        Ok(())
    }

    fn cached_gem_extraction_root(&self, project_root: &Path) -> Result<PathBuf> {
        let cache_root = match &self.cached_gem_root_override {
            Some(root) => root.clone(),
            None => crate::utils::ruby_fast_lsp_user_cache_root()?,
        };
        let canonical_project_root = project_root.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize Ruby project root {} for cached gem isolation",
                project_root.display()
            )
        })?;
        let project_key = format!(
            "{:x}",
            Sha256::digest(canonical_project_root.to_string_lossy().as_bytes())
        );
        Ok(cache_root.join("gems").join(project_key))
    }

    /// Find Gemfile in workspace hierarchy
    fn find_gemfile(&self) -> Result<PathBuf> {
        if let Some(root) = &self.workspace_root {
            // Check workspace root
            let gemfile = root.join("Gemfile");
            if gemfile.exists() {
                return Ok(gemfile);
            }

            return Err(anyhow!(
                "No Gemfile found at Ruby project root {}",
                root.display()
            ));
        }

        // Fallback to current directory
        let current = std::env::current_dir()?.join("Gemfile");
        if current.exists() {
            return Ok(current);
        }

        Err(anyhow!("No Gemfile found in workspace hierarchy"))
    }

    fn ruby_command(&self) -> Command {
        if let Some(executable) = &self.ruby_executable {
            let mut command = Command::new(executable);
            command.env_remove("GEM_HOME");
            command.env_remove("GEM_PATH");
            command.env_remove("RUBY_VERSION");
            if let Some(java_home) = &self.java_home {
                command.env("JAVA_HOME", java_home);
            }
            if let Some(root) = &self.workspace_root {
                command.current_dir(root);
            }
            return command;
        }
        if let Some(root) = &self.workspace_root {
            if let Some(ruby_path) = workspace_ruby_path(root) {
                let mut command = Command::new(ruby_path);
                command.current_dir(root);
                return command;
            }
        }

        let mut command = Command::new("ruby");
        if let Some(root) = &self.workspace_root {
            command.current_dir(root);
        }
        command
    }

    /// Process gem data from JSON output
    fn process_gem_json(
        &mut self,
        data: &[u8],
        source_label: &str,
        source: GemSource,
    ) -> Result<()> {
        let gems: Vec<GemDiscoveryRecord> =
            serde_json::from_slice(data).context("failed to parse gem discovery JSON")?;

        for gem in gems {
            if gem.name.is_empty()
                || gem.version.is_empty()
                || gem.platform.is_empty()
                || gem.gem_dir.as_os_str().is_empty()
            {
                return Err(anyhow!(
                    "{source_label} gem discovery returned an incomplete identity: {gem:?}"
                ));
            }
            let locked_version = locked_version_for(&gem.version, &gem.platform);

            let gem_info = GemInfo {
                name: gem.name.clone(),
                version: gem.version,
                platform: gem.platform,
                locked_version,
                source,
                path: gem.gem_dir,
                lib_paths: gem.lib_dirs,
                dependencies: gem.dependencies,
                is_default: gem.default_gem,
            };

            self.discovered_gems
                .entry(gem.name)
                .or_default()
                .push(gem_info);
        }

        debug!(
            "Processed {} gems from {} source",
            self.discovered_gems.len(),
            source_label
        );
        Ok(())
    }

    /// Resolve and validate gem library paths
    fn resolve_gem_lib_paths(&mut self) {
        for versions in self.discovered_gems.values_mut() {
            for gem in versions.iter_mut() {
                // Filter out non-existent lib paths
                gem.lib_paths.retain(|p| p.exists() && p.is_dir());

                // Try default lib path if none exist
                if gem.lib_paths.is_empty() {
                    let default_lib = gem.path.join("lib");
                    if default_lib.exists() && default_lib.is_dir() {
                        gem.lib_paths.push(default_lib);
                    }
                }
            }
        }
    }

    // ========================================================================
    // Version Selection
    // ========================================================================

    /// Select the preferred version of a gem from multiple available versions
    fn select_preferred_version<'a>(&self, versions: &'a [GemInfo]) -> Option<&'a GemInfo> {
        let name = versions.first()?.name.as_str();
        assert!(
            versions.iter().all(|candidate| candidate.name == name),
            "INVARIANT VIOLATED: gem candidate bucket contains multiple names. This is a bug because discovered_gems is keyed by gem name. Fix: insert every candidate under its own exact name."
        );

        if let Some(locked) = self.locked_gems.get(name) {
            let exact = |source| {
                versions.iter().find(|candidate| {
                    candidate.source == source
                        && candidate.locked_version == locked.locked_version
                        && self.gem_platform_matches_active_engine(&candidate.platform)
                        && !candidate.lib_paths.is_empty()
                })
            };

            if let Some(installed) = exact(GemSource::BundlerInstalled) {
                return Some(installed);
            }

            return match locked.source {
                LockedGemSource::Registry => {
                    exact(GemSource::GlobalInstalled).or_else(|| exact(GemSource::VendorArchive))
                }
                LockedGemSource::Git => exact(GemSource::VendorGit),
                LockedGemSource::Path => None,
            };
        }

        if let Some(installed) = versions
            .iter()
            .find(|candidate| candidate.source == GemSource::BundlerInstalled)
        {
            return Some(installed);
        }

        if self.explicitly_included_gems.contains(name) {
            return versions
                .iter()
                .filter(|candidate| candidate.source == GemSource::GlobalInstalled)
                .max_by(|a, b| compare_versions(&a.version, &b.version));
        }

        None
    }

    fn gem_platform_matches_active_engine(&self, platform: &str) -> bool {
        match self.active_ruby_engine {
            ActiveRubyEngine::JRuby => platform == "ruby" || platform == "java",
            ActiveRubyEngine::Other => platform != "java",
        }
    }

    fn required_gems_with_dependencies(&self) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        let mut roots = self.required_gems.iter().cloned().collect::<Vec<_>>();
        roots.sort();
        let mut queue = roots.into_iter().collect::<VecDeque<String>>();

        while let Some(name) = queue.pop_front() {
            if self.excluded_gems.contains(&name) {
                continue;
            }
            if !seen.insert(name.clone()) {
                continue;
            }
            ordered.push(name.clone());

            let Some(gem_versions) = self.discovered_gems.get(&name) else {
                continue;
            };
            let Some(gem_info) = self.select_preferred_version(gem_versions) else {
                continue;
            };

            for dependency in &gem_info.dependencies {
                if !seen.contains(dependency) {
                    queue.push_back(dependency.clone());
                }
            }
        }

        ordered
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

    pub fn get_all_gems(&self) -> Vec<&GemInfo> {
        self.discovered_gems
            .values()
            .filter_map(|candidates| self.select_preferred_version(candidates))
            .collect()
    }

    pub fn get_gem_lib_paths(&self) -> Vec<PathBuf> {
        self.discovered_gems
            .values()
            .filter_map(|v| self.select_preferred_version(v))
            .flat_map(|g| g.lib_paths.iter().cloned())
            .collect()
    }

    pub fn get_gem_paths(&self, name: &str) -> Vec<PathBuf> {
        self.discovered_gems
            .get(name)
            .and_then(|v| self.select_preferred_version(v))
            .map(|g| g.lib_paths.clone())
            .unwrap_or_default()
    }

    pub fn get_gem_lib_paths_for_gems(&self, names: &[String]) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = names.iter().flat_map(|n| self.get_gem_paths(n)).collect();

        // Deduplicate while preserving order
        let mut seen = HashSet::new();
        paths.retain(|p| seen.insert(p.clone()));
        paths
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Compare two gem version strings
fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };

    let parts_a = parse(a);
    let parts_b = parse(b);

    for (x, y) in parts_a.iter().zip(parts_b.iter()) {
        match x.cmp(y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

fn select_locked_identity_for_engine(
    name: &str,
    identities: &[LockedGemIdentity],
    engine: ActiveRubyEngine,
) -> Result<LockedGemIdentity> {
    assert!(
        !identities.is_empty(),
        "INVARIANT VIOLATED: lock identity selection received no candidates for `{name}`. This is a bug because grouping creates an entry only after parsing an identity. Fix: never call selection with an empty lockfile group."
    );
    assert!(
        identities.iter().all(|identity| identity.name == name),
        "INVARIANT VIOLATED: lock identity group for `{name}` contains another gem name. This is a bug because lock identities are grouped by exact gem name. Fix: insert each parsed identity into its own name bucket."
    );

    let source = identities[0].source;
    if identities.iter().any(|identity| identity.source != source) {
        return Err(anyhow!(
            "Gemfile.lock contains conflicting source identities for `{name}`: {identities:?}"
        ));
    }

    if identities.len() == 1 {
        return Ok(identities[0].clone());
    }
    if source != LockedGemSource::Registry {
        return Err(anyhow!(
            "Gemfile.lock contains multiple non-registry identities for `{name}`: {identities:?}"
        ));
    }

    let (java, non_java): (Vec<_>, Vec<_>) = identities
        .iter()
        .partition(|identity| identity.locked_version.ends_with("-java"));
    let preferred = match engine {
        ActiveRubyEngine::JRuby if java.len() == 1 => java[0],
        ActiveRubyEngine::JRuby if java.is_empty() && non_java.len() == 1 => non_java[0],
        ActiveRubyEngine::Other if non_java.len() == 1 => non_java[0],
        ActiveRubyEngine::Other if non_java.is_empty() && java.len() == 1 => java[0],
        ActiveRubyEngine::JRuby | ActiveRubyEngine::Other => {
            return Err(anyhow!(
                "Gemfile.lock contains ambiguous platform identities for `{name}` and active engine {engine:?}: {identities:?}"
            ));
        }
    };
    Ok(preferred.clone())
}

fn parse_locked_gems(content: &str) -> Result<Vec<LockedGemIdentity>> {
    let mut source = None;
    let mut in_specs = false;
    let mut gems = Vec::<LockedGemIdentity>::new();

    for line in content.lines() {
        if !line.starts_with(' ') {
            source = match line {
                "GEM" => Some(LockedGemSource::Registry),
                "GIT" => Some(LockedGemSource::Git),
                "PATH" => Some(LockedGemSource::Path),
                _ => None,
            };
            in_specs = false;
            continue;
        }
        let Some(source) = source else {
            continue;
        };
        if line == "  specs:" {
            in_specs = true;
            continue;
        }
        if !in_specs {
            continue;
        }

        if let Some(dependency) = line.strip_prefix("      ") {
            let Some(current) = gems.last_mut() else {
                return Err(anyhow!(
                    "Gemfile.lock contains a dependency before its owning specification"
                ));
            };
            if current.source != source {
                return Err(anyhow!(
                    "Gemfile.lock dependency source changed before its owning specification"
                ));
            }
            let dependency = dependency
                .trim()
                .split([' ', '('])
                .next()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| anyhow!("Gemfile.lock contains an empty dependency name"))?;
            validate_archive_identity_component("dependency name", dependency)?;
            current.dependencies.push(dependency.to_string());
            continue;
        }

        let Some(spec) = line.strip_prefix("    ") else {
            continue;
        };
        let (name, locked_version) = spec
            .trim()
            .split_once(" (")
            .and_then(|(name, version)| version.strip_suffix(')').map(|version| (name, version)))
            .ok_or_else(|| anyhow!("Gemfile.lock contains an invalid gem specification: {spec}"))?;
        validate_archive_identity_component("gem name", name)?;
        validate_archive_identity_component("gem version", locked_version)?;
        gems.push(LockedGemIdentity {
            name: name.to_string(),
            locked_version: locked_version.to_string(),
            source,
            dependencies: Vec::new(),
        });
    }

    Ok(gems)
}

fn locked_version_for(version: &str, platform: &str) -> String {
    if platform == "ruby" {
        version.to_string()
    } else {
        format!("{version}-{platform}")
    }
}

fn validate_archive_identity_component(kind: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(anyhow!(
            "Gemfile.lock {kind} `{value}` cannot safely identify a vendor cache archive"
        ));
    }
    Ok(())
}

fn extract_cached_gem_archive(
    extraction_root: &Path,
    archive_path: &Path,
    locked: &LockedGemIdentity,
) -> Result<GemInfo> {
    let archive_size = std::fs::metadata(archive_path)
        .with_context(|| format!("failed to inspect {}", archive_path.display()))?
        .len();
    if archive_size > MAX_CACHED_GEM_ARCHIVE_BYTES {
        return Err(anyhow!(
            "archive is {archive_size} bytes, exceeding the {MAX_CACHED_GEM_ARCHIVE_BYTES}-byte limit"
        ));
    }
    let archive_bytes = std::fs::read(archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;
    let metadata_gzip =
        read_gem_package_member(&archive_bytes, "metadata.gz", MAX_CACHED_GEM_METADATA_BYTES)?;
    let metadata_yaml = decompress_bounded(
        &metadata_gzip,
        MAX_CACHED_GEM_METADATA_BYTES,
        "gem metadata",
    )?;
    let metadata = parse_cached_gem_metadata(&metadata_yaml)?;
    if metadata.name != locked.name || metadata.locked_version != locked.locked_version {
        return Err(anyhow!(
            "archive metadata identifies {} v{}, but Gemfile.lock requires {} v{}",
            metadata.name,
            metadata.locked_version,
            locked.name,
            locked.locked_version
        ));
    }

    let checksum = format!("{:x}", Sha256::digest(&archive_bytes));
    let gem_root = extraction_root
        .join(&checksum)
        .join(format!("{}-{}", locked.name, locked.locked_version));
    let completion_marker = gem_root.join(".complete");
    let marker_matches =
        std::fs::read_to_string(&completion_marker).is_ok_and(|contents| contents == checksum);
    if !marker_matches {
        extract_cached_gem_data(
            &archive_bytes,
            extraction_root,
            &gem_root,
            &checksum,
            &metadata.require_paths,
        )?;
    }

    let lib_paths = metadata
        .require_paths
        .iter()
        .map(|path| gem_root.join(path))
        .collect::<Vec<_>>();
    if lib_paths.iter().any(|path| !path.is_dir()) {
        return Err(anyhow!(
            "archive did not contain every declared require path for {} v{}",
            locked.name,
            locked.locked_version
        ));
    }

    Ok(GemInfo {
        name: locked.name.clone(),
        version: metadata.version,
        platform: metadata.platform,
        locked_version: locked.locked_version.clone(),
        source: GemSource::VendorArchive,
        path: gem_root,
        lib_paths,
        dependencies: locked.dependencies.clone(),
        is_default: false,
    })
}

fn read_gem_package_member(
    archive_bytes: &[u8],
    member_name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let mut package = tar::Archive::new(Cursor::new(archive_bytes));
    for entry in package
        .entries()
        .context("failed to read gem package tar")?
    {
        let mut entry = entry.context("failed to read gem package entry")?;
        let path = entry
            .path()
            .context("gem package entry has an invalid path")?;
        if path != Path::new(member_name) {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(anyhow!("gem package member `{member_name}` is not a file"));
        }
        let size = entry.size();
        if size > max_bytes {
            return Err(anyhow!(
                "gem package member `{member_name}` is {size} bytes, exceeding the {max_bytes}-byte limit"
            ));
        }
        let mut bytes = Vec::with_capacity(
            usize::try_from(size)
                .context("gem package member size cannot be represented on this platform")?,
        );
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read gem package member `{member_name}`"))?;
        return Ok(bytes);
    }
    Err(anyhow!(
        "gem package is missing required member `{member_name}`"
    ))
}

fn decompress_bounded(compressed: &[u8], max_bytes: u64, label: &str) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(Cursor::new(compressed));
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut output)
        .with_context(|| format!("failed to decompress {label}"))?;
    if u64::try_from(output.len()).context("decompressed length cannot fit u64")? > max_bytes {
        return Err(anyhow!(
            "{label} exceeds the {max_bytes}-byte decompression limit"
        ));
    }
    Ok(output)
}

fn parse_cached_gem_metadata(metadata_yaml: &[u8]) -> Result<CachedGemMetadata> {
    let value: YamlValue =
        serde_yaml::from_slice(metadata_yaml).context("failed to parse gem metadata YAML")?;
    let name = yaml_string_field(&value, "name")?;
    let version_value = yaml_field(&value, "version")?;
    let version = match untag_yaml(version_value) {
        YamlValue::String(version) => version.clone(),
        YamlValue::Mapping(_) => yaml_string_field(version_value, "version")?,
        other => {
            return Err(anyhow!(
                "gem metadata version has unsupported YAML shape: {other:?}"
            ));
        }
    };
    let platform = yaml_string_field(&value, "platform")?;
    let platform = if platform.is_empty() {
        "ruby".to_string()
    } else {
        platform
    };
    let locked_version = locked_version_for(&version, &platform);
    let require_paths_value = yaml_field(&value, "require_paths")?;
    let YamlValue::Sequence(paths) = untag_yaml(require_paths_value) else {
        return Err(anyhow!("gem metadata require_paths must be a sequence"));
    };
    if paths.is_empty() {
        return Err(anyhow!("gem metadata require_paths must not be empty"));
    }
    let require_paths = paths
        .iter()
        .map(|value| {
            let YamlValue::String(path) = untag_yaml(value) else {
                return Err(anyhow!("gem metadata require path must be a string"));
            };
            validate_relative_archive_path(path)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CachedGemMetadata {
        name,
        version,
        platform,
        locked_version,
        require_paths,
    })
}

fn untag_yaml(value: &YamlValue) -> &YamlValue {
    match value {
        YamlValue::Tagged(tagged) => untag_yaml(&tagged.value),
        YamlValue::Null
        | YamlValue::Bool(_)
        | YamlValue::Number(_)
        | YamlValue::String(_)
        | YamlValue::Sequence(_)
        | YamlValue::Mapping(_) => value,
    }
}

fn yaml_field<'a>(value: &'a YamlValue, field: &str) -> Result<&'a YamlValue> {
    let YamlValue::Mapping(mapping) = untag_yaml(value) else {
        return Err(anyhow!("gem metadata root must be a mapping"));
    };
    mapping
        .get(YamlValue::String(field.to_string()))
        .ok_or_else(|| anyhow!("gem metadata is missing `{field}`"))
}

fn yaml_string_field(value: &YamlValue, field: &str) -> Result<String> {
    let field_value = yaml_field(value, field)?;
    match untag_yaml(field_value) {
        YamlValue::String(value) => Ok(value.clone()),
        other => Err(anyhow!(
            "gem metadata `{field}` must be a string, found {other:?}"
        )),
    }
}

fn validate_relative_archive_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(anyhow!(
            "gem archive path `{}` is not a safe relative path",
            path.display()
        ));
    }
    Ok(path.to_path_buf())
}

fn extract_cached_gem_data(
    package_bytes: &[u8],
    cache_root: &Path,
    gem_root: &Path,
    checksum: &str,
    require_paths: &[PathBuf],
) -> Result<()> {
    let data_gzip =
        read_gem_package_member(package_bytes, "data.tar.gz", MAX_CACHED_GEM_ARCHIVE_BYTES)?;
    let temporary_root = cache_root.join(format!(".{checksum}.tmp-{}", std::process::id()));
    if temporary_root.exists() {
        std::fs::remove_dir_all(&temporary_root).with_context(|| {
            format!(
                "failed to clear incomplete cached gem extraction {}",
                temporary_root.display()
            )
        })?;
    }
    std::fs::create_dir_all(&temporary_root).with_context(|| {
        format!(
            "failed to create cached gem extraction {}",
            temporary_root.display()
        )
    })?;

    let extraction = (|| -> Result<()> {
        let decoder = GzDecoder::new(Cursor::new(data_gzip));
        let mut archive = tar::Archive::new(decoder);
        let mut file_count = 0usize;
        let mut extracted_bytes = 0u64;
        for entry in archive.entries().context("failed to read gem data tar")? {
            let mut entry = entry.context("failed to read gem data entry")?;
            let path = entry.path().context("gem data entry has an invalid path")?;
            let path_text = path
                .to_str()
                .ok_or_else(|| anyhow!("gem data entry path is not valid UTF-8"))?;
            let path = validate_relative_archive_path(path_text)?;
            if !require_paths
                .iter()
                .any(|require_path| path.starts_with(require_path))
            {
                continue;
            }

            match entry.header().entry_type() {
                EntryType::Directory => continue,
                EntryType::Regular => {}
                entry_type => {
                    return Err(anyhow!(
                        "gem data entry {} uses unsupported type {entry_type:?}",
                        path.display()
                    ));
                }
            }
            file_count += 1;
            if file_count > MAX_CACHED_GEM_FILES {
                return Err(anyhow!(
                    "gem data contains more than {MAX_CACHED_GEM_FILES} files"
                ));
            }
            extracted_bytes = extracted_bytes
                .checked_add(entry.size())
                .ok_or_else(|| anyhow!("gem extracted byte count overflowed u64"))?;
            if extracted_bytes > MAX_CACHED_GEM_EXTRACTED_BYTES {
                return Err(anyhow!(
                    "gem data exceeds the {MAX_CACHED_GEM_EXTRACTED_BYTES}-byte extraction limit"
                ));
            }

            let destination = temporary_root.join(&path);
            let parent = destination.parent().ok_or_else(|| {
                anyhow!(
                    "gem data destination {} has no parent",
                    destination.display()
                )
            })?;
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cached gem directory {}", parent.display())
            })?;
            let mut output = std::fs::File::create(&destination).with_context(|| {
                format!("failed to create cached gem file {}", destination.display())
            })?;
            let copied = std::io::copy(&mut entry, &mut output)
                .with_context(|| format!("failed to extract cached gem file {}", path.display()))?;
            if copied != entry.size() {
                return Err(anyhow!(
                    "cached gem file {} declared {} bytes but yielded {copied}",
                    path.display(),
                    entry.size()
                ));
            }
        }
        if file_count == 0 {
            return Err(anyhow!(
                "gem data contains no regular files under its declared require paths"
            ));
        }
        std::fs::write(temporary_root.join(".complete"), checksum)
            .context("failed to write cached gem completion marker")?;
        Ok(())
    })();

    if let Err(error) = extraction {
        let _ = std::fs::remove_dir_all(&temporary_root);
        return Err(error);
    }

    let checksum_root = gem_root.parent().ok_or_else(|| {
        anyhow!(
            "cached gem destination {} has no checksum parent",
            gem_root.display()
        )
    })?;
    std::fs::create_dir_all(checksum_root).with_context(|| {
        format!(
            "failed to create cached gem checksum directory {}",
            checksum_root.display()
        )
    })?;
    if gem_root.exists() {
        std::fs::remove_dir_all(gem_root).with_context(|| {
            format!(
                "failed to replace incomplete cached gem destination {}",
                gem_root.display()
            )
        })?;
    }
    std::fs::rename(&temporary_root, gem_root).with_context(|| {
        format!(
            "failed to publish cached gem extraction {}",
            gem_root.display()
        )
    })?;
    Ok(())
}

fn workspace_ruby_path(workspace_root: &Path) -> Option<PathBuf> {
    let version = std::fs::read_to_string(workspace_root.join(".ruby-version")).ok()?;
    let version = normalize_ruby_version(version.trim())?;
    let rvm_version = if version.starts_with("ruby-")
        || version.starts_with("jruby-")
        || version.starts_with("truffleruby-")
    {
        version.to_string()
    } else {
        format!("ruby-{version}")
    };
    let manager_version = version.strip_prefix("ruby-").unwrap_or(version);
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        PathBuf::from(&home)
            .join(".rvm")
            .join("wrappers")
            .join(&rvm_version)
            .join("ruby"),
        PathBuf::from(&home)
            .join(".rvm")
            .join("rubies")
            .join(&rvm_version)
            .join("bin")
            .join("ruby"),
        PathBuf::from(&home)
            .join(".rbenv")
            .join("versions")
            .join(manager_version)
            .join("bin")
            .join("ruby"),
        PathBuf::from(&home)
            .join(".asdf")
            .join("installs")
            .join("ruby")
            .join(manager_version)
            .join("bin")
            .join("ruby"),
    ];

    candidates.into_iter().find(|path| path.is_file())
}

fn normalize_ruby_version(version: &str) -> Option<&str> {
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(version)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs;
    use std::io::Cursor;
    use tar::{Builder, Header};
    use tempfile::TempDir;

    fn create_test_indexer() -> IndexerGem {
        let temp_dir = TempDir::new().unwrap();
        IndexerGem::new(Some(temp_dir.path().to_path_buf()))
    }

    #[test]
    fn test_gem_indexer_creation() {
        let indexer = create_test_indexer();
        assert_eq!(indexer.gem_count(), 0);
        assert!(indexer.get_required_gems().is_empty());
    }

    #[test]
    fn gem_discovery_requires_the_project_roots_own_gemfile() {
        let workspace = TempDir::new().unwrap();
        std::fs::create_dir_all(workspace.path().join("service")).unwrap();
        std::fs::write(workspace.path().join("service/Gemfile"), "").unwrap();
        let indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));

        assert!(
            indexer.find_gemfile().is_err(),
            "a container folder must be expanded into projects before Bundler discovery"
        );
    }

    #[test]
    fn locked_git_gem_uses_extracted_vendor_cache_without_executing_gemspec() {
        let workspace = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GIT\n  remote: git@github.com:emerose/pbkdf2-ruby.git\n  revision: b8c9fd171c32d4abcab52be629996448cf0bf63a\n  specs:\n    pbkdf2 (0.2.0)\n\nGEM\n",
        )
        .unwrap();
        let cache = workspace
            .path()
            .join("vendor/cache/pbkdf2-ruby-b8c9fd171c32");
        std::fs::create_dir_all(cache.join("lib")).unwrap();
        std::fs::write(cache.join("lib/pbkdf2.rb"), "class PBKDF2; end\n").unwrap();
        std::fs::write(
            cache.join("pbkdf2.gemspec"),
            "raise 'must never execute project gemspec'\n",
        )
        .unwrap();
        let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));

        indexer.discover_cached_git_gems().unwrap();

        let gem = &indexer.discovered_gems["pbkdf2"][0];
        assert_eq!(gem.version, "0.2.0");
        assert_eq!(gem.platform, "ruby");
        assert_eq!(gem.source, GemSource::VendorGit);
        assert_eq!(gem.path, cache);
        assert_eq!(gem.lib_paths, [cache.join("lib")]);
    }

    fn append_tar_file(builder: &mut Builder<Vec<u8>>, path: &str, content: &[u8]) {
        let mut header = Header::new_gnu();
        header.set_size(u64::try_from(content.len()).unwrap());
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, path, Cursor::new(content))
            .unwrap();
    }

    fn gzip(content: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::copy(&mut Cursor::new(content), &mut encoder).unwrap();
        encoder.finish().unwrap()
    }

    fn create_cached_gem_with_require_path(
        path: &Path,
        name: &str,
        version: &str,
        platform: &str,
        require_path: &str,
    ) {
        let metadata = format!(
            "--- !ruby/object:Gem::Specification\n\
             name: {name}\n\
             version: !ruby/object:Gem::Version\n\
             \x20 version: {version}\n\
             platform: {platform}\n\
             require_paths:\n\
             - {require_path}\n"
        );
        let mut data = Builder::new(Vec::new());
        append_tar_file(
            &mut data,
            "lib/example.rb",
            b"module Example; class Cached; end; end\n",
        );
        let data = data.into_inner().unwrap();

        let mut package = Builder::new(Vec::new());
        append_tar_file(&mut package, "metadata.gz", &gzip(metadata.as_bytes()));
        append_tar_file(&mut package, "data.tar.gz", &gzip(&data));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, package.into_inner().unwrap()).unwrap();
    }

    fn create_cached_gem(path: &Path, name: &str, version: &str, platform: &str) {
        create_cached_gem_with_require_path(path, name, version, platform, "lib");
    }

    #[test]
    fn discovers_only_exact_locked_java_platform_gem_roots_for_selected_jruby() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project");
        let rvm = fixture.path().join(".rvm");
        let runtime = rvm.join("rubies/jruby-9.2.21.0");
        let executable = runtime.join("bin/jruby");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"fixture").unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Gemfile.lock"),
            concat!(
                "GEM\n",
                "  remote: https://rubygems.org/\n",
                "  specs:\n",
                "    bson (4.14.1-java)\n",
                "    bson (4.14.1)\n",
                "    rack (3.0.0)\n",
                "PLATFORMS\n",
                "  java\n",
            ),
        )
        .unwrap();
        let exact = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
        let wrong = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.0-java");
        let ruby = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1");
        fs::create_dir_all(&exact).unwrap();
        fs::create_dir_all(&wrong).unwrap();
        fs::create_dir_all(&ruby).unwrap();

        assert_eq!(
            discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
            vec![exact.canonicalize().unwrap()]
        );
    }

    #[test]
    fn project_local_locked_java_gem_precedes_the_selected_rvm_runtime_copy() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project");
        let rvm = fixture.path().join(".rvm");
        let executable = rvm.join("rubies/jruby-9.2.21.0/bin/jruby");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"fixture").unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Gemfile.lock"),
            concat!(
                "GEM\n",
                "  remote: https://rubygems.org/\n",
                "  specs:\n",
                "    bson (4.14.1-java)\n",
                "PLATFORMS\n",
                "  java\n",
            ),
        )
        .unwrap();
        let local = project.join("vendor/bundle/jruby/2.5.0/gems/bson-4.14.1-java");
        let global = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
        fs::create_dir_all(&local).unwrap();
        fs::create_dir_all(&global).unwrap();

        assert_eq!(
            discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
            vec![local.canonicalize().unwrap()]
        );
    }

    #[test]
    fn duplicate_project_local_locked_java_gem_installations_fail_closed() {
        let fixture = tempfile::tempdir().unwrap();
        let project = fixture.path().join("project");
        let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"fixture").unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Gemfile.lock"),
            concat!(
                "GEM\n",
                "  remote: https://rubygems.org/\n",
                "  specs:\n",
                "    bson (4.14.1-java)\n",
                "PLATFORMS\n",
                "  java\n",
            ),
        )
        .unwrap();
        for compatibility in ["2.5.0", "3.1.0"] {
            fs::create_dir_all(
                project
                    .join("vendor/bundle/jruby")
                    .join(compatibility)
                    .join("gems/bson-4.14.1-java"),
            )
            .unwrap();
        }

        let error = discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ambiguous installations in project vendor/bundle"),
            "unexpected error: {error:?}"
        );
    }

    fn create_cached_gem_indexer(project_root: &Path, cache_root: &Path) -> IndexerGem {
        let mut indexer = IndexerGem::new(Some(project_root.to_path_buf()));
        indexer.set_cached_gem_root_for_test(cache_root.to_path_buf());
        indexer
    }

    #[test]
    fn locked_registry_gem_uses_project_local_vendor_cache_archive() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  remote: https://rubygems.org/\n  specs:\n    example (1.2.3-java)\n\nPLATFORMS\n  java\n",
        )
        .unwrap();
        create_cached_gem(
            &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
            "example",
            "1.2.3",
            "java",
        );
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

        indexer.discover_cached_gem_archives().unwrap();

        let gem = &indexer.discovered_gems["example"][0];
        assert_eq!(gem.version, "1.2.3");
        assert_eq!(gem.platform, "java");
        assert_eq!(gem.locked_version, "1.2.3-java");
        assert_eq!(gem.source, GemSource::VendorArchive);
        assert!(
            gem.path.starts_with(extraction_cache.path().join("gems")),
            "cached archive extraction must use the external cache"
        );
        assert!(
            !gem.path.starts_with(workspace.path()),
            "cached archive extraction must not mutate the Ruby project"
        );
        assert_eq!(gem.lib_paths, [gem.path.join("lib")]);
        assert_eq!(
            std::fs::read_to_string(gem.path.join("lib/example.rb")).unwrap(),
            "module Example; class Cached; end; end\n"
        );
    }

    #[test]
    fn cached_registry_gem_must_match_locked_platform_version() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3-java)\n",
        )
        .unwrap();
        create_cached_gem(
            &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
            "example",
            "1.2.3",
            "ruby",
        );
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

        indexer.discover_cached_gem_archives().unwrap();

        assert!(
            !indexer.discovered_gems.contains_key("example"),
            "an archive whose metadata disagrees with the lockfile must fail closed"
        );
    }

    #[test]
    fn cached_registry_gem_rejects_unsafe_require_path() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        create_cached_gem_with_require_path(
            &workspace.path().join("vendor/cache/example-1.2.3.gem"),
            "example",
            "1.2.3",
            "ruby",
            "../lib",
        );
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

        indexer.discover_cached_gem_archives().unwrap();

        assert!(
            !indexer.discovered_gems.contains_key("example"),
            "an archive with a traversing require path must fail closed"
        );
        assert!(
            !extraction_cache.path().join("lib").exists(),
            "unsafe archive paths must never escape their checksum directory"
        );
    }

    #[test]
    fn cached_registry_gems_are_isolated_to_the_owning_project() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        let first = workspace.path().join("first");
        let second = workspace.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        for root in [&first, &second] {
            std::fs::write(
                root.join("Gemfile.lock"),
                "GEM\n  specs:\n    example (1.2.3)\n",
            )
            .unwrap();
        }
        create_cached_gem(
            &first.join("vendor/cache/example-1.2.3.gem"),
            "example",
            "1.2.3",
            "ruby",
        );
        let mut first_indexer = create_cached_gem_indexer(&first, extraction_cache.path());
        let mut second_indexer = create_cached_gem_indexer(&second, extraction_cache.path());

        first_indexer.discover_cached_gem_archives().unwrap();
        second_indexer.discover_cached_gem_archives().unwrap();

        assert!(first_indexer.discovered_gems.contains_key("example"));
        let first_path = &first_indexer.discovered_gems["example"][0].path;
        assert!(
            !first_path.starts_with(second_indexer.cached_gem_extraction_root(&second).unwrap()),
            "different projects must have different external extraction roots"
        );
        assert!(
            !second_indexer.discovered_gems.contains_key("example"),
            "a sibling project must not borrow another project's vendor cache"
        );
    }

    #[test]
    fn cached_registry_gem_wins_over_unrelated_global_version() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        create_cached_gem(
            &workspace.path().join("vendor/cache/example-1.2.3.gem"),
            "example",
            "1.2.3",
            "ruby",
        );
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
        indexer.discovered_gems.insert(
            "example".to_string(),
            vec![GemInfo {
                name: "example".to_string(),
                version: "9.0.0".to_string(),
                platform: "ruby".to_string(),
                locked_version: "9.0.0".to_string(),
                source: GemSource::GlobalInstalled,
                path: PathBuf::from("/global/example-9.0.0"),
                lib_paths: vec![PathBuf::from("/global/example-9.0.0/lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        indexer.discover_cached_gem_archives().unwrap();

        let selected = indexer.get_gem("example").unwrap();
        assert_eq!(selected.version, "1.2.3");
        assert!(
            selected
                .path
                .starts_with(extraction_cache.path().join("gems")),
            "the owning project's locked cache must beat an unrelated global gem"
        );
    }

    #[test]
    fn exact_installed_registry_gem_wins_before_vendor_archive_fallback() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        create_cached_gem(
            &workspace.path().join("vendor/cache/example-1.2.3.gem"),
            "example",
            "1.2.3",
            "ruby",
        );
        let installed_path = workspace.path().join("installed/example-1.2.3");
        std::fs::create_dir_all(installed_path.join("lib")).unwrap();
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
        indexer.discovered_gems.insert(
            "example".to_string(),
            vec![GemInfo {
                name: "example".to_string(),
                version: "1.2.3".to_string(),
                platform: "ruby".to_string(),
                locked_version: "1.2.3".to_string(),
                source: GemSource::GlobalInstalled,
                path: installed_path.clone(),
                lib_paths: vec![installed_path.join("lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        indexer.discover_cached_gem_archives().unwrap();

        let selected = indexer.get_gem("example").unwrap();
        assert_eq!(
            selected.path, installed_path,
            "an exact installed gem must win before extracting the project archive fallback"
        );
        assert!(
            !extraction_cache.path().join("gems").exists(),
            "an exact installed gem must avoid unnecessary archive extraction"
        );
    }

    #[test]
    fn wrong_installed_platform_uses_exact_vendor_archive_fallback() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3-java)\n",
        )
        .unwrap();
        create_cached_gem(
            &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
            "example",
            "1.2.3",
            "java",
        );
        let installed_path = workspace.path().join("installed/example-1.2.3");
        std::fs::create_dir_all(installed_path.join("lib")).unwrap();
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
        indexer.set_selected_runtime(
            PathBuf::from("/runtimes/jruby/bin/jruby"),
            RubyImplementation::JRuby,
            Some(PathBuf::from("/jdks/17")),
        );
        indexer.detect_active_ruby_engine().unwrap();
        indexer.discovered_gems.insert(
            "example".to_string(),
            vec![GemInfo {
                name: "example".to_string(),
                version: "1.2.3".to_string(),
                platform: "ruby".to_string(),
                locked_version: "1.2.3".to_string(),
                source: GemSource::GlobalInstalled,
                path: installed_path,
                lib_paths: vec![workspace.path().join("installed/example-1.2.3/lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        indexer.discover_cached_gem_archives().unwrap();

        let selected = indexer.get_gem("example").unwrap();
        assert_eq!(selected.source, GemSource::VendorArchive);
        assert_eq!(selected.platform, "java");
        assert_eq!(selected.locked_version, "1.2.3-java");
    }

    #[test]
    fn registry_install_cannot_substitute_for_locked_git_dependency() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GIT\n  remote: https://example.test/example.git\n  revision: abcdef1234567890\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        let global_path = workspace.path().join("global/example-1.2.3");
        std::fs::create_dir_all(global_path.join("lib")).unwrap();
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
        indexer.discovered_gems.insert(
            "example".to_string(),
            vec![GemInfo {
                name: "example".to_string(),
                version: "1.2.3".to_string(),
                platform: "ruby".to_string(),
                locked_version: "1.2.3".to_string(),
                source: GemSource::GlobalInstalled,
                path: global_path.clone(),
                lib_paths: vec![global_path.join("lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        indexer.discover_cached_gem_archives().unwrap();

        assert!(
            indexer.get_gem("example").is_none(),
            "a registry installation must never replace a Git-locked dependency"
        );
    }

    #[test]
    fn unrelated_global_version_is_unavailable_without_exact_locked_source() {
        let workspace = TempDir::new().unwrap();
        let extraction_cache = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        let global_path = workspace.path().join("global/example-9.0.0");
        std::fs::create_dir_all(global_path.join("lib")).unwrap();
        let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
        indexer.discovered_gems.insert(
            "example".to_string(),
            vec![GemInfo {
                name: "example".to_string(),
                version: "9.0.0".to_string(),
                platform: "ruby".to_string(),
                locked_version: "9.0.0".to_string(),
                source: GemSource::GlobalInstalled,
                path: global_path.clone(),
                lib_paths: vec![global_path.join("lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        indexer.discover_cached_gem_archives().unwrap();

        assert!(
            indexer.get_gem("example").is_none(),
            "a different globally installed version must never substitute for the lockfile identity"
        );
        assert!(
            !indexer.has_gem("example"),
            "availability accessors must not expose rejected candidates"
        );
    }

    #[test]
    fn lock_parser_preserves_source_identity_and_dependencies() {
        let identities = parse_locked_gems(
            "GIT\n  remote: https://example.test/tool.git\n  revision: abcdef123456\n  specs:\n    tool (2.0.0)\n      rack\n\nPATH\n  remote: components/local\n  specs:\n    local (0.4.0)\n\nGEM\n  specs:\n    rack (3.1.0)\n      base64\n",
        )
        .unwrap();

        assert_eq!(
            identities,
            vec![
                LockedGemIdentity {
                    name: "tool".to_string(),
                    locked_version: "2.0.0".to_string(),
                    source: LockedGemSource::Git,
                    dependencies: vec!["rack".to_string()],
                },
                LockedGemIdentity {
                    name: "local".to_string(),
                    locked_version: "0.4.0".to_string(),
                    source: LockedGemSource::Path,
                    dependencies: Vec::new(),
                },
                LockedGemIdentity {
                    name: "rack".to_string(),
                    locked_version: "3.1.0".to_string(),
                    source: LockedGemSource::Registry,
                    dependencies: vec!["base64".to_string()],
                },
            ]
        );
    }

    #[test]
    fn valid_ruby_and_java_lock_variants_are_not_duplicate_source_identities() {
        let workspace = TempDir::new().unwrap();
        std::fs::write(
            workspace.path().join("Gemfile.lock"),
            "GEM\n  specs:\n    bcrypt-ruby (3.0.1)\n    bcrypt-ruby (3.0.1-java)\n\nPLATFORMS\n  java\n  ruby\n",
        )
        .unwrap();
        let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
        indexer.active_ruby_engine = ActiveRubyEngine::JRuby;

        indexer
            .load_locked_gems()
            .expect("Bundler multi-platform variants must be accepted");
        assert_eq!(
            indexer.locked_gems["bcrypt-ruby"].locked_version, "3.0.1-java",
            "JRuby must select the java lock variant"
        );
    }

    #[test]
    fn explicitly_included_unlocked_gem_uses_active_ruby_highest_version() {
        let mut indexer = create_test_indexer();
        indexer.set_explicitly_included_gems(HashSet::from(["example".to_string()]));
        indexer.discovered_gems.insert(
            "example".to_string(),
            ["1.2.3", "2.0.0"]
                .into_iter()
                .map(|version| GemInfo {
                    name: "example".to_string(),
                    version: version.to_string(),
                    platform: "ruby".to_string(),
                    locked_version: version.to_string(),
                    source: GemSource::GlobalInstalled,
                    path: PathBuf::from(format!("/global/example-{version}")),
                    lib_paths: vec![PathBuf::from(format!("/global/example-{version}/lib"))],
                    dependencies: Vec::new(),
                    is_default: false,
                })
                .collect(),
        );

        assert_eq!(indexer.get_gem("example").unwrap().version, "2.0.0");
    }

    #[test]
    fn test_version_comparison() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
    }

    #[test]
    fn test_required_gems_include_transitive_dependencies() {
        let mut indexer = create_test_indexer();
        indexer.set_required_gems(HashSet::from(["rspec".to_string()]));
        indexer.discovered_gems.insert(
            "rspec".to_string(),
            vec![GemInfo {
                name: "rspec".to_string(),
                version: "3.13.2".to_string(),
                platform: "ruby".to_string(),
                locked_version: "3.13.2".to_string(),
                source: GemSource::BundlerInstalled,
                path: PathBuf::from("/tmp/rspec"),
                lib_paths: vec![PathBuf::from("/tmp/rspec/lib")],
                dependencies: vec![
                    "rspec-core".to_string(),
                    "rspec-expectations".to_string(),
                    "rspec-mocks".to_string(),
                ],
                is_default: false,
            }],
        );
        indexer.discovered_gems.insert(
            "rspec-core".to_string(),
            vec![GemInfo {
                name: "rspec-core".to_string(),
                version: "3.13.6".to_string(),
                platform: "ruby".to_string(),
                locked_version: "3.13.6".to_string(),
                source: GemSource::BundlerInstalled,
                path: PathBuf::from("/tmp/rspec-core"),
                lib_paths: vec![PathBuf::from("/tmp/rspec-core/lib")],
                dependencies: vec!["rspec-support".to_string()],
                is_default: false,
            }],
        );
        indexer.discovered_gems.insert(
            "rspec-support".to_string(),
            vec![GemInfo {
                name: "rspec-support".to_string(),
                version: "3.13.7".to_string(),
                platform: "ruby".to_string(),
                locked_version: "3.13.7".to_string(),
                source: GemSource::BundlerInstalled,
                path: PathBuf::from("/tmp/rspec-support"),
                lib_paths: vec![PathBuf::from("/tmp/rspec-support/lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );

        let gems = indexer.required_gems_with_dependencies();

        assert_eq!(
            gems,
            vec![
                "rspec".to_string(),
                "rspec-core".to_string(),
                "rspec-expectations".to_string(),
                "rspec-mocks".to_string(),
                "rspec-support".to_string(),
            ]
        );
    }

    #[test]
    fn test_excluded_gems_win_over_roots_and_transitive_dependencies() {
        let mut indexer = create_test_indexer();
        indexer.set_required_gems(HashSet::from(["rails".to_string(), "debug".to_string()]));
        indexer.set_excluded_gems(HashSet::from([
            "debug".to_string(),
            "activesupport".to_string(),
        ]));
        indexer.discovered_gems.insert(
            "rails".to_string(),
            vec![GemInfo {
                name: "rails".to_string(),
                version: "8.0.0".to_string(),
                platform: "ruby".to_string(),
                locked_version: "8.0.0".to_string(),
                source: GemSource::BundlerInstalled,
                path: PathBuf::from("/tmp/rails"),
                lib_paths: vec![PathBuf::from("/tmp/rails/lib")],
                dependencies: vec!["activesupport".to_string()],
                is_default: false,
            }],
        );

        assert_eq!(indexer.required_gems_with_dependencies(), vec!["rails"]);
    }

    #[test]
    fn test_workspace_ruby_path_uses_rvm_ruby_version_file() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join(".ruby-version"), "ruby-3.3.11\n").unwrap();
        let fake_home = temp_dir.path().join("home");
        let ruby_path = fake_home
            .join(".rvm")
            .join("rubies")
            .join("ruby-3.3.11")
            .join("bin")
            .join("ruby");
        std::fs::create_dir_all(ruby_path.parent().unwrap()).unwrap();
        std::fs::write(&ruby_path, "").unwrap();

        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);
        let detected = workspace_ruby_path(temp_dir.path());
        if let Some(home) = old_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(detected, Some(ruby_path));
    }

    #[test]
    fn workspace_ruby_path_supports_rvm_jruby_version_file() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
        let fake_home = temp_dir.path().join("home");
        let ruby_path = fake_home
            .join(".rvm")
            .join("rubies")
            .join("jruby-9.2.21.0")
            .join("bin")
            .join("ruby");
        std::fs::create_dir_all(ruby_path.parent().unwrap()).unwrap();
        std::fs::write(&ruby_path, "").unwrap();

        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &fake_home);
        let detected = workspace_ruby_path(temp_dir.path());
        if let Some(home) = old_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(detected, Some(ruby_path));
    }
}
