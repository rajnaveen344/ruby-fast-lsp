use crate::core::names::fqn_id::{ConstLookupId, FqnId};
use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange,
};

use super::*;

fn file() -> SourceFileId {
    SourceFileId(1)
}

#[test]
fn replace_file_removes_stale_reference_facts_for_same_file_only() {
    let target = FqnId(1);
    let mut store = ReferenceStore::default();
    store.add(
        target,
        ReferenceFact::new(TextRange::new(file(), 0, 4), None),
    );
    store.add(
        target,
        ReferenceFact::new(TextRange::new(SourceFileId(2), 0, 4), None),
    );

    store.replace_file(
        file(),
        [(
            target,
            ReferenceFact::new(TextRange::new(file(), 10, 14), None),
        )],
    );

    let facts = store.facts_for(target);
    assert_eq!(facts.len(), 2);
    assert!(facts
        .iter()
        .any(|fact| fact.range.file_id == file() && fact.range.start_byte == 10));
    assert!(facts
        .iter()
        .any(|fact| fact.range.file_id == SourceFileId(2)));
}

#[test]
fn replace_file_keeps_every_target_sorted_and_leaves_other_files_intact() {
    let shared = FqnId(1);
    let untouched = FqnId(2);
    let dropped = FqnId(3);
    let fact = |file: u32, start: u32| {
        ReferenceFact::new(TextRange::new(SourceFileId(file), start, start + 1), None)
    };
    let mut store = ReferenceStore::default();
    store.replace_file(
        SourceFileId(1),
        [(shared, fact(1, 5)), (dropped, fact(1, 9))],
    );
    store.replace_file(
        SourceFileId(3),
        [(shared, fact(3, 2)), (untouched, fact(3, 4))],
    );
    store.replace_file(
        SourceFileId(2),
        [
            (untouched, fact(2, 7)),
            (shared, fact(2, 8)),
            (shared, fact(2, 1)),
        ],
    );

    store.replace_file(
        SourceFileId(1),
        [
            (untouched, fact(1, 3)),
            (shared, fact(1, 6)),
            (shared, fact(1, 0)),
        ],
    );

    let starts = |target| {
        store
            .facts_for(target)
            .iter()
            .map(|fact| (fact.range.file_id.0, fact.range.start_byte))
            .collect::<Vec<_>>()
    };
    assert_eq!(starts(shared), [(1, 0), (1, 6), (2, 1), (2, 8), (3, 2)]);
    assert_eq!(starts(untouched), [(1, 3), (2, 7), (3, 4)]);
    assert!(
        starts(dropped).is_empty(),
        "a target the file no longer names loses its facts"
    );
    assert_eq!(
        store.targets_for_exact_range(TextRange::new(SourceFileId(1), 0, 1)),
        [shared]
    );
    assert_eq!(store.facts_in_file(SourceFileId(3)).len(), 2);
}

fn method_row(
    access: MethodReferenceAccess,
    owner_kind: NamespaceKind,
    is_super: bool,
    preferred_definition_range: Option<TextRange>,
    diagnostics: Option<MethodReferenceDiagnostics>,
    names: &mut Vec<FullyQualifiedName>,
) -> StoredMethodReferenceCandidate {
    StoredMethodReferenceCandidate::new(
        TextRange::new(file(), 10, 14),
        ConstLookupId(7),
        owner_kind,
        RubyMethod::new("save").expect("valid Ruby method"),
        is_super,
        access,
        Some(FqnId(3)),
        Some(TextRange::new(file(), 4, 20)),
        preferred_definition_range,
        diagnostics,
        |fqn| {
            names.push(fqn);
            FqnId(u32::try_from(names.len() - 1).expect("test interns few names"))
        },
    )
}

fn user() -> FullyQualifiedName {
    FullyQualifiedName::namespace(vec![RubyConstant::new("User").expect("valid constant")])
}

fn ordinary_diagnostics(range: TextRange) -> MethodReferenceDiagnostics {
    MethodReferenceDiagnostics {
        diagnostic_range: range,
        receiver_label: Some(MethodReceiverLabel::ReceiverType),
        receiver_expression_range: Some(TextRange::new(file(), 4, 9)),
        receiver_type: Some(Box::new(RubyType::Class(user()))),
        diagnose_unresolved: true,
        allow_unindexed_owner: false,
        safe_navigation: false,
        signature: Some(MethodCallSignatureCandidate {
            positional_count: 2,
            ..MethodCallSignatureCandidate::default()
        }),
    }
}

#[test]
fn ordinary_method_row_is_compact_and_owns_no_heap() {
    assert!(std::mem::size_of::<StoredMethodReferenceCandidate>() <= 72);
    let mut names = Vec::new();
    let row = method_row(
        MethodReferenceAccess::ExplicitReceiver,
        NamespaceKind::Instance,
        false,
        None,
        Some(ordinary_diagnostics(TextRange::new(file(), 10, 14))),
        &mut names,
    );
    assert_eq!(row.heap_bytes(), 0);

    let mut names = Vec::new();
    let written = method_row(
        MethodReferenceAccess::Normal,
        NamespaceKind::Instance,
        false,
        None,
        Some(MethodReferenceDiagnostics {
            receiver_label: Some(MethodReceiverLabel::Written("User".to_string())),
            ..ordinary_diagnostics(TextRange::new(file(), 10, 14))
        }),
        &mut names,
    );
    assert_eq!(written.heap_bytes(), 0);
}

#[test]
fn ordinary_method_row_round_trips_every_field() {
    let mut names = Vec::new();
    let row = method_row(
        MethodReferenceAccess::ExplicitReceiver,
        NamespaceKind::Instance,
        false,
        None,
        Some(ordinary_diagnostics(TextRange::new(file(), 10, 14))),
        &mut names,
    );
    assert_eq!(row.owner(), ConstLookupId(7));
    assert_eq!(row.owner_kind(), NamespaceKind::Instance);
    assert_eq!(
        row.method(),
        RubyMethod::new("save").expect("valid Ruby method")
    );
    assert!(!row.is_super());
    assert_eq!(row.access(), MethodReferenceAccess::ExplicitReceiver);
    assert_eq!(row.caller(), Some(FqnId(3)));
    assert_eq!(
        row.call_expression_range(),
        Some(TextRange::new(file(), 4, 20))
    );
    assert_eq!(row.preferred_definition_range(), None);
    let diagnostics = row.diagnostics().expect("diagnostics were stored");
    assert_eq!(
        diagnostics.diagnostic_range(),
        TextRange::new(file(), 10, 14)
    );
    assert_eq!(
        diagnostics.receiver_expression_range(),
        Some(TextRange::new(file(), 4, 9))
    );
    let receiver = diagnostics.receiver_type().expect("receiver was stored");
    assert_eq!(
        receiver.expand(|id| names[id.0 as usize].clone()),
        RubyType::Class(user())
    );
    assert_eq!(
        diagnostics.receiver_label(),
        Some(StoredReceiverLabel::ReceiverType)
    );
    assert!(diagnostics.diagnose_unresolved());
    assert!(!diagnostics.allow_unindexed_owner());
    assert!(!diagnostics.safe_navigation());
    assert_eq!(
        diagnostics.signature(),
        Some(MethodCallSignature {
            positional_count: 2,
            has_positional_splat: false,
            has_nonempty_keyword_hash: false,
            trailing_positional_may_be_options_hash: false,
            keyword_args: &[],
            has_keyword_splat: false,
        })
    );
}

#[test]
fn rare_method_row_payloads_round_trip_through_the_cold_box() {
    let keyword = KeywordArgCandidate {
        name: ustr::Ustr::from("force"),
        range: TextRange::new(file(), 15, 20),
    };
    let structural = RubyType::union([RubyType::Class(user()), RubyType::nil_class()]);
    let mut names = Vec::new();
    let row = method_row(
        MethodReferenceAccess::VisibilityBypass,
        NamespaceKind::Singleton,
        true,
        Some(TextRange::new(SourceFileId(2), 1, 5)),
        Some(MethodReferenceDiagnostics {
            diagnostic_range: TextRange::new(file(), 8, 14),
            receiver_label: Some(MethodReceiverLabel::Rendered("User | nil".to_string())),
            receiver_expression_range: None,
            receiver_type: Some(Box::new(structural.clone())),
            diagnose_unresolved: false,
            allow_unindexed_owner: true,
            safe_navigation: true,
            signature: Some(MethodCallSignatureCandidate {
                positional_count: 1,
                has_positional_splat: true,
                has_nonempty_keyword_hash: true,
                trailing_positional_may_be_options_hash: true,
                keyword_args: vec![keyword.clone()],
                has_keyword_splat: true,
            }),
        }),
        &mut names,
    );
    assert!(row.heap_bytes() > 0);
    assert!(names.is_empty(), "structural receivers are not interned");
    assert_eq!(row.owner_kind(), NamespaceKind::Singleton);
    assert!(row.is_super());
    assert_eq!(row.access(), MethodReferenceAccess::VisibilityBypass);
    assert_eq!(
        row.preferred_definition_range(),
        Some(TextRange::new(SourceFileId(2), 1, 5))
    );
    let diagnostics = row.diagnostics().expect("diagnostics were stored");
    assert_eq!(
        diagnostics.diagnostic_range(),
        TextRange::new(file(), 8, 14)
    );
    assert_eq!(diagnostics.receiver_expression_range(), None);
    assert_eq!(
        diagnostics.receiver_type(),
        Some(StoredReceiverType::Structural(&structural))
    );
    assert_eq!(
        diagnostics.receiver_label(),
        Some(StoredReceiverLabel::Text("User | nil"))
    );
    assert!(!diagnostics.diagnose_unresolved());
    assert!(diagnostics.allow_unindexed_owner());
    assert!(diagnostics.safe_navigation());
    assert_eq!(
        diagnostics.signature(),
        Some(MethodCallSignature {
            positional_count: 1,
            has_positional_splat: true,
            has_nonempty_keyword_hash: true,
            trailing_positional_may_be_options_hash: true,
            keyword_args: std::slice::from_ref(&keyword),
            has_keyword_splat: true,
        })
    );
}

#[test]
fn method_row_without_diagnostics_keeps_every_access_kind() {
    for access in [
        MethodReferenceAccess::Normal,
        MethodReferenceAccess::ExplicitReceiver,
        MethodReferenceAccess::VisibilityBypass,
        MethodReferenceAccess::InstanceMethodReflection,
    ] {
        let mut names = Vec::new();
        let row = method_row(
            access,
            NamespaceKind::Instance,
            false,
            None,
            None,
            &mut names,
        );
        assert_eq!(row.access(), access);
        assert!(row.diagnostics().is_none());
        assert_eq!(row.heap_bytes(), 0);
    }
    let mut names = Vec::new();
    let written = method_row(
        MethodReferenceAccess::Normal,
        NamespaceKind::Instance,
        false,
        None,
        Some(MethodReferenceDiagnostics {
            receiver_label: Some(MethodReceiverLabel::Written("super".to_string())),
            receiver_type: None,
            signature: None,
            ..ordinary_diagnostics(TextRange::new(file(), 10, 14))
        }),
        &mut names,
    );
    let diagnostics = written.diagnostics().expect("diagnostics were stored");
    assert_eq!(
        diagnostics.receiver_label(),
        Some(StoredReceiverLabel::Text("super"))
    );
    assert_eq!(diagnostics.receiver_type(), None);
    assert_eq!(diagnostics.signature(), None);
}
