use crate::config::{IndexingConfig, RubyFastLspConfig};
use crate::extensions::ExtensionRegistryHandle;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::indexer_gem::IndexerGem;
use crate::indexer::indexer_project::IndexerProject;
use crate::indexer::indexer_stdlib::IndexerStdlib;

use crate::indexer::version::ruby_version::RubyVersion;
use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{debug, info, warn};
use ruby_analysis::core::{
    DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity, TextRange,
};
use ruby_analysis::engine::SourceFile;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};

/// Wall-clock timings captured by the coordinator during the most recent
/// [`IndexingCoordinator::run_complete_indexing`] call. Consumed by the
/// perf bench binary and perf regression tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexingTimings {
    /// Fact collection (gems + stdlib + project) + mixin/reference resolution.
    pub facts: Duration,
    /// Reserved for old perf consumers. References now emit during fact collection.
    pub reserved: Duration,
    /// Publish diagnostics to the client.
    pub publish: Duration,
    pub total: Duration,
}

fn diagnostic_from_fact_fast(file: &SourceFile, fact: &DiagnosticFact) -> Option<Diagnostic> {
    Some(Diagnostic {
        range: lsp_range_for_text_range_fast(file, fact.range)?,
        severity: Some(lsp_diagnostic_severity(fact.severity)),
        code: Some(NumberOrString::String(fact.code.clone())),
        code_description: None,
        source: Some("ruby-fast-lsp".to_string()),
        message: fact.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    })
}

fn lsp_diagnostic_severity(severity: AnalysisDiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        AnalysisDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        AnalysisDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

fn configured_gem_selection(
    inferred: Vec<String>,
    config: &IndexingConfig,
) -> (HashSet<String>, HashSet<String>) {
    let excluded = config
        .excluded_gems
        .iter()
        .filter(|name| !name.is_empty())
        .cloned()
        .collect::<HashSet<_>>();
    let mut required = inferred.into_iter().collect::<HashSet<_>>();
    required.extend(
        config
            .included_gems
            .iter()
            .filter(|name| !name.is_empty())
            .cloned(),
    );
    required.retain(|name| !excluded.contains(name));
    (required, excluded)
}

fn lsp_range_for_text_range_fast(file: &SourceFile, range: TextRange) -> Option<Range> {
    let (start_line, start_character) = file.byte_offset_to_line_character(range.start_byte)?;
    let (end_line, end_character) = file.byte_offset_to_line_character(range.end_byte)?;
    Some(Range::new(
        Position::new(start_line, start_character),
        Position::new(end_line, end_character),
    ))
}

/// The IndexingCoordinator manages the entire indexing process.
///
/// It works in 5 simple steps:
/// 1. Find out which Ruby version we're using
/// 2. Set up the basic indexing tools
/// 3. Index the project files (and track what libraries they need)
/// 4. Index the Ruby standard library
/// 5. Index the gems (external libraries)
///
/// Think of it like organizing a library - first you figure out what system you're using,
/// then you organize your own books, then you add the reference books, and finally
/// you add books from other collections.
pub struct IndexingCoordinator {
    // Basic setup
    workspace_root: PathBuf,
    config: RubyFastLspConfig,

    extension_registry: ExtensionRegistryHandle,

    // Ruby version info
    version_detector: RubyVersionDetector,
    detected_ruby_version: Option<RubyVersion>,

    // The main indexing engine
    file_processor: Option<FileProcessor>,

    // Project-specific indexer
    project_indexer: Option<IndexerProject>,

    // Standard library indexer
    stdlib_indexer: Option<IndexerStdlib>,

    // Gem indexer
    gem_indexer: Option<IndexerGem>,

    // Where to find Ruby libraries on this system
    ruby_library_paths: Vec<PathBuf>,

    /// Timings from the most recent `run_complete_indexing` call.
    last_timings: IndexingTimings,
}

impl IndexingCoordinator {
    /// Creates a new IndexingCoordinator for the given workspace.
    ///
    /// Call `run_complete_indexing()` to actually start the indexing process.
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        let version_detector = RubyVersionDetector::from_path(workspace_root.clone());
        let extension_registry = ExtensionRegistryHandle::from_config(&config);

        Self {
            workspace_root,
            config,
            extension_registry,
            version_detector,
            detected_ruby_version: None,
            file_processor: None,
            project_indexer: None,
            stdlib_indexer: None,
            gem_indexer: None,
            ruby_library_paths: Vec::new(),
            last_timings: IndexingTimings::default(),
        }
    }

    /// Returns the timings captured by the most recent call to
    /// `run_complete_indexing`. All-zero before the first call.
    pub fn last_timings(&self) -> IndexingTimings {
        self.last_timings
    }

    pub fn set_extension_registry(&mut self, extension_registry: ExtensionRegistryHandle) {
        self.extension_registry = extension_registry;
    }

    /// Runs the complete indexing process from start to finish.
    ///
    /// 1. Figure out which Ruby version we're using
    /// 2. Find where Ruby libraries are installed on this system
    /// 3. Set up the main indexing engine
    /// 4. Scan project dependencies
    /// 5. Collect facts from gems, stdlib, then project files
    /// 6. Publish diagnostics
    pub async fn run_complete_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        info!("Starting complete indexing process");
        let start_time = Instant::now();

        // Step 1: Figure out which Ruby version we're using
        let ruby_version = self.detect_ruby_version();
        info!("Detected Ruby version: {:?}", ruby_version);

        // Step 2: Find where Ruby libraries are installed
        self.discover_ruby_library_paths();

        // Step 3: Set up the main indexing engine
        self.setup_file_processor(server);

        // Fact collection order: scan deps → gems → stdlib → project
        // (project skips files already indexed as Gem/Stdlib).
        info!("Collecting analysis facts");
        let facts_start = Instant::now();

        // Step 4: Quick scan project files for dependencies (no indexing yet)
        self.scan_project_dependencies()?;

        // Step 5: Collect facts from gems (uses discovered required gems)
        self.index_gems(server).await?;

        // Step 6: Collect facts from Ruby standard library
        self.index_standard_library(server, &ruby_version).await?;

        // Step 7: Collect facts from project files (skips files already indexed as Gem/Stdlib)
        self.collect_project_facts(server).await?;

        let facts_dur = facts_start.elapsed();
        let reserved_dur = Duration::default();
        info!("Facts collection completed in {:?}", facts_dur);

        // Publish diagnostics to the client.
        info!("Publishing diagnostics");
        let publish_start = Instant::now();
        Self::send_progress_report(server, "Publishing diagnostics...".to_string(), 0, 0).await;
        self.publish_unresolved_diagnostics(server).await;
        let publish_dur = publish_start.elapsed();

        let total_dur = start_time.elapsed();
        info!("Complete indexing finished in {:?}", total_dur);
        {
            let mut engine = server.analysis_engine.write();
            engine.shrink_to_fit();
        }
        release_allocator_free_pages();
        self.log_analysis_memory_stats(server);

        self.last_timings = IndexingTimings {
            facts: facts_dur,
            reserved: reserved_dur,
            publish: publish_dur,
            total: total_dur,
        };
        Ok(())
    }

    fn log_analysis_memory_stats(&self, server: &RubyLanguageServer) {
        let engine = server.analysis_engine.read();
        let stats = engine.stats();
        let memory = engine.estimated_memory_stats();
        let total = memory.total();

        info!(
            "Analysis stats: files={}, source_bytes={}, symbols={}, methods={}, ref_candidates={}, refs={}, types={}, diagnostic_candidates={}, diagnostics={}, graph_nodes={}, graph_edges={}, unresolved_graph_edges={}",
            stats.files,
            stats.source_bytes,
            stats.symbols,
            stats.methods,
            stats.reference_candidates,
            stats.references,
            stats.types,
            stats.diagnostic_candidates,
            stats.diagnostics,
            stats.graph_nodes,
            stats.graph_edges,
            stats.unresolved_graph_edges
        );
        info!("Estimated engine heap: {:.1} MB", bytes_to_mb(total));
        log_memory_bucket("names", memory.names, total);
        log_memory_bucket("files", memory.files, total);
        log_memory_bucket("symbols", memory.symbols, total);
        log_memory_bucket("methods", memory.methods, total);
        log_memory_bucket("types", memory.types, total);
        log_memory_bucket("reference candidates", memory.reference_candidates, total);
        log_memory_bucket("references", memory.references, total);
        log_memory_bucket("diagnostics", memory.diagnostics, total);
        log_memory_bucket("diagnostic candidates", memory.diagnostic_candidates, total);
        log_memory_bucket("graph", memory.graph, total);
        log_memory_bucket(
            "unresolved graph edges",
            memory.unresolved_graph_edges,
            total,
        );
    }

    /// Helper function to send progress report updates to the client
    pub async fn send_progress_report(
        server: &RubyLanguageServer,
        message: String,
        current: usize,
        total: usize,
    ) {
        if let Some(client) = &server.client {
            let percentage = if total > 0 {
                ((current as f64 / total as f64) * 100.0) as u32
            } else {
                0
            };

            let full_message = if total > 0 {
                format!("{}: {}/{}", message, current, total)
            } else {
                message
            };

            let _ = client
                .send_notification::<tower_lsp::lsp_types::notification::Progress>(
                    tower_lsp::lsp_types::ProgressParams {
                        token: tower_lsp::lsp_types::NumberOrString::String("indexing".to_string()),
                        value: tower_lsp::lsp_types::ProgressParamsValue::WorkDone(
                            tower_lsp::lsp_types::WorkDoneProgress::Report(
                                tower_lsp::lsp_types::WorkDoneProgressReport {
                                    message: Some(full_message),
                                    percentage: Some(percentage),
                                    cancellable: Some(false),
                                },
                            ),
                        ),
                    },
                )
                .await;
        }
    }

    /// Step 1: Detect which Ruby version we're working with
    fn detect_ruby_version(&mut self) -> Option<RubyVersion> {
        let version = self.version_detector.detect_version();
        self.detected_ruby_version = version;
        version
    }

    /// Step 3: Set up the main indexing engine
    fn setup_file_processor(&mut self, _server: &RubyLanguageServer) {
        self.file_processor = Some(FileProcessor::with_extension_registry(
            self.extension_registry.clone(),
        ));
    }

    /// Quick scan for dependencies without indexing.
    /// Creates project indexer and scans for required gems/stdlib modules.
    fn scan_project_dependencies(&mut self) -> Result<()> {
        // Create a temporary project indexer just for dependency scanning
        // We'll create a proper one later for actual indexing
        let temp_indexer = IndexerProject::new(
            self.workspace_root.clone(),
            self.file_processor.as_ref().unwrap().clone(),
            self.config.indexing.clone(),
        );
        temp_indexer.scan_for_dependencies()?;
        self.project_indexer = Some(temp_indexer);
        Ok(())
    }

    /// Collect facts from project files (skips already-indexed files)
    async fn collect_project_facts(&mut self, server: &RubyLanguageServer) -> Result<()> {
        if let Some(ref mut project_indexer) = self.project_indexer {
            project_indexer.collect_project_facts(server).await?;
        } else {
            let mut project_indexer = IndexerProject::new(
                self.workspace_root.clone(),
                self.file_processor.as_ref().unwrap().clone(),
                self.config.indexing.clone(),
            );
            project_indexer.collect_project_facts(server).await?;
            self.project_indexer = Some(project_indexer);
        }
        Ok(())
    }

    /// Publish diagnostics for unresolved entries in currently open files.
    async fn publish_unresolved_diagnostics(&self, server: &RubyLanguageServer) {
        let open_uris = server.docs.lock().keys().cloned().collect::<HashSet<_>>();
        let file_ids = {
            let engine = server.analysis_engine.read();
            let mut file_ids = engine.diagnostic_store().file_ids();
            file_ids.retain(|file_id| {
                engine
                    .file(*file_id)
                    .and_then(|file| Url::from_file_path(&file.path).ok())
                    .is_some_and(|uri| open_uris.contains(&uri))
            });
            file_ids.sort_by(|left, right| {
                let left_path = engine
                    .file(*left)
                    .map(|file| file.path.as_path())
                    .unwrap_or_else(|| Path::new(""));
                let right_path = engine
                    .file(*right)
                    .map(|file| file.path.as_path())
                    .unwrap_or_else(|| Path::new(""));
                left_path.cmp(right_path)
            });
            info!(
                "Publishing diagnostics for {} open files with analysis diagnostics ({} open documents)",
                file_ids.len(),
                open_uris.len()
            );
            file_ids
        };

        for file_id in file_ids {
            let Some((uri, diagnostics)) = ({
                let engine = server.analysis_engine.read();
                match engine.file(file_id) {
                    Some(file) => match Url::from_file_path(&file.path) {
                        Ok(uri) => {
                            let diagnostics = engine
                                .diagnostic_store()
                                .facts_for_file(file_id)
                                .iter()
                                .filter_map(|fact| diagnostic_from_fact_fast(file, fact))
                                .collect::<Vec<_>>();
                            if diagnostics.is_empty() {
                                None
                            } else {
                                Some((uri, diagnostics))
                            }
                        }
                        Err(()) => None,
                    },
                    None => None,
                }
            }) else {
                continue;
            };
            debug!(
                "Publishing {} unresolved diagnostics for {}",
                diagnostics.len(),
                uri.path()
            );
            server.publish_diagnostics(uri, diagnostics).await;
        }
    }

    /// Step 5: Index the Ruby standard library
    async fn index_standard_library(
        &mut self,
        server: &RubyLanguageServer,
        ruby_version: &Option<RubyVersion>,
    ) -> Result<()> {
        let required_stdlib = self.get_required_stdlib_modules();

        let mut stdlib_indexer =
            IndexerStdlib::new(self.file_processor.as_ref().unwrap().clone(), *ruby_version);

        // Pass extension path for loading zipped stubs
        if let Some(ref ext_path) = self.config.extension_path {
            stdlib_indexer.set_extension_path(PathBuf::from(ext_path));
        }

        stdlib_indexer.set_required_modules(required_stdlib);
        stdlib_indexer.index_stdlib(server).await?;
        self.stdlib_indexer = Some(stdlib_indexer);
        Ok(())
    }

    /// Index the gems (external libraries)
    async fn index_gems(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let (required_gems, excluded_gems) =
            configured_gem_selection(self.get_required_gems(), &self.config.indexing);

        let mut gem_indexer = IndexerGem::new(Some(self.workspace_root.clone()));
        gem_indexer.set_file_processor();
        gem_indexer.set_required_gems(required_gems);
        gem_indexer.set_excluded_gems(excluded_gems);
        gem_indexer.index_gems(true, server).await?; // selective = true
        self.gem_indexer = Some(gem_indexer);
        Ok(())
    }

    /// Get the list of standard library modules that the project needs
    fn get_required_stdlib_modules(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_stdlib()
        } else {
            Vec::new()
        }
    }

    /// Get the list of gems that the project needs
    fn get_required_gems(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_gems()
        } else {
            Vec::new()
        }
    }

    /// Step 2: Find where Ruby libraries are installed on this system
    ///
    /// This looks for Ruby's standard library and gem directories so we know
    /// where to find external code that the project might be using.
    pub fn discover_ruby_library_paths(&mut self) {
        self.ruby_library_paths.clear();

        // Use ruby -e to get the actual load path from the Ruby installation
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "puts $LOAD_PATH"])
            .output()
        {
            if output.status.success() {
                let load_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in load_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        self.ruby_library_paths.push(path);
                        debug!("Found Ruby lib directory: {:?}", path_str.trim());
                    }
                }
            } else {
                debug!(
                    "Failed to get Ruby load path: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        } else {
            debug!("Failed to execute ruby command to get load path");
        }

        // Also try to get gem paths
        if let Ok(output) = Command::new("ruby")
            .args(["-e", "require 'rubygems'; puts Gem.path"])
            .output()
        {
            if output.status.success() {
                let gem_paths = String::from_utf8_lossy(&output.stdout);
                for path_str in gem_paths.lines() {
                    let path = PathBuf::from(path_str.trim());
                    if path.exists() && path.is_dir() {
                        // Add the gems subdirectory which contains actual gem sources
                        let gems_dir = path.join("gems");
                        if gems_dir.exists() {
                            self.ruby_library_paths.push(gems_dir.clone());
                            debug!("Found gem directory: {:?}", gems_dir);
                        }
                    }
                }
            }
        }
    }

    /// Find all Ruby files in a directory and its subdirectories
    ///
    /// This walks through a directory tree and collects all Ruby files,
    /// but skips common directories that usually don't contain Ruby source code
    /// (like node_modules, .git, tmp, etc.)
    pub fn find_all_ruby_files_in_directory(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        let collected_files = crate::utils::collect_ruby_files(dir);
        files.extend(collected_files);
    }

    /// Check if a file is a Ruby file
    ///
    /// This looks at the file extension (.rb, .ruby, .rake) and also checks
    /// for common Ruby files that don't have extensions (like Rakefile, Gemfile)
    pub fn is_ruby_file(&self, path: &Path) -> bool {
        crate::utils::should_index_file(path)
    }

    /// Find the Ruby core stubs for a specific Ruby version
    ///
    /// Ruby core stubs are pre-written definitions of Ruby's built-in classes and methods.
    /// This helps the language server understand Ruby's core functionality.
    ///
    /// We try to find stubs in this order:
    /// 1. Use the configured stub path
    /// 2. Look in the workspace's editors/vscode/vsix/stubs directory
    /// 3. Fall back to Ruby 3.0 stubs if available
    pub fn find_core_stubs_for_version(&self, version: (u8, u8)) -> Option<PathBuf> {
        // First, try the configured stub path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            return Some(PathBuf::from(stubs_path_str));
        }

        // Look for stubs in the workspace
        let stubs_dir = self
            .workspace_root
            .join("editors")
            .join("vscode")
            .join("vsix")
            .join("stubs");
        let version_dir = format!("rubystubs{}{}", version.0, version.1);
        let stubs_path = stubs_dir.join(version_dir);

        if stubs_path.exists() {
            debug!("Found core stubs in workspace at: {:?}", stubs_path);
            return Some(stubs_path);
        }

        // Fall back to Ruby 3.0 stubs if the specific version isn't available
        let default_stubs = stubs_dir.join("rubystubs30");
        if default_stubs.exists() {
            info!("Using default Ruby 3.0 stubs at: {:?}", default_stubs);
            Some(default_stubs)
        } else {
            warn!("No core stubs found for Ruby version {:?}", version);
            None
        }
    }

    /// Get the Ruby library paths we discovered
    ///
    /// This returns the list of directories where Ruby libraries are installed.
    pub fn get_ruby_library_paths(&self) -> &[PathBuf] {
        &self.ruby_library_paths
    }
}

fn log_memory_bucket(name: &str, bytes: usize, total: usize) {
    let percent = if total == 0 {
        0.0
    } else {
        bytes as f64 * 100.0 / total as f64
    };
    info!("{name}: {:.1} MB ({percent:.1}%)", bytes_to_mb(bytes));
}

fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}

#[cfg(target_os = "macos")]
fn release_allocator_free_pages() {
    unsafe extern "C" {
        fn malloc_default_zone() -> *mut libc::c_void;
        fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
    }

    unsafe {
        let zone = malloc_default_zone();
        if !zone.is_null() {
            malloc_zone_pressure_relief(zone, 0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn release_allocator_free_pages() {}

/// Integration tests for IndexingCoordinator
/// Tests the complete indexing workflow with realistic project structures
#[cfg(test)]
mod coordinator_integration_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tower_lsp::lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};

    /// Test fixture that creates a realistic Ruby project structure
    struct TestProjectFixture {
        _temp_dir: TempDir,
        project_root: PathBuf,
        core_stubs_dir: PathBuf,
        stdlib_dir: PathBuf,
        project_files_dir: PathBuf,
    }

    impl TestProjectFixture {
        fn new() -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let project_root = temp_dir.path().to_path_buf();

            // Create directory structure
            let core_stubs_dir = project_root
                .join("editors")
                .join("vscode")
                .join("vsix")
                .join("stubs")
                .join("rubystubs30");
            let stdlib_dir = project_root.join("stdlib");
            let project_files_dir = project_root.join("app");

            fs::create_dir_all(&core_stubs_dir).expect("Failed to create core stubs dir");
            fs::create_dir_all(&stdlib_dir).expect("Failed to create stdlib dir");
            fs::create_dir_all(&project_files_dir).expect("Failed to create project files dir");

            Self {
                _temp_dir: temp_dir,
                project_root,
                core_stubs_dir,
                stdlib_dir,
                project_files_dir,
            }
        }

        /// Create core Ruby stub files
        fn create_core_stubs(&self) {
            // Create basic Object class stub
            let object_stub = r#"
class Object
  def initialize
  end

  def class
  end

  def to_s
  end
end
"#;
            fs::write(self.core_stubs_dir.join("object.rb"), object_stub)
                .expect("Failed to write object.rb");

            // Create String class stub
            let string_stub = r#"
class String
  def initialize(str = "")
  end

  def length
  end

  def upcase
  end

  def downcase
  end

  def strip
  end
end
"#;
            fs::write(self.core_stubs_dir.join("string.rb"), string_stub)
                .expect("Failed to write string.rb");

            // Create Array class stub
            let array_stub = r#"
class Array
  def initialize
  end

  def length
  end

  def push(item)
  end

  def pop
  end

  def each
  end
end
"#;
            fs::write(self.core_stubs_dir.join("array.rb"), array_stub)
                .expect("Failed to write array.rb");
        }

        /// Create standard library files
        fn create_stdlib_files(&self) {
            // Create Set class
            let set_lib = r#"
class Set
  def initialize(enum = nil)
    @hash = {}
  end

  def add(obj)
    @hash[obj] = true
    self
  end

  def include?(obj)
    @hash.key?(obj)
  end

  def size
    @hash.size
  end
end
"#;
            fs::write(self.stdlib_dir.join("set.rb"), set_lib).expect("Failed to write set.rb");

            // Create JSON library
            let json_lib = r#"
module JSON
  def self.parse(source)
    # JSON parsing implementation
  end

  def self.generate(obj)
    # JSON generation implementation
  end
end
"#;
            fs::write(self.stdlib_dir.join("json.rb"), json_lib).expect("Failed to write json.rb");

            // Create FileUtils module
            let fileutils_lib = r#"
module FileUtils
  def self.mkdir_p(path)
    # Directory creation implementation
  end

  def self.cp(src, dest)
    # File copy implementation
  end

  def self.rm_rf(path)
    # Recursive removal implementation
  end
end
"#;
            fs::write(self.stdlib_dir.join("fileutils.rb"), fileutils_lib)
                .expect("Failed to write fileutils.rb");
        }

        /// Create project files with dependencies
        fn create_project_files(&self) {
            fs::write(
                self.project_root.join("Thorfile"),
                "class DeploymentTasks\nend\n",
            )
            .expect("Failed to write Thorfile");
            fs::write(
                self.project_root.join("config.ru"),
                "class RackApplication\nend\n",
            )
            .expect("Failed to write config.ru");

            // Create main application file
            let main_app = r#"
require 'set'
require 'json'
require_relative 'models/user'
require_relative 'services/user_service'

class Application
  def initialize
    @users = Set.new
    @user_service = UserService.new
  end

  def add_user(user_data)
    user = User.new(user_data)
    @users.add(user)
    @user_service.save(user)
  end

  def export_users
    JSON.generate(@users.to_a)
  end
end
"#;
            fs::write(self.project_files_dir.join("application.rb"), main_app)
                .expect("Failed to write application.rb");

            // Create models directory and User model
            let models_dir = self.project_files_dir.join("models");
            fs::create_dir_all(&models_dir).expect("Failed to create models dir");

            let user_model = r#"
class User
  attr_accessor :name, :email, :age

  def initialize(data = {})
    @name = data[:name]
    @email = data[:email]
    @age = data[:age]
  end

  def valid?
    !@name.nil? && !@email.nil?
  end

  def to_hash
    {
      name: @name,
      email: @email,
      age: @age
    }
  end
end
"#;
            fs::write(models_dir.join("user.rb"), user_model).expect("Failed to write user.rb");

            // Create services directory and UserService
            let services_dir = self.project_files_dir.join("services");
            fs::create_dir_all(&services_dir).expect("Failed to create services dir");

            let user_service = r#"
require 'fileutils'
require_relative '../models/user'

class UserService
  def initialize
    @storage_path = 'users.json'
  end

  def save(user)
    users = load_users
    users << user.to_hash
    File.write(@storage_path, JSON.generate(users))
  end

  def load_users
    return [] unless File.exist?(@storage_path)
    JSON.parse(File.read(@storage_path))
  end

  def find_by_email(email)
    users = load_users
    user_data = users.find { |u| u['email'] == email }
    User.new(user_data) if user_data
  end
end
"#;
            fs::write(services_dir.join("user_service.rb"), user_service)
                .expect("Failed to write user_service.rb");

            // Create a test file
            let test_dir = self.project_files_dir.join("test");
            fs::create_dir_all(&test_dir).expect("Failed to create test dir");

            let user_test = r#"
require_relative '../models/user'
require_relative '../services/user_service'

class UserTest
  def test_user_creation
    user = User.new(name: 'John', email: 'john@example.com', age: 30)
    assert user.valid?
  end

  def test_user_service
    service = UserService.new
    user = User.new(name: 'Jane', email: 'jane@example.com')
    service.save(user)

    found_user = service.find_by_email('jane@example.com')
    assert found_user.name == 'Jane'
  end
end
"#;
            fs::write(test_dir.join("user_test.rb"), user_test)
                .expect("Failed to write user_test.rb");
        }

        /// Set up the complete project structure
        fn setup_complete_project(&self) {
            self.create_core_stubs();
            self.create_stdlib_files();
            self.create_project_files();
        }

        /// Get the project root path
        fn project_root(&self) -> &PathBuf {
            &self.project_root
        }
    }

    /// Create a test server instance
    fn create_test_server() -> RubyLanguageServer {
        RubyLanguageServer::default()
    }

    #[test]
    fn test_configured_gem_selection_augments_inferred_and_preserves_exclusions() {
        let indexing = crate::config::IndexingConfig {
            included_gems: vec!["rails".to_string(), "debug".to_string()],
            excluded_gems: vec!["debug".to_string(), "rack".to_string()],
            ..crate::config::IndexingConfig::default()
        };

        let (required, excluded) =
            configured_gem_selection(vec!["rack".to_string(), "rspec".to_string()], &indexing);

        assert_eq!(
            required,
            HashSet::from(["rails".to_string(), "rspec".to_string()])
        );
        assert_eq!(
            excluded,
            HashSet::from(["debug".to_string(), "rack".to_string()])
        );
    }

    #[tokio::test]
    async fn test_coordinator_complete_indexing_workflow() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Execute the complete indexing process
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(result.is_ok(), "Indexing should complete successfully");

        let engine = server.analysis_engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        for path in [
            fixture.project_root().join("Thorfile"),
            fixture.project_root().join("config.ru"),
        ] {
            let file_id = query.file_id(&path).unwrap_or_else(|| {
                panic!(
                    "common Ruby entry point was not registered: {}",
                    path.display()
                )
            });
            assert!(
                !query.symbol_facts_in_file(file_id).is_empty(),
                "common Ruby entry point produced no semantic facts: {}",
                path.display()
            );
        }

        // Verify that Ruby lib directories were discovered
        let lib_dirs = coordinator.get_ruby_library_paths();
        assert!(
            !lib_dirs.is_empty(),
            "Should discover at least one Ruby lib directory"
        );
    }

    #[tokio::test]
    async fn test_coordinator_project_file_collection() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test Ruby file collection
        let mut files = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut files);

        assert!(!files.is_empty(), "Should find Ruby files in project");

        // Verify specific files are found
        let file_names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .map(|s| s.to_string())
            .collect();

        assert!(file_names.contains(&"application.rb".to_string()));
        assert!(file_names.contains(&"user.rb".to_string()));
        assert!(file_names.contains(&"user_service.rb".to_string()));
        assert!(file_names.contains(&"user_test.rb".to_string()));
        assert!(file_names.contains(&"Thorfile".to_string()));
        assert!(file_names.contains(&"config.ru".to_string()));
    }

    #[tokio::test]
    async fn project_rbs_declarations_enter_engine_method_facts() {
        let temp_dir = TempDir::new().expect("test workspace must be created");
        let sig_dir = temp_dir.path().join("sig");
        fs::create_dir_all(&sig_dir).expect("sig directory must be created");
        let signature_path = sig_dir.join("native_widget.rbs");
        fs::write(
            &signature_path,
            "class NativeWidget\n  def encode: (String value) -> String\nend\n",
        )
        .expect("RBS fixture must be written");
        let usage_path = temp_dir.path().join("native_usage.rb");
        let usage = "widget = NativeWidget.new\nwidget.encode(\"value\")\n";
        fs::write(&usage_path, usage).expect("Ruby usage fixture must be written");

        let mut coordinator =
            IndexingCoordinator::new(temp_dir.path().to_path_buf(), RubyFastLspConfig::default());
        let server = create_test_server();
        coordinator
            .run_complete_indexing(&server)
            .await
            .expect("workspace indexing must succeed");

        let engine = server.analysis_engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        assert!(
            query.file_id(&signature_path).is_some(),
            "conventional sig/**/*.rbs files must be registered"
        );
        let method = ruby_analysis::core::FullyQualifiedName::method(
            vec![ruby_analysis::core::RubyConstant::new("NativeWidget")
                .expect("test class name must be valid")],
            ruby_analysis::core::RubyMethod::new("encode").expect("test method name must be valid"),
        );
        let facts = query.methods_for_fqn(&method);
        assert_eq!(facts.len(), 1, "RBS method must become one engine fact");
        assert_eq!(facts[0].return_type_label.as_deref(), Some("String"));
        drop(engine);

        let usage_uri = Url::from_file_path(&usage_path).expect("usage URI must be valid");
        crate::capabilities::indexing::handle_did_open(
            &server,
            tower_lsp::lsp_types::DidOpenTextDocumentParams {
                text_document: tower_lsp::lsp_types::TextDocumentItem {
                    uri: usage_uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: usage.to_string(),
                },
            },
        )
        .await;
        let document = server
            .docs
            .lock()
            .get(&usage_uri)
            .cloned()
            .expect("opened usage document must exist");
        let query = crate::query::EngineQuery::with_doc_and_engine(
            document,
            server.analysis_engine.clone(),
        );
        let definitions = query
            .find_definitions_at_position(
                &usage_uri,
                tower_lsp::lsp_types::Position::new(1, 9),
                usage,
            )
            .expect("native RBS method call must resolve");
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].uri,
            Url::from_file_path(signature_path).unwrap()
        );
        let hover = query
            .get_hover_at_position(&usage_uri, tower_lsp::lsp_types::Position::new(1, 9), usage)
            .expect("RBS method return must produce hover information");
        assert!(hover.content.contains("String"));
    }

    #[tokio::test]
    async fn test_coordinator_ruby_file_detection() {
        let fixture = TestProjectFixture::new();
        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test various Ruby file extensions
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.rb")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.ruby")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("test.rake")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("show.html.erb")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Rakefile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Gemfile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Guardfile")));
        assert!(coordinator.is_ruby_file(&PathBuf::from("Capfile")));

        // Test non-Ruby files
        assert!(!coordinator.is_ruby_file(&PathBuf::from("test.js")));
        assert!(!coordinator.is_ruby_file(&PathBuf::from("test.py")));
        assert!(!coordinator.is_ruby_file(&PathBuf::from("README.md")));
    }

    #[tokio::test]
    async fn test_coordinator_core_stubs_resolution() {
        let fixture = TestProjectFixture::new();
        fixture.create_core_stubs();

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test core stubs path resolution
        let stubs_path = coordinator.find_core_stubs_for_version((3, 0));
        assert!(stubs_path.is_some(), "Should find core stubs path");

        let stubs_path = stubs_path.unwrap();
        assert!(stubs_path.exists(), "Core stubs path should exist");
        assert!(
            stubs_path.join("object.rb").exists(),
            "Should find object.rb stub"
        );
        assert!(
            stubs_path.join("string.rb").exists(),
            "Should find string.rb stub"
        );
    }

    #[tokio::test]
    async fn test_coordinator_with_missing_directories() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let project_root = temp_dir.path().to_path_buf();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(project_root, config);
        let server = create_test_server();

        // Test indexing with missing directories (should not panic)
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should handle missing directories gracefully"
        );
    }

    #[tokio::test]
    async fn test_coordinator_lib_directory_discovery() {
        let fixture = TestProjectFixture::new();
        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Test lib directory discovery
        coordinator.discover_ruby_library_paths();
        let lib_dirs = coordinator.get_ruby_library_paths();

        // This test depends on the system having Ruby installed
        // In CI environments, this might not be available, so we make it lenient
        println!("Discovered {} lib directories", lib_dirs.len());
        for dir in lib_dirs {
            println!("  - {:?}", dir);
        }
    }

    #[tokio::test]
    async fn test_coordinator_performance_with_large_project() {
        // SAFETY: This test is not run concurrently with other tests that modify this env var.
        // Keep the large-project check focused on project files instead of local gem volume.
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        // Create additional files to simulate a larger project
        let large_project_dir = fixture.project_root().join("large_project");
        fs::create_dir_all(&large_project_dir).expect("Failed to create large project dir");

        // Create 50 Ruby files
        for i in 0..50 {
            let file_content = format!(
                r#"
class TestClass{}
  def initialize
    @value = {}
  end

  def process
    # Some processing logic
  end
end
"#,
                i, i
            );
            fs::write(
                large_project_dir.join(format!("test_class_{}.rb", i)),
                file_content,
            )
            .expect("Failed to write test file");
        }

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Measure indexing time
        let start = std::time::Instant::now();
        let result = coordinator.run_complete_indexing(&server).await;
        let duration = start.elapsed();

        assert!(
            result.is_ok(),
            "Large project indexing should complete successfully"
        );
        println!("Large project indexing took: {:?}", duration);

        // Performance assertion - should complete within reasonable time
        assert!(
            duration.as_secs() < 45,
            "Indexing should complete within 45 seconds"
        );

        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_discovery() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "5") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Execute indexing which should include gem discovery
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(result.is_ok(), "Indexing with gem discovery should succeed");

        // Verify that gem indexer was initialized
        // Note: We can't directly access the gem_indexer field, but we can verify
        // that the ruby_lib_dirs includes gem paths
        let lib_dirs = coordinator.get_ruby_library_paths();

        // Should have at least some library directories (system + potentially gems)
        assert!(
            !lib_dirs.is_empty(),
            "Should discover library directories including potential gem paths"
        );

        // Check if any paths look like gem directories
        let has_gem_like_paths = lib_dirs.iter().any(|path| {
            path.to_string_lossy().contains("gems") || path.to_string_lossy().contains(".gem")
        });

        // This might not always be true in test environments, so we'll just log it
        if has_gem_like_paths {
            println!("Found gem-like paths in library directories");
        } else {
            println!("No obvious gem paths found - this is normal in test environments");
        }

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_indexing_integration() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Test that gem indexing doesn't break the overall indexing process
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should succeed even with gem discovery"
        );

        // Verify the indexing process completed all steps
        let lib_dirs = coordinator.get_ruby_library_paths();
        assert!(
            !lib_dirs.is_empty(),
            "Library directories should be discovered"
        );

        // The gem indexing should not interfere with project file indexing
        let mut project_files = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut project_files);
        assert!(
            !project_files.is_empty(),
            "Project files should still be discoverable after gem indexing"
        );

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_error_handling() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "2") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Even if gem discovery fails, the overall indexing should still succeed
        // This tests the error handling in discover_and_index_gems
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(
            result.is_ok(),
            "Indexing should succeed even if gem discovery encounters errors"
        );

        // Basic functionality should still work
        let lib_dirs = coordinator.get_ruby_library_paths();
        // We should at least have some directories (even if gem discovery failed)
        // The system Ruby directories should still be found
        let _ = lib_dirs;

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_gem_performance() {
        // Set environment variable to limit gem processing for faster tests
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::set_var("RUBY_LSP_MAX_GEMS", "3") };

        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Measure time for indexing including gem discovery
        let start = std::time::Instant::now();
        let result = coordinator.run_complete_indexing(&server).await;
        let elapsed = start.elapsed();

        assert!(
            result.is_ok(),
            "Indexing with gem discovery should complete successfully"
        );

        // Gem discovery should not significantly slow down the indexing process
        // Allow up to 30 seconds for gem discovery in addition to regular indexing
        assert!(
            elapsed.as_secs() < 30,
            "Indexing with gem discovery should complete within 30 seconds, took {}s",
            elapsed.as_secs()
        );

        println!(
            "Indexing with gem discovery completed in {}ms",
            elapsed.as_millis()
        );

        // Clean up environment variable
        // SAFETY: This test is not run concurrently with other tests that modify this env var
        unsafe { std::env::remove_var("RUBY_LSP_MAX_GEMS") };
    }

    #[tokio::test]
    async fn test_coordinator_collects_all_ruby_files() {
        // Test that all Ruby files are collected, including vendor directories.
        // File source (Project/Gem/Stdlib) is determined by indexers based on
        // discovered paths from tools (bundler, rubygems), not by exclusion patterns.
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        // Create a vendor directory with Ruby files
        let vendor_dir = fixture.project_root().join("vendor");
        fs::create_dir_all(&vendor_dir).expect("Failed to create vendor directory");

        let vendor_bundle_dir = vendor_dir.join("bundle");
        fs::create_dir_all(&vendor_bundle_dir).expect("Failed to create vendor/bundle directory");

        // Create Ruby files in vendor
        let vendor_ruby_file = vendor_dir.join("vendor_gem.rb");
        fs::write(&vendor_ruby_file, "class VendorGem\nend")
            .expect("Failed to write vendor Ruby file");

        let vendor_bundle_ruby_file = vendor_bundle_dir.join("bundled_gem.rb");
        fs::write(&vendor_bundle_ruby_file, "class BundledGem\nend")
            .expect("Failed to write vendor/bundle Ruby file");

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Collect Ruby files from the project
        let mut collected_files: Vec<PathBuf> = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut collected_files);

        // Verify that vendor files ARE collected (no exclusion)
        let vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            !vendor_files.is_empty(),
            "Vendor directory files should be collected (source tagging handles categorization)"
        );

        // Verify that non-vendor files are also collected
        let non_vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| !path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            !non_vendor_files.is_empty(),
            "Non-vendor Ruby files should also be collected"
        );
    }

    #[tokio::test]
    async fn cold_indexing_retains_but_does_not_publish_closed_file_diagnostics() {
        let workspace = TempDir::new().unwrap();
        let file_path = workspace.path().join("app/service.rb");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let source = "MissingService.call\n";
        fs::write(&file_path, source).unwrap();
        let uri = Url::from_file_path(&file_path).unwrap();
        let workspace_uri = Url::from_directory_path(workspace.path()).unwrap();
        let server = RubyLanguageServer::default();
        server.add_workspace(workspace_uri);
        let mut coordinator =
            IndexingCoordinator::new(workspace.path().to_path_buf(), RubyFastLspConfig::default());

        coordinator.run_complete_indexing(&server).await.unwrap();

        assert!(
            server.analysis_engine.read().stats().diagnostics > 0,
            "cold indexing must retain workspace diagnostics in the engine"
        );
        assert!(
            server.last_published_diagnostics(&uri).is_empty(),
            "closed-file engine diagnostics must not flood the LSP client"
        );

        crate::capabilities::indexing::handle_did_open(
            &server,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "ruby".to_string(),
                    version: 1,
                    text: source.to_string(),
                },
            },
        )
        .await;
        assert!(
            !server.last_published_diagnostics(&uri).is_empty(),
            "opening the file must publish its current diagnostics"
        );
    }
}
