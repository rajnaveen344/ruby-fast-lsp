//! Readable shape details without changing the canonical semantic type display.

use ruby_analysis::core::{RubyType, ShapeType};
use tower_lsp::lsp_types::{InlayHintTooltip, MarkupContent, MarkupKind};

pub(super) fn type_tooltip(ruby_type: &RubyType) -> InlayHintTooltip {
    if !contains_shape(ruby_type) {
        return InlayHintTooltip::String(ruby_type.to_string());
    }
    let text = format_type(ruby_type, 0);
    // Literal keys/values may contain backticks. Keep the whole type inside
    // one code block even when its source contains Markdown punctuation.
    let longest_run = text
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(3.max(longest_run + 1));
    InlayHintTooltip::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: format!("{fence}ruby\n{text}\n{fence}"),
    })
}

fn contains_shape(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Array(members) | RubyType::Union(members) => members.iter().any(contains_shape),
        RubyType::Hash(keys, values) => keys.iter().chain(values).any(contains_shape),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => false,
    }
}

fn format_type(ruby_type: &RubyType, depth: usize) -> String {
    match ruby_type {
        RubyType::Shape(shape) => format_shape(shape, depth),
        RubyType::Array(members) => format!("Array<{}>", format_union(members, depth, false)),
        RubyType::Hash(keys, values) => format!(
            "Hash<{}, {}>",
            format_union(keys, depth, true),
            format_union(values, depth, true)
        ),
        RubyType::Union(members) => format_union(members, depth, true),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => ruby_type.to_string(),
    }
}

fn format_shape(shape: &ShapeType, depth: usize) -> String {
    let prefix = if shape.is_frozen() { "frozen " } else { "" };
    let indent = "  ".repeat(depth);
    let mut fields = shape
        .fields()
        .iter()
        .map(|field| {
            let optional = if field.is_required() { "" } else { "?" };
            format!(
                "{indent}  {}{optional}: {}",
                field.key(),
                format_type(field.value(), depth + 1)
            )
        })
        .collect::<Vec<_>>();
    if let Some(rest) = shape.rest() {
        fields.push(format!(
            "{indent}  ...Hash<{}, {}>",
            format_type(rest.key(), depth + 1),
            format_type(rest.value(), depth + 1)
        ));
    } else if !shape.is_exact() {
        fields.push(format!("{indent}  ..."));
    }
    if fields.is_empty() {
        format!("{prefix}{{ }}")
    } else {
        format!("{prefix}{{\n{}\n{indent}}}", fields.join(",\n"))
    }
}

fn format_union(members: &[RubyType], depth: usize, parenthesized: bool) -> String {
    if let [member] = members {
        return format_type(member, depth);
    }
    if members.iter().any(contains_shape) {
        let indent = "  ".repeat(depth);
        let members = members
            .iter()
            .map(|member| format_type(member, depth + 1))
            .collect::<Vec<_>>()
            .join(&format!("\n{indent}  | "));
        let body = format!("\n{indent}  {members}\n{indent}");
        if parenthesized {
            format!("({body})")
        } else {
            body
        }
    } else {
        let members = members
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
        if parenthesized {
            format!("({members})")
        } else {
            members
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::{
        LiteralKey, LiteralValue, ShapeExactness, ShapeField, ShapeRest, ShapeStability,
    };

    #[test]
    fn formatted_shape_tooltip_preserves_quoted_keys_optional_fields_and_rest() {
        let shape = ShapeType::try_new(
            [ShapeField::optional(
                LiteralKey::string("path, {type} | ```"),
                RubyType::Literal(Box::new(LiteralValue::symbol("ready, ok"))),
            )],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::string())),
            ShapeExactness::Open,
            ShapeStability::Frozen,
        )
        .unwrap();
        let InlayHintTooltip::MarkupContent(markup) =
            type_tooltip(&RubyType::Shape(Box::new(shape)))
        else {
            panic!("shape tooltip must preserve code-block formatting");
        };
        assert_eq!(markup.value, "````ruby\nfrozen {\n  \"path, {type} | ```\"?: :\"ready, ok\",\n  ...Hash<Symbol, String>\n}\n````");
    }
}
