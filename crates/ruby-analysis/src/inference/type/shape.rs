//! Proof-first read operations for Hash-backed structural shapes.
//!
//! Mutable identity and invalidation live in TypeTracker. Callers may use
//! these operations only after their reaching receiver type is proven; an
//! invalidated receiver is `RubyType::Unknown` and never reaches this module.

use crate::core::{
    FullyQualifiedName, LiteralKey, RubyType, ShapeFieldPresence, ShapeType, UnknownReason,
};

pub(crate) fn indexed_read(
    receiver: &RubyType,
    key: Option<&LiteralKey>,
) -> Result<RubyType, UnknownReason> {
    resolve_shape_variants(receiver, |shape| match key {
        Some(key) => read_literal_key(shape, key, true),
        None => read_dynamic_key(shape, true),
    })
}

pub(crate) fn fetch(
    receiver: &RubyType,
    key: Option<&LiteralKey>,
    default: Option<&RubyType>,
) -> Result<RubyType, UnknownReason> {
    resolve_shape_variants(receiver, |shape| {
        let (mut alternatives, default_reachable) = match key {
            Some(key) => fetch_literal_key(shape, key)?,
            None => (fetch_dynamic_key(shape)?, true),
        };
        if let Some(default) = default.filter(|_| default_reachable) {
            if *default == RubyType::Unknown {
                return Err(UnknownReason::UnresolvedAssignmentValue);
            }
            alternatives.push(default.clone());
        }
        proven_union(alternatives)
    })
}

pub(crate) fn dig(
    receiver: &RubyType,
    keys: &[Option<LiteralKey>],
) -> Result<RubyType, UnknownReason> {
    if keys.is_empty() {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    dig_at(receiver, keys, 0)
}

pub(crate) fn keys(receiver: &RubyType) -> Result<RubyType, UnknownReason> {
    project_shape_variants_to_array(receiver, |shape| {
        shape
            .fields()
            .iter()
            .map(|field| field.key().generic_type())
            .chain(shape.rest().map(|rest| rest.key().clone()))
            .collect()
    })
}

pub(crate) fn values(receiver: &RubyType) -> Result<RubyType, UnknownReason> {
    project_shape_variants_to_array(receiver, |shape| {
        shape
            .fields()
            .iter()
            .map(|field| field.value().widen_literals())
            .chain(shape.rest().map(|rest| rest.value().widen_literals()))
            .collect()
    })
}

pub(crate) fn key_presence(
    receiver: &RubyType,
    key: Option<&LiteralKey>,
) -> Result<RubyType, UnknownReason> {
    let Some(key) = key else {
        return Ok(RubyType::boolean());
    };
    let mut always_present = true;
    let mut always_absent = true;
    for alternative in union_members(receiver) {
        let RubyType::Shape(shape) = alternative else {
            return Err(UnknownReason::IncompleteUnionMember);
        };
        match shape.field(key) {
            Some(field) if field.presence() == ShapeFieldPresence::Required => {
                always_absent = false;
            }
            Some(_) => {
                always_present = false;
                always_absent = false;
            }
            None if shape.is_exact() => {
                always_present = false;
            }
            None if shape
                .rest()
                .is_some_and(|rest| key.generic_type().is_subtype_of(rest.key())) =>
            {
                always_present = false;
                always_absent = false;
            }
            None => {
                always_present = false;
                if shape.rest().is_none() {
                    always_absent = false;
                }
            }
        }
    }
    match (always_present, always_absent) {
        (true, false) => Ok(RubyType::true_class()),
        (false, true) => Ok(RubyType::false_class()),
        (false, false) => Ok(RubyType::boolean()),
        (true, true) => Err(UnknownReason::UnresolvedMethodReturn),
    }
}

pub(crate) fn each_return(receiver: &RubyType, has_block: bool) -> Result<RubyType, UnknownReason> {
    if !type_is_shape_only(receiver) {
        return Err(UnknownReason::IncompleteUnionMember);
    }
    if has_block {
        return Ok(receiver.clone());
    }
    Ok(RubyType::Class(
        FullyQualifiedName::try_from("Enumerator").expect(
            "INVARIANT VIOLATED: built-in Enumerator is not a valid constant name. This is a bug because the hard-coded Ruby core name must always parse. Fix: keep built-in type names valid Ruby constants.",
        ),
    ))
}

pub(crate) fn generic_hash_projection(receiver: &RubyType) -> Result<RubyType, UnknownReason> {
    resolve_shape_variants(receiver, |shape| Ok(shape.generic_hash_type()))
}

pub(crate) fn argument_free_method_return(
    receiver: &RubyType,
    method_name: &str,
) -> Option<Result<RubyType, UnknownReason>> {
    match method_name {
        "keys" => Some(keys(receiver)),
        "values" => Some(values(receiver)),
        "each" | "each_pair" | "each_key" | "each_value" => Some(each_return(receiver, false)),
        _ => None,
    }
}

pub(crate) fn operation_requires_call_arguments(method_name: &str) -> bool {
    matches!(
        method_name,
        "[]" | "fetch"
            | "dig"
            | "key?"
            | "has_key?"
            | "include?"
            | "member?"
            | "[]="
            | "delete"
            | "merge"
            | "merge!"
            | "update"
    )
}

pub(crate) fn is_shape_only(ruby_type: &RubyType) -> bool {
    type_is_shape_only(ruby_type)
}

fn dig_at(
    receiver: &RubyType,
    keys: &[Option<LiteralKey>],
    index: usize,
) -> Result<RubyType, UnknownReason> {
    let value = indexed_read(receiver, keys[index].as_ref())?;
    if index + 1 == keys.len() {
        return Ok(value);
    }

    let mut results = Vec::new();
    for alternative in union_members(&value) {
        if alternative == RubyType::nil_class() {
            results.push(RubyType::nil_class());
        } else if type_is_shape_only(&alternative) {
            results.push(dig_at(&alternative, keys, index + 1)?);
        } else {
            return Err(UnknownReason::IncompleteUnionMember);
        }
    }
    proven_union(results)
}

fn resolve_shape_variants(
    receiver: &RubyType,
    mut resolve: impl FnMut(&ShapeType) -> Result<RubyType, UnknownReason>,
) -> Result<RubyType, UnknownReason> {
    let mut results = Vec::new();
    for alternative in union_members(receiver) {
        let RubyType::Shape(shape) = alternative else {
            return Err(UnknownReason::IncompleteUnionMember);
        };
        results.push(resolve(&shape)?);
    }
    proven_union(results)
}

fn project_shape_variants_to_array(
    receiver: &RubyType,
    mut project: impl FnMut(&ShapeType) -> Vec<RubyType>,
) -> Result<RubyType, UnknownReason> {
    let mut elements = Vec::new();
    for alternative in union_members(receiver) {
        let RubyType::Shape(shape) = alternative else {
            return Err(UnknownReason::IncompleteUnionMember);
        };
        if !shape.is_exact() && shape.rest().is_none() {
            return Err(UnknownReason::UnresolvedMethodReturn);
        }
        elements.extend(project(&shape));
    }
    if elements.iter().any(RubyType::contains_unknown) {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    Ok(RubyType::Array(RubyType::canonical_union_members(elements)))
}

fn read_literal_key(
    shape: &ShapeType,
    key: &LiteralKey,
    missing_is_nil: bool,
) -> Result<RubyType, UnknownReason> {
    let alternatives = match shape.field(key) {
        Some(field) if field.presence() == ShapeFieldPresence::Required => {
            vec![field.value().clone()]
        }
        Some(field) => vec![field.value().clone(), RubyType::nil_class()],
        None if shape.is_exact() && missing_is_nil => vec![RubyType::nil_class()],
        None if shape.is_exact() => Vec::new(),
        None => match shape.rest() {
            Some(rest) if key.generic_type().is_subtype_of(rest.key()) => {
                let mut values = vec![rest.value().clone()];
                if missing_is_nil {
                    values.push(RubyType::nil_class());
                }
                values
            }
            Some(_) => {
                if missing_is_nil {
                    vec![RubyType::nil_class()]
                } else {
                    Vec::new()
                }
            }
            None => return Err(UnknownReason::UnresolvedMethodReturn),
        },
    };
    if alternatives.is_empty() {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    proven_union(alternatives)
}

fn read_dynamic_key(shape: &ShapeType, missing_is_nil: bool) -> Result<RubyType, UnknownReason> {
    let mut alternatives = shape
        .fields()
        .iter()
        .map(|field| field.value().clone())
        .collect::<Vec<_>>();
    if let Some(rest) = shape.rest() {
        alternatives.push(rest.value().clone());
    } else if !shape.is_exact() {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    if missing_is_nil {
        alternatives.push(RubyType::nil_class());
    }
    proven_union(alternatives)
}

fn fetch_literal_key(
    shape: &ShapeType,
    key: &LiteralKey,
) -> Result<(Vec<RubyType>, bool), UnknownReason> {
    match shape.field(key) {
        Some(field) => Ok((
            vec![field.value().clone()],
            field.presence() == ShapeFieldPresence::Optional,
        )),
        None if shape.is_exact() => Ok((Vec::new(), true)),
        None => match shape.rest() {
            Some(rest) if key.generic_type().is_subtype_of(rest.key()) => {
                Ok((vec![rest.value().clone()], true))
            }
            Some(_) => Ok((Vec::new(), true)),
            None => Err(UnknownReason::UnresolvedMethodReturn),
        },
    }
}

fn fetch_dynamic_key(shape: &ShapeType) -> Result<Vec<RubyType>, UnknownReason> {
    let mut alternatives = shape
        .fields()
        .iter()
        .map(|field| field.value().clone())
        .collect::<Vec<_>>();
    if let Some(rest) = shape.rest() {
        alternatives.push(rest.value().clone());
    } else if !shape.is_exact() {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    Ok(alternatives)
}

fn proven_union(alternatives: Vec<RubyType>) -> Result<RubyType, UnknownReason> {
    if alternatives.is_empty() || alternatives.iter().any(|ty| *ty == RubyType::Unknown) {
        return Err(UnknownReason::UnresolvedMethodReturn);
    }
    let joined = RubyType::union(alternatives);
    (joined != RubyType::Unknown)
        .then_some(joined)
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn union_members(ruby_type: &RubyType) -> Vec<RubyType> {
    match ruby_type {
        RubyType::Union(members) => members.clone(),
        ruby_type => vec![ruby_type.clone()],
    }
}

fn type_is_shape_only(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Union(members) => members.iter().all(type_is_shape_only),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ShapeExactness, ShapeField, ShapeRest, ShapeStability};

    fn exact(fields: impl IntoIterator<Item = ShapeField>) -> RubyType {
        RubyType::Shape(Box::new(
            ShapeType::try_new(
                fields,
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            )
            .expect("test shape must be valid"),
        ))
    }

    fn open(fields: impl IntoIterator<Item = ShapeField>, rest: Option<ShapeRest>) -> RubyType {
        RubyType::Shape(Box::new(
            ShapeType::try_new(
                fields,
                rest,
                ShapeExactness::Open,
                ShapeStability::TrackedMutable,
            )
            .expect("test open shape must be valid"),
        ))
    }

    #[test]
    fn literal_dynamic_and_nested_reads_are_proof_first() {
        let nested = exact([ShapeField::required(
            LiteralKey::symbol("name"),
            RubyType::string(),
        )]);
        let receiver = exact([
            ShapeField::required(LiteralKey::symbol("id"), RubyType::integer()),
            ShapeField::required(LiteralKey::symbol("user"), nested),
        ]);

        assert_eq!(
            indexed_read(&receiver, Some(&LiteralKey::symbol("id"))).unwrap(),
            RubyType::integer()
        );
        assert_eq!(
            indexed_read(&receiver, None).unwrap(),
            RubyType::union([
                RubyType::integer(),
                RubyType::nil_class(),
                exact([ShapeField::required(
                    LiteralKey::symbol("name"),
                    RubyType::string(),
                )]),
            ])
        );
        assert_eq!(
            dig(
                &receiver,
                &[
                    Some(LiteralKey::symbol("user")),
                    Some(LiteralKey::symbol("name")),
                ],
            )
            .unwrap(),
            RubyType::string()
        );
    }

    #[test]
    fn required_optional_absent_and_defaulted_reads_follow_ruby_paths() {
        let receiver = exact([
            ShapeField::required(LiteralKey::symbol("id"), RubyType::integer()),
            ShapeField::optional(LiteralKey::symbol("label"), RubyType::string()),
        ]);

        assert_eq!(
            indexed_read(&receiver, Some(&LiteralKey::symbol("label"))).unwrap(),
            RubyType::union([RubyType::nil_class(), RubyType::string()])
        );
        assert_eq!(
            indexed_read(&receiver, Some(&LiteralKey::symbol("missing"))).unwrap(),
            RubyType::nil_class()
        );
        assert_eq!(
            fetch(&receiver, Some(&LiteralKey::symbol("label")), None).unwrap(),
            RubyType::string(),
            "an optional field either returns its value or raises, so NilClass is not a normal fetch result"
        );
        assert_eq!(
            fetch(
                &receiver,
                Some(&LiteralKey::symbol("id")),
                Some(&RubyType::string()),
            )
            .unwrap(),
            RubyType::integer(),
            "a required field makes the fetch default unreachable"
        );
        assert_eq!(
            fetch(
                &receiver,
                Some(&LiteralKey::symbol("missing")),
                Some(&RubyType::false_class()),
            )
            .unwrap(),
            RubyType::false_class()
        );
        assert_eq!(
            fetch(&receiver, Some(&LiteralKey::symbol("missing")), None),
            Err(UnknownReason::UnresolvedMethodReturn),
            "a definitely missing fetch has no normal return path in the current RubyType domain"
        );
    }

    #[test]
    fn open_rest_and_dynamic_reads_never_claim_one_field() {
        let receiver = open(
            [ShapeField::required(
                LiteralKey::symbol("id"),
                RubyType::integer(),
            )],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::string())),
        );

        assert_eq!(
            indexed_read(&receiver, Some(&LiteralKey::symbol("other"))).unwrap(),
            RubyType::union([RubyType::nil_class(), RubyType::string()])
        );
        assert_eq!(
            indexed_read(&receiver, None).unwrap(),
            RubyType::union([
                RubyType::integer(),
                RubyType::nil_class(),
                RubyType::string(),
            ])
        );
        assert_eq!(
            keys(&receiver).unwrap(),
            RubyType::Array(vec![RubyType::symbol()])
        );
        assert_eq!(
            values(&receiver).unwrap(),
            RubyType::Array(vec![RubyType::integer(), RubyType::string()])
        );
        assert_eq!(
            key_presence(&receiver, Some(&LiteralKey::string("other"))).unwrap(),
            RubyType::false_class(),
            "a Symbol-only rest contract proves an absent String key"
        );
    }

    #[test]
    fn open_shape_without_rest_refuses_incomplete_projections() {
        let receiver = open(
            [ShapeField::required(
                LiteralKey::symbol("id"),
                RubyType::integer(),
            )],
            None,
        );

        assert_eq!(
            indexed_read(&receiver, Some(&LiteralKey::symbol("other"))),
            Err(UnknownReason::UnresolvedMethodReturn)
        );
        assert_eq!(keys(&receiver), Err(UnknownReason::UnresolvedMethodReturn));
        assert_eq!(
            values(&receiver),
            Err(UnknownReason::UnresolvedMethodReturn)
        );
    }

    #[test]
    fn keys_and_values_flatten_complete_shape_variants_into_one_array_type() {
        let receiver = RubyType::union([
            exact([ShapeField::required(
                LiteralKey::symbol("id"),
                RubyType::integer(),
            )]),
            exact([ShapeField::required(
                LiteralKey::string("label"),
                RubyType::string(),
            )]),
            exact([]),
        ]);

        assert_eq!(
            keys(&receiver).unwrap(),
            RubyType::Array(vec![RubyType::string(), RubyType::symbol()]),
            "Hash#keys has one Array result whose element type joins every reachable shape variant"
        );
        assert_eq!(
            values(&receiver).unwrap(),
            RubyType::Array(vec![RubyType::integer(), RubyType::string()]),
            "Hash#values has one Array result whose element type joins every reachable shape variant"
        );
    }

    #[test]
    fn presence_and_each_results_respect_completeness_and_block_shape() {
        let receiver = exact([
            ShapeField::required(LiteralKey::symbol("id"), RubyType::integer()),
            ShapeField::optional(LiteralKey::symbol("label"), RubyType::string()),
        ]);

        assert_eq!(
            key_presence(&receiver, Some(&LiteralKey::symbol("id"))).unwrap(),
            RubyType::true_class()
        );
        assert_eq!(
            key_presence(&receiver, Some(&LiteralKey::symbol("missing"))).unwrap(),
            RubyType::false_class()
        );
        assert_eq!(
            key_presence(&receiver, Some(&LiteralKey::symbol("label"))).unwrap(),
            RubyType::boolean()
        );
        assert_eq!(each_return(&receiver, true).unwrap(), receiver);
        assert_eq!(
            each_return(&receiver, false).unwrap().to_string(),
            "Enumerator"
        );
    }
}
