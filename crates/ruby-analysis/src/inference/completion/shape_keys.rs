//! Hash-shape key completion for literal-keyed receivers.

use crate::core::{LiteralKey, RubyType};
use crate::indexer::{RubyDocument, ShapeKeyCompletionTarget, ShapeKeySyntax};
use std::collections::BTreeSet;

use super::CompletionSemanticQuery;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeKeyCompletionResult {
    pub keys: Vec<LiteralKey>,
    pub replacement_start: u32,
    pub replacement_end: u32,
}

pub fn shape_key_completions_for_target(
    query: &impl CompletionSemanticQuery,
    document: &RubyDocument,
    target: &ShapeKeyCompletionTarget,
) -> ShapeKeyCompletionResult {
    let file_id = document.analysis_file_id();
    let receiver_type =
        match query.exact_expression_type(file_id, target.receiver_start, target.receiver_end) {
            Some(RubyType::Unknown) => None,
            Some(ruby_type) => Some(ruby_type),
            None => (|| {
                let name = target.receiver_local_name.as_ref()?;
                let scope_id = document
                    .variable_scopes
                    .find_scope_for_variable_at(name, file_id, target.receiver_start)
                    .or_else(|| {
                        document
                            .variable_scopes
                            .scope_at_position(file_id, target.receiver_start)
                    })?;
                document
                    .variable_scopes
                    .get_type_at_position(name, scope_id, file_id, target.receiver_start)
                    .cloned()
            })(),
        };
    let keys = receiver_type
        .map(|receiver_type| common_shape_keys(&receiver_type))
        .unwrap_or_default()
        .into_iter()
        .filter(|key| match (target.syntax, key) {
            (ShapeKeySyntax::Symbol, LiteralKey::Symbol(value))
            | (ShapeKeySyntax::String, LiteralKey::String(value)) => {
                value.starts_with(&target.partial)
            }
            (ShapeKeySyntax::Symbol, LiteralKey::String(_))
            | (ShapeKeySyntax::String, LiteralKey::Symbol(_)) => false,
        })
        .collect::<Vec<_>>();
    ShapeKeyCompletionResult {
        keys,
        replacement_start: target.replacement_start,
        replacement_end: target.replacement_end,
    }
}

fn common_shape_keys(receiver_type: &RubyType) -> Vec<LiteralKey> {
    let shapes = match receiver_type {
        RubyType::Shape(shape) => vec![shape.as_ref()],
        RubyType::Union(members) => {
            let mut shapes = Vec::with_capacity(members.len());
            for member in members {
                let RubyType::Shape(shape) = member else {
                    return Vec::new();
                };
                shapes.push(shape.as_ref());
            }
            shapes
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => return Vec::new(),
    };
    let Some((first, rest)) = shapes.split_first() else {
        return Vec::new();
    };
    let mut common = first
        .fields()
        .iter()
        .map(|field| field.key().clone())
        .collect::<BTreeSet<_>>();
    for shape in rest {
        let keys = shape
            .fields()
            .iter()
            .map(|field| field.key().clone())
            .collect::<BTreeSet<_>>();
        common = common.intersection(&keys).cloned().collect();
    }
    common.into_iter().collect()
}
