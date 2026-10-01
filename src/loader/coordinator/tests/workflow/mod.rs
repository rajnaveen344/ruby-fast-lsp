//! Complete indexing workflow over realistic project fixtures.

use super::*;
use crate::loader::scheduling::navigation_demand;
use crate::loader::scheduling::status;
use crate::lsp::capabilities::indexing;
use crate::lsp::capabilities::navigation::definitions;

mod core_stubs;
mod demand_batches;
mod gem_indexing;
mod project_files;
mod runtime_selection;

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
        fs::write(test_dir.join("user_test.rb"), user_test).expect("Failed to write user_test.rb");
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
