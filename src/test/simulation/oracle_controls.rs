//! Handwritten Ruby programs and declarative model inputs share a reviewed
//! expected target. Neither side generates its expectation from its observation.
//! The Ruby side runs explicitly in `support/simulation/run_oracle_controls.py`.

use super::graph::NamespaceBuilder;
use super::{OracleState, SyntheticProject};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    receiver: String,
    method: String,
    kind: String,
    expected: Option<String>,
    namespaces: Vec<Namespace>,
    ruby: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Namespace {
    name: String,
    kind: String,
    superclass: Option<String>,
    #[serde(default)]
    includes: Vec<String>,
    #[serde(default)]
    prepends: Vec<String>,
    #[serde(default)]
    methods: Vec<Method>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Method {
    name: String,
    kind: String,
    #[serde(default)]
    private: bool,
}

fn populate(builder: &mut NamespaceBuilder<'_>, namespace: &Namespace) {
    if let Some(parent) = &namespace.superclass {
        builder.superclass(parent);
    }
    for include in &namespace.includes {
        builder.include(include);
    }
    for prepend in &namespace.prepends {
        builder.prepend(prepend);
    }
    for method in &namespace.methods {
        let fact = match method.kind.as_str() {
            "instance" => builder.method(&method.name),
            "class" => builder.class_method(&method.name),
            other => panic!("unsupported reviewed method kind: {other}"),
        };
        if method.private {
            fact.private();
        }
    }
}

#[test]
fn independent_oracle_matches_reviewed_neutral_ruby_dispatch_contracts() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../../support/simulation/oracle_cases.json"
    ))
    .expect("reviewed neutral Ruby controls must parse without unknown fields");
    let ids = cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        cases.len(),
        6,
        "all six reviewed dispatch controls must run"
    );
    assert_eq!(ids.len(), cases.len(), "control identities must be unique");
    for case in cases {
        assert!(
            !case.ruby.is_empty(),
            "a control must include independent Ruby source"
        );
        let mut project = SyntheticProject::new(&case.id);
        for namespace in &case.namespaces {
            match namespace.kind.as_str() {
                "class" => project.class(&namespace.name, |builder| populate(builder, namespace)),
                "module" => project.module(&namespace.name, |builder| populate(builder, namespace)),
                other => panic!("unsupported reviewed namespace kind: {other}"),
            };
        }
        let render = project.render();
        let oracle = OracleState::all_files(&project, &render.map);
        let actual = match case.kind.as_str() {
            "instance" => oracle.resolve_public_instance_method(&case.receiver, &case.method),
            "class" => oracle.resolve_class_method(&case.receiver, &case.method),
            other => panic!("unsupported reviewed query kind: {other}"),
        };
        assert_eq!(
            actual.map(|target| target.signature()),
            case.expected,
            "independent neutral Ruby oracle contract: {}",
            case.id
        );
    }
}
