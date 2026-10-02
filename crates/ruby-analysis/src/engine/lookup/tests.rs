//! `lookup::method` answers match an expected table for every receiver,
//! method, access, and want over one fixture, and classify absence.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use ruby_prism::Visit;
use url::Url;

use super::{method, LookupReceiver, LookupUnknown, MethodAnswer, MethodFound};
use super::{MethodRequest, MethodWant};
use crate::core::{
    FullyQualifiedName, MethodFact, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceKind,
};
use crate::engine::{Project, ResolveMode, SourceFileInput, View};
use crate::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use crate::indexer::RubyDocument;
use crate::inference::semantics::ReceiverAccess;

const FIXTURE: &str = r#"
class Base
  def greet
    "hello"
  end

  def shared
    1
  end

  protected

  def guarded
    :guard
  end

  private

  def secret
    2.0
  end
end

module Greeting
  def wave
    "wave"
  end
end

class Child < Base
  include Greeting

  def greet
    super
  end

  def compare(other)
    other.guarded
  end
end

class Other
  def shared
    "other"
  end
end

class Orphan < MissingParent
  def own
    1
  end
end

def top_helper
  :top
end
"#;

const METHODS: &[&str] = &[
    "greet",
    "shared",
    "guarded",
    "secret",
    "wave",
    "own",
    "compare",
    "top_helper",
    "absent",
];

fn project() -> Project {
    let path = PathBuf::from("/workspace/lib/lookup_fixture.rb");
    let uri = Url::from_file_path(&path).unwrap();
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path,
        content: FIXTURE.to_string(),
        kind: SourceKind::Project,
    });
    let engine = Arc::new(RwLock::new(engine));
    let document = RubyDocument::with_analysis_file_id(uri, FIXTURE.to_string(), 0, file_id);
    let mut collector = FactCollector::analysis_only(
        document,
        Arc::new(NullFactCollectorExtensionHost),
        engine.clone(),
    );
    collector.visit(&ruby_prism::parse(FIXTURE.as_bytes()).node());
    let mut output = collector.finish();
    output.analysis.types.append(&mut output.flow_types);
    engine
        .write()
        .update(file_id, output.analysis, ResolveMode::Immediate);
    Arc::try_unwrap(engine)
        .unwrap_or_else(|_| panic!("the collector released its engine handle"))
        .into_inner()
}

fn namespace(name: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()])
}

fn root() -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(Vec::new(), NamespaceKind::Instance)
}

fn namespaces() -> Vec<FullyQualifiedName> {
    let mut namespaces = ["Base", "Greeting", "Child", "Other", "Orphan", "Nowhere"]
        .into_iter()
        .map(namespace)
        .collect::<Vec<_>>();
    namespaces.push(root());
    namespaces
}

fn methods() -> Vec<RubyMethod> {
    METHODS
        .iter()
        .map(|name| RubyMethod::new(name).unwrap())
        .collect()
}

fn receiver_types() -> Vec<RubyType> {
    vec![
        RubyType::union([RubyType::class("Base"), RubyType::class("Other")]),
        RubyType::union([RubyType::class("Child"), RubyType::class("Orphan")]),
        RubyType::union([RubyType::class("Base"), RubyType::class("Nowhere")]),
    ]
}

fn accesses<'a>(
    child: &'a FullyQualifiedName,
    other: &'a FullyQualifiedName,
) -> Vec<ReceiverAccess<'a>> {
    vec![
        ReceiverAccess::Any,
        ReceiverAccess::Public,
        ReceiverAccess::Protected { caller: child },
        ReceiverAccess::Protected { caller: other },
    ]
}

fn request<'a>(
    receiver: LookupReceiver<'a>,
    method: RubyMethod,
    access: ReceiverAccess<'a>,
    want: MethodWant,
) -> MethodRequest<'a> {
    MethodRequest {
        receiver,
        method,
        access,
        want,
    }
}

#[test]
fn top_level_receiver_is_the_root_namespace() {
    let engine = project();
    let view = engine.view();
    let root = root();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for name in methods() {
        for access in accesses(&child, &other) {
            for want in [
                MethodWant::Callees,
                MethodWant::Facts,
                MethodWant::Return,
                MethodWant::Reference,
                MethodWant::Signatures,
            ] {
                assert_eq!(
                    method(&view, request(LookupReceiver::TopLevel, name, access, want)),
                    method(
                        &view,
                        request(LookupReceiver::Namespace(&root), name, access, want)
                    ),
                    "{name} {access:?} {want:?}"
                );
            }
        }
    }
    let top_helper = RubyMethod::new("top_helper").unwrap();
    assert!(matches!(
        method(
            &view,
            request(
                LookupReceiver::TopLevel,
                top_helper,
                ReceiverAccess::Any,
                MethodWant::Reference
            )
        ),
        MethodAnswer::Found(MethodFound::Reference(_))
    ));
}

/// The expected answers for the fixture, one line per receiver, method, and
/// want. Regenerate with `LOOKUP_EXPECTED_BLESS=1` and review the diff.
const EXPECTED: &str = include_str!("expected_answers.txt");

const WANTS: [MethodWant; 5] = [
    MethodWant::Callees,
    MethodWant::Facts,
    MethodWant::Return,
    MethodWant::Reference,
    MethodWant::Signatures,
];

fn fact_label(fact: &MethodFact) -> String {
    format!("{}@{}", fact.fqn, fact.range.start_byte)
}

fn facts_label<'a>(facts: impl IntoIterator<Item = &'a MethodFact>) -> String {
    let facts = facts.into_iter().map(fact_label).collect::<Vec<_>>();
    format!("[{}]", facts.join(", "))
}

/// A stable text form of an answer that keeps every field the lookup chose.
fn render(answer: &MethodAnswer) -> String {
    match answer {
        MethodAnswer::Found(MethodFound::Callees(callees)) => {
            let callees = callees
                .iter()
                .map(|callee| {
                    let starts = callee
                        .definition_ranges
                        .iter()
                        .map(|range| range.start_byte.to_string())
                        .collect::<Vec<_>>();
                    format!(
                        "{}#{} {:?}@[{}]",
                        callee.owner,
                        callee.method,
                        callee.resolution,
                        starts.join(",")
                    )
                })
                .collect::<Vec<_>>();
            format!("callees [{}]", callees.join(", "))
        }
        MethodAnswer::Found(MethodFound::Facts { owner, facts }) => {
            format!("facts {owner} {}", facts_label(facts))
        }
        MethodAnswer::Found(MethodFound::Return(return_type)) => format!("return {return_type}"),
        MethodAnswer::Found(MethodFound::Reference(fact)) => {
            format!("reference {}", fact_label(fact))
        }
        MethodAnswer::Found(MethodFound::Signatures(facts)) => {
            format!("signatures {}", facts_label(facts.iter()))
        }
        MethodAnswer::Ambiguous { owner, method } => format!("ambiguous {owner}#{method}"),
        MethodAnswer::Missing => "missing".to_string(),
        MethodAnswer::Unknown(reason) => format!("unknown {reason:?}"),
    }
}

fn access_label(access: ReceiverAccess<'_>) -> String {
    match access {
        ReceiverAccess::Any => "any".to_string(),
        ReceiverAccess::Public => "public".to_string(),
        ReceiverAccess::Protected { caller } => format!("protected({caller})"),
    }
}

/// One line per want; accesses that share an answer collapse into `all`, and
/// `any only` means every restricted access is unsupported. `accesses` starts
/// with `Any`. A want that is unsupported for every access has no line.
fn render_receiver(
    view: &View<'_>,
    label: &str,
    receiver: LookupReceiver<'_>,
    accesses: &[ReceiverAccess<'_>],
    lines: &mut Vec<String>,
) {
    for name in methods() {
        for want in WANTS {
            let answers = accesses
                .iter()
                .map(|access| render(&method(view, request(receiver, name, *access, want))))
                .collect::<Vec<_>>();
            let unsupported = render(&MethodAnswer::Unknown(LookupUnknown::Unsupported));
            let answers = if answers.iter().all(|answer| *answer == answers[0]) {
                if answers[0] == unsupported {
                    continue;
                }
                format!("all: {}", answers[0])
            } else if answers[1..].iter().all(|answer| *answer == unsupported) {
                format!("any only: {}", answers[0])
            } else {
                let answers = accesses
                    .iter()
                    .zip(&answers)
                    .map(|(access, answer)| format!("{}: {answer}", access_label(*access)))
                    .collect::<Vec<_>>();
                answers.join(" | ")
            };
            lines.push(format!("{label} {name} {want:?} => {answers}"));
        }
    }
}

fn expected_table(view: &View<'_>) -> String {
    let (child, other) = (namespace("Child"), namespace("Other"));
    let accesses = accesses(&child, &other);
    let mut lines = Vec::new();
    for owner in namespaces() {
        let receivers = [
            ("namespace", LookupReceiver::Namespace(&owner)),
            ("reflection", LookupReceiver::Reflection(&owner)),
            ("super", LookupReceiver::Super { owner: &owner }),
        ];
        for (kind, receiver) in receivers {
            render_receiver(
                view,
                &format!("{kind}({owner})"),
                receiver,
                &accesses,
                &mut lines,
            );
        }
    }
    for receiver_type in receiver_types() {
        let label = format!("type({receiver_type})");
        let receiver = LookupReceiver::Type(&receiver_type);
        render_receiver(view, &label, receiver, &accesses, &mut lines);
    }
    lines.push(String::new());
    lines.join("\n")
}

#[test]
fn answers_match_expected_table() {
    let engine = project();
    let actual = expected_table(&engine.view());
    if std::env::var_os("LOOKUP_EXPECTED_BLESS").is_some() {
        let path = PathBuf::from(file!());
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path.with_file_name("expected_answers.txt"));
        std::fs::write(path, &actual).unwrap();
        return;
    }
    for (line, (actual, expected)) in actual.lines().zip(EXPECTED.lines()).enumerate() {
        assert_eq!(actual, expected, "expected_answers.txt line {}", line + 1);
    }
    assert_eq!(actual.lines().count(), EXPECTED.lines().count());
}

#[test]
fn table_anchors_prove_returns() {
    let engine = project();
    let view = engine.view();
    let greet = RubyMethod::new("greet").unwrap();
    let (base, child) = (namespace("Base"), namespace("Child"));
    let answer = |receiver| {
        method(
            &view,
            request(receiver, greet, ReceiverAccess::Any, MethodWant::Return),
        )
    };
    assert_eq!(
        answer(LookupReceiver::Namespace(&base)),
        MethodAnswer::Found(MethodFound::Return(RubyType::string())),
        "the fixture must prove at least one return"
    );
    assert_eq!(
        answer(LookupReceiver::Super { owner: &child }),
        MethodAnswer::Found(MethodFound::Return(RubyType::string())),
        "`super` from Child#greet must reach Base#greet"
    );
}

#[test]
fn type_receivers_reject_single_definition_wants() {
    let engine = project();
    let view = engine.view();
    let name = RubyMethod::new("shared").unwrap();
    for receiver_type in receiver_types() {
        for want in [MethodWant::Facts, MethodWant::Return, MethodWant::Reference] {
            assert_eq!(
                method(
                    &view,
                    request(
                        LookupReceiver::Type(&receiver_type),
                        name,
                        ReceiverAccess::Any,
                        want
                    )
                ),
                MethodAnswer::Unknown(LookupUnknown::Unsupported)
            );
        }
    }
}

/// Unknown lookup edges suppress missing-method claims: only a fully known
/// receiver chain proves absence.
#[test]
fn absence_is_missing_only_on_known_receivers_with_complete_ancestry() {
    let engine = project();
    let view = engine.view();
    let absent = RubyMethod::new("absent").unwrap();
    let base = namespace("Base");
    let orphan = namespace("Orphan");
    let nowhere = namespace("Nowhere");
    let greeting = namespace("Greeting");
    let answer =
        |receiver, want| method(&view, request(receiver, absent, ReceiverAccess::Any, want));

    for want in [MethodWant::Reference, MethodWant::Facts] {
        assert_eq!(
            answer(LookupReceiver::Namespace(&base), want),
            MethodAnswer::Missing
        );
        assert_eq!(
            answer(LookupReceiver::Namespace(&orphan), want),
            MethodAnswer::Unknown(LookupUnknown::IncompleteChain)
        );
        assert_eq!(
            answer(LookupReceiver::Namespace(&nowhere), want),
            MethodAnswer::Unknown(LookupUnknown::Receiver)
        );
    }
    assert_eq!(
        answer(LookupReceiver::Super { owner: &base }, MethodWant::Callees),
        MethodAnswer::Missing
    );
    assert_eq!(
        answer(
            LookupReceiver::Super { owner: &orphan },
            MethodWant::Callees
        ),
        MethodAnswer::Unknown(LookupUnknown::IncompleteChain)
    );
    assert_eq!(
        answer(LookupReceiver::Namespace(&nowhere), MethodWant::Callees),
        MethodAnswer::Unknown(LookupUnknown::Receiver)
    );
    assert_eq!(
        answer(LookupReceiver::Namespace(&greeting), MethodWant::Facts),
        MethodAnswer::Unknown(LookupUnknown::Receiver),
        "a module's own chain does not show what its includers supply"
    );
    assert_eq!(
        answer(LookupReceiver::Namespace(&base), MethodWant::Return),
        MethodAnswer::Unknown(LookupUnknown::NoEvidence)
    );

    let own = RubyMethod::new("own").unwrap();
    assert!(
        matches!(
            method(
                &view,
                request(
                    LookupReceiver::Namespace(&orphan),
                    own,
                    ReceiverAccess::Any,
                    MethodWant::Reference
                )
            ),
            MethodAnswer::Found(MethodFound::Reference(_))
        ),
        "a method on the receiver itself wins before the unresolved edge"
    );
}

#[test]
fn module_receivers_dispatch_to_includers() {
    let engine = project();
    let view = engine.view();
    let wave = RubyMethod::new("wave").unwrap();
    let greeting = namespace("Greeting");
    let child = namespace("Child");
    let callees = method(
        &view,
        request(
            LookupReceiver::Namespace(&child),
            wave,
            ReceiverAccess::Any,
            MethodWant::Callees,
        ),
    )
    .into_callees()
    .unwrap();
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0].owner, greeting);
    assert!(matches!(
        method(
            &view,
            request(
                LookupReceiver::Namespace(&greeting),
                wave,
                ReceiverAccess::Any,
                MethodWant::Reference
            )
        ),
        MethodAnswer::Found(MethodFound::Reference(_))
    ));
}

#[test]
fn private_and_protected_methods_follow_access() {
    let engine = project();
    let view = engine.view();
    let base = namespace("Base");
    let child = namespace("Child");
    let other = namespace("Other");
    let found = |name: &str, access| {
        method(
            &view,
            request(
                LookupReceiver::Namespace(&base),
                RubyMethod::new(name).unwrap(),
                access,
                MethodWant::Signatures,
            ),
        )
        .into_signatures()
        .len()
    };
    assert_eq!(found("secret", ReceiverAccess::Any), 1);
    assert_eq!(found("secret", ReceiverAccess::Public), 0);
    assert_eq!(
        found("guarded", ReceiverAccess::Protected { caller: &child }),
        1
    );
    assert_eq!(
        found("guarded", ReceiverAccess::Protected { caller: &other }),
        0
    );
    assert_eq!(found("guarded", ReceiverAccess::Public), 0);
}
