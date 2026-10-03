//! Snapshot encoding and restoration of ranges, names, types, and type subjects.

use super::{
    SnapshotFqn, SnapshotLiteral, SnapshotRange, SnapshotRubyType, SnapshotShapeExactness,
    SnapshotShapeField, SnapshotShapeFieldPresence, SnapshotShapeRest, SnapshotShapeStability,
    SnapshotTypeProvenance, SnapshotTypeSubject,
};
use crate::core::{
    FullyQualifiedName, LiteralKey, LiteralValue, NamespaceKind, RubyConstant, RubyMethod,
    RubyType, ShapeExactness, ShapeField, ShapeFieldPresence, ShapeRest, ShapeStability, ShapeType,
    SourceFileId, TextRange, TypeProvenance, TypeSubject,
};

pub(super) fn snapshot_range(range: TextRange) -> SnapshotRange {
    SnapshotRange {
        start_byte: range.start_byte,
        end_byte: range.end_byte,
    }
}

pub(super) fn restore_range(
    range: SnapshotRange,
    file_id: SourceFileId,
) -> Result<TextRange, String> {
    if range.start_byte > range.end_byte {
        return Err(format!(
            "persistent range starts at {} after ending at {}",
            range.start_byte, range.end_byte
        ));
    }
    Ok(TextRange::new(file_id, range.start_byte, range.end_byte))
}

pub(super) fn snapshot_parts(parts: &[RubyConstant]) -> Result<Vec<String>, String> {
    Ok(parts.iter().map(|part| part.as_str().to_string()).collect())
}

pub(super) fn restore_parts(parts: Vec<String>) -> Result<Vec<RubyConstant>, String> {
    if parts.len() > 4096 {
        return Err(format!(
            "persistent FQN has {} parts; maximum is 4096",
            parts.len()
        ));
    }
    parts
        .into_iter()
        .map(|part| {
            if part.starts_with('\0') {
                RubyConstant::from_canonical_generated_owner(&part).map_err(|error| {
                    format!("invalid persistent generated owner identity: {error}")
                })
            } else {
                RubyConstant::new(&part)
                    .map_err(|error| format!("invalid persistent Ruby constant `{part}`: {error}"))
            }
        })
        .collect()
}

pub(super) fn snapshot_fqn(fqn: &FullyQualifiedName) -> Result<SnapshotFqn, String> {
    Ok(match fqn {
        FullyQualifiedName::Namespace(parts, kind) => SnapshotFqn::Namespace {
            parts: snapshot_parts(parts)?,
            singleton: matches!(kind, NamespaceKind::Singleton),
        },
        FullyQualifiedName::Constant(parts) => SnapshotFqn::Constant {
            parts: snapshot_parts(parts)?,
        },
        FullyQualifiedName::Method(parts, method) => SnapshotFqn::Method {
            parts: snapshot_parts(parts)?,
            name: method.as_str().to_string(),
        },
        FullyQualifiedName::LocalVariable(name) => SnapshotFqn::LocalVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::InstanceVariable(name) => SnapshotFqn::InstanceVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::ClassVariable(name) => SnapshotFqn::ClassVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::GlobalVariable(name) => SnapshotFqn::GlobalVariable {
            name: name.as_str().to_string(),
        },
    })
}

pub(super) fn restore_fqn(fqn: SnapshotFqn) -> Result<FullyQualifiedName, String> {
    match fqn {
        SnapshotFqn::Namespace { parts, singleton } => Ok(FullyQualifiedName::namespace_with_kind(
            restore_parts(parts)?,
            if singleton {
                NamespaceKind::Singleton
            } else {
                NamespaceKind::Instance
            },
        )),
        SnapshotFqn::Constant { parts } => Ok(FullyQualifiedName::constant(restore_parts(parts)?)),
        SnapshotFqn::Method { parts, name } => Ok(FullyQualifiedName::method(
            restore_parts(parts)?,
            RubyMethod::new(&name)
                .map_err(|error| format!("invalid persistent Ruby method `{name}`: {error}"))?,
        )),
        SnapshotFqn::LocalVariable { name } => FullyQualifiedName::local_variable(name)
            .map_err(|error| format!("invalid persistent local variable: {error}")),
        SnapshotFqn::InstanceVariable { name } => FullyQualifiedName::instance_variable(name)
            .map_err(|error| format!("invalid persistent instance variable: {error}")),
        SnapshotFqn::ClassVariable { name } => FullyQualifiedName::class_variable(name)
            .map_err(|error| format!("invalid persistent class variable: {error}")),
        SnapshotFqn::GlobalVariable { name } => FullyQualifiedName::global_variable(name)
            .map_err(|error| format!("invalid persistent global variable: {error}")),
    }
}

pub(super) fn snapshot_ruby_type(ruby_type: &RubyType) -> Result<SnapshotRubyType, String> {
    Ok(match ruby_type {
        RubyType::Class(fqn) => SnapshotRubyType::Class {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::Module(fqn) => SnapshotRubyType::Module {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::ClassReference(fqn) => SnapshotRubyType::ClassReference {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::ModuleReference(fqn) => SnapshotRubyType::ModuleReference {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::Literal(value) => SnapshotRubyType::Literal {
            value: snapshot_literal_value(value),
        },
        RubyType::Array(elements) => SnapshotRubyType::Array {
            elements: elements
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Hash(keys, values) => SnapshotRubyType::Hash {
            keys: keys
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
            values: values
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Shape(shape) => SnapshotRubyType::Shape {
            fields: shape
                .fields()
                .iter()
                .map(|field| {
                    Ok(SnapshotShapeField {
                        key: snapshot_literal_key(field.key()),
                        value: snapshot_ruby_type(field.value())?,
                        presence: match field.presence() {
                            ShapeFieldPresence::Required => SnapshotShapeFieldPresence::Required,
                            ShapeFieldPresence::Optional => SnapshotShapeFieldPresence::Optional,
                        },
                    })
                })
                .collect::<Result<_, String>>()?,
            rest: shape
                .rest()
                .map(|rest| {
                    Ok::<_, String>(Box::new(SnapshotShapeRest {
                        key: snapshot_ruby_type(rest.key())?,
                        value: snapshot_ruby_type(rest.value())?,
                    }))
                })
                .transpose()?,
            exactness: match shape.exactness() {
                ShapeExactness::Exact => SnapshotShapeExactness::Exact,
                ShapeExactness::Open => SnapshotShapeExactness::Open,
            },
            stability: match shape.stability() {
                ShapeStability::TrackedMutable => SnapshotShapeStability::TrackedMutable,
                ShapeStability::Frozen => SnapshotShapeStability::Frozen,
            },
        },
        RubyType::Union(types) => SnapshotRubyType::Union {
            types: types
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Unknown => SnapshotRubyType::Unknown,
    })
}

pub(super) fn restore_ruby_type(
    ruby_type: SnapshotRubyType,
    depth: usize,
) -> Result<RubyType, String> {
    if depth > 64 {
        return Err("persistent Ruby type exceeds the maximum nesting depth of 64".to_string());
    }
    match ruby_type {
        SnapshotRubyType::Class { fqn } => Ok(RubyType::Class(restore_fqn(fqn)?)),
        SnapshotRubyType::Module { fqn } => Ok(RubyType::Module(restore_fqn(fqn)?)),
        SnapshotRubyType::ClassReference { fqn } => Ok(RubyType::ClassReference(restore_fqn(fqn)?)),
        SnapshotRubyType::ModuleReference { fqn } => {
            Ok(RubyType::ModuleReference(restore_fqn(fqn)?))
        }
        SnapshotRubyType::Literal { value } => {
            Ok(RubyType::Literal(Box::new(restore_literal_value(value))))
        }
        SnapshotRubyType::Array { elements } => Ok(RubyType::Array(
            elements
                .into_iter()
                .map(|element| restore_ruby_type(element, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Hash { keys, values } => Ok(RubyType::Hash(
            keys.into_iter()
                .map(|key| restore_ruby_type(key, depth + 1))
                .collect::<Result<_, _>>()?,
            values
                .into_iter()
                .map(|value| restore_ruby_type(value, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Shape {
            fields,
            rest,
            exactness,
            stability,
        } => {
            let fields = fields
                .into_iter()
                .map(|field| {
                    let key = restore_literal_key(field.key);
                    let value = restore_ruby_type(field.value, depth + 1)?;
                    Ok(match field.presence {
                        SnapshotShapeFieldPresence::Required => ShapeField::required(key, value),
                        SnapshotShapeFieldPresence::Optional => ShapeField::optional(key, value),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let rest = rest
                .map(|rest| {
                    Ok::<_, String>(ShapeRest::new(
                        restore_ruby_type(rest.key, depth + 1)?,
                        restore_ruby_type(rest.value, depth + 1)?,
                    ))
                })
                .transpose()?;
            let exactness = match exactness {
                SnapshotShapeExactness::Exact => ShapeExactness::Exact,
                SnapshotShapeExactness::Open => ShapeExactness::Open,
            };
            let stability = match stability {
                SnapshotShapeStability::TrackedMutable => ShapeStability::TrackedMutable,
                SnapshotShapeStability::Frozen => ShapeStability::Frozen,
            };
            ShapeType::try_new(fields, rest, exactness, stability)
                .map(|shape| RubyType::Shape(Box::new(shape)))
                .map_err(|error| format!("invalid persistent shape type: {error}"))
        }
        SnapshotRubyType::Union { types } => Ok(RubyType::Union(
            types
                .into_iter()
                .map(|ruby_type| restore_ruby_type(ruby_type, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Unknown => Ok(RubyType::Unknown),
    }
}

fn snapshot_literal_value(value: &LiteralValue) -> SnapshotLiteral {
    match value {
        LiteralValue::Symbol(value) => SnapshotLiteral::Symbol(value.clone()),
        LiteralValue::String(value) => SnapshotLiteral::String(value.clone()),
    }
}

pub(super) fn snapshot_literal_key(key: &LiteralKey) -> SnapshotLiteral {
    match key {
        LiteralKey::Symbol(value) => SnapshotLiteral::Symbol(value.clone()),
        LiteralKey::String(value) => SnapshotLiteral::String(value.clone()),
    }
}

fn restore_literal_value(value: SnapshotLiteral) -> LiteralValue {
    match value {
        SnapshotLiteral::Symbol(value) => LiteralValue::Symbol(value),
        SnapshotLiteral::String(value) => LiteralValue::String(value),
    }
}

pub(super) fn restore_literal_key(key: SnapshotLiteral) -> LiteralKey {
    match key {
        SnapshotLiteral::Symbol(value) => LiteralKey::Symbol(value),
        SnapshotLiteral::String(value) => LiteralKey::String(value),
    }
}

pub(super) fn snapshot_type_subject(subject: &TypeSubject) -> Result<SnapshotTypeSubject, String> {
    Ok(match subject {
        TypeSubject::Constant(fqn) => SnapshotTypeSubject::Constant {
            fqn: snapshot_fqn(fqn)?,
        },
        TypeSubject::Local { scope_id, name } => SnapshotTypeSubject::Local {
            scope_id: *scope_id,
            name: name.clone(),
        },
        TypeSubject::InstanceVariable { owner, name } => SnapshotTypeSubject::InstanceVariable {
            owner: snapshot_fqn(owner)?,
            name: name.clone(),
        },
        TypeSubject::ClassVariable { owner, name } => SnapshotTypeSubject::ClassVariable {
            owner: snapshot_fqn(owner)?,
            name: name.clone(),
        },
        TypeSubject::GlobalVariable(name) => {
            SnapshotTypeSubject::GlobalVariable { name: name.clone() }
        }
        TypeSubject::MethodReturn(fqn) => SnapshotTypeSubject::MethodReturn {
            fqn: snapshot_fqn(fqn)?,
        },
        TypeSubject::Parameter { method, name } => SnapshotTypeSubject::Parameter {
            method: snapshot_fqn(method)?,
            name: name.clone(),
        },
        TypeSubject::Expression(range) => SnapshotTypeSubject::Expression {
            range: snapshot_range(*range),
        },
    })
}

pub(super) fn restore_type_subject(
    subject: SnapshotTypeSubject,
    file_id: SourceFileId,
) -> Result<TypeSubject, String> {
    Ok(match subject {
        SnapshotTypeSubject::Constant { fqn } => TypeSubject::Constant(restore_fqn(fqn)?),
        SnapshotTypeSubject::Local { scope_id, name } => TypeSubject::Local { scope_id, name },
        SnapshotTypeSubject::InstanceVariable { owner, name } => TypeSubject::InstanceVariable {
            owner: restore_fqn(owner)?,
            name,
        },
        SnapshotTypeSubject::ClassVariable { owner, name } => TypeSubject::ClassVariable {
            owner: restore_fqn(owner)?,
            name,
        },
        SnapshotTypeSubject::GlobalVariable { name } => TypeSubject::GlobalVariable(name),
        SnapshotTypeSubject::MethodReturn { fqn } => TypeSubject::MethodReturn(restore_fqn(fqn)?),
        SnapshotTypeSubject::Parameter { method, name } => TypeSubject::Parameter {
            method: restore_fqn(method)?,
            name,
        },
        SnapshotTypeSubject::Expression { range } => {
            TypeSubject::Expression(restore_range(range, file_id)?)
        }
    })
}

pub(super) fn snapshot_provenance(provenance: TypeProvenance) -> SnapshotTypeProvenance {
    match provenance {
        TypeProvenance::Literal => SnapshotTypeProvenance::Literal,
        TypeProvenance::Assignment => SnapshotTypeProvenance::Assignment,
        TypeProvenance::Flow => SnapshotTypeProvenance::Flow,
        TypeProvenance::Rbs => SnapshotTypeProvenance::Rbs,
        TypeProvenance::Yard => SnapshotTypeProvenance::Yard,
        TypeProvenance::Runtime => SnapshotTypeProvenance::Runtime,
        TypeProvenance::Extension => SnapshotTypeProvenance::Extension,
        TypeProvenance::Inferred => SnapshotTypeProvenance::Inferred,
    }
}

pub(super) fn restore_provenance(provenance: SnapshotTypeProvenance) -> TypeProvenance {
    match provenance {
        SnapshotTypeProvenance::Literal => TypeProvenance::Literal,
        SnapshotTypeProvenance::Assignment => TypeProvenance::Assignment,
        SnapshotTypeProvenance::Flow => TypeProvenance::Flow,
        SnapshotTypeProvenance::Rbs => TypeProvenance::Rbs,
        SnapshotTypeProvenance::Yard => TypeProvenance::Yard,
        SnapshotTypeProvenance::Runtime => TypeProvenance::Runtime,
        SnapshotTypeProvenance::Extension => TypeProvenance::Extension,
        SnapshotTypeProvenance::Inferred => TypeProvenance::Inferred,
    }
}
