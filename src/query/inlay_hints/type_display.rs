//! Editor presentation only: summaries never replace the engine's exact types.

use super::tooltip::type_tooltip;
use ruby_analysis::core::{FullyQualifiedName, LiteralValue, RubyType, ShapeType};
use std::collections::{BTreeMap, BTreeSet};
use tower_lsp::lsp_types::InlayHintTooltip;

// Leave room for ": " or " -> " below VS Code's default 43-character limit.
const MAX_TYPE_LABEL_WIDTH: usize = 36;

pub(super) struct TypeDisplay {
    pub parts: Vec<TypeLabelPart>,
    pub tooltip: InlayHintTooltip,
}

/// Keep semantic identity attached while shortening the visible text. Never
/// reconstruct navigation targets from abbreviated labels.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct TypeLabelPart {
    pub value: String,
    pub target: Option<FullyQualifiedName>,
}

impl TypeLabelPart {
    fn text(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            target: None,
        }
    }

    fn named(value: impl Into<String>, target: FullyQualifiedName) -> Self {
        Self {
            value: value.into(),
            target: Some(target),
        }
    }

    fn core(name: &'static str) -> Self {
        Self::named(name, FullyQualifiedName::try_from(name).expect(
            "INVARIANT VIOLATED: built-in inlay type name is invalid. This is a bug because presentation uses Ruby core constants. Fix: use the canonical core type name."
        ))
    }
}

impl TypeDisplay {
    pub fn new(ruby_type: &RubyType) -> Self {
        let mut label = Label::from_type(ruby_type);
        if width(&label.text()) > MAX_TYPE_LABEL_WIDTH {
            let names = short_names(&label.parts());
            label.shorten_names(&names);
        }
        Self {
            parts: label.fit(MAX_TYPE_LABEL_WIDTH),
            tooltip: type_tooltip(ruby_type),
        }
    }
}

#[derive(PartialEq, Eq)]
enum Label {
    Atom(TypeLabelPart),
    Apply(&'static str, Vec<Label>),
    Union(Vec<Label>, bool),
}

impl Label {
    fn from_type(ruby_type: &RubyType) -> Self {
        match ruby_type {
            RubyType::Class(fqn) => Self::Atom(TypeLabelPart::named(fqn.to_string(), fqn.clone())),
            RubyType::Module(fqn) => {
                Self::Atom(TypeLabelPart::named(format!("module {fqn}"), fqn.clone()))
            }
            RubyType::ClassReference(fqn) => Self::Apply(
                "Class",
                vec![Self::Atom(TypeLabelPart::named(
                    fqn.to_string(),
                    fqn.clone(),
                ))],
            ),
            RubyType::ModuleReference(fqn) => Self::Apply(
                "Module",
                vec![Self::Atom(TypeLabelPart::named(
                    fqn.to_string(),
                    fqn.clone(),
                ))],
            ),
            RubyType::Literal(value) => {
                let mut part = match value.as_ref() {
                    LiteralValue::Symbol(_) => TypeLabelPart::core("Symbol"),
                    LiteralValue::String(_) => TypeLabelPart::core("String"),
                };
                part.value = value.to_string();
                Self::Atom(part)
            }
            RubyType::Shape(shape) => Self::shapes(&[shape]),
            RubyType::Array(elements) => Self::Apply("Array", vec![Self::union(elements, false)]),
            RubyType::Hash(keys, values) => Self::Apply(
                "Hash",
                vec![Self::union(keys, true), Self::union(values, true)],
            ),
            RubyType::Union(members) => Self::union(members, true),
            RubyType::Unknown => Self::Atom(TypeLabelPart::text("?")),
        }
    }

    fn shapes(shapes: &[&ShapeType]) -> Self {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for shape in shapes {
            // An exact empty variant contributes no entries to a generic Hash
            // summary. Open/unknown tails must still contribute their Unknown.
            if shape.is_exact() && shape.fields().is_empty() && shape.rest().is_none() {
                continue;
            }
            let RubyType::Hash(shape_keys, shape_values) = shape.generic_hash_type() else {
                panic!("INVARIANT VIOLATED: shape generic view is not a Hash. This is a bug because shape presentation relies on the canonical Hash projection. Fix: preserve the generic_hash_type contract.");
            };
            keys.extend(shape_keys);
            values.extend(shape_values);
        }
        Self::from_type(&RubyType::Hash(
            RubyType::canonical_union_members(keys),
            RubyType::canonical_union_members(values),
        ))
    }

    fn union(members: &[RubyType], parenthesized: bool) -> Self {
        let shapes = members
            .iter()
            .filter_map(|member| match member {
                RubyType::Shape(shape) => Some(shape.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut labels = Vec::new();
        let mut summarized_shapes = false;
        for member in members {
            let label = if matches!(member, RubyType::Shape(_)) {
                if summarized_shapes {
                    continue;
                }
                summarized_shapes = true;
                Self::shapes(&shapes)
            } else {
                Self::from_type(member)
            };
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        match labels.len() {
            0 => Self::Atom(TypeLabelPart::text("?")),
            1 => labels.remove(0),
            _ => Self::Union(labels, parenthesized),
        }
    }

    fn text(&self) -> String {
        self.parts()
            .iter()
            .map(|part| part.value.as_str())
            .collect()
    }

    fn parts(&self) -> Vec<TypeLabelPart> {
        match self {
            Self::Atom(part) => vec![part.clone()],
            Self::Apply(name, arguments) => {
                Self::application(name, arguments.iter().map(Self::parts))
            }
            Self::Union(members, parenthesized) => {
                Self::alternatives(members.iter().map(Self::parts), *parenthesized)
            }
        }
    }

    fn application(
        name: &'static str,
        arguments: impl IntoIterator<Item = Vec<TypeLabelPart>>,
    ) -> Vec<TypeLabelPart> {
        let mut parts = vec![TypeLabelPart::core(name), TypeLabelPart::text("<")];
        Self::join(&mut parts, arguments, ", ");
        parts.push(TypeLabelPart::text(">"));
        parts
    }

    fn alternatives(
        members: impl IntoIterator<Item = Vec<TypeLabelPart>>,
        parenthesized: bool,
    ) -> Vec<TypeLabelPart> {
        let mut parts = Vec::new();
        if parenthesized {
            parts.push(TypeLabelPart::text("("));
        }
        Self::join(&mut parts, members, " | ");
        if parenthesized {
            parts.push(TypeLabelPart::text(")"));
        }
        parts
    }

    fn join(
        parts: &mut Vec<TypeLabelPart>,
        groups: impl IntoIterator<Item = Vec<TypeLabelPart>>,
        separator: &str,
    ) {
        for (index, group) in groups.into_iter().enumerate() {
            if index != 0 {
                parts.push(TypeLabelPart::text(separator));
            }
            parts.extend(group);
        }
    }

    fn fit(&self, budget: usize) -> Vec<TypeLabelPart> {
        if width(&self.text()) <= budget {
            return self.parts();
        }
        match self {
            Self::Atom(part) => {
                let mut part = part.clone();
                part.value = abbreviate(&part.value, budget);
                vec![part]
            }
            Self::Apply(name, arguments) => {
                let punctuation = name.len() + 2 + 2 * arguments.len().saturating_sub(1);
                if budget < punctuation + arguments.len() {
                    return vec![TypeLabelPart::text("…")];
                }
                let available = (budget - punctuation) / arguments.len();
                Self::application(
                    name,
                    arguments.iter().map(|argument| argument.fit(available)),
                )
            }
            Self::Union(members, parenthesized) => {
                let shown = members.len().min(3);
                let omitted = usize::from(shown < members.len());
                let punctuation = 2 * usize::from(*parenthesized) + 3 * (shown + omitted - 1);
                if budget < punctuation + shown + omitted {
                    return vec![TypeLabelPart::text("…")];
                }
                let available = (budget - punctuation - omitted) / shown;
                let mut groups = members
                    .iter()
                    .take(shown)
                    .map(|member| member.fit(available))
                    .collect::<Vec<_>>();
                if omitted != 0 {
                    groups.push(vec![TypeLabelPart::text("…")]);
                }
                Self::alternatives(groups, *parenthesized)
            }
        }
    }

    fn shorten_names(&mut self, names: &BTreeMap<String, String>) {
        match self {
            Self::Atom(part) => {
                if let Some(target) = &part.target {
                    let full = target.to_string();
                    if let Some(short) = names.get(&full) {
                        if part.value == full {
                            part.value = short.clone();
                        } else if part.value == format!("module {full}") {
                            part.value = format!("module {short}");
                        }
                    }
                }
            }
            Self::Apply(_, members) | Self::Union(members, _) => {
                for member in members {
                    member.shorten_names(names);
                }
            }
        }
    }
}

fn width(text: &str) -> usize {
    text.encode_utf16().count()
}

fn abbreviate(text: &str, budget: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for character in text.chars() {
        if used + character.len_utf16() + 1 > budget {
            break;
        }
        result.push(character);
        used += character.len_utf16();
    }
    result.push('…');
    result
}

fn short_names(parts: &[TypeLabelPart]) -> BTreeMap<String, String> {
    let names = parts
        .iter()
        .filter_map(|part| part.target.as_ref().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    let parts = names
        .iter()
        .map(|name| (name, name.split("::").collect::<Vec<_>>()))
        .collect::<Vec<_>>();
    parts
        .iter()
        .map(|(full, segments)| {
            for count in 1..segments.len() {
                let suffix = &segments[segments.len() - count..];
                if !parts
                    .iter()
                    .any(|(other, parts)| other != full && parts.ends_with(suffix))
                {
                    return ((*full).clone(), format!("…::{}", suffix.join("::")));
                }
            }
            ((*full).clone(), (*full).clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::{FullyQualifiedName, LiteralValue};

    fn label(display: &TypeDisplay) -> String {
        display
            .parts
            .iter()
            .map(|part| part.value.as_str())
            .collect()
    }

    #[test]
    fn compact_inlay_names_keep_ambiguous_leaves_distinct() {
        let ruby_type = RubyType::union([
            RubyType::class("Workshop::Runtime::Read::Entry"),
            RubyType::class("Workshop::Runtime::Write::Entry"),
        ]);
        let display = TypeDisplay::new(&ruby_type);
        assert_eq!(label(&display), "(…::Read::Entry | …::Write::Entry)");
        assert!(matches!(&display.tooltip,
            InlayHintTooltip::String(text) if text == &ruby_type.to_string()));
    }

    #[test]
    fn compact_inlay_nested_types_keep_balanced_delimiters_and_full_tooltips() {
        let ruby_type = RubyType::array_of(RubyType::hash_of(
            RubyType::class("Workshop::Runtime::Registry::VeryLongLookupKeyName"),
            RubyType::union((0..6).map(|index| {
                RubyType::class(&format!("Workshop::Runtime::VeryLongResultVariant{index}"))
            })),
        ));
        let display = TypeDisplay::new(&ruby_type);
        assert!(
            width(&label(&display)) <= MAX_TYPE_LABEL_WIDTH,
            "{}",
            label(&display)
        );
        assert!(
            label(&display).starts_with("Array<Hash<"),
            "{}",
            label(&display)
        );
        assert_eq!(
            label(&display).matches('<').count(),
            label(&display).matches('>').count()
        );
        assert_eq!(
            label(&display).matches('(').count(),
            label(&display).matches(')').count()
        );
        assert!(label(&display).contains('…'));
        assert!(matches!(&display.tooltip,
            InlayHintTooltip::String(text) if text == &ruby_type.to_string()));
    }

    #[test]
    fn compact_inlay_abbreviation_is_unicode_safe() {
        let ruby_type = RubyType::Literal(Box::new(LiteralValue::string("🙂".repeat(50))));
        let display = TypeDisplay::new(&ruby_type);
        assert!(width(&label(&display)) <= MAX_TYPE_LABEL_WIDTH);
        assert!(label(&display).ends_with('…'));
        assert!(matches!(&display.tooltip,
            InlayHintTooltip::String(text) if text == &ruby_type.to_string()));
    }

    #[test]
    fn compact_inlay_preserves_class_object_and_collection_wrappers() {
        let ruby_type = RubyType::array_of(RubyType::ClassReference(
            FullyQualifiedName::try_from("Workshop::Runtime::Services::Reports::Entry").unwrap(),
        ));
        let display = TypeDisplay::new(&ruby_type);
        assert_eq!(label(&display), "Array<Class<…::Entry>>");
        assert!(matches!(&display.tooltip,
            InlayHintTooltip::String(text) if text == &ruby_type.to_string()));
        assert_eq!(
            label(&TypeDisplay::new(&RubyType::array_of(RubyType::symbol()))),
            "Array<Symbol>"
        );
    }

    #[test]
    fn compact_inlay_open_shape_tail_does_not_acquire_a_closed_variants_precision() {
        use ruby_analysis::core::{LiteralKey, ShapeExactness, ShapeField, ShapeStability};
        let open = ShapeType::try_new(
            [],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        let closed = ShapeType::try_new(
            [ShapeField::required(
                LiteralKey::symbol("north"),
                RubyType::float(),
            )],
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        let ruby_type = RubyType::union([
            RubyType::Shape(Box::new(open)),
            RubyType::Shape(Box::new(closed)),
        ]);
        let display = TypeDisplay::new(&ruby_type);
        assert_eq!(label(&display), "Hash<?, ?>");
        let InlayHintTooltip::MarkupContent(markup) = display.tooltip else {
            panic!("shape variants must have a formatted tooltip");
        };
        assert_eq!(
            markup.value,
            "```ruby\n(\n  {\n    ...\n  }\n  | {\n    north: Float\n  }\n)\n```"
        );
    }
}
