use crate::core::{
    LiteralKey, RubyType, ShapeExactness, ShapeField, ShapeStability, ShapeType, UnknownReason,
};
use crate::inference::r#type::literal::literal_key;
use crate::inference::type_tracker::flow::shapes::identities::ShapeAlternativeTransition;
use ruby_prism::*;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::inference::type_tracker) fn type_contains_shape(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Union(members) => members.iter().any(type_contains_shape),
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

pub(in crate::inference::type_tracker) fn type_is_shape_only(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Union(members) => !members.is_empty() && members.iter().all(type_is_shape_only),
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

pub(in crate::inference::type_tracker) fn shape_alternatives(
    ruby_type: &RubyType,
) -> Result<Vec<RubyType>, UnknownReason> {
    match ruby_type {
        RubyType::Shape(_) => Ok(vec![ruby_type.clone()]),
        RubyType::Union(members) => {
            let mut shapes = Vec::new();
            for member in members {
                shapes.extend(shape_alternatives(member)?);
            }
            Ok(shapes)
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => Err(UnknownReason::MutableShapeInvalidated),
    }
}

pub(in crate::inference::type_tracker) fn non_shape_alternatives(
    ruby_type: &RubyType,
) -> Vec<RubyType> {
    match ruby_type {
        RubyType::Shape(_) => Vec::new(),
        RubyType::Union(members) => members.iter().flat_map(non_shape_alternatives).collect(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => vec![ruby_type.clone()],
    }
}

pub(in crate::inference::type_tracker) fn map_shape_alternatives(
    ruby_type: &RubyType,
    mut transform: impl FnMut(&ShapeType) -> Result<ShapeType, UnknownReason>,
) -> Result<RubyType, UnknownReason> {
    let mut transformed = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: shape_alternatives returned non-shape type `{alternative}`. This is a bug because callers rely on exhaustive shape-only mapping. Fix: keep shape_alternatives filtering explicit."
            );
        };
        transformed.push(RubyType::Shape(Box::new(transform(&shape)?)));
    }
    let joined = RubyType::union(transformed);
    (joined != RubyType::Unknown)
        .then_some(joined)
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn rebuild_shape(
    shape: &ShapeType,
    fields: impl IntoIterator<Item = ShapeField>,
    stability: ShapeStability,
) -> Result<ShapeType, UnknownReason> {
    ShapeType::try_new(fields, shape.rest().cloned(), shape.exactness(), stability)
        .map_err(|_| UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn shape_with_required_field(
    ruby_type: &RubyType,
    key: LiteralKey,
    value: RubyType,
) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        let mut fields = shape
            .fields()
            .iter()
            .map(|field| (field.key().clone(), field.clone()))
            .collect::<BTreeMap<_, _>>();
        fields.insert(
            key.clone(),
            ShapeField::required(key.clone(), value.clone()),
        );
        rebuild_shape(shape, fields.into_values(), shape.stability())
    })
}

pub(in crate::inference::type_tracker) fn shape_with_contained_transitions(
    ruby_type: &RubyType,
    key: &LiteralKey,
    transitions: &[ShapeAlternativeTransition],
) -> Result<RubyType, UnknownReason> {
    let RubyType::Shape(shape) = ruby_type else {
        return Err(UnknownReason::MutableShapeInvalidated);
    };
    let Some(existing) = shape.field(key) else {
        return Err(UnknownReason::MutableShapeInvalidated);
    };
    if !existing.is_required() || !type_is_shape_only(existing.value()) {
        return Err(UnknownReason::MutableShapeInvalidated);
    }
    let mut transitioned_values = Vec::new();
    for before in shape_alternatives(existing.value())? {
        let mut matches = transitions
            .iter()
            .filter(|transition| transition.before == before);
        let transition = matches
            .next()
            .ok_or(UnknownReason::MutableShapeInvalidated)?;
        if matches.next().is_some() {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        transitioned_values.push(transition.after.clone());
    }
    let value = RubyType::union(transitioned_values);
    if value == RubyType::Unknown {
        return Err(UnknownReason::ShapeBoundExceeded);
    }
    let fields = shape.fields().iter().map(|field| {
        if field.key() == key {
            ShapeField::required(key.clone(), value.clone())
        } else {
            field.clone()
        }
    });
    let rebuilt = rebuild_shape(shape, fields, shape.stability())?;
    Ok(RubyType::Shape(Box::new(rebuilt)))
}

pub(in crate::inference::type_tracker) fn shape_without_field(
    ruby_type: &RubyType,
    key: &LiteralKey,
) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        let fields = shape
            .fields()
            .iter()
            .filter(|field| field.key() != key)
            .cloned();
        rebuild_shape(shape, fields, shape.stability())
    })
}

pub(in crate::inference::type_tracker) fn shape_cleared(
    ruby_type: &RubyType,
) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        ShapeType::try_new(
            std::iter::empty(),
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .map_err(|_| UnknownReason::ShapeBoundExceeded)
    })
}

pub(in crate::inference::type_tracker) fn shape_frozen(
    ruby_type: &RubyType,
) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        rebuild_shape(
            shape,
            shape.fields().iter().cloned(),
            ShapeStability::Frozen,
        )
    })
}

pub(in crate::inference::type_tracker) fn merge_shape_types(
    left: &RubyType,
    right: &RubyType,
    preserve_left_stability: bool,
) -> Result<RubyType, UnknownReason> {
    let left_alternatives = shape_alternatives(left)?;
    let right_alternatives = shape_alternatives(right)?;
    let mut merged = Vec::new();
    for left in &left_alternatives {
        let RubyType::Shape(left_shape) = left else {
            panic!(
                "INVARIANT VIOLATED: left shape alternative is not a Shape. This is a bug because merge_shape_types consumes shape_alternatives. Fix: keep the helper return contract exhaustive."
            );
        };
        for right in &right_alternatives {
            let RubyType::Shape(right_shape) = right else {
                panic!(
                    "INVARIANT VIOLATED: right shape alternative is not a Shape. This is a bug because merge_shape_types consumes shape_alternatives. Fix: keep the helper return contract exhaustive."
                );
            };
            if !left_shape.is_exact()
                || left_shape.rest().is_some()
                || !right_shape.is_exact()
                || right_shape.rest().is_some()
            {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
            if preserve_left_stability && left_shape.is_frozen() {
                merged.push(left.clone());
                continue;
            }
            let mut fields = left_shape
                .fields()
                .iter()
                .map(|field| (field.key().clone(), field.clone()))
                .collect::<BTreeMap<_, _>>();
            for field in right_shape.fields() {
                fields.insert(field.key().clone(), field.clone());
            }
            let stability = if preserve_left_stability {
                left_shape.stability()
            } else {
                ShapeStability::TrackedMutable
            };
            let shape =
                ShapeType::try_new(fields.into_values(), None, ShapeExactness::Exact, stability)
                    .map_err(|_| UnknownReason::ShapeBoundExceeded)?;
            merged.push(RubyType::Shape(Box::new(shape)));
        }
    }
    let joined = RubyType::union(merged);
    (joined != RubyType::Unknown)
        .then_some(joined)
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn shape_literal_read_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
) -> RubyType {
    let resolved = RubyType::union_from_proven(
        shape_alternatives(ruby_type).unwrap_or_default(),
        |alternative| {
            let RubyType::Shape(shape) = alternative else {
                return None;
            };
            match shape.field(key) {
                Some(field) if field.is_required() => Some(field.value().clone()),
                Some(field) => Some(RubyType::optional(field.value().clone())),
                None if shape.is_exact() => Some(RubyType::nil_class()),
                None => None,
            }
        },
    );
    resolved.unwrap_or(RubyType::Unknown)
}

pub(in crate::inference::type_tracker) fn shape_field_keys(
    ruby_type: &RubyType,
) -> Result<BTreeSet<LiteralKey>, UnknownReason> {
    let mut keys = BTreeSet::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: shape_alternatives returned non-shape type `{alternative}`. This is a bug because merge mutation key discovery accepts only shapes. Fix: keep shape_alternatives exhaustive."
            );
        };
        keys.extend(shape.fields().iter().map(|field| field.key().clone()));
    }
    Ok(keys)
}

pub(in crate::inference::type_tracker) fn literal_array_index(node: &Node<'_>) -> Option<i32> {
    let integer = node.as_integer_node()?;
    integer.value().try_into().ok()
}

pub(in crate::inference::type_tracker) fn literal_hash_keys(
    hash: &HashNode<'_>,
) -> Option<BTreeSet<LiteralKey>> {
    let mut keys = BTreeSet::new();
    for element in hash.elements().iter() {
        let assoc = element.as_assoc_node()?;
        keys.insert(literal_key(&assoc.key())?);
    }
    Some(keys)
}

pub(in crate::inference::type_tracker) fn precise_contained_shape_field(
    ruby_type: &RubyType,
    key: &LiteralKey,
) -> Option<RubyType> {
    let RubyType::Shape(shape) = ruby_type else {
        // A union would need variant-specific containment identities to avoid
        // destroying correlations when the child mutates. Keep that boundary
        // fail-closed until the flow graph can represent those identities.
        return None;
    };
    let field = shape.field(key)?;
    if !field.is_required() || !type_is_shape_only(field.value()) {
        return None;
    }
    Some(field.value().clone())
}
