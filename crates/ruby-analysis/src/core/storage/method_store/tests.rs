use crate::core::{FqnId, FullyQualifiedName, RubyMethod, SourceFileId, TextRange};

use super::*;

fn stored_method(
    fqn: FqnId,
    owner: FqnId,
    method: Option<RubyMethod>,
    range: TextRange,
) -> StoredMethodFact {
    StoredMethodFact {
        fqn,
        owner,
        method,
        range,
        name_range: range,
        params: Vec::new(),
        param_facts: Vec::new(),
        parameter_shape_complete: false,
        delegate_receiver: None,
        visibility: MethodVisibility::Public,
        availability: MethodAvailability::Available,
        documentation: None,
        return_type_label: None,
        higher_order: None,
    }
}

fn file() -> SourceFileId {
    SourceFileId(1)
}

#[test]
fn ordinary_method_has_no_higher_order_payload_and_empty_replacement_clears_it() {
    let method = RubyMethod::new("transform").unwrap();
    let ordinary = MethodFact::new(
        FullyQualifiedName::method(Vec::new(), method),
        FullyQualifiedName::namespace(Vec::new()),
        TextRange::new(file(), 0, 8),
    );
    assert!(ordinary.higher_order.is_none());

    let signature = CallableSignature {
        receiver_type_parameters: Vec::new(),
        type_parameters: Vec::new(),
        parameters: Vec::new(),
        block: crate::core::CallableBlockTemplate {
            parameters: Vec::new(),
            return_type: CallableTypeTemplate::Unconstrained,
            required: true,
        },
        return_type: CallableTypeTemplate::Unconstrained,
    };
    let with_signature = ordinary.with_callable_signatures(vec![signature]);
    assert_eq!(with_signature.callable_signatures().len(), 1);
    assert!(with_signature.higher_order.is_some());

    let cleared = with_signature.with_callable_signatures(Vec::new());
    assert!(cleared.callable_signatures().is_empty());
    assert!(cleared.higher_order.is_none());
}

#[test]
fn replace_file_removes_stale_method_facts_for_same_file_only() {
    let fqn = FqnId(1);
    let other_fqn = FqnId(2);
    let owner = FqnId(3);
    let name = RubyMethod::new("name").unwrap();
    let email = RubyMethod::new("email").unwrap();
    let mut store = MethodStore::default();
    store.replace_file(
        file(),
        [stored_method(
            fqn,
            owner,
            Some(name),
            TextRange::new(file(), 0, 8),
        )],
    );
    store.replace_file(
        SourceFileId(2),
        [stored_method(
            other_fqn,
            owner,
            Some(email),
            TextRange::new(SourceFileId(2), 0, 8),
        )],
    );

    store.replace_file(
        file(),
        [stored_method(
            fqn,
            owner,
            Some(name),
            TextRange::new(file(), 10, 18),
        )],
    );

    assert_eq!(store.facts_for(fqn).len(), 1);
    assert_eq!(store.facts_for(fqn)[0].range.start_byte, 10);
    assert_eq!(store.facts_for(other_fqn).len(), 1);
}

#[test]
fn exact_owner_name_match_borrows_one_effective_fact_and_deduplicates() {
    let fqn = FqnId(1);
    let owner = FqnId(2);
    let method = RubyMethod::new("call").unwrap();
    let fact = stored_method(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(3), 4, 12),
    );
    let mut store = MethodStore::default();
    store.replace_file(fact.range.file_id, [fact.clone(), fact]);

    let match_result = store.effective_fact_matching_owner_name(owner, &method);
    let StoredMethodFactMatch::Unique(selected) = match_result else {
        panic!("identical stored method facts must collapse to one borrowed match")
    };
    let first_id = store.facts_by_owner_name[&(owner, method)][0];
    assert!(std::ptr::eq(
        selected,
        store
            .fact(first_id)
            .expect("the indexed method fact must remain in the arena")
    ));
}

#[test]
fn exact_owner_name_match_preserves_availability_and_ambiguity() {
    let fqn = FqnId(1);
    let owner = FqnId(2);
    let method = RubyMethod::new("call").unwrap();
    let mut available = stored_method(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(1), 0, 8),
    );
    let mut unavailable = stored_method(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(2), 0, 8),
    );
    unavailable.availability = MethodAvailability::Unavailable {
        reason: "JRuby runtime API".to_string(),
    };

    let mut store = MethodStore::default();
    store.replace_file(available.range.file_id, [available.clone()]);
    store.replace_file(unavailable.range.file_id, [unavailable]);
    assert!(matches!(
        store.effective_fact_matching_owner_name(owner, &method),
        StoredMethodFactMatch::Unique(fact)
            if matches!(fact.availability, MethodAvailability::Unavailable { .. })
    ));

    available.range = TextRange::new(SourceFileId(3), 0, 8);
    available.availability = MethodAvailability::Absent {
        reason: "not defined by this runtime".to_string(),
    };
    store.replace_file(available.range.file_id, [available]);
    assert!(matches!(
        store.effective_fact_matching_owner_name(owner, &method),
        StoredMethodFactMatch::Missing
    ));

    let mut ambiguous = MethodStore::default();
    ambiguous.replace_file(
        SourceFileId(1),
        [stored_method(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(1), 0, 8),
        )],
    );
    ambiguous.replace_file(
        SourceFileId(2),
        [stored_method(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(2), 0, 8),
        )],
    );
    assert!(matches!(
        ambiguous.effective_fact_matching_owner_name(owner, &method),
        StoredMethodFactMatch::Ambiguous
    ));
}
