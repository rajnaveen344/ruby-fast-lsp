use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness")
        .to_path_buf()
}

fn rails_package(index_output: &str) -> (TempDir, std::path::PathBuf) {
    rails_package_with_response(index_output, "empty_output.json")
}

fn rails_package_with_response(
    index_output: &str,
    response_output: &str,
) -> (TempDir, std::path::PathBuf) {
    let source = workspace_root().join("extensions/rails-ruby");
    let temp = TempDir::new().expect("rails extension temp package must be created");
    let package = temp.path().join("rails-ruby");
    std::fs::create_dir(&package).expect("rails extension package directory must be created");
    if std::env::var("RUBY_FAST_LSP_TEST_BUILT_RAILS").as_deref() == Ok("1") {
        std::fs::copy(
            source.join("extension.toml"),
            package.join("extension.toml"),
        )
        .expect("rails extension manifest must be copied");
        let artifact_dir = package.join("target/wasm32-wasip1/release");
        std::fs::create_dir_all(&artifact_dir)
            .expect("built Rails fixture artifact directory must be created");
        std::fs::copy(
            source.join(
                "target/wasm32-wasip1/release/ruby_fast_lsp_rails_extension.wasm",
            ),
            artifact_dir.join("ruby_fast_lsp_rails_extension.wasm"),
        )
        .expect(
            "built Rails Rust Wasm is required; run extensions/rails-ruby/build-and-test.sh before setting RUBY_FAST_LSP_TEST_BUILT_RAILS=1",
        );
        return (temp, package);
    }
    let manifest = std::fs::read_to_string(source.join("extension.toml"))
        .expect("rails extension manifest must be readable")
        .lines()
        .filter(|line| {
            !line.starts_with("checksum_sha256")
                && *line != "[applicability]"
                && !line.starts_with("locked_gems =")
        })
        .map(|line| {
            if line.starts_with("wasm =") {
                "wasm = \"extension.wasm\""
            } else if line.starts_with("output =") {
                "output = \"extension.wasm\""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(package.join("extension.toml"), format!("{manifest}\n"))
        .expect("deterministic fixture manifest must be written without the production checksum");
    let mut wat = std::fs::read_to_string(source.join("contract.wat.in"))
        .expect("rails extension contract fixture must be readable");
    for (name, pointer, file) in [
        ("NAMES", 1024_u64, "indexed_call_names.json"),
        ("INDEX", 2048_u64, index_output),
        ("EMPTY", 8192_u64, "empty_output.json"),
        ("RESPONSE", 12000_u64, response_output),
    ] {
        let payload = std::fs::read(source.join(file))
            .unwrap_or_else(|err| panic!("rails fixture `{file}` must be readable: {err}"));
        serde_json::from_slice::<serde_json::Value>(&payload)
            .unwrap_or_else(|err| panic!("rails fixture `{file}` must be valid JSON: {err}"));
        let escaped = payload
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>();
        let packed = (pointer << 32) | payload.len() as u64;
        wat = wat
            .replace(&format!("__{name}_DATA__"), &escaped)
            .replace(&format!("__{name}_PACKED__"), &packed.to_string());
    }
    let wasm = wat::parse_str(wat).expect("rails extension contract fixture must compile");
    std::fs::write(package.join("extension.wasm"), wasm)
        .expect("rails extension fixture Wasm must be written");
    (temp, package)
}

#[tokio::test]
async fn controller_actions_expose_open_view_lenses() {
    let (_temp, package) =
        rails_package_with_response("empty_output.json", "view_response_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/controllers/admin/users_controller.rb",
            "class Admin::UsersController < ApplicationController\n  def show\n  end\nend\n",
        )
        .await;

    let lenses = editor
        .code_lens("app/controllers/admin/users_controller.rb")
        .await;
    let open_view = lenses
        .iter()
        .filter_map(|lens| lens.command.as_ref())
        .find(|command| command.title == "Open View")
        .expect("public controller action must expose an Open View lens");
    assert_eq!(open_view.command, "ruby-fast-lsp.rails.openView");
    let arguments = open_view
        .arguments
        .as_ref()
        .expect("Open View command must carry controller/action arguments");
    assert!(
        arguments[0]
            .as_str()
            .is_some_and(|uri| uri.ends_with("/app/controllers/admin/users_controller.rb")),
        "Open View must carry the controller URI, got {arguments:?}"
    );
    assert_eq!(arguments[1], serde_json::json!("admin/users"));
    assert_eq!(arguments[2], serde_json::json!("show"));
}

#[tokio::test]
async fn active_record_association_uses_public_semantic_contracts() {
    let (_temp, package) = rails_package("index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/user.rb",
            "module Billing\n  class Account\n    def label\n      \"account\"\n    end\n  end\nend\n\nclass User\n  belongs_to :account, class_name: \"Billing::Account\"\n  def display\n    account.label\n  end\nend\n",
        )
        .await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "rails extension must remain loaded after association indexing, got {statuses:?}"
    );

    let target = editor.goto_definition("app/models/user.rb", 9, 40).await;
    assert_eq!(
        target.len(),
        1,
        "class_name must reference Billing::Account"
    );
    assert_eq!(target[0].range.start.line, 1);

    let definition = editor.goto_definition("app/models/user.rb", 11, 6).await;
    assert_eq!(definition.len(), 1, "association reader must resolve");
    assert_eq!(definition[0].range.start.line, 9);
    assert_eq!(definition[0].range.start.character, 13);

    let hover = editor.hover("app/models/user.rb", 11, 6).await;
    assert!(
        hover.as_ref().is_some_and(|hover| {
            format!("{:?}", hover.contents).contains("(Billing::Account | NilClass)")
        }),
        "association reader must carry its structured target type, got {hover:?}"
    );

    editor
        .set(
            "app/models/user.rb",
            "module Billing\n  class Account\n    def label\n      \"account\"\n    end\n  end\nend\n\nclass User\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor
            .goto_definition("app/models/user.rb", 10, 6)
            .await
            .is_empty(),
        "removing the association must remove its generated reader"
    );
}

#[tokio::test]
async fn polymorphic_association_does_not_invent_a_constant_target() {
    let (_temp, package) = rails_package("polymorphic_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/attachment.rb",
            "class Attachment\n  belongs_to :subject, polymorphic: true\n  def attached\n    subject\n  end\nend\n",
        )
        .await;

    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "polymorphic indexing must not disable the Rails extension, got {statuses:?}"
    );
    assert!(
        editor
            .goto_definition("app/models/attachment.rb", 1, 16)
            .await
            .is_empty(),
        "polymorphic DSL argument must not guess a Subject constant"
    );
    let definition = editor
        .goto_definition("app/models/attachment.rb", 3, 6)
        .await;
    assert_eq!(definition.len(), 1, "polymorphic reader must still exist");
    assert_eq!(definition[0].range.start.line, 1);
}

#[tokio::test]
async fn callbacks_and_custom_validations_reference_private_methods() {
    let (_temp, package) = rails_package("callback_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/user.rb",
            "class User\n  before_save :normalize_account\n  validate :account_is_active\n  private\n  def normalize_account\n  end\n  def account_is_active\n  end\nend\n",
        )
        .await;

    let callback = editor.goto_definition("app/models/user.rb", 1, 18).await;
    assert_eq!(callback.len(), 1, "callback symbol must resolve");
    assert_eq!(callback[0].range.start.line, 4);

    let validation = editor.goto_definition("app/models/user.rb", 2, 15).await;
    assert_eq!(validation.len(), 1, "custom validation symbol must resolve");
    assert_eq!(validation[0].range.start.line, 6);

    let references = editor.references("app/models/user.rb", 4, 8).await;
    assert!(
        references.iter().any(|location| {
            location.range.start.line == 1 && location.range.start.character == 14
        }),
        "callback symbol must enter ordinary engine method references, got {references:?}"
    );

    editor
        .set(
            "app/models/user.rb",
            "class User\n  private\n  def normalize_account\n  end\n  def account_is_active\n  end\nend\n",
        )
        .await;
    let references = editor.references("app/models/user.rb", 2, 8).await;
    assert!(
        references.iter().all(|location| {
            !(location.range.start.line == 1 && location.range.start.character == 14)
        }),
        "removing callback declarations must remove stale method references, got {references:?}"
    );
}

#[tokio::test]
async fn routes_generate_typed_helpers_and_controller_action_navigation() {
    let (_temp, package) = rails_package("route_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/controllers/application_controller.rb",
            "class ApplicationController\nend\n",
        )
        .await;
    editor
        .open(
            "app/controllers/users_controller.rb",
            "class UsersController < ApplicationController\n  def show\n  end\n  def links\n    users_path\n    account_path\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :users\n  get \"/account\", to: \"users#show\", as: :account\nend\n",
        )
        .await;

    let resource_controller = editor.goto_definition("config/routes.rb", 1, 15).await;
    assert_eq!(
        resource_controller.len(),
        1,
        "resources must target its controller"
    );
    assert!(resource_controller[0]
        .uri
        .path()
        .ends_with("/app/controllers/users_controller.rb"));

    let explicit_controller = editor.goto_definition("config/routes.rb", 2, 25).await;
    assert_eq!(
        explicit_controller.len(),
        1,
        "route controller segment must navigate"
    );
    assert_eq!(explicit_controller[0].range.start.line, 0);
    let explicit_action = editor.goto_definition("config/routes.rb", 2, 31).await;
    assert_eq!(
        explicit_action.len(),
        1,
        "route action segment must navigate"
    );
    assert_eq!(explicit_action[0].range.start.line, 1);

    let resource_helper = editor
        .goto_definition("app/controllers/users_controller.rb", 4, 6)
        .await;
    assert_eq!(
        resource_helper.len(),
        1,
        "resource helper must resolve through ApplicationController"
    );
    assert_eq!(resource_helper[0].range.start.line, 1);
    let named_helper = editor
        .goto_definition("app/controllers/users_controller.rb", 5, 6)
        .await;
    assert_eq!(named_helper.len(), 1, "named route helper must resolve");
    assert_eq!(named_helper[0].range.start.line, 2);
    let hover = editor
        .hover("app/controllers/users_controller.rb", 5, 6)
        .await;
    assert!(
        hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("String")),
        "route helper must carry its String return type, got {hover:?}"
    );

    editor
        .set(
            "config/routes.rb",
            "Rails.application.routes.draw do\nend\n",
        )
        .await;
    assert!(
        editor
            .goto_definition("app/controllers/users_controller.rb", 4, 6)
            .await
            .is_empty(),
        "removing routes must remove stale generated helpers"
    );
}

#[tokio::test]
async fn namespaced_resources_use_lexical_route_frame_arguments() {
    let (_temp, package) = rails_package("nested_route_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/controllers/application_controller.rb",
            "class ApplicationController\nend\n",
        )
        .await;
    editor
        .open(
            "app/controllers/admin/users_controller.rb",
            "module Admin\n  class UsersController < ApplicationController\n    def index\n    end\n    def links\n      admin_users_path\n    end\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  namespace :admin do\n    resources :users\n  end\nend\n",
        )
        .await;

    let controller = editor.goto_definition("config/routes.rb", 2, 17).await;
    assert_eq!(
        controller.len(),
        1,
        "nested resource must target Admin::UsersController"
    );
    assert!(controller[0]
        .uri
        .path()
        .ends_with("/app/controllers/admin/users_controller.rb"));
    let helper = editor
        .goto_definition("app/controllers/admin/users_controller.rb", 5, 8)
        .await;
    assert_eq!(helper.len(), 1, "namespaced resource helper must resolve");
    assert_eq!(helper[0].range.start.line, 2);
}

#[tokio::test]
async fn active_job_enqueue_entry_point_navigates_to_perform() {
    let (_temp, package) = rails_package("job_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/jobs/billing/email_job.rb",
            "module Billing\n  class EmailJob < ActiveJob::Base\n    def perform(user)\n    end\n  end\nend\n\nBilling::EmailJob.perform_later(user)\n",
        )
        .await;

    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "Active Job indexing must keep the extension loaded, got {statuses:?}"
    );

    let definition = editor
        .goto_definition("app/jobs/billing/email_job.rb", 7, 24)
        .await;
    assert_eq!(
        definition.len(),
        1,
        "perform_later must navigate to the job's perform method"
    );
    assert_eq!(definition[0].range.start.line, 2);

    let references = editor
        .references("app/jobs/billing/email_job.rb", 2, 8)
        .await;
    assert!(
        references.iter().any(|location| {
            location.range.start.line == 7 && location.range.start.character == 18
        }),
        "enqueue entry point must enter ordinary engine method references, got {references:?}"
    );

    editor
        .set(
            "app/jobs/billing/email_job.rb",
            "module Billing\n  class EmailJob < ActiveJob::Base\n    def perform(user)\n    end\n  end\nend\n",
        )
        .await;
    let references = editor
        .references("app/jobs/billing/email_job.rb", 2, 8)
        .await;
    assert!(
        references
            .iter()
            .all(|location| location.range.start.line != 7),
        "removing the enqueue call must remove its stale perform reference, got {references:?}"
    );
}

#[tokio::test]
async fn active_support_concern_model_facts_flow_through_includers() {
    let (_temp, package) = rails_package("concern_index_output.json");
    let mut editor = FakeEditor::with_extension_package(package).await;
    editor
        .open(
            "app/models/concerns/accountable.rb",
            "class Account\nend\n\nmodule Accountable\n  extend ActiveSupport::Concern\n  included do\n    belongs_to :account\n  end\nend\n\nclass User\n  include Accountable\n  def display\n    account\n  end\nend\n",
        )
        .await;

    let definition = editor
        .goto_definition("app/models/concerns/accountable.rb", 13, 6)
        .await;
    assert_eq!(
        definition.len(),
        1,
        "model facts declared in a concern must resolve through the includer's MRO"
    );
    assert_eq!(definition[0].range.start.line, 6);
    let hover = editor
        .hover("app/models/concerns/accountable.rb", 13, 6)
        .await;
    assert!(
        hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("Account")),
        "concern-provided association must retain its type, got {hover:?}"
    );

    editor
        .set(
            "app/models/concerns/accountable.rb",
            "class Account\nend\n\nmodule Accountable\n  extend ActiveSupport::Concern\nend\n\nclass User\n  include Accountable\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor
            .goto_definition("app/models/concerns/accountable.rb", 10, 6)
            .await
            .is_empty(),
        "removing a concern DSL declaration must remove inherited generated facts"
    );
}
