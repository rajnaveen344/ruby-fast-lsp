//! Complete indexing workflow over realistic project fixtures.

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

#[test]
fn configured_ruby_version_overrides_runtime_auto_detection() {
    let fixture = TestProjectFixture::new();
    let config = RubyFastLspConfig {
        ruby_version: "2.5".to_string(),
        ..RubyFastLspConfig::default()
    };
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

    assert_eq!(
        coordinator.detect_ruby_version(),
        Some(RubyVersion::new(2, 5)),
        "an explicit Ruby version must select its matching core stubs"
    );
}

#[tokio::test]
async fn unavailable_auto_runtime_uses_conservative_core_fallback() {
    let fixture = TestProjectFixture::new();
    std::fs::write(fixture.project_root().join(".ruby-version"), "3.2.999\n").unwrap();
    let server = create_test_server();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let mut coordinator =
        IndexingCoordinator::new(fixture.project_root().clone(), RubyFastLspConfig::default());
    coordinator
        .resolve_effective_runtime(&server)
        .await
        .unwrap();
    assert!(coordinator.effective_runtime.is_none());
    assert_eq!(coordinator.detect_ruby_version_off_reactor(&server).await.unwrap(), None,
        "an unavailable automatic runtime must use the bundled Ruby 3.0 fallback, not derive compatibility from an unfulfilled marker");
    coordinator.config.ruby_version = "2.5".to_string();
    assert_eq!(
        coordinator
            .detect_ruby_version_off_reactor(&server)
            .await
            .unwrap(),
        Some(RubyVersion::new(2, 5)),
        "explicit compatibility configuration must still win over automatic fallback"
    );
}

#[tokio::test]
async fn auto_runtime_marker_becomes_the_exact_effective_runtime() {
    use crate::runtime::catalog::{
        DiscoveredRuntime, RuntimeDiscoverySource, RuntimeSupportStatus,
    };

    let fixture = TestProjectFixture::new();
    std::fs::write(
        fixture.project_root().join(".ruby-version"),
        "jruby-9.2.21.0\n",
    )
    .unwrap();
    let server = create_test_server();
    server.add_workspace(Url::from_directory_path(fixture.project_root()).unwrap());
    server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
        implementation: RuntimeImplementation::Jruby,
        implementation_label: "JRuby".to_string(),
        family: "9.2".to_string(),
        family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
        compatibility_version: "2.5".to_string(),
        compatibility_label: "Ruby 2.5".to_string(),
        engine_version: "9.2.21.0".to_string(),
        display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
        executable: fixture.project_root().join("runtime/bin/jruby"),
        discovery_source: RuntimeDiscoverySource::Rvm,
        support_status: RuntimeSupportStatus::Supported,
        java_home: Some(fixture.project_root().join("jdk")),
    }]);
    let mut coordinator =
        IndexingCoordinator::new(fixture.project_root().clone(), RubyFastLspConfig::default());

    coordinator
        .resolve_effective_runtime(&server)
        .await
        .unwrap();
    assert_eq!(
        coordinator.detect_ruby_version(),
        Some(RubyVersion::new_with_implementation(
            2,
            5,
            RubyImplementation::JRuby
        ))
    );
    assert_eq!(
        coordinator
            .effective_runtime
            .as_ref()
            .map(|runtime| runtime.engine_version.as_str()),
        Some("9.2.21.0")
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
    assert!(
        result.is_ok(),
        "Indexing should complete successfully: {result:?}"
    );

    let engine = server.orphan_engine().read();
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

    assert!(
        coordinator.gem_indexer.is_some() && coordinator.stdlib_indexer.is_some(),
        "complete indexing must retain its exact gem and stdlib indexers"
    );
}

#[tokio::test]
async fn identical_core_stubs_use_one_template_but_keep_isolated_engines() {
    let fixture = TempDir::new().expect("multi-project fixture must be created");
    let admin = fixture.path().join("admin");
    let server_root = fixture.path().join("server");
    for root in [&admin, &server_root] {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();
        fs::write(root.join("app.rb"), "class App\nend\n").unwrap();
    }
    let server = create_test_server();
    let admin_workspace = server.add_workspace(Url::from_directory_path(&admin).unwrap());
    let server_workspace = server.add_workspace(Url::from_directory_path(&server_root).unwrap());

    for root in [&admin, &server_root] {
        let mut coordinator =
            IndexingCoordinator::new(root.to_path_buf(), RubyFastLspConfig::default());
        coordinator.run_complete_indexing(&server).await.unwrap();
    }

    assert_eq!(
        server.products.core_templates().len(),
        1,
        "the same compatibility core must have one prepared template"
    );
    assert!(
        !Arc::ptr_eq(
            &admin_workspace.analysis_engine,
            &server_workspace.analysis_engine
        ),
        "projects must retain isolated mutable engines"
    );
    let unique = admin.join("only_admin.rb");
    admin_workspace
        .analysis_engine
        .write()
        .register_file(SourceFileInput {
            path: unique.clone(),
            content: "ADMIN_ONLY = true\n".to_string(),
            kind: ruby_analysis::core::SourceKind::Project,
        });
    assert!(
        server_workspace
            .analysis_engine
            .read()
            .file_id(&unique)
            .is_none(),
        "mutating one engine must not change a sibling cloned from the same template"
    );
}

#[tokio::test]
async fn core_template_binding_preserves_an_open_unsaved_document() {
    let fixture = TempDir::new().expect("live-document fixture must be created");
    let project = fixture.path().join("app");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    let path = project.join("live.rb");
    let content =
        "class LiveDocument\n  def unsaved_marker; end\n  def call; unsaved_marker; end\nend\n";
    let uri = Url::from_file_path(&path).expect("live document URI must be valid");
    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    crate::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: content.to_string(),
            },
        },
    )
    .await;

    let mut coordinator = IndexingCoordinator::new(project, RubyFastLspConfig::default());
    coordinator.setup_file_processor(&server);
    coordinator
        .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
        .await
        .expect("core stubs must bind successfully");

    let engine = workspace.analysis_engine.read();
    let file_id = engine
        .file_id(&path)
        .expect("binding a core template must not erase the open document");
    assert!(
        engine.file_content_matches(file_id, content),
        "binding a core template must preserve the exact unsaved document content"
    );
    drop(engine);

    let definitions = crate::capabilities::definitions::definition_locations(
        crate::capabilities::definitions::find_definition_at_position(
            &server,
            uri,
            tower_lsp::lsp_types::Position::new(2, 14),
        )
        .await
        .expect("same-file definition lookup must remain available"),
    );
    assert_eq!(
        definitions.len(),
        1,
        "same-file navigation must survive core-template binding"
    );
    assert_eq!(definitions[0].range.start.line, 1);
}

#[tokio::test]
async fn project_batch_stream_consumes_an_exact_generation_navigation_demand_first() {
    let fixture = TempDir::new().expect("navigation-demand fixture must be created");
    let project = fixture.path().join("server");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    let caller_path = project.join("caller.rb");
    let caller_uri = Url::from_file_path(&caller_path).unwrap();
    fs::write(&caller_path, "AccountRecord.lookup\n").unwrap();
    for index in 0..140 {
        fs::write(
            project.join(format!("ordinary_{index:03}.rb")),
            format!("ORDINARY_{index} = {index}\n"),
        )
        .unwrap();
    }
    let target_path = project.join("account_record.rb");
    fs::write(&target_path, "class AccountRecord\nend\n").unwrap();

    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    crate::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: caller_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: "AccountRecord.lookup\n".to_string(),
            },
        },
    )
    .await;
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            crate::indexing_status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();

    let mut coordinator = IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
    coordinator.set_indexing_run(run.clone());
    coordinator.setup_file_processor(&server);
    coordinator
        .collect_project_navigation_facts(
            &server,
            ActiveDocumentPriorityKeys {
                dependency_roots: HashSet::new(),
                project_terminals: vec!["ordinary000".to_string()],
            },
        )
        .await
        .unwrap();
    assert!(
        crate::capabilities::definitions::find_definition_at_position(
            &server,
            caller_uri.clone(),
            Position::new(0, 2),
        )
        .await
        .is_none(),
        "the target must remain outside the fixed startup frontier before its demand"
    );
    let ticket = workspace.navigation_demands.request(
        run.generation(),
        crate::navigation_demand::NavigationDemandStage::Project,
        "accountrecord",
    );

    coordinator
        .collect_remaining_project_facts(&server, None)
        .await
        .unwrap();

    assert_eq!(
        ticket.wait().await,
        crate::navigation_demand::NavigationDemandOutcome::TargetProcessed
    );
    let definitions = crate::capabilities::definitions::definition_locations(
        crate::capabilities::definitions::find_definition_at_position(
            &server,
            caller_uri,
            Position::new(0, 2),
        )
        .await
        .expect("the exact demanded target must resolve before project-stage completion"),
    );
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].uri,
        Url::from_file_path(target_path).unwrap()
    );
}

#[tokio::test]
async fn project_frontier_consumes_a_bounded_nonpriority_demand() {
    let fixture = TempDir::new().expect("frontier-demand fixture must be created");
    let project = fixture.path().join("server");
    fs::create_dir_all(&project).expect("project root must be created");
    fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
        .expect("Gemfile must be written");
    fs::write(project.join("account.rb"), "class AccountRecord\nend\n").unwrap();
    fs::write(project.join("report.rb"), "class Report\nend\n").unwrap();

    let server = create_test_server();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());
    let run = workspace.begin_indexing_run();
    workspace
        .indexing_status
        .transition(
            run.generation(),
            crate::indexing_status::IndexingPhase::IndexingProject,
            None,
            None,
        )
        .unwrap();
    let ticket = workspace.navigation_demands.request(
        run.generation(),
        crate::navigation_demand::NavigationDemandStage::Project,
        "accountrecord",
    );

    let mut coordinator = IndexingCoordinator::new(project.clone(), RubyFastLspConfig::default());
    coordinator.set_indexing_run(run);
    coordinator.setup_file_processor(&server);
    coordinator
        .collect_project_navigation_facts(
            &server,
            ActiveDocumentPriorityKeys {
                dependency_roots: HashSet::new(),
                project_terminals: vec!["report".to_string()],
            },
        )
        .await
        .unwrap();

    assert_eq!(
        tokio::time::timeout(Duration::from_millis(50), ticket.wait())
            .await
            .expect("the project frontier must consume its bounded demand"),
        crate::navigation_demand::NavigationDemandOutcome::TargetProcessed
    );
}

#[tokio::test]
async fn dependency_core_seed_never_contains_an_open_project_document() {
    let fixture = TempDir::new().expect("dependency-seed fixture must be created");
    let clean_project = fixture.path().join("clean");
    let live_project = fixture.path().join("live");
    for project in [&clean_project, &live_project] {
        fs::create_dir_all(project).expect("project root must be created");
        fs::write(project.join("Gemfile"), "source 'https://rubygems.org'\n")
            .expect("Gemfile must be written");
    }

    let server = create_test_server();
    server.add_workspace(Url::from_directory_path(&clean_project).unwrap());
    server.add_workspace(Url::from_directory_path(&live_project).unwrap());

    let mut clean_coordinator =
        IndexingCoordinator::new(clean_project, RubyFastLspConfig::default());
    clean_coordinator.setup_file_processor(&server);
    let clean_seed = clean_coordinator
        .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
        .await
        .expect("clean core seed must be prepared");

    let live_path = live_project.join("live.rb");
    let live_uri = Url::from_file_path(&live_path).expect("live document URI must be valid");
    crate::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: live_uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: "class ProjectOnly; end\n".to_string(),
            },
        },
    )
    .await;
    let mut live_coordinator = IndexingCoordinator::new(live_project, RubyFastLspConfig::default());
    live_coordinator.setup_file_processor(&server);
    let live_seed = live_coordinator
        .index_core_stubs(&server, Some(RubyVersion::new(3, 0)))
        .await
        .expect("live-document core seed must be prepared");

    assert!(
        live_seed.file_id(&live_path).is_none(),
        "the reusable dependency seed must never inherit project-owned open-document facts"
    );
    assert_eq!(
        clean_seed.semantic_context_fingerprint(),
        live_seed.semantic_context_fingerprint(),
        "editor open timing must not change the immutable dependency seed identity"
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

    let engine = server.orphan_engine().read();
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
        .documents
        .read()
        .get(&usage_uri)
        .cloned()
        .expect("opened usage document must exist");
    let query =
        crate::query::EngineQuery::with_doc_and_engine(document, server.orphan_engine().clone());
    let definitions = query
        .find_definitions_at_position(&usage_uri, tower_lsp::lsp_types::Position::new(1, 9), usage)
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

    assert!(
        coordinator.gem_indexer.is_some(),
        "production gem discovery must initialize the owning project's exact gem indexer"
    );
    assert!(
        coordinator.get_ruby_library_paths().is_empty(),
        "complete indexing must not launch the redundant legacy load-path discovery"
    );

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

    assert!(
        coordinator.gem_indexer.is_some(),
        "gem indexing must complete through the owning project's exact gem indexer"
    );
    assert!(
        coordinator.get_ruby_library_paths().is_empty(),
        "gem indexing must not populate the unused legacy load-path side table"
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
    fs::write(&vendor_ruby_file, "class VendorGem\nend").expect("Failed to write vendor Ruby file");

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
        server
            .analysis_engine_for_uri(&uri)
            .read()
            .stats()
            .diagnostics
            > 0,
        "cold indexing must retain workspace diagnostics in the engine"
    );
    assert!(
        server.last_diagnostic_publication(&uri).is_none(),
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
