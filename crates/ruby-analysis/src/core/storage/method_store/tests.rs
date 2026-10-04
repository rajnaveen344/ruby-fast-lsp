use crate::core::names::fqn_id::FqnId;
use crate::core::{FullyQualifiedName, RubyMethod, SourceFileId, TextRange};

use super::*;

fn stored_method(
    fqn: FqnId,
    owner: FqnId,
    method: Option<RubyMethod>,
    range: TextRange,
) -> StoredMethodFact {
    stored_method_with_availability(fqn, owner, method, range, MethodAvailability::Available)
}

fn stored_method_with_availability(
    fqn: FqnId,
    owner: FqnId,
    method: Option<RubyMethod>,
    range: TextRange,
    availability: MethodAvailability,
) -> StoredMethodFact {
    let mut declaration = MethodFact::new(
        FullyQualifiedName::namespace(Vec::new()),
        FullyQualifiedName::namespace(Vec::new()),
        range,
    );
    declaration.availability = availability;
    let mut stored = StoredMethodFact::new(declaration, |_| fqn);
    stored.owner = owner;
    stored.method = method;
    stored
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
        block: crate::core::callables::callable_signature::CallableBlockTemplate {
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

    assert_eq!(store.facts_for(fqn).count(), 1);
    assert_eq!(store.facts_for(fqn).next().unwrap().range.start_byte, 10);
    assert_eq!(store.facts_for(other_fqn).count(), 1);
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
    let first_id = store.facts_by_owner_name.get(&(owner, method))[0];
    assert!(std::ptr::eq(selected, store.facts.get(first_id)));
}

#[test]
fn exact_owner_name_match_preserves_availability_and_ambiguity() {
    let fqn = FqnId(1);
    let owner = FqnId(2);
    let method = RubyMethod::new("call").unwrap();
    let available = stored_method(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(1), 0, 8),
    );
    let unavailable = stored_method_with_availability(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(2), 0, 8),
        MethodAvailability::Unavailable {
            reason: "JRuby runtime API".to_string(),
        },
    );

    let mut store = MethodStore::default();
    store.replace_file(available.range.file_id, [available]);
    store.replace_file(unavailable.range.file_id, [unavailable]);
    assert!(matches!(
        store.effective_fact_matching_owner_name(owner, &method),
        StoredMethodFactMatch::Unique(fact)
            if matches!(fact.availability(), MethodAvailability::Unavailable { .. })
    ));

    let absent = stored_method_with_availability(
        fqn,
        owner,
        Some(method),
        TextRange::new(SourceFileId(3), 0, 8),
        MethodAvailability::Absent {
            reason: "not defined by this runtime".to_string(),
        },
    );
    store.replace_file(absent.range.file_id, [absent]);
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

#[test]
fn stored_method_fact_keeps_rare_details_out_of_line() {
    assert!(
        std::mem::size_of::<StoredMethodFact>() <= 88,
        "stored method rows must stay compact; move rare fields into RareMethodDetails"
    );
    let ordinary = stored_method(FqnId(1), FqnId(2), None, TextRange::new(file(), 0, 8));
    assert!(ordinary.rare.is_none());

    let unavailable = stored_method_with_availability(
        FqnId(1),
        FqnId(2),
        None,
        TextRange::new(file(), 0, 8),
        MethodAvailability::Unavailable {
            reason: "runtime API".to_string(),
        },
    );
    assert!(unavailable.rare.is_some());
    let expanded = unavailable.expand(
        FullyQualifiedName::namespace(Vec::new()),
        FullyQualifiedName::namespace(Vec::new()),
    );
    assert_eq!(&expanded.availability, unavailable.availability());
}
