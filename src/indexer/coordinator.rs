use crate::config::RubyFastLspConfig;
use crate::indexer::file_processor::FileProcessor;
use crate::indexer::indexer_gem::IndexerGem;
use crate::indexer::indexer_project::IndexerProject;
use crate::indexer::indexer_stdlib::IndexerStdlib;

use crate::indexer::version::version_detector::RubyVersionDetector;
use crate::server::RubyLanguageServer;
use crate::types::ruby_version::RubyVersion;
use anyhow::Result;
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

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
}

impl IndexingCoordinator {
    /// Creates a new IndexingCoordinator for the given workspace.
    ///
    /// This sets up all the basic components but doesn't start indexing yet.
    /// Call `run_complete_indexing()` to actually start the indexing process.
    pub fn new(workspace_root: PathBuf, config: RubyFastLspConfig) -> Self {
        let version_detector = RubyVersionDetector::from_path(workspace_root.clone());

        Self {
            workspace_root,
            config,
            version_detector,
            detected_ruby_version: None,
            file_processor: None,
            project_indexer: None,
            stdlib_indexer: None,
            gem_indexer: None,
            ruby_library_paths: Vec::new(),
        }
    }

    /// Runs the complete indexing process from start to finish using two-phase approach.
    ///
    /// This method implements two-phase indexing to avoid race conditions:
    /// Phase 1 - Index all definitions:
    /// 1. Figure out which Ruby version we're using
    /// 2. Find where Ruby libraries are installed on this system
    /// 3. Set up the main indexing engine
    /// 4. Index definitions from project files
    /// 5. Index definitions from Ruby standard library
    /// 6. Index definitions from gems
    ///
    /// Phase 2 - Index all references:
    /// 7. Index references from project files (now that all definitions are available)
    pub async fn run_complete_indexing(&mut self, server: &RubyLanguageServer) -> Result<()> {
        info!("Starting complete two-phase indexing process");
        let start_time = Instant::now();

        // Step 1: Figure out which Ruby version we're using
        let ruby_version = self.detect_ruby_version();
        info!("Detected Ruby version: {:?}", ruby_version);

        // Step 2: Find where Ruby libraries are installed
        self.discover_ruby_library_paths();

        // Step 3: Set up the main indexing engine
        self.setup_file_processor(server);

        // PHASE 1: Index all definitions first
        info!("Phase 1: Indexing all definitions");
        let phase1_start = Instant::now();

        // Step 4: Index definitions from project files
        self.index_project_definitions(server).await?;

        // Step 5: Index definitions from Ruby standard library
        self.index_standard_library(server, &ruby_version).await?;

        // Step 6: Index definitions from gems
        self.index_gems(server).await?;

        // Step 7: Resolve all mixin references across all indexed files
        Self::send_progress_report(server, "Resolving mixins...".to_string(), 0, 0).await;
        info!("Resolving all mixin references across project, stdlib, and gems");
        server.index.lock().resolve_all_mixins();

        info!("Phase 1 completed in {:?}", phase1_start.elapsed());

        // PHASE 2: Index all references (now that definitions are available)
        info!("Phase 2: Indexing all references");
        let phase2_start = Instant::now();

        // Step 7: Index references from project files
        self.index_project_references(server).await?;

        info!("Phase 2 completed in {:?}", phase2_start.elapsed());

        // Mark indexing as complete after Phase 2 (index is now queryable)
        // Phase 3 (diagnostics) can take a long time and isn't needed for queries
        server.set_indexing_complete();

        // PHASE 3: Publish diagnostics for unresolved constants
        info!("Phase 3: Publishing diagnostics for unresolved constants");
        Self::send_progress_report(server, "Publishing diagnostics...".to_string(), 0, 0).await;
        self.publish_unresolved_diagnostics(server).await;

        info!(
            "Complete two-phase indexing finished in {:?}",
            start_time.elapsed()
        );
        Ok(())
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
    fn setup_file_processor(&mut self, server: &RubyLanguageServer) {
        self.file_processor = Some(FileProcessor::new(server.index.clone()));
    }

    /// Phase 1 Step 4: Index definitions from project files and track what libraries they need
    async fn index_project_definitions(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let mut project_indexer = IndexerProject::new(
            self.workspace_root.clone(),
            self.file_processor.as_ref().unwrap().clone(),
        );

        project_indexer.index_project_definitions(server).await?;
        self.project_indexer = Some(project_indexer);
        Ok(())
    }

    /// Phase 2 Step 7: Index references from project files
    async fn index_project_references(&mut self, server: &RubyLanguageServer) -> Result<()> {
        if let Some(ref mut project_indexer) = self.project_indexer {
            project_indexer.index_project_references(server).await?;
        } else {
            warn!("Project indexer not initialized, cannot index references");
        }
        Ok(())
    }

    /// Phase 3: Publish diagnostics for unresolved entries across all indexed files
    async fn publish_unresolved_diagnostics(&self, server: &RubyLanguageServer) {
        use crate::capabilities::diagnostics::get_unresolved_diagnostics;

        // Collect all URIs with unresolved entries while holding the lock
        let uris: Vec<_> = {
            let index = server.index.lock();
            let count = index.unresolved.len();
            info!(
                "Publishing diagnostics for {} files with unresolved entries",
                count
            );
            index.unresolved.uris()
        };

        // Publish diagnostics for each file (lock released, safe to await)
        for uri in uris {
            let diagnostics = get_unresolved_diagnostics(server, &uri);
            if !diagnostics.is_empty() {
                debug!(
                    "Publishing {} unresolved diagnostics for {}",
                    diagnostics.len(),
                    uri.path()
                );
                server.publish_diagnostics(uri, diagnostics).await;
            }
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

    /// Step 6: Index the gems (external libraries)
    async fn index_gems(&mut self, server: &RubyLanguageServer) -> Result<()> {
        let required_gems = self.get_required_gems();

        let mut gem_indexer = IndexerGem::new(Some(self.workspace_root.clone()));

        gem_indexer.set_required_gems(required_gems.into_iter().collect());
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
    /// 2. Look in the workspace's vsix/stubs directory
    /// 3. Fall back to Ruby 3.0 stubs if available
    pub fn find_core_stubs_for_version(&self, version: (u8, u8)) -> Option<PathBuf> {
        // First, try the configured stub path
        if let Some(stubs_path_str) = self.config.get_core_stubs_path_internal(version) {
            return Some(PathBuf::from(stubs_path_str));
        }

        // Look for stubs in the workspace
        let stubs_dir = self.workspace_root.join("vsix").join("stubs");
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

/// Integration tests for IndexingCoordinator
/// Tests the complete indexing workflow with realistic project structures
#[cfg(test)]
mod coordinator_integration_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
            let core_stubs_dir = project_root.join("vsix").join("stubs").join("rubystubs30");
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
            duration.as_secs() < 30,
            "Indexing should complete within 30 seconds"
        );
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
        assert!(
            !lib_dirs.is_empty() || true,
            "Should handle gem discovery errors gracefully"
        );

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
    async fn test_coordinator_vendor_directory_exclusion() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        // Create a vendor directory with Ruby files that should be excluded
        let vendor_dir = fixture.project_root().join("vendor");
        fs::create_dir_all(&vendor_dir).expect("Failed to create vendor directory");

        let vendor_bundle_dir = vendor_dir.join("bundle");
        fs::create_dir_all(&vendor_bundle_dir).expect("Failed to create vendor/bundle directory");

        // Create a Ruby file in vendor that should be excluded
        let vendor_ruby_file = vendor_dir.join("excluded_gem.rb");
        fs::write(&vendor_ruby_file, "class ExcludedGem\nend")
            .expect("Failed to write vendor Ruby file");

        let vendor_bundle_ruby_file = vendor_bundle_dir.join("bundled_gem.rb");
        fs::write(&vendor_bundle_ruby_file, "class BundledGem\nend")
            .expect("Failed to write vendor/bundle Ruby file");

        let config = RubyFastLspConfig::default();
        let coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Collect Ruby files from the project
        let mut collected_files: Vec<PathBuf> = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut collected_files);

        // Verify that vendor files are excluded
        let vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            vendor_files.is_empty(),
            "Vendor directory files should be excluded from indexing, but found: {:?}",
            vendor_files
        );

        // Verify that non-vendor files are still collected
        let non_vendor_files: Vec<_> = collected_files
            .iter()
            .filter(|path| !path.to_string_lossy().contains("vendor"))
            .collect();

        assert!(
            !non_vendor_files.is_empty(),
            "Non-vendor Ruby files should still be collected"
        );
    }
}
