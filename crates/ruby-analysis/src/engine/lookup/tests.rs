//! `lookup::method` answers equal the legacy access-flavoured wrappers for
//! every receiver, access, and want they cover, and classify absence.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use ruby_prism::Visit;
use url::Url;

use super::{method, method_cached, LookupReceiver, LookupUnknown, MethodAnswer, MethodFound};
use super::{MethodRequest, MethodWant};
use crate::core::{
    FullyQualifiedName, MethodFact, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceKind,
};
use crate::engine::resolution::{method_facts_in_chain, method_lookup_chain, MethodLookupResult};
use crate::engine::{AnalysisQueryCache, Project, ResolveMode, SourceFileInput};
use crate::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use crate::indexer::RubyDocument;
use crate::inference::semantics::{ReceiverAccess, Semantics};

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

fn is_any(access: ReceiverAccess<'_>) -> bool {
    matches!(access, ReceiverAccess::Any)
}

/// A comparable projection of the legacy reference result.
fn legacy_reference(
    result: MethodLookupResult,
) -> Option<(Option<Arc<MethodFact>>, FullyQualifiedName)> {
    match result {
        MethodLookupResult::Found(fact) => Some((Some(fact.clone()), fact.owner.clone())),
        MethodLookupResult::Ambiguous { owner, .. } => Some((None, owner)),
        MethodLookupResult::Missing | MethodLookupResult::Unknown(_) => None,
    }
}

fn answer_reference(answer: MethodAnswer) -> Option<(Option<Arc<MethodFact>>, FullyQualifiedName)> {
    match answer.into_reference() {
        MethodAnswer::Found(fact) => Some((Some(fact.clone()), fact.owner.clone())),
        MethodAnswer::Ambiguous { owner, .. } => Some((None, owner)),
        MethodAnswer::Missing | MethodAnswer::Unknown(_) => None,
    }
}

#[test]
fn namespace_callees_equal_legacy_wrappers() {
    let engine = project();
    let view = engine.view();
    let cache = AnalysisQueryCache::default();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for owner in namespaces() {
        for name in methods() {
            for access in accesses(&child, &other) {
                let legacy = match access {
                    ReceiverAccess::Any => view.resolve_method_callees(&owner, &name),
                    ReceiverAccess::Public => view.resolve_public_method_callees(&owner, &name),
                    ReceiverAccess::Protected { caller } => {
                        view.resolve_protected_method_callees(&owner, &name, caller)
                    }
                };
                let request = request(
                    LookupReceiver::Namespace(&owner),
                    name,
                    access,
                    MethodWant::Callees,
                );
                assert_eq!(
                    method(&view, request).into_callees(),
                    legacy,
                    "{owner} {name} {access:?}"
                );
                assert_eq!(
                    method_cached(&view, request, &cache).into_callees(),
                    legacy,
                    "cached {owner} {name} {access:?}"
                );
            }
            let cached = view.resolve_method_callees_cached(&owner, &name, &cache);
            let request = request(
                LookupReceiver::Namespace(&owner),
                name,
                ReceiverAccess::Any,
                MethodWant::Callees,
            );
            assert_eq!(method_cached(&view, request, &cache).into_callees(), cached);
        }
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

#[test]
fn type_callees_and_signatures_equal_legacy_wrappers() {
    let engine = project();
    let view = engine.view();
    let cache = AnalysisQueryCache::default();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for receiver_type in receiver_types() {
        for name in methods() {
            for access in accesses(&child, &other) {
                let legacy = match access {
                    ReceiverAccess::Any => {
                        view.resolve_method_callees_for_type(&receiver_type, &name)
                    }
                    ReceiverAccess::Public => {
                        view.resolve_public_method_callees_for_type(&receiver_type, &name)
                    }
                    ReceiverAccess::Protected { caller } => view
                        .resolve_protected_method_callees_for_type(&receiver_type, &name, caller),
                };
                let callees = request(
                    LookupReceiver::Type(&receiver_type),
                    name,
                    access,
                    MethodWant::Callees,
                );
                assert_eq!(
                    method(&view, callees).into_callees(),
                    legacy,
                    "{receiver_type:?} {name} {access:?}"
                );

                let legacy = match access {
                    ReceiverAccess::Any => {
                        view.resolve_method_signature_facts_for_type(&receiver_type, &name)
                    }
                    ReceiverAccess::Public => view.resolve_method_signature_facts_for_type_inner(
                        &receiver_type,
                        &name,
                        false,
                        None,
                        None,
                    ),
                    ReceiverAccess::Protected { caller } => view
                        .resolve_protected_method_signature_facts_for_type(
                            &receiver_type,
                            &name,
                            caller,
                        ),
                };
                let signatures = request(
                    LookupReceiver::Type(&receiver_type),
                    name,
                    access,
                    MethodWant::Signatures,
                );
                assert_eq!(*method(&view, signatures).into_signatures(), legacy);
                assert_eq!(
                    *method_cached(&view, signatures, &cache).into_signatures(),
                    legacy
                );
            }
            let cached =
                view.resolve_method_signature_facts_for_type_cached(&receiver_type, &name, &cache);
            let signatures = request(
                LookupReceiver::Type(&receiver_type),
                name,
                ReceiverAccess::Any,
                MethodWant::Signatures,
            );
            assert_eq!(
                *method_cached(&view, signatures, &cache).into_signatures(),
                cached
            );
        }
    }
}

#[test]
fn namespace_returns_equal_legacy_wrappers() {
    let engine = project();
    let view = engine.view();
    let cache = AnalysisQueryCache::default();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for owner in namespaces() {
        for name in methods() {
            for access in accesses(&child, &other) {
                let (legacy, legacy_cached) = match access {
                    ReceiverAccess::Any => (
                        view.method_return_type_for_receiver(&owner, &name),
                        view.method_return_type_for_receiver_cached(&owner, &name, &cache),
                    ),
                    ReceiverAccess::Public => (
                        view.method_return_type_for_public_receiver(&owner, &name),
                        view.method_return_type_for_public_receiver_cached(&owner, &name, &cache),
                    ),
                    ReceiverAccess::Protected { caller } => (
                        view.method_return_type_for_protected_receiver(&owner, &name, caller),
                        view.method_return_type_for_protected_receiver_cached(
                            &owner, &name, caller, &cache,
                        ),
                    ),
                };
                let request = request(
                    LookupReceiver::Namespace(&owner),
                    name,
                    access,
                    MethodWant::Return,
                );
                assert_eq!(
                    method(&view, request).into_return_type(),
                    legacy,
                    "{owner} {name} {access:?}"
                );
                assert_eq!(
                    method_cached(&view, request, &cache).into_return_type(),
                    legacy_cached
                );
            }
        }
    }
    let greet = RubyMethod::new("greet").unwrap();
    assert_eq!(
        method(
            &view,
            request(
                LookupReceiver::Namespace(&namespace("Base")),
                greet,
                ReceiverAccess::Any,
                MethodWant::Return
            )
        ),
        MethodAnswer::Found(MethodFound::Return(RubyType::string())),
        "the fixture must prove at least one return"
    );
}

#[test]
fn namespace_signatures_and_facts_equal_legacy_paths() {
    let engine = project();
    let view = engine.view();
    let cache = AnalysisQueryCache::default();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for owner in namespaces() {
        for name in methods() {
            for access in accesses(&child, &other) {
                let (allow_private, caller) = access.visibility();
                let legacy = match access {
                    ReceiverAccess::Any => view.resolve_method_signature_facts(&owner, &name),
                    ReceiverAccess::Public => {
                        view.resolve_method_signature_facts_inner(&owner, &name, false, None)
                    }
                    ReceiverAccess::Protected { caller } => {
                        view.resolve_protected_method_signature_facts(&owner, &name, caller)
                    }
                };
                let signatures = request(
                    LookupReceiver::Namespace(&owner),
                    name,
                    access,
                    MethodWant::Signatures,
                );
                assert_eq!(
                    *method(&view, signatures).into_signatures(),
                    legacy,
                    "{owner} {name} {access:?}"
                );
                assert_eq!(
                    *method_cached(&view, signatures, &cache).into_signatures(),
                    legacy
                );

                let chain = method_lookup_chain(&engine, &owner);
                let legacy = method_facts_in_chain(&engine, &chain, &name, allow_private, caller);
                for receiver in [
                    LookupReceiver::Namespace(&owner),
                    LookupReceiver::Reflection(&owner),
                ] {
                    let facts = request(receiver, name, access, MethodWant::Facts);
                    assert_eq!(
                        method(&view, facts).into_facts(),
                        legacy.clone(),
                        "{owner} {name} {access:?}"
                    );
                }
            }
            assert_eq!(
                *method_cached(
                    &view,
                    request(
                        LookupReceiver::Namespace(&owner),
                        name,
                        ReceiverAccess::Any,
                        MethodWant::Signatures
                    ),
                    &cache
                )
                .into_signatures(),
                view.resolve_method_signature_facts_cached(&owner, &name, &cache)
            );
            assert_eq!(
                *method_cached(
                    &view,
                    request(
                        LookupReceiver::Namespace(&owner),
                        name,
                        ReceiverAccess::Any,
                        MethodWant::Signatures
                    ),
                    &cache
                )
                .into_signatures(),
                *view.resolve_method_signature_facts_cached_arc(&owner, &name, &cache)
            );
        }
    }
}

#[test]
fn super_and_reflection_equal_legacy_paths() {
    let engine = project();
    let view = engine.view();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for owner in namespaces() {
        for name in methods() {
            let super_callee = view.resolve_super_method_callee(&owner, &name);
            let reflected = view.resolve_reflected_method_callee(&owner, &name);
            let super_return = Semantics::super_method_return_type(&view, &owner, &name, &|_| None);
            for access in accesses(&child, &other) {
                let super_receiver = LookupReceiver::Super { owner: &owner };
                let reflection = LookupReceiver::Reflection(&owner);
                let callees = method(
                    &view,
                    request(super_receiver, name, access, MethodWant::Callees),
                );
                let reflected_callees = method(
                    &view,
                    request(reflection, name, access, MethodWant::Callees),
                );
                let returns = method(
                    &view,
                    request(super_receiver, name, access, MethodWant::Return),
                );
                if is_any(access) {
                    assert_eq!(
                        callees.into_callees(),
                        super_callee.clone().map(|callee| vec![callee])
                    );
                    assert_eq!(
                        reflected_callees.into_callees(),
                        reflected.clone().map(|callee| vec![callee])
                    );
                    assert_eq!(returns.into_return_type(), super_return, "{owner} {name}");
                } else {
                    for answer in [callees, reflected_callees, returns] {
                        assert_eq!(answer, MethodAnswer::Unknown(LookupUnknown::Unsupported));
                    }
                }
                for (receiver, want) in [
                    (super_receiver, MethodWant::Signatures),
                    (super_receiver, MethodWant::Facts),
                    (reflection, MethodWant::Return),
                    (reflection, MethodWant::Signatures),
                ] {
                    assert_eq!(
                        method(&view, request(receiver, name, access, want)),
                        MethodAnswer::Unknown(LookupUnknown::Unsupported)
                    );
                }
            }
        }
    }
    let greet = RubyMethod::new("greet").unwrap();
    assert_eq!(
        method(
            &view,
            request(
                LookupReceiver::Super { owner: &child },
                greet,
                ReceiverAccess::Any,
                MethodWant::Return
            )
        ),
        MethodAnswer::Found(MethodFound::Return(RubyType::string())),
        "`super` from Child#greet must reach Base#greet"
    );
}

#[test]
fn references_equal_legacy_paths() {
    let engine = project();
    let view = engine.view();
    let (child, other) = (namespace("Child"), namespace("Other"));
    for owner in namespaces() {
        for name in methods() {
            let mut chain_cache = crate::engine::resolution::MethodLookupChainCache::new();
            let cases = [
                (
                    LookupReceiver::Namespace(&owner),
                    view.resolve_method_reference(&owner, &name),
                ),
                (
                    LookupReceiver::Reflection(&owner),
                    view.resolve_instance_method_reference_with_chain_cache(
                        &owner,
                        &name,
                        &mut chain_cache,
                    ),
                ),
                (
                    LookupReceiver::Super { owner: &owner },
                    view.resolve_super_method_reference(&owner, &name),
                ),
            ];
            for (receiver, legacy) in cases {
                let legacy = legacy_reference(legacy);
                for access in accesses(&child, &other) {
                    let answer = method(
                        &view,
                        request(receiver, name, access, MethodWant::Reference),
                    );
                    if is_any(access) {
                        assert_eq!(answer_reference(answer), legacy, "{owner} {name}");
                    } else {
                        assert_eq!(answer, MethodAnswer::Unknown(LookupUnknown::Unsupported));
                    }
                }
            }
        }
    }
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
