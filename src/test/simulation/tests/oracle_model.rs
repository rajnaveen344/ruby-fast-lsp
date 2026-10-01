use super::coverage::{resolved_constant, resolved_signature};
use super::hierarchy_projects::{
    block_type_flow_project, const_namespace_project, mro_prepend_project, mro_project,
    reopened_namespace_project,
};
use super::phase1_project;
use crate::test::simulation::{
    EditOp, EngineSimulationRunner, MethodTarget, OracleState, SimulationRunner,
};
use std::collections::BTreeSet;

#[tokio::test]
async fn generated_project_semantic_diagnostic_false_positive_budget_is_zero() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.assert_semantic_false_positive_budget(0).await;
}

#[test]
fn generated_project_emits_complex_ruby_graph() {
    let project = phase1_project();
    let render = project.render();

    assert!(
        (40..=60).contains(&render.files.len()),
        "expected 40-60 files, got {}",
        render.files.len()
    );
    assert!(
        (100..=150).contains(&project.enabled_method_count()),
        "expected 100-150 methods, got {}",
        project.enabled_method_count()
    );
    assert!(
        (30..=60).contains(&project.meaningful_edge_count()),
        "expected 30-60 meaningful graph edges, got {}",
        project.meaningful_edge_count()
    );
    assert!(render
        .map
        .calls
        .iter()
        .any(|call| call.shape_name == "ivar"));
    assert!(render
        .map
        .calls
        .iter()
        .any(|call| call.shape_name == "one-hop"));
    for shape in [
        "bare-do-block",
        "bare-brace-block",
        "bare-lambda",
        "bare-proc",
        "array-block-param",
        "yield-block-param",
    ] {
        assert!(
            render.map.calls.iter().any(|call| call.shape_name == shape),
            "expected generated call shape `{shape}`"
        );
    }
    assert!(!render.map.constant_refs.is_empty());
}

#[test]
fn generated_project_applies_graph_edits_to_files() {
    let mut project = phase1_project();
    let initial = project.render();

    let step = project
        .edits
        .iter()
        .find(|step| step.name == "delete invoice currency")
        .expect("test edit must exist")
        .clone();
    project.apply_step(&step);
    let after_constant_delete = project.render();
    assert_ne!(
        initial.files["billing/invoice.rb"],
        after_constant_delete.files["billing/invoice.rb"]
    );
    assert_eq!(
        initial.files["payments/gateway.rb"],
        after_constant_delete.files["payments/gateway.rb"]
    );

    let step = project
        .edits
        .iter()
        .find(|step| step.name == "change inheritance and mixin")
        .expect("test edit must exist")
        .clone();
    project.apply_step(&step);
    let after_inheritance = project.render();
    assert!(after_inheritance.files["billing/invoice.rb"].contains("Payments::FallbackGateway"));
    assert!(!after_inheritance.files["billing/invoice.rb"].contains("include Audit::Trackable"));
}

#[test]
fn generated_project_oracle_resolves_mro_table() {
    let mut project = mro_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Second#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemoveInclude {
        owner: "SimMro::Child".to_string(),
        included: "SimMro::Second".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::First#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemoveInclude {
        owner: "SimMro::Child".to_string(),
        included: "SimMro::First".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Base#resolve_token".to_string())
    );

    project.apply_op(&EditOp::DeleteMethod(MethodTarget::parse(
        "SimMro::Base#resolve_token",
    )));
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        None
    );
}

#[test]
fn generated_project_oracle_resolves_prepend_before_own_method() {
    let mut project = mro_prepend_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::Second#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemovePrepend {
        owner: "SimMroPrepend::Child".to_string(),
        prepended: "SimMroPrepend::Second".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::First#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemovePrepend {
        owner: "SimMroPrepend::Child".to_string(),
        prepended: "SimMroPrepend::First".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::Child#resolve_token".to_string())
    );
}

#[test]
fn generated_project_oracle_tracks_partial_namespace_visibility() {
    let project = mro_project();
    let render = project.render();
    let indexed_files = ["sim_mro/child.rb", "sim_mro/caller.rb", "sim_mro/base.rb"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();
    let oracle = OracleState::with_indexed_files(&project, &render.map, indexed_files);

    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Base#resolve_token".to_string())
    );
}

#[test]
fn generated_project_oracle_resolves_constant_namespace_table() {
    let project = const_namespace_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);

    assert_eq!(
        resolved_constant(&oracle, &project, "TOKEN"),
        Some("SimConst::Outer::Inner::TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "PARENT_TOKEN"),
        Some("SimConst::Outer::PARENT_TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "::SimConst::Outer::TOKEN"),
        Some("SimConst::Outer::TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "Shared::TOKEN"),
        Some("SimConst::Shared::TOKEN".to_string())
    );
}

#[tokio::test]
async fn generated_project_resolves_reopened_namespace_fragments() {
    let runner = SimulationRunner::start(reopened_namespace_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
    runner.check_types().await;
}

#[tokio::test]
async fn generated_project_checks_block_type_flow_hover_and_hints() {
    let runner = SimulationRunner::start(block_type_flow_project()).await;

    runner.check_hover().await;
    runner.check_types().await;
}

#[test]
fn generated_project_oracle_tracks_partial_constant_visibility() {
    let project = const_namespace_project();
    let render = project.render();
    let indexed_files = [
        "sim_const/outer.rb",
        "sim_const/outer/inner.rb",
        "sim_const/outer/inner/reader.rb",
    ]
    .into_iter()
    .map(String::from)
    .collect::<BTreeSet<_>>();
    let oracle = OracleState::with_indexed_files(&project, &render.map, indexed_files);

    assert_eq!(resolved_constant(&oracle, &project, "Shared::TOKEN"), None);
    assert_eq!(
        resolved_constant(&oracle, &project, "PARENT_TOKEN"),
        Some("SimConst::Outer::PARENT_TOKEN".to_string())
    );
}

#[test]
fn generated_project_engine_oracle_checks_definitions_and_references() {
    let runner = EngineSimulationRunner::start(phase1_project());
    runner.check_definitions();
    runner.check_references();
    runner.check_types();
}

#[test]
fn generated_project_engine_oracle_updates_after_graph_edits() {
    let project = phase1_project();
    let mut runner = EngineSimulationRunner::start(project.clone());
    runner.check_definitions();
    runner.check_references();
    runner.check_types();

    for step in &project.edits {
        runner.apply_step(step);
        runner.check_definitions();
        runner.check_references();
        runner.check_types();
    }
}
