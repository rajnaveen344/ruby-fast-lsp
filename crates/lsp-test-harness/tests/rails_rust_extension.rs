use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

fn rails_package_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("test harness crate must live under crates/lsp-test-harness")
        .join("extensions/rails-ruby")
}

async fn rails_editor(version: &str) -> (TempDir, FakeEditor) {
    let workspace = TempDir::new().expect("Rails workspace must be created");
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://rubygems.org'\ngem 'rails'\n",
    )
    .expect("Rails Gemfile must be written");
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        format!("GEM\n  remote: https://rubygems.org/\n  specs:\n    rails ({version})\n"),
    )
    .expect("Rails lockfile must be written");
    let editor =
        FakeEditor::with_extension_package_and_workspace(rails_package_dir(), workspace.path())
            .await;
    (workspace, editor)
}

fn workspace_file(workspace: &TempDir, relative: &str) -> String {
    workspace
        .path()
        .join(relative)
        .to_string_lossy()
        .to_string()
}

#[tokio::test]
async fn packaged_rails_rust_wasm_preserves_the_full_public_contract() {
    let (workspace, mut editor) = rails_editor("8.1.3").await;
    let statuses = editor.extension_status().await;
    assert!(
        statuses
            .iter()
            .any(|status| status.id == "rails-ruby" && status.status == "loaded"),
        "Rails Rust extension must load, got {statuses:?}"
    );

    let application_controller =
        workspace_file(&workspace, "app/controllers/application_controller.rb");
    editor
        .open(
            &application_controller,
            "class ApplicationController\nend\n",
        )
        .await;
    let users_controller = workspace_file(&workspace, "app/controllers/users_controller.rb");
    editor
        .open(
            &users_controller,
            "class UsersController < ApplicationController\n  def show\n  end\n  def links\n    users_path\n    account_path\n  end\nend\n",
        )
        .await;
    let routes = workspace_file(&workspace, "config/routes.rb");
    editor
        .open(
            &routes,
            "Rails.application.routes.draw do\n  resources :users\n  get \"/account\", to: \"users#show\", as: :account\nend\n",
        )
        .await;

    let model = workspace_file(&workspace, "app/models/user.rb");
    editor
        .open(
            &model,
            "module Billing\n  class Account\n  end\nend\n\nclass User\n  belongs_to :account, class_name: \"Billing::Account\"\n  before_save :normalize_account\n  def display\n    account\n  end\n  private\n  def normalize_account\n  end\nend\n",
        )
        .await;
    let association = editor.goto_definition(&model, 9, 6).await;
    assert_eq!(association.len(), 1, "association reader must resolve");
    assert_eq!(association[0].range.start.line, 6);
    let class_name = editor.goto_definition(&model, 6, 42).await;
    assert_eq!(class_name.len(), 1, "class_name must navigate");
    assert_eq!(class_name[0].range.start.line, 1);
    let callback = editor.goto_definition(&model, 7, 18).await;
    assert_eq!(callback.len(), 1, "callback symbol must navigate");
    assert_eq!(callback[0].range.start.line, 12);

    assert_eq!(
        editor.goto_definition(&users_controller, 4, 6).await.len(),
        1,
        "resource helper must resolve"
    );
    assert_eq!(
        editor.goto_definition(&users_controller, 5, 6).await.len(),
        1,
        "named route helper must resolve"
    );
    assert_eq!(
        editor.goto_definition(&routes, 2, 31).await.len(),
        1,
        "route action segment must navigate"
    );

    editor
        .set(&routes, "Rails.application.routes.draw do\nend\n")
        .await;
    assert!(
        editor
            .goto_definition(&users_controller, 4, 6)
            .await
            .is_empty(),
        "removing resource routes must remove generated helpers"
    );
    assert!(
        editor
            .goto_definition(&users_controller, 5, 6)
            .await
            .is_empty(),
        "removing named routes must remove generated helpers"
    );

    let job = workspace_file(&workspace, "app/jobs/billing/email_job.rb");
    editor
        .open(
            &job,
            "module Billing\n  class EmailJob\n    def perform(user)\n    end\n  end\nend\nBilling::EmailJob.perform_later(user)\n",
        )
        .await;
    let perform = editor.goto_definition(&job, 6, 24).await;
    assert_eq!(
        perform.len(),
        1,
        "enqueue entry point must navigate to perform"
    );
    assert_eq!(perform[0].range.start.line, 2);

    let lenses = editor.code_lens(&users_controller).await;
    assert_eq!(
        lenses
            .iter()
            .filter_map(|lens| lens.command.as_ref())
            .filter(|command| command.title == "Open View")
            .count(),
        2,
        "public controller actions must retain Open View lenses"
    );

    editor
        .set(
            &model,
            "module Billing\n  class Account\n  end\nend\n\nclass User\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor.goto_definition(&model, 7, 6).await.is_empty(),
        "removing an association must remove generated facts"
    );
}

#[tokio::test]
async fn packaged_rails_rust_wasm_handles_concerns_polymorphism_and_nested_routes() {
    let (workspace, mut editor) = rails_editor("6.1.7").await;
    let application_controller =
        workspace_file(&workspace, "app/controllers/application_controller.rb");
    editor
        .open(
            &application_controller,
            "class ApplicationController\nend\n",
        )
        .await;
    let controller = workspace_file(&workspace, "app/controllers/admin/users_controller.rb");
    editor
        .open(
            &controller,
            "module Admin\n  class UsersController < ApplicationController\n    def links\n      admin_users_path\n    end\n  end\nend\n",
        )
        .await;
    let routes = workspace_file(&workspace, "config/routes.rb");
    editor
        .open(
            &routes,
            "Rails.application.routes.draw do\n  namespace :admin do\n    resources :users\n  end\nend\n",
        )
        .await;
    assert_eq!(
        editor.goto_definition(&controller, 3, 8).await.len(),
        1,
        "nested namespace frames must prefix generated helpers"
    );
    let controller_target = editor.goto_definition(&routes, 2, 17).await;
    assert_eq!(controller_target.len(), 1);
    assert!(controller_target[0]
        .uri
        .path()
        .ends_with("/app/controllers/admin/users_controller.rb"));

    let concern = workspace_file(&workspace, "app/models/concerns/accountable.rb");
    editor
        .open(
            &concern,
            "class Account\nend\n\nmodule Accountable\n  extend ActiveSupport::Concern\n  included do\n    belongs_to :account\n  end\nend\n\nclass User\n  include Accountable\n  def display\n    account\n  end\nend\n",
        )
        .await;
    let inherited = editor.goto_definition(&concern, 13, 6).await;
    assert_eq!(
        inherited.len(),
        1,
        "association facts in concerns must flow through core-owned MRO"
    );
    assert_eq!(inherited[0].range.start.line, 6);

    let polymorphic = workspace_file(&workspace, "app/models/attachment.rb");
    editor
        .open(
            &polymorphic,
            "class Attachment\n  belongs_to :subject, polymorphic: true\n  def attached\n    subject\n  end\nend\n",
        )
        .await;
    assert!(
        editor.goto_definition(&polymorphic, 1, 16).await.is_empty(),
        "polymorphic associations must not invent a target constant"
    );
    assert_eq!(
        editor.goto_definition(&polymorphic, 3, 6).await.len(),
        1,
        "polymorphic reader must still be generated"
    );

    editor
        .set(
            &concern,
            "class Account\nend\n\nmodule Accountable\n  extend ActiveSupport::Concern\nend\n\nclass User\n  include Accountable\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor.goto_definition(&concern, 10, 6).await.is_empty(),
        "removing a concern DSL declaration must remove inherited generated facts"
    );
}

#[tokio::test]
async fn packaged_rails_manifest_fails_closed_for_unsupported_versions() {
    let (workspace, mut editor) = rails_editor("9.0.0").await;
    let model = workspace_file(&workspace, "app/models/user.rb");
    editor
        .open(
            &model,
            "class User\n  belongs_to :account\n  def display\n    account\n  end\nend\n",
        )
        .await;
    assert!(
        editor.goto_definition(&model, 3, 6).await.is_empty(),
        "unsupported Rails versions must not receive generated association facts"
    );

    let controller = workspace_file(&workspace, "app/controllers/users_controller.rb");
    editor
        .open(
            &controller,
            "class UsersController\n  def show\n  end\nend\n",
        )
        .await;
    assert!(
        editor.code_lens(&controller).await.is_empty(),
        "unsupported Rails versions must not receive extension lenses"
    );
    let status = editor
        .extension_status()
        .await
        .into_iter()
        .find(|status| status.id == "rails-ruby")
        .expect("Rails package must remain discoverable");
    assert_eq!(status.status, "loaded");
}
