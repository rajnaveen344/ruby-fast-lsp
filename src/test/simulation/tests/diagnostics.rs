use super::phase1_project;
use crate::test::simulation::{CallShape, SimulationRunner, SyntheticProject};

#[tokio::test]
async fn generated_project_updates_method_diagnostics() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_initial().await;

    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "delete gateway capture")
                .expect("test edit must exist"),
        )
        .await;
    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "restore gateway capture")
                .expect("test edit must exist"),
        )
        .await;
}

#[tokio::test]
async fn generated_project_runtime_core_methods_do_not_warn() {
    let mut project = SyntheticProject::new("synthetic_runtime_core");
    project
        .class("BasicObject", |_class| {})
        .module("Kernel", |module| {
            module.method("puts");
            module.method("warn");
            module.method("nil?");
        })
        .class("Object", |class| {
            class.include("Kernel");
        })
        .class("ScenarioExtractor", |class| {
            class
                .method("to_output")
                .calls("Kernel#puts", CallShape::Bare);
            class
                .method("to_warning")
                .calls("Kernel#warn", CallShape::Bare);
            class
                .method("to_nil_check")
                .calls("Kernel#nil?", CallShape::Bare);
        })
        .class("MinimalRuntime::Leaf", |class| {
            class.superclass("BasicObject");
            class
                .method("to_output")
                .calls("Kernel#puts", CallShape::Bare);
        });

    let runner = SimulationRunner::start(project).await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "puts")
        .await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "warn")
        .await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "nil?")
        .await;
    runner
        .assert_unresolved_method("minimal_runtime/leaf.rb", "puts")
        .await;
}

#[tokio::test]
async fn generated_project_rbs_builtin_methods_do_not_warn() {
    let mut project = SyntheticProject::new("synthetic_rbs_builtins");
    project.raw_file(
        "stdlib/core_receivers.rb",
        r#"
items = [1, 2, 3]
items.include?(2)
items.empty?
items.first
items.fetch(0)
items << 4

lookup = {name: "ruby", count: 1}
lookup.fetch(:name)
lookup.empty?
lookup.each_key {}

name = "ruby"
name.empty?
name.upcase
name.to_s

count = 1
count.zero?
count.to_s

missing = nil
missing.nil?
missing.to_s

items.nope_builtin(2)
lookup.nope_hash
name.nope_string
count.nope_integer
missing.nope_nil
"#,
    );

    let runner = SimulationRunner::start(project).await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "include?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "empty?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "first")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "fetch")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "<<")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "each_key")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "upcase")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "to_s")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "zero?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "nil?")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_builtin")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_hash")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_string")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_integer")
        .await;
}

#[tokio::test]
async fn generated_project_updates_constant_diagnostics() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_initial().await;

    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "delete invoice currency")
                .expect("test edit must exist"),
        )
        .await;
    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "restore invoice currency")
                .expect("test edit must exist"),
        )
        .await;
}
