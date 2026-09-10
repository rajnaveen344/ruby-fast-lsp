use crate::core::{LiteralKey, LiteralValue, RubyType, ShapeField, ShapeType, UnknownReason};
use crate::inference::r#type::literal::literal_key;
use crate::inference::type_tracker::flow::shapes::aliases::shape_local_key_read;
use crate::inference::type_tracker::flow::shapes::identities::{ShapeIdentity, ShapeIdentityState};
use crate::inference::type_tracker::flow::shapes::values::{
    rebuild_shape, shape_alternatives, shape_without_field,
};
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum ShapePredicateMatch {
    Matches,
    DoesNotMatch,
    Inconclusive,
}

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn narrow_shape_key_presence(
        &mut self,
        predicate: &Node<'_>,
        truth: bool,
    ) -> bool {
        let Some(call) = predicate.as_call_node() else {
            return true;
        };
        let method_name = call.name().as_slice();
        if method_name == b"!" && call.arguments().is_none() {
            return call
                .receiver()
                .map(|receiver| self.narrow_shape_key_presence(&receiver, !truth))
                .unwrap_or(true);
        }
        if !matches!(
            method_name,
            b"key?" | b"has_key?" | b"include?" | b"member?"
        ) {
            return true;
        }
        let Some(receiver) = call.receiver() else {
            return true;
        };
        let Some(read) = receiver.as_local_variable_read_node() else {
            return true;
        };
        let Some(arguments) = call.arguments() else {
            return true;
        };
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return true;
        }
        let Some(key) = literal_key(&argument_nodes[0]) else {
            return true;
        };
        let name = String::from_utf8_lossy(read.name().as_slice());
        let identities = self.environment.shape_identities(name.as_ref());
        if identities.is_empty() {
            return true;
        }

        self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_presence_type(ruby_type, &key, truth)
        })
    }

    pub(in crate::inference::type_tracker) fn narrow_shape_predicate(
        &mut self,
        predicate: &Node<'_>,
        truth: bool,
    ) -> bool {
        let Some(call) = predicate.as_call_node() else {
            return true;
        };
        if call.name().as_slice() == b"!" && call.arguments().is_none() {
            return call
                .receiver()
                .map(|receiver| self.narrow_shape_predicate(&receiver, !truth))
                .unwrap_or(true);
        }
        if let Some(reaches) = self.narrow_shape_literal_comparison(&call, truth) {
            return reaches;
        }
        self.narrow_shape_key_presence(predicate, truth)
    }

    pub(in crate::inference::type_tracker) fn narrow_shape_literal_comparison(
        &mut self,
        call: &CallNode<'_>,
        truth: bool,
    ) -> Option<bool> {
        let equality_when_true = match call.name().as_slice() {
            b"==" | b"eql?" => true,
            b"!=" => false,
            _ => return None,
        };
        let arguments = call.arguments()?;
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return None;
        }
        let receiver = call.receiver()?;
        let left_read = shape_local_key_read(&receiver);
        let right_read = shape_local_key_read(&argument_nodes[0]);
        let left_literal = literal_value(&receiver);
        let right_literal = literal_value(&argument_nodes[0]);
        let (name, key, literal) =
            if let (Some((name, key)), Some(literal)) = (left_read, right_literal) {
                (name, key, literal)
            } else if let (Some(literal), Some((name, key))) = (left_literal, right_read) {
                (name, key, literal)
            } else {
                return None;
            };
        let identities = self.environment.shape_identities(&name);
        if identities.is_empty() {
            return None;
        }
        let require_match = truth == equality_when_true;
        Some(
            self.apply_shape_identity_narrowing(&identities, |ruby_type| {
                narrow_shape_literal_type(ruby_type, &key, &literal, require_match)
            }),
        )
    }

    pub(in crate::inference::type_tracker) fn narrow_shape_literal_set(
        &mut self,
        name: &str,
        key: &LiteralKey,
        literals: &[LiteralValue],
        require_match: bool,
    ) -> bool {
        assert!(
            !literals.is_empty(),
            "INVARIANT VIOLATED: shape discriminator narrowing received an empty literal set. This is a bug because an empty Ruby when clause cannot reach this helper. Fix: require at least one supported literal condition before narrowing."
        );
        let identities = self.environment.shape_identities(name);
        if identities.is_empty() {
            return true;
        }
        self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_literal_set_type(ruby_type, key, literals, require_match)
        })
    }

    pub(in crate::inference::type_tracker) fn narrow_shape_hash_pattern(
        &mut self,
        predicate: &Node<'_>,
        pattern: &Node<'_>,
        require_match: bool,
    ) -> Option<bool> {
        let read = predicate.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        let requirements = hash_pattern_requirements(pattern)?;
        if requirements.is_empty() {
            return None;
        }
        let identities = self.environment.shape_identities(&name);
        if identities.is_empty() {
            return None;
        }
        let reaches = self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_pattern_type(ruby_type, &requirements, require_match)
        });
        if !reaches || !require_match {
            return Some(reaches);
        }

        // A successful Hash pattern proves every listed key is present even
        // when its source contract marked the field optional. Apply that
        // presence proof only after filtering complete variants; open/rest
        // shapes still remain conservative about the field value.
        for (key, _) in &requirements {
            let active_identities = self.environment.shape_identities(&name);
            let reaches = self.apply_shape_identity_narrowing(&active_identities, |ruby_type| {
                narrow_shape_presence_type(ruby_type, key, true)
            });
            if !reaches {
                return Some(false);
            }
        }
        Some(true)
    }

    pub(in crate::inference::type_tracker) fn apply_shape_identity_narrowing(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        mut narrow: impl FnMut(&RubyType) -> Result<Option<RubyType>, UnknownReason>,
    ) -> bool {
        let mut retained = BTreeSet::new();
        let mut updates = Vec::new();
        for identity in identities {
            let state = self.environment.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape guard references absent identity {:?}. This is a bug because a local binding and its identity states must be joined atomically. Fix: preserve the complete FlowEnvironment across branches.",
                    identity
                )
            });
            match state {
                ShapeIdentityState::Invalidated(_) => {
                    retained.insert(*identity);
                }
                ShapeIdentityState::Proven(ruby_type) => match narrow(ruby_type) {
                    Ok(Some(narrowed)) => {
                        retained.insert(*identity);
                        updates.push((*identity, narrowed));
                    }
                    Ok(None) => {}
                    Err(reason) => {
                        self.environment.invalidate_identities(identities, reason);
                        return true;
                    }
                },
            }
        }
        if retained.is_empty() {
            return false;
        }
        for (identity, ruby_type) in updates {
            self.environment
                .shape_states
                .insert(identity, ShapeIdentityState::Proven(ruby_type));
        }
        if &retained != identities {
            for binding in self.environment.shape_bindings.values_mut() {
                binding.retain(|identity| {
                    !identities.contains(identity) || retained.contains(identity)
                });
            }
        }
        self.environment.synchronize_shape_aliases();
        true
    }
}
pub(in crate::inference::type_tracker) fn literal_value(node: &Node<'_>) -> Option<LiteralValue> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(LiteralValue::symbol(
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
        ));
    }
    node.as_string_node()
        .map(|string| LiteralValue::string(String::from_utf8_lossy(string.unescaped()).to_string()))
}

pub(in crate::inference::type_tracker) fn narrow_shape_literal_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    literal: &LiteralValue,
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: discriminator narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep discriminator inputs restricted to complete shape identities."
            );
        };
        let relationship = shape_literal_relationship(&shape, key, literal);
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn narrow_shape_literal_set_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    literals: &[LiteralValue],
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: case discriminator narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep case narrowing inputs restricted to complete shape identities."
            );
        };
        let relationships = literals
            .iter()
            .map(|literal| shape_literal_relationship(&shape, key, literal))
            .collect::<Vec<_>>();
        let relationship = if relationships
            .iter()
            .any(|candidate| *candidate == ShapePredicateMatch::Matches)
        {
            ShapePredicateMatch::Matches
        } else if relationships
            .iter()
            .all(|candidate| *candidate == ShapePredicateMatch::DoesNotMatch)
        {
            ShapePredicateMatch::DoesNotMatch
        } else {
            ShapePredicateMatch::Inconclusive
        };
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn shape_literal_relationship(
    shape: &ShapeType,
    key: &LiteralKey,
    literal: &LiteralValue,
) -> ShapePredicateMatch {
    let Some(field) = shape.field(key) else {
        return if shape.is_exact() {
            ShapePredicateMatch::DoesNotMatch
        } else {
            ShapePredicateMatch::Inconclusive
        };
    };
    if !field.is_required() {
        return ShapePredicateMatch::Inconclusive;
    }
    ruby_type_literal_relationship(field.value(), literal)
}

pub(in crate::inference::type_tracker) fn ruby_type_literal_relationship(
    ruby_type: &RubyType,
    literal: &LiteralValue,
) -> ShapePredicateMatch {
    match ruby_type {
        RubyType::Literal(value) => {
            if value.as_ref() == literal {
                ShapePredicateMatch::Matches
            } else {
                ShapePredicateMatch::DoesNotMatch
            }
        }
        RubyType::Union(members) => {
            let relationships = members
                .iter()
                .map(|member| ruby_type_literal_relationship(member, literal))
                .collect::<Vec<_>>();
            if relationships
                .iter()
                .all(|relationship| *relationship == ShapePredicateMatch::Matches)
            {
                ShapePredicateMatch::Matches
            } else if relationships
                .iter()
                .all(|relationship| *relationship == ShapePredicateMatch::DoesNotMatch)
            {
                ShapePredicateMatch::DoesNotMatch
            } else {
                ShapePredicateMatch::Inconclusive
            }
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => ShapePredicateMatch::Inconclusive,
    }
}

pub(in crate::inference::type_tracker) fn hash_pattern_requirements(
    pattern: &Node<'_>,
) -> Option<Vec<(LiteralKey, Option<LiteralValue>)>> {
    let hash = pattern.as_hash_pattern_node()?;
    let mut requirements = Vec::new();
    for element in hash.elements().iter() {
        let assoc = element.as_assoc_node()?;
        let key = literal_key(&assoc.key())?;
        let value = assoc.value();
        let literal = literal_value(&value);
        let supported_value = literal.is_some()
            || value.as_local_variable_target_node().is_some()
            || value
                .as_implicit_node()
                .is_some_and(|implicit| implicit.value().as_local_variable_target_node().is_some());
        if !supported_value {
            return None;
        }
        requirements.push((key, literal));
    }
    Some(requirements)
}

pub(in crate::inference::type_tracker) fn narrow_shape_pattern_type(
    ruby_type: &RubyType,
    requirements: &[(LiteralKey, Option<LiteralValue>)],
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: Hash pattern narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep pattern inputs restricted to complete shape identities."
            );
        };
        let relationships = requirements
            .iter()
            .map(|(key, literal)| match (shape.field(key), literal) {
                (None, _) if shape.is_exact() => ShapePredicateMatch::DoesNotMatch,
                (None, _) => ShapePredicateMatch::Inconclusive,
                (Some(field), _) if !field.is_required() => ShapePredicateMatch::Inconclusive,
                (Some(_), None) => ShapePredicateMatch::Matches,
                (Some(field), Some(literal)) => {
                    ruby_type_literal_relationship(field.value(), literal)
                }
            })
            .collect::<Vec<_>>();
        let relationship = if relationships
            .iter()
            .any(|candidate| *candidate == ShapePredicateMatch::DoesNotMatch)
        {
            ShapePredicateMatch::DoesNotMatch
        } else if relationships
            .iter()
            .all(|candidate| *candidate == ShapePredicateMatch::Matches)
        {
            ShapePredicateMatch::Matches
        } else {
            ShapePredicateMatch::Inconclusive
        };
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

pub(in crate::inference::type_tracker) fn narrow_shape_presence_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    truth: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: presence narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep the narrowing input filter exhaustive."
            );
        };
        match (shape.field(key), truth) {
            (Some(field), true) if field.is_required() => {
                retained.push(RubyType::Shape(shape));
            }
            (Some(field), true) => {
                let mut fields = shape
                    .fields()
                    .iter()
                    .map(|field| (field.key().clone(), field.clone()))
                    .collect::<BTreeMap<_, _>>();
                fields.insert(
                    key.clone(),
                    ShapeField::required(key.clone(), field.value().clone()),
                );
                retained.push(RubyType::Shape(Box::new(rebuild_shape(
                    &shape,
                    fields.into_values(),
                    shape.stability(),
                )?)));
            }
            (Some(field), false) if field.is_required() => {}
            (Some(_), false) => {
                retained.push(shape_without_field(&RubyType::Shape(shape), key)?);
            }
            (None, true) if shape.is_exact() => {}
            (None, false) if shape.is_exact() => {
                retained.push(RubyType::Shape(shape));
            }
            (None, true) => {
                if let Some(rest) = shape.rest() {
                    if key.generic_type().is_subtype_of(rest.key()) {
                        let mut fields = shape
                            .fields()
                            .iter()
                            .map(|field| (field.key().clone(), field.clone()))
                            .collect::<BTreeMap<_, _>>();
                        fields.insert(
                            key.clone(),
                            ShapeField::required(key.clone(), rest.value().clone()),
                        );
                        retained.push(RubyType::Shape(Box::new(rebuild_shape(
                            &shape,
                            fields.into_values(),
                            shape.stability(),
                        )?)));
                    }
                } else {
                    // An open shape without a rest contract proves only that
                    // the branch can be reached, not the new field's value.
                    retained.push(RubyType::Shape(shape));
                }
            }
            (None, false) => retained.push(RubyType::Shape(shape)),
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let joined = RubyType::union(retained);
    if joined == RubyType::Unknown {
        return Err(UnknownReason::ShapeBoundExceeded);
    }
    Ok(Some(joined))
}
