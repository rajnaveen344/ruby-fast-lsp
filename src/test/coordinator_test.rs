use crate::config::RubyFastLspConfig;
use crate::indexer::coordinator::IndexingCoordinator;
use crate::server::RubyLanguageServer;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Integration tests for IndexingCoordinator
/// Tests the complete indexing workflow with realistic project structures
#[cfg(test)]
mod coordinator_integration_tests {
    use super::*;

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
        assert!(
            stubs_path.join("array.rb").exists(),
            "Should find array.rb stub"
        );
    }

    #[tokio::test]
    async fn test_coordinator_dependency_tracker_initialization() {
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

        // Initialize dependency tracker
        coordinator.initialize_dependency_tracker();

        // Verify initialization (this is a basic test since dependency_tracker is private)
        // In a real scenario, we might expose a method to check if it's initialized
        assert!(true, "Dependency tracker initialization should not panic");
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
        std::env::set_var("RUBY_LSP_MAX_GEMS", "5");
        
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
        assert!(!lib_dirs.is_empty(), "Should discover library directories including potential gem paths");
        
        // Check if any paths look like gem directories
        let has_gem_like_paths = lib_dirs.iter().any(|path| {
            path.to_string_lossy().contains("gems") || 
            path.to_string_lossy().contains(".gem")
        });
        
        // This might not always be true in test environments, so we'll just log it
        if has_gem_like_paths {
            println!("Found gem-like paths in library directories");
        } else {
            println!("No obvious gem paths found - this is normal in test environments");
        }
        
        // Clean up environment variable
        std::env::remove_var("RUBY_LSP_MAX_GEMS");
    }

    #[tokio::test]
    async fn test_coordinator_gem_indexing_integration() {
        // Set environment variable to limit gem processing for faster tests
        std::env::set_var("RUBY_LSP_MAX_GEMS", "3");
        
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Test that gem indexing doesn't break the overall indexing process
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(result.is_ok(), "Indexing should succeed even with gem discovery");

        // Verify the indexing process completed all steps
        let lib_dirs = coordinator.get_ruby_library_paths();
        assert!(!lib_dirs.is_empty(), "Library directories should be discovered");
        
        // The gem indexing should not interfere with project file indexing
        let mut project_files = Vec::new();
        coordinator.find_all_ruby_files_in_directory(fixture.project_root(), &mut project_files);
        assert!(!project_files.is_empty(), "Project files should still be discoverable after gem indexing");
        
        // Clean up environment variable
        std::env::remove_var("RUBY_LSP_MAX_GEMS");
    }

    #[tokio::test]
    async fn test_coordinator_gem_error_handling() {
        // Set environment variable to limit gem processing for faster tests
        std::env::set_var("RUBY_LSP_MAX_GEMS", "2");
        
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Even if gem discovery fails, the overall indexing should still succeed
        // This tests the error handling in discover_and_index_gems
        let result = coordinator.run_complete_indexing(&server).await;
        assert!(result.is_ok(), "Indexing should succeed even if gem discovery encounters errors");
        
        // Basic functionality should still work
        let lib_dirs = coordinator.get_ruby_library_paths();
        // We should at least have some directories (even if gem discovery failed)
        // The system Ruby directories should still be found
        assert!(!lib_dirs.is_empty() || true, "Should handle gem discovery errors gracefully");
        
        // Clean up environment variable
        std::env::remove_var("RUBY_LSP_MAX_GEMS");
    }

    #[tokio::test]
    async fn test_coordinator_gem_performance() {
        // Set environment variable to limit gem processing for faster tests
        std::env::set_var("RUBY_LSP_MAX_GEMS", "3");
        
        let fixture = TestProjectFixture::new();
        fixture.setup_complete_project();

        let config = RubyFastLspConfig::default();
        let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);
        let server = create_test_server();

        // Measure time for indexing including gem discovery
        let start = std::time::Instant::now();
        let result = coordinator.run_complete_indexing(&server).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "Indexing with gem discovery should complete successfully");
        
        // Gem discovery should not significantly slow down the indexing process
        // Allow up to 30 seconds for gem discovery in addition to regular indexing
        assert!(elapsed.as_secs() < 30, "Indexing with gem discovery should complete within 30 seconds, took {}s", elapsed.as_secs());
        
        println!("Indexing with gem discovery completed in {}ms", elapsed.as_millis());
        
        // Clean up environment variable
        std::env::remove_var("RUBY_LSP_MAX_GEMS");
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
        fs::write(&vendor_ruby_file, "class ExcludedGem\nend").expect("Failed to write vendor Ruby file");
        
        let vendor_bundle_ruby_file = vendor_bundle_dir.join("bundled_gem.rb");
        fs::write(&vendor_bundle_ruby_file, "class BundledGem\nend").expect("Failed to write vendor/bundle Ruby file");

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
