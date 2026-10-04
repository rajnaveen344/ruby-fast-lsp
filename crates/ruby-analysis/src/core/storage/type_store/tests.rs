use crate::core::{FullyQualifiedName, RubyConstant, RubyMethod};

use super::*;

fn file() -> SourceFileId {
    SourceFileId(1)
}

fn constant_subject(name: &str) -> TypeSubject {
    TypeSubject::Constant(FullyQualifiedName::constant(vec![
        RubyConstant::new(name).unwrap()
    ]))
}

fn method_return_subject(owner: &str, name: &str) -> TypeSubject {
    TypeSubject::MethodReturn(FullyQualifiedName::method(
        vec![RubyConstant::new(owner).unwrap()],
        RubyMethod::new(name).unwrap(),
    ))
}

#[test]
fn resolves_latest_fact_before_position() {
    let subject = constant_subject("VALUE");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Literal,
    ));
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::string(),
        TextRange::new(file(), 20, 32),
        TypeProvenance::Literal,
    ));

    assert!(matches!(
        store.type_at(&subject, file(), 12),
        TypeResolution::Resolved(TypeFact {
            ruby_type: RubyType::Class(_),
            ..
        })
    ));

    match store.type_at(&subject, file(), 40) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected resolved latest fact, got {other:?}"),
    }
}

#[test]
fn unresolved_when_no_fact_exists() {
    let store = TypeStore::default();
    assert_eq!(
        store.type_at(&constant_subject("MISSING"), file(), 0),
        TypeResolution::Unresolved
    );
}

#[test]
fn latest_non_unknown_type_with_range_returns_only_the_winning_fact() {
    let subject = constant_subject("VALUE");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::string(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::Unknown,
        TextRange::new(file(), 30, 38),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        TextRange::new(file(), 20, 28),
        TypeProvenance::Assignment,
    ));

    let (ruby_type, range) = store
        .latest_non_unknown_type_with_range(&subject)
        .expect("the latest known type must be returned");
    assert_eq!(*ruby_type, RubyType::integer());
    assert_eq!(range, TextRange::new(file(), 20, 28));
    assert!(store
        .latest_non_unknown_type_with_range(&constant_subject("MISSING"))
        .is_none());
}

#[test]
fn method_return_type_view_retains_unknown_in_arena_order() {
    let first = method_return_subject("First", "call");
    let unknown = method_return_subject("Unknown", "call");
    let second = method_return_subject("Second", "call");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        first.clone(),
        RubyType::string(),
        TextRange::new(SourceFileId(1), 0, 8),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        constant_subject("IGNORED"),
        RubyType::integer(),
        TextRange::new(SourceFileId(1), 10, 18),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        unknown.clone(),
        RubyType::Unknown,
        TextRange::new(SourceFileId(1), 20, 28),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        second.clone(),
        RubyType::integer(),
        TextRange::new(SourceFileId(2), 0, 8),
        TypeProvenance::Inferred,
    ));

    let all_returns = store.method_return_types().collect::<Vec<_>>();
    let TypeSubject::MethodReturn(first_fqn) = first else {
        panic!("test method subject must be a method return")
    };
    let TypeSubject::MethodReturn(second_fqn) = second else {
        panic!("test method subject must be a method return")
    };
    let TypeSubject::MethodReturn(unknown_fqn) = unknown else {
        panic!("test unknown method subject must be a method return")
    };
    assert_eq!(
        all_returns,
        vec![
            (&first_fqn, &RubyType::string()),
            (&unknown_fqn, &RubyType::Unknown),
            (&second_fqn, &RubyType::integer()),
        ],
        "the local collector view must retain an Unknown proof kill in arena order"
    );
}

#[test]
fn constant_type_facts_match_all_facts_constant_filter_in_arena_order() {
    let first = constant_subject("FIRST");
    let ignored_method = method_return_subject("Owner", "call");
    let unknown = constant_subject("UNKNOWN");
    let expression_range = TextRange::new(file(), 30, 36);
    let second = constant_subject("SECOND");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        first.clone(),
        RubyType::string(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        ignored_method,
        RubyType::integer(),
        TextRange::new(file(), 10, 18),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        TypeSubject::Expression(expression_range),
        RubyType::boolean(),
        expression_range,
        TypeProvenance::Literal,
    ));
    store.add(TypeFact::new(
        unknown.clone(),
        RubyType::Unknown,
        TextRange::new(file(), 40, 48),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        second.clone(),
        RubyType::integer(),
        TextRange::new(SourceFileId(2), 0, 8),
        TypeProvenance::Assignment,
    ));

    let from_view = store
        .constant_type_facts()
        .map(|(fqn, range, ruby_type)| (fqn.clone(), range, ruby_type.clone()))
        .collect::<Vec<_>>();
    let from_all = store
        .all_facts()
        .into_iter()
        .filter_map(|fact| {
            let TypeSubject::Constant(constant) = fact.subject else {
                return None;
            };
            Some((constant, fact.range, fact.ruby_type))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        from_view, from_all,
        "the constant view must match all_facts filtered to Constant subjects in arena order"
    );
    let TypeSubject::Constant(first_fqn) = first else {
        panic!("test constant subject must be a constant");
    };
    let TypeSubject::Constant(unknown_fqn) = unknown else {
        panic!("test unknown constant subject must be a constant");
    };
    let TypeSubject::Constant(second_fqn) = second else {
        panic!("test second constant subject must be a constant");
    };
    assert_eq!(
        from_view,
        vec![
            (first_fqn, TextRange::new(file(), 0, 8), RubyType::string()),
            (
                unknown_fqn,
                TextRange::new(file(), 40, 48),
                RubyType::Unknown
            ),
            (
                second_fqn,
                TextRange::new(SourceFileId(2), 0, 8),
                RubyType::integer()
            ),
        ]
    );
}

#[test]
fn identical_ruby_types_share_one_internal_value() {
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        constant_subject("FIRST"),
        RubyType::string(),
        TextRange::new(file(), 0, 5),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        constant_subject("SECOND"),
        RubyType::string(),
        TextRange::new(file(), 10, 16),
        TypeProvenance::Assignment,
    ));

    assert_eq!(store.fact_count(), 2);
    assert_eq!(store.ruby_types.len(), 1);
}

#[test]
fn stored_type_fact_retains_the_compact_arena_layout() {
    assert_eq!(
        size_of::<StoredTypeFact>(),
        24,
        "adding retained fields to every type fact requires real-project memory evidence"
    );
}

#[test]
fn expression_facts_use_file_local_range_identity_without_subject_buckets() {
    let old_range = TextRange::new(file(), 0, 5);
    let new_range = TextRange::new(file(), 10, 15);
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        TypeSubject::Expression(old_range),
        RubyType::string(),
        old_range,
        TypeProvenance::Literal,
    ));

    assert_eq!(store.subjects.len(), 0);
    assert_eq!(store.facts_by_subject.len(), 0);
    assert_eq!(
        store.facts_for(&TypeSubject::Expression(old_range))[0].ruby_type,
        RubyType::string()
    );
    assert!(matches!(
        store.type_at(&TypeSubject::Expression(old_range), file(), 4),
        TypeResolution::Resolved(TypeFact {
            ruby_type: RubyType::Class(_),
            ..
        })
    ));

    store.replace_file(
        file(),
        [TypeFact::new(
            TypeSubject::Expression(new_range),
            RubyType::integer(),
            new_range,
            TypeProvenance::Literal,
        )],
    );

    assert_eq!(store.subjects.len(), 0);
    assert_eq!(store.facts_by_subject.len(), 0);
    assert!(store
        .facts_for(&TypeSubject::Expression(old_range))
        .is_empty());
    assert_eq!(
        store.facts_for(&TypeSubject::Expression(new_range))[0].ruby_type,
        RubyType::integer()
    );
}

#[test]
fn updates_only_matching_inferred_method_returns_in_place() {
    let inferred = method_return_subject("Target", "call");
    let contracted = method_return_subject("Contracted", "call");
    let other_file = method_return_subject("Other", "call");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        inferred.clone(),
        RubyType::Unknown,
        TextRange::new(file(), 0, 8),
        TypeProvenance::Inferred,
    ));
    store.add(TypeFact::new(
        contracted.clone(),
        RubyType::string(),
        TextRange::new(file(), 10, 18),
        TypeProvenance::Rbs,
    ));
    store.add(TypeFact::new(
        other_file.clone(),
        RubyType::Unknown,
        TextRange::new(SourceFileId(2), 0, 8),
        TypeProvenance::Inferred,
    ));

    let TypeSubject::MethodReturn(inferred_fqn) = &inferred else {
        panic!("test subject must be a method return")
    };
    let TypeSubject::MethodReturn(contracted_fqn) = &contracted else {
        panic!("test subject must be a method return")
    };
    let TypeSubject::MethodReturn(other_fqn) = &other_file else {
        panic!("test subject must be a method return")
    };
    let updated = store.update_inferred_method_return_types_in_file(
        file(),
        [
            (inferred_fqn, RubyType::integer()),
            (contracted_fqn, RubyType::boolean()),
            (other_fqn, RubyType::boolean()),
        ],
    );

    assert_eq!(updated, 1);
    assert_eq!(store.facts_for(&inferred)[0].ruby_type, RubyType::integer());
    assert_eq!(
        store.facts_for(&contracted)[0].ruby_type,
        RubyType::string()
    );
    assert_eq!(store.facts_for(&other_file)[0].ruby_type, RubyType::Unknown);
}

#[test]
fn replace_file_removes_stale_facts_for_same_file_only() {
    let subject = constant_subject("VALUE");
    let other_subject = constant_subject("OTHER");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        other_subject.clone(),
        RubyType::string(),
        TextRange::new(SourceFileId(2), 0, 8),
        TypeProvenance::Assignment,
    ));

    store.replace_file(
        file(),
        [TypeFact::new(
            subject.clone(),
            RubyType::string(),
            TextRange::new(file(), 10, 18),
            TypeProvenance::Assignment,
        )],
    );

    assert_eq!(
        store.type_at(&subject, file(), 4),
        TypeResolution::Unresolved
    );
    match store.type_at(&subject, file(), 14) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected replacement fact, got {other:?}"),
    }
    match store.type_at(&other_subject, SourceFileId(2), 4) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected other file fact to survive, got {other:?}"),
    }
}

#[test]
fn replace_file_restores_order_after_append_only_additions() {
    let subject = constant_subject("VALUE");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::string(),
        TextRange::new(SourceFileId(3), 0, 8),
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        TextRange::new(SourceFileId(1), 0, 8),
        TypeProvenance::Assignment,
    ));

    store.replace_file(
        SourceFileId(2),
        [TypeFact::new(
            subject.clone(),
            RubyType::boolean(),
            TextRange::new(SourceFileId(2), 0, 8),
            TypeProvenance::Assignment,
        )],
    );

    assert_eq!(
        store
            .facts_for(&subject)
            .into_iter()
            .map(|fact| fact.range.file_id)
            .collect::<Vec<_>>(),
        vec![SourceFileId(1), SourceFileId(2), SourceFileId(3)]
    );
}

#[test]
fn ambiguous_when_same_position_has_multiple_types() {
    let subject = constant_subject("VALUE");
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Literal,
    ));
    store.add(TypeFact::new(
        subject.clone(),
        RubyType::string(),
        TextRange::new(file(), 0, 8),
        TypeProvenance::Extension,
    ));

    match store.type_at(&subject, file(), 4) {
        TypeResolution::Ambiguous(facts) => assert_eq!(facts.len(), 2),
        other => panic!("expected ambiguous facts, got {other:?}"),
    }
}

#[test]
fn named_file_query_selects_without_materializing_unrelated_expression_facts() {
    let wanted = constant_subject("WANTED");
    let excluded = TextRange::new(file(), 20, 26);
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        wanted.clone(),
        RubyType::integer(),
        TextRange::new(file(), 0, 6),
        TypeProvenance::Assignment,
    ));
    for offset in 1..20 {
        let range = TextRange::new(file(), offset, offset + 1);
        store.add(TypeFact::new(
            TypeSubject::Expression(range),
            RubyType::string(),
            range,
            TypeProvenance::Literal,
        ));
    }
    store.add(TypeFact::new(
        wanted.clone(),
        RubyType::string(),
        excluded,
        TypeProvenance::Assignment,
    ));

    let resolution = store.named_type_in_file_before_matching(file(), 30, |subject, range| {
        subject == &wanted && range != excluded
    });
    match resolution {
        NamedTypeResolution::Resolved(ruby_type) => {
            assert_eq!(*ruby_type, RubyType::integer())
        }
        other => panic!("expected the latest non-excluded named fact, got {other:?}"),
    }
}

#[test]
fn named_file_query_preserves_same_position_type_ambiguity() {
    let wanted = constant_subject("WANTED");
    let range = TextRange::new(file(), 10, 16);
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        wanted.clone(),
        RubyType::integer(),
        range,
        TypeProvenance::Assignment,
    ));
    store.add(TypeFact::new(
        wanted.clone(),
        RubyType::string(),
        range,
        TypeProvenance::Flow,
    ));

    match store.named_type_in_file_before_matching(file(), 20, |subject, _| subject == &wanted) {
        NamedTypeResolution::Ambiguous => {}
        other => panic!("expected conflicting latest named facts, got {other:?}"),
    }
}

#[test]
fn expression_type_at_selects_exact_range_among_many_file_facts() {
    let mut facts = Vec::new();
    for index in 0..64u32 {
        let range = TextRange::new(file(), index * 10, index * 10 + 5);
        facts.push(TypeFact::new(
            TypeSubject::Expression(range),
            if index == 32 {
                RubyType::string()
            } else {
                RubyType::integer()
            },
            range,
            TypeProvenance::Literal,
        ));
    }
    let overlapping_local = TextRange::new(file(), 320, 325);
    facts.push(TypeFact::new(
        TypeSubject::Local {
            scope_id: 0,
            name: "value".into(),
        },
        RubyType::boolean(),
        overlapping_local,
        TypeProvenance::Assignment,
    ));

    let mut store = TypeStore::default();
    store.replace_file(file(), facts);

    let target = TextRange::new(file(), 320, 325);
    match store.type_at(&TypeSubject::Expression(target), file(), 320) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected the exact expression fact, got {other:?}"),
    }
    assert_eq!(
        store.facts_for(&TypeSubject::Expression(target))[0].ruby_type,
        RubyType::string()
    );
    assert!(matches!(
        store.type_at(
            &TypeSubject::Expression(TextRange::new(file(), 320, 330)),
            file(),
            320
        ),
        TypeResolution::Unresolved
    ));
}

#[test]
fn expression_type_at_distinguishes_shared_start_bytes() {
    let short_range = TextRange::new(file(), 0, 5);
    let long_range = TextRange::new(file(), 0, 10);
    let mut store = TypeStore::default();
    store.replace_file(
        file(),
        [
            TypeFact::new(
                TypeSubject::Expression(short_range),
                RubyType::string(),
                short_range,
                TypeProvenance::Literal,
            ),
            TypeFact::new(
                TypeSubject::Expression(long_range),
                RubyType::integer(),
                long_range,
                TypeProvenance::Literal,
            ),
        ],
    );

    match store.type_at(&TypeSubject::Expression(short_range), file(), 0) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected the short expression, got {other:?}"),
    }
    match store.type_at(&TypeSubject::Expression(long_range), file(), 0) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
        other => panic!("expected the long expression, got {other:?}"),
    }
}

#[test]
fn append_only_expression_lookup_survives_unsorted_file_index() {
    let later_range = TextRange::new(file(), 40, 45);
    let earlier_range = TextRange::new(file(), 0, 5);
    let mut store = TypeStore::default();
    store.add(TypeFact::new(
        TypeSubject::Expression(later_range),
        RubyType::integer(),
        later_range,
        TypeProvenance::Literal,
    ));
    store.add(TypeFact::new(
        TypeSubject::Expression(earlier_range),
        RubyType::string(),
        earlier_range,
        TypeProvenance::Literal,
    ));

    match store.type_at(&TypeSubject::Expression(earlier_range), file(), 0) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected unsorted add() lookup to stay linear, got {other:?}"),
    }
    assert_eq!(
        store.facts_for(&TypeSubject::Expression(later_range))[0].ruby_type,
        RubyType::integer()
    );
}

#[test]
#[should_panic(expected = "invariant violated: TextRange start_byte must be <= end_byte")]
fn invalid_range_panics() {
    let _ = TextRange::new(file(), 10, 9);
}

#[test]
fn agreed_type_outside_a_file_ignores_that_file_and_rejects_disagreement() {
    let subject = constant_subject("LIMIT");
    let fact = |ruby_type: RubyType, file_id: u32| {
        TypeFact::new(
            subject.clone(),
            ruby_type,
            TextRange::new(SourceFileId(file_id), 0, 5),
            TypeProvenance::Literal,
        )
    };
    let mut store = TypeStore::default();
    store.add(fact(RubyType::integer(), 1));
    store.add(fact(RubyType::integer(), 2));
    store.add(fact(RubyType::string(), 3));

    assert_eq!(
        store.agreed_type_outside_file(&subject, SourceFileId(3)),
        Some(&RubyType::integer()),
        "the excluded file's own fact must not count"
    );
    assert_eq!(
        store.agreed_type_outside_file(&subject, SourceFileId(1)),
        None
    );
    assert_eq!(
        store.agreed_type_outside_file(&constant_subject("MISSING"), SourceFileId(1)),
        None
    );
}
