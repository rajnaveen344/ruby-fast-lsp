//! Canonical bounded structural types for Hash-backed Ruby values.
//!
//! This module owns only the reusable type algebra. Prism traversal, mutable
//! alias identities, control-flow joins, and editor projection belong to their
//! existing indexer, inference, engine, and adapter layers.

use std::fmt::{self, Display, Formatter};

use super::RubyType;

/// Maximum number of canonical fields retained in one shape.
pub const MAX_SHAPE_FIELDS: usize = 32;
/// Maximum number of nested Shape values, counting the root shape as one.
pub const MAX_SHAPE_DEPTH: usize = 8;
/// Maximum number of complete shape variants retained in one semantic union.
pub const MAX_SHAPE_UNION_VARIANTS: usize = 8;
/// Maximum number of live local aliases tracked for one mutable Hash identity.
pub const MAX_SHAPE_ALIASES: usize = 8;
/// Maximum fixed-point iterations for one shape-solving unit.
pub const MAX_SHAPE_SOLVE_ITERATIONS: usize = 16;

/// A bounded literal value retained for discriminated shape unions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LiteralValue {
    Symbol(String),
    String(String),
}

impl LiteralValue {
    pub fn symbol(value: impl Into<String>) -> Self {
        Self::Symbol(value.into())
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    /// Widen a retained literal to the ordinary Ruby class used by generic
    /// method lookup and generic Hash projections.
    pub fn widened_type(&self) -> RubyType {
        match self {
            Self::Symbol(_) => RubyType::symbol(),
            Self::String(_) => RubyType::string(),
        }
    }
}

impl Display for LiteralValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(value) => write_symbol_literal(formatter, value),
            Self::String(value) => write_quoted(formatter, value),
        }
    }
}

/// Literal keys supported by the first Hash-backed shape representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LiteralKey {
    Symbol(String),
    String(String),
}

impl LiteralKey {
    pub fn symbol(value: impl Into<String>) -> Self {
        Self::Symbol(value.into())
    }

    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    pub fn generic_type(&self) -> RubyType {
        match self {
            Self::Symbol(_) => RubyType::symbol(),
            Self::String(_) => RubyType::string(),
        }
    }

    pub fn literal_type(&self) -> RubyType {
        match self {
            Self::Symbol(value) => RubyType::Literal(Box::new(LiteralValue::Symbol(value.clone()))),
            Self::String(value) => RubyType::Literal(Box::new(LiteralValue::String(value.clone()))),
        }
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        match self {
            Self::Symbol(value) | Self::String(value) => value.capacity(),
        }
    }
}

impl Display for LiteralKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(value) if simple_symbol_key(value) => formatter.write_str(value),
            Self::Symbol(value) => write_symbol_literal(formatter, value),
            Self::String(value) => write_quoted(formatter, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShapeFieldPresence {
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeField {
    key: LiteralKey,
    value: RubyType,
    presence: ShapeFieldPresence,
}

impl ShapeField {
    pub fn required(key: LiteralKey, value: RubyType) -> Self {
        Self {
            key,
            value,
            presence: ShapeFieldPresence::Required,
        }
    }

    pub fn optional(key: LiteralKey, value: RubyType) -> Self {
        Self {
            key,
            value,
            presence: ShapeFieldPresence::Optional,
        }
    }

    pub fn key(&self) -> &LiteralKey {
        &self.key
    }

    pub fn value(&self) -> &RubyType {
        &self.value
    }

    pub fn presence(&self) -> ShapeFieldPresence {
        self.presence
    }

    pub fn is_required(&self) -> bool {
        self.presence == ShapeFieldPresence::Required
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShapeExactness {
    Exact,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShapeStability {
    TrackedMutable,
    Frozen,
}

/// Generic contract for keys not listed as canonical shape fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeRest {
    key: RubyType,
    value: RubyType,
}

impl ShapeRest {
    pub fn new(key: RubyType, value: RubyType) -> Self {
        Self { key, value }
    }

    pub fn key(&self) -> &RubyType {
        &self.key
    }

    pub fn value(&self) -> &RubyType {
        &self.value
    }
}

/// A canonical bounded structural type for one Hash-backed value.
///
/// Fields are sorted by key and duplicate-free. Construction rejects partial
/// Unknown evidence, invalid exact/rest combinations, and measured bound
/// excesses, so consumers cannot observe a silently truncated shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeType {
    fields: Box<[ShapeField]>,
    rest: Option<Box<ShapeRest>>,
    exactness: ShapeExactness,
    stability: ShapeStability,
    depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeConstructionError {
    FieldBoundExceeded { actual: usize, limit: usize },
    DepthBoundExceeded { actual: usize, limit: usize },
    DuplicateField(LiteralKey),
    ExactShapeHasRest,
    UnprovenField(LiteralKey),
    UnprovenRest,
}

impl Display for ShapeConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldBoundExceeded { actual, limit } => write!(
                formatter,
                "shape has {actual} fields, exceeding the fixed limit of {limit}"
            ),
            Self::DepthBoundExceeded { actual, limit } => write!(
                formatter,
                "shape depth is {actual}, exceeding the fixed limit of {limit}"
            ),
            Self::DuplicateField(key) => {
                write!(formatter, "shape contains duplicate field `{key}`")
            }
            Self::ExactShapeHasRest => {
                formatter.write_str("an exact shape cannot have a generic rest contract")
            }
            Self::UnprovenField(key) => {
                write!(formatter, "shape field `{key}` contains Unknown evidence")
            }
            Self::UnprovenRest => {
                formatter.write_str("shape rest contract contains Unknown evidence")
            }
        }
    }
}

impl std::error::Error for ShapeConstructionError {}

impl ShapeType {
    pub fn try_new(
        fields: impl IntoIterator<Item = ShapeField>,
        rest: Option<ShapeRest>,
        exactness: ShapeExactness,
        stability: ShapeStability,
    ) -> Result<Self, ShapeConstructionError> {
        let mut fields = fields.into_iter().collect::<Vec<_>>();
        if fields.len() > MAX_SHAPE_FIELDS {
            return Err(ShapeConstructionError::FieldBoundExceeded {
                actual: fields.len(),
                limit: MAX_SHAPE_FIELDS,
            });
        }
        if exactness == ShapeExactness::Exact && rest.is_some() {
            return Err(ShapeConstructionError::ExactShapeHasRest);
        }

        fields.sort_by(|left, right| left.key.cmp(&right.key));
        if let Some(duplicate) = fields
            .windows(2)
            .find(|pair| pair[0].key == pair[1].key)
            .map(|pair| pair[0].key.clone())
        {
            return Err(ShapeConstructionError::DuplicateField(duplicate));
        }
        if let Some(unproven) = fields
            .iter()
            .find(|field| RubyType::contains_unknown(&field.value))
        {
            return Err(ShapeConstructionError::UnprovenField(unproven.key.clone()));
        }
        if rest.as_ref().is_some_and(|rest| {
            RubyType::contains_unknown(&rest.key) || RubyType::contains_unknown(&rest.value)
        }) {
            return Err(ShapeConstructionError::UnprovenRest);
        }

        let nested_depth = fields
            .iter()
            .map(|field| shape_depth_in_type(&field.value))
            .chain(rest.as_ref().into_iter().flat_map(|rest| {
                [
                    shape_depth_in_type(&rest.key),
                    shape_depth_in_type(&rest.value),
                ]
            }))
            .max()
            .unwrap_or(0);
        let depth =
            nested_depth
                .checked_add(1)
                .ok_or(ShapeConstructionError::DepthBoundExceeded {
                    actual: usize::MAX,
                    limit: MAX_SHAPE_DEPTH,
                })?;
        if depth > MAX_SHAPE_DEPTH {
            return Err(ShapeConstructionError::DepthBoundExceeded {
                actual: depth,
                limit: MAX_SHAPE_DEPTH,
            });
        }

        Ok(Self {
            fields: fields.into_boxed_slice(),
            rest: rest.map(Box::new),
            exactness,
            stability,
            depth: u8::try_from(depth).expect(
                "INVARIANT VIOLATED: an accepted shape depth did not fit u8. This is a bug because MAX_SHAPE_DEPTH is required to fit u8. Fix: keep the public bound and stored depth representation aligned.",
            ),
        })
    }

    pub fn fields(&self) -> &[ShapeField] {
        &self.fields
    }

    pub fn field(&self, key: &LiteralKey) -> Option<&ShapeField> {
        self.fields
            .binary_search_by(|field| field.key.cmp(key))
            .ok()
            .map(|index| &self.fields[index])
    }

    pub fn rest(&self) -> Option<&ShapeRest> {
        self.rest.as_deref()
    }

    pub fn exactness(&self) -> ShapeExactness {
        self.exactness
    }

    pub fn stability(&self) -> ShapeStability {
        self.stability
    }

    pub fn depth(&self) -> usize {
        usize::from(self.depth)
    }

    pub fn is_exact(&self) -> bool {
        self.exactness == ShapeExactness::Exact
    }

    pub fn is_frozen(&self) -> bool {
        self.stability == ShapeStability::Frozen
    }

    /// Canonical generic Hash view used by ordinary RBS method lookup.
    pub fn generic_hash_type(&self) -> RubyType {
        let mut keys = self
            .fields
            .iter()
            .map(|field| field.key.generic_type())
            .collect::<Vec<_>>();
        let mut values = self
            .fields
            .iter()
            .map(|field| field.value.widen_literals())
            .collect::<Vec<_>>();

        match self.rest.as_deref() {
            Some(rest) => {
                keys.push(rest.key.widen_literals());
                values.push(rest.value.widen_literals());
            }
            None if self.exactness == ShapeExactness::Open => {
                keys.push(RubyType::Unknown);
                values.push(RubyType::Unknown);
            }
            None => {}
        }

        RubyType::Hash(
            RubyType::canonical_union_members(keys),
            RubyType::canonical_union_members(values),
        )
    }

    /// Directional structural compatibility used by `RubyType::is_subtype_of`.
    pub fn is_subtype_of(&self, target: &Self) -> bool {
        for target_field in target.fields.iter() {
            let source_field = self.field(&target_field.key);
            match (source_field, target_field.presence) {
                (Some(source), ShapeFieldPresence::Required)
                    if source.presence == ShapeFieldPresence::Required
                        && source.value.is_subtype_of(&target_field.value) => {}
                (Some(source), ShapeFieldPresence::Optional)
                    if source.value.is_subtype_of(&target_field.value) => {}
                (None, ShapeFieldPresence::Optional) => match self.rest.as_deref() {
                    Some(source_rest)
                        if target_field
                            .key
                            .generic_type()
                            .is_subtype_of(&source_rest.key) =>
                    {
                        if !source_rest.value.is_subtype_of(&target_field.value) {
                            return false;
                        }
                    }
                    Some(_) => {}
                    None if self.exactness == ShapeExactness::Exact => {}
                    None => return false,
                },
                (Some(_), ShapeFieldPresence::Required | ShapeFieldPresence::Optional)
                | (None, ShapeFieldPresence::Required) => return false,
            }
        }

        if target.exactness == ShapeExactness::Exact {
            return self.exactness == ShapeExactness::Exact
                && self.rest.is_none()
                && self.fields.len() == target.fields.len();
        }

        let Some(target_rest) = target.rest.as_deref() else {
            return true;
        };

        for source_field in self
            .fields
            .iter()
            .filter(|field| target.field(&field.key).is_none())
        {
            if !source_field
                .key
                .generic_type()
                .is_subtype_of(&target_rest.key)
                || !source_field.value.is_subtype_of(&target_rest.value)
            {
                return false;
            }
        }

        match self.rest.as_deref() {
            Some(source_rest) => {
                source_rest.key.is_subtype_of(&target_rest.key)
                    && source_rest.value.is_subtype_of(&target_rest.value)
            }
            None => self.exactness == ShapeExactness::Exact,
        }
    }

    /// Rebuild the canonical shape after substituting every direct field/rest
    /// type. The same constructor rechecks bounds and proof completeness.
    pub fn try_map_types(
        &self,
        mut substitute: impl FnMut(&RubyType) -> RubyType,
    ) -> Result<Self, ShapeConstructionError> {
        let fields = self
            .fields
            .iter()
            .map(|field| ShapeField {
                key: field.key.clone(),
                value: substitute(&field.value),
                presence: field.presence,
            })
            .collect::<Vec<_>>();
        let rest = self.rest.as_deref().map(|rest| ShapeRest {
            key: substitute(&rest.key),
            value: substitute(&rest.value),
        });
        Self::try_new(fields, rest, self.exactness, self.stability)
    }

    pub(crate) fn fields_allocation_bytes(&self) -> usize {
        self.fields.len() * std::mem::size_of::<ShapeField>()
    }
}

impl Display for ShapeType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if self.stability == ShapeStability::Frozen {
            formatter.write_str("frozen ")?;
        }
        formatter.write_str("{ ")?;
        let mut needs_separator = false;
        for field in self.fields.iter() {
            if needs_separator {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{}", field.key)?;
            if field.presence == ShapeFieldPresence::Optional {
                formatter.write_str("?")?;
            }
            write!(formatter, ": {}", field.value)?;
            needs_separator = true;
        }
        if let Some(rest) = self.rest.as_deref() {
            if needs_separator {
                formatter.write_str(", ")?;
            }
            write!(formatter, "...Hash<{}, {}>", rest.key, rest.value)?;
            needs_separator = true;
        } else if self.exactness == ShapeExactness::Open {
            if needs_separator {
                formatter.write_str(", ")?;
            }
            formatter.write_str("...")?;
            needs_separator = true;
        }
        if needs_separator {
            formatter.write_str(" ")?;
        }
        formatter.write_str("}")
    }
}

pub(crate) fn shape_depth_in_type(ruby_type: &RubyType) -> usize {
    match ruby_type {
        RubyType::Shape(shape) => shape.depth(),
        RubyType::Array(types) | RubyType::Union(types) => {
            types.iter().map(shape_depth_in_type).max().unwrap_or(0)
        }
        RubyType::Hash(keys, values) => keys
            .iter()
            .chain(values.iter())
            .map(shape_depth_in_type)
            .max()
            .unwrap_or(0),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => 0,
    }
}

fn simple_symbol_key(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn write_symbol_literal(formatter: &mut Formatter<'_>, value: &str) -> fmt::Result {
    if simple_symbol_key(value) {
        write!(formatter, ":{value}")
    } else {
        formatter.write_str(":")?;
        write_quoted(formatter, value)
    }
}

fn write_quoted(formatter: &mut Formatter<'_>, value: &str) -> fmt::Result {
    write!(formatter, "{value:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(fields: impl IntoIterator<Item = ShapeField>) -> ShapeType {
        ShapeType::try_new(
            fields,
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .expect("the test shape must be valid")
    }

    #[test]
    fn construction_sorts_fields_and_rejects_duplicates() {
        let shape = exact([
            ShapeField::required(LiteralKey::symbol("name"), RubyType::string()),
            ShapeField::required(LiteralKey::symbol("age"), RubyType::integer()),
        ]);
        assert_eq!(shape.to_string(), "{ age: Integer, name: String }");
        assert_eq!(
            shape.field(&LiteralKey::symbol("age")).unwrap().value(),
            &RubyType::integer()
        );

        let duplicate = ShapeType::try_new(
            [
                ShapeField::required(LiteralKey::symbol("name"), RubyType::string()),
                ShapeField::optional(LiteralKey::symbol("name"), RubyType::string()),
            ],
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        );
        assert_eq!(
            duplicate,
            Err(ShapeConstructionError::DuplicateField(LiteralKey::symbol(
                "name"
            )))
        );
    }

    #[test]
    fn display_distinguishes_literal_optional_open_rest_and_frozen_states() {
        let shape = ShapeType::try_new(
            [
                ShapeField::required(
                    LiteralKey::symbol("kind"),
                    RubyType::Literal(Box::new(LiteralValue::symbol("number"))),
                ),
                ShapeField::optional(LiteralKey::string("display name"), RubyType::string()),
            ],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::integer())),
            ShapeExactness::Open,
            ShapeStability::Frozen,
        )
        .unwrap();
        assert_eq!(
            shape.to_string(),
            "frozen { kind: :number, \"display name\"?: String, ...Hash<Symbol, Integer> }"
        );
    }

    #[test]
    fn generic_hash_projection_widens_literals_and_open_unknowns_absorb() {
        let exact_shape = exact([
            ShapeField::required(
                LiteralKey::symbol("kind"),
                RubyType::Literal(Box::new(LiteralValue::symbol("ready"))),
            ),
            ShapeField::required(LiteralKey::symbol("value"), RubyType::integer()),
        ]);
        assert_eq!(
            exact_shape.generic_hash_type().to_string(),
            "Hash<Symbol, (Integer | Symbol)>"
        );

        let open = ShapeType::try_new(
            [ShapeField::required(
                LiteralKey::symbol("value"),
                RubyType::integer(),
            )],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        assert_eq!(open.generic_hash_type().to_string(), "Hash<?, ?>");
    }

    #[test]
    fn structural_compatibility_is_directional_and_unknown_is_not_a_wildcard() {
        let source = exact([
            ShapeField::required(LiteralKey::symbol("id"), RubyType::integer()),
            ShapeField::required(LiteralKey::symbol("name"), RubyType::string()),
        ]);
        let open_target = ShapeType::try_new(
            [ShapeField::required(
                LiteralKey::symbol("id"),
                RubyType::integer(),
            )],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        assert!(source.is_subtype_of(&open_target));
        assert!(!open_target.is_subtype_of(&source));

        let optional_string_target = ShapeType::try_new(
            [ShapeField::optional(
                LiteralKey::symbol("label"),
                RubyType::string(),
            )],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        let integer_rest_source = ShapeType::try_new(
            [],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::integer())),
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        let untyped_open_source = ShapeType::try_new(
            [],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        assert!(!integer_rest_source.is_subtype_of(&optional_string_target));
        assert!(!untyped_open_source.is_subtype_of(&optional_string_target));

        assert!(matches!(
            ShapeType::try_new(
                [ShapeField::required(
                    LiteralKey::symbol("id"),
                    RubyType::Unknown,
                )],
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            ),
            Err(ShapeConstructionError::UnprovenField(_))
        ));
    }

    #[test]
    fn fixed_field_and_depth_bounds_fail_closed() {
        let fields = (0..=MAX_SHAPE_FIELDS).map(|index| {
            ShapeField::required(
                LiteralKey::symbol(format!("k{index:02}")),
                RubyType::integer(),
            )
        });
        assert!(matches!(
            ShapeType::try_new(
                fields,
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            ),
            Err(ShapeConstructionError::FieldBoundExceeded {
                actual: 33,
                limit: MAX_SHAPE_FIELDS,
            })
        ));

        let mut nested = exact([ShapeField::required(
            LiteralKey::symbol("value"),
            RubyType::integer(),
        )]);
        for _ in 1..MAX_SHAPE_DEPTH {
            nested = exact([ShapeField::required(
                LiteralKey::symbol("value"),
                RubyType::Shape(Box::new(nested)),
            )]);
        }
        assert_eq!(nested.depth(), MAX_SHAPE_DEPTH);
        assert!(matches!(
            ShapeType::try_new(
                [ShapeField::required(
                    LiteralKey::symbol("value"),
                    RubyType::Shape(Box::new(nested)),
                )],
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            ),
            Err(ShapeConstructionError::DepthBoundExceeded {
                actual: 9,
                limit: MAX_SHAPE_DEPTH,
            })
        ));
    }

    #[test]
    fn mapping_recanonicalizes_and_rechecks_proof() {
        let shape = exact([ShapeField::required(
            LiteralKey::symbol("value"),
            RubyType::string(),
        )]);
        let mapped = shape.try_map_types(|_| RubyType::integer()).unwrap();
        assert_eq!(mapped.to_string(), "{ value: Integer }");
        assert!(matches!(
            shape.try_map_types(|_| RubyType::Unknown),
            Err(ShapeConstructionError::UnprovenField(_))
        ));
    }
}
