use crate::core::{
    FullyQualifiedName, LiteralKey, LiteralValue, ShapeConstructionError, ShapeExactness,
    ShapeField, ShapeStability, ShapeType, UnknownReason,
};
use crate::r#type::ruby::RubyType;
use ruby_prism::*;
use std::collections::BTreeMap;

/// Analyzes Ruby literals and determines their types
pub struct LiteralAnalyzer;

impl Default for LiteralAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl LiteralAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a node and return its inferred type if it's a literal
    pub fn analyze_literal(&self, node: &Node) -> Option<RubyType> {
        // Handle ProgramNode and StatementsNode by analyzing their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_literal(&first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.analyze_literal(&first_stmt);
            }
            return None;
        }

        // String literals
        if node.as_string_node().is_some()
            || node.as_interpolated_string_node().is_some()
            || node.as_x_string_node().is_some()
            || node.as_interpolated_x_string_node().is_some()
        {
            return Some(RubyType::string());
        }

        // Numeric literals
        if node.as_integer_node().is_some() {
            return Some(RubyType::integer());
        }
        if node.as_float_node().is_some() {
            return Some(RubyType::float());
        }
        if node.as_rational_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Rational").unwrap(),
            ));
        }
        if node.as_imaginary_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Complex").unwrap(),
            ));
        }

        // Symbol literals
        if node.as_symbol_node().is_some() || node.as_interpolated_symbol_node().is_some() {
            return Some(RubyType::symbol());
        }

        // Boolean and nil literals
        if node.as_true_node().is_some() {
            return Some(RubyType::true_class());
        }
        if node.as_false_node().is_some() {
            return Some(RubyType::false_class());
        }
        if node.as_nil_node().is_some() {
            return Some(RubyType::nil_class());
        }

        // Regular expression literals
        if node.as_regular_expression_node().is_some()
            || node.as_interpolated_regular_expression_node().is_some()
        {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Regexp").unwrap(),
            ));
        }

        // Array literals - analyze element types
        if let Some(array_node) = node.as_array_node() {
            return Some(self.analyze_array_literal(&array_node));
        }

        // Hash literals - analyze key and value types
        if let Some(hash_node) = node.as_hash_node() {
            return Some(self.analyze_hash_literal(&hash_node));
        }

        // Range literals - simplified to just return Range class
        if node.as_range_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Range").unwrap(),
            ));
        }

        // Proc/lambda literals
        if node.as_lambda_node().is_some() {
            return Some(RubyType::Class(
                FullyQualifiedName::try_from("Proc").unwrap(),
            ));
        }

        // NOTE: Self is intentionally NOT handled here. It's not a literal -
        // its type depends on the class/module context and is resolved in
        // MethodResolver::resolve_receiver_type with proper namespace context.

        // Handle array indexing: array[index] where array is a literal
        // This handles cases like ["a", "b", "c"][0]
        if let Some(call_node) = node.as_call_node() {
            return self.analyze_array_access(&call_node);
        }

        // Other nodes are not literals
        None
    }

    /// Analyze array access expressions like `array[0]` or `["a", "b"][1]`
    fn analyze_array_access(&self, call_node: &CallNode) -> Option<RubyType> {
        // Check if this is a `[]` method call
        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        if method_name != "[]" {
            return None;
        }

        // Get the receiver (the array)
        let receiver = call_node.receiver()?;

        // Analyze the receiver - must be an array literal or array-like expression
        let receiver_type = self.analyze_literal(&receiver)?;

        // If the receiver is an array, return one of its element types
        match receiver_type {
            RubyType::Array(element_types) => {
                // If all elements have the same type, return that type
                // Otherwise return a union of element types
                if element_types.len() == 1 {
                    Some(element_types.into_iter().next().unwrap())
                } else if element_types.is_empty() {
                    // Empty array - element could be nil
                    Some(RubyType::nil_class())
                } else {
                    // Multiple element types - could be any of them, union with nil
                    // (since the index might be out of bounds)
                    Some(RubyType::union(element_types))
                }
            }
            RubyType::Hash(_, value_types) => {
                // Hash access - return value type(s)
                if value_types.len() == 1 {
                    // Hash access can return nil if key not found
                    Some(
                        value_types
                            .into_iter()
                            .next()
                            .unwrap()
                            .union_with(&RubyType::nil_class()),
                    )
                } else if value_types.is_empty() {
                    Some(RubyType::nil_class())
                } else {
                    Some(RubyType::union(value_types).union_with(&RubyType::nil_class()))
                }
            }
            _ => None,
        }
    }

    /// Analyze an array literal and infer element types
    fn analyze_array_literal(&self, array_node: &ArrayNode) -> RubyType {
        infer_array_literal_type(array_node, |node| {
            self.analyze_literal(node).unwrap_or(RubyType::Unknown)
        })
    }

    /// Analyze a hash literal and infer key and value types
    fn analyze_hash_literal(&self, hash_node: &HashNode) -> RubyType {
        infer_hash_literal_type(hash_node, |node| {
            self.analyze_literal(node).unwrap_or(RubyType::Unknown)
        })
        .unwrap_or(RubyType::Unknown)
    }

    /// Check if a node represents a literal value
    pub fn is_literal(&self, node: &Node) -> bool {
        // Handle ProgramNode and StatementsNode by checking their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.is_literal(&first_stmt);
            }
            return false;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return self.is_literal(&first_stmt);
            }
            return false;
        }

        self.analyze_literal(node).is_some()
    }

    /// Get the literal value as a string if possible
    pub fn get_literal_value(&self, node: &Node) -> Option<String> {
        // Handle ProgramNode and StatementsNode by analyzing their first statement
        if let Some(program_node) = node.as_program_node() {
            let statements_node = program_node.statements();
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return get_literal_value(first_stmt);
            }
            return None;
        }

        if let Some(statements_node) = node.as_statements_node() {
            if let Some(first_stmt) = statements_node.body().iter().next() {
                return get_literal_value(first_stmt);
            }
            return None;
        }

        if let Some(string_node) = node.as_string_node() {
            return Some(String::from_utf8_lossy(string_node.unescaped()).to_string());
        }

        if let Some(integer_node) = node.as_integer_node() {
            return Some(format!("{:?}", integer_node.value()));
        }

        if let Some(float_node) = node.as_float_node() {
            return Some(format!("{:?}", float_node.value()));
        }

        if node.as_true_node().is_some() {
            return Some("true".to_string());
        }

        if node.as_false_node().is_some() {
            return Some("false".to_string());
        }

        if node.as_nil_node().is_some() {
            return Some("nil".to_string());
        }

        None
    }
}

/// Infer one Array literal through a caller-owned element resolver.
///
/// Keeping this traversal shared lets local-flow inference recursively retain
/// Hash shapes nested in arrays without introducing a second literal policy.
pub(crate) fn infer_array_literal_type(
    array_node: &ArrayNode<'_>,
    mut infer_value: impl FnMut(&Node<'_>) -> RubyType,
) -> RubyType {
    infer_array_literal_type_fallible(array_node, |element| {
        Ok::<RubyType, ShapeConstructionError>(infer_value(element))
    })
    .expect(
        "INVARIANT VIOLATED: infallible Array literal inference returned a shape construction error. This is a bug because the wrapper converts every element to Ok. Fix: keep error creation inside the caller-provided fallible resolver.",
    )
}

/// Fallible Array literal inference used when nested shape-bound failures must
/// remain proof-carrying instead of being flattened into an Unknown element.
pub(crate) fn infer_array_literal_type_fallible(
    array_node: &ArrayNode<'_>,
    mut infer_value: impl FnMut(&Node<'_>) -> Result<RubyType, ShapeConstructionError>,
) -> Result<RubyType, ShapeConstructionError> {
    let element_types = array_node
        .elements()
        .iter()
        .map(|element| infer_value(&element))
        .collect::<Result<Vec<_>, _>>()?;

    if element_types.is_empty() {
        Ok(RubyType::Array(vec![RubyType::Unknown]))
    } else {
        Ok(RubyType::Array(RubyType::canonical_union_members(
            element_types,
        )))
    }
}

/// Project a Shape to its generic Hash view only when this exact call
/// expression constructs the receiver.
///
/// An immediate literal has no pre-existing alias or escape. Shapes reached
/// through locals or other expressions stay precise and fail closed at method
/// boundaries until mutable identity tracking is available.
pub(crate) fn project_immediate_hash_receiver_type(
    receiver: &Node<'_>,
    inferred: RubyType,
) -> RubyType {
    match inferred {
        RubyType::Shape(shape) if receiver.as_hash_node().is_some() => shape.generic_hash_type(),
        inferred => inferred,
    }
}

/// Infer one Hash literal through a caller-owned value resolver.
///
/// Fully proven Symbol/String-keyed literals become exact canonical shapes.
/// Unsupported keys, incomplete values, and unknown splats retain only the
/// exhaustive generic Hash projection. Fixed shape bounds are returned as an
/// error so proof-carrying callers can attach `shape_bound_exceeded` rather
/// than publishing a partial prefix.
pub(crate) fn infer_hash_literal_type(
    hash_node: &HashNode<'_>,
    mut infer_value: impl FnMut(&Node<'_>) -> RubyType,
) -> Result<RubyType, ShapeConstructionError> {
    infer_hash_literal_type_fallible(hash_node, |value| {
        Ok::<RubyType, ShapeConstructionError>(infer_value(value))
    })
}

/// Fallible Hash literal inference used by proof-carrying callers. A nested
/// field/depth failure aborts the whole enclosing collection, so no partial
/// outer shape or generic Hash can conceal the exceeded bound.
pub(crate) fn infer_hash_literal_type_fallible(
    hash_node: &HashNode<'_>,
    mut infer_value: impl FnMut(&Node<'_>) -> Result<RubyType, ShapeConstructionError>,
) -> Result<RubyType, ShapeConstructionError> {
    let mut fields = BTreeMap::<LiteralKey, ShapeField>::new();
    let mut key_types = Vec::new();
    let mut value_types = Vec::new();
    let mut shape_complete = true;

    for element in hash_node.elements().iter() {
        if let Some(assoc) = element.as_assoc_node() {
            let key_node = assoc.key();
            let generic_key_type = infer_value(&key_node)?.widen_literals();
            key_types.push(generic_key_type);

            let value_node = assoc.value();
            let value_type = match literal_shape_value(&value_node) {
                Some(literal) => literal,
                None => infer_value(&value_node)?,
            };
            value_types.push(value_type.widen_literals());

            match literal_key(&key_node) {
                Some(key) if !RubyType::contains_unknown(&value_type) => {
                    fields.insert(key.clone(), ShapeField::required(key, value_type));
                }
                Some(_) | None => shape_complete = false,
            }
            continue;
        }

        if let Some(splat) = element.as_assoc_splat_node() {
            let splat_type = splat
                .value()
                .map(|value| infer_value(&value))
                .transpose()?
                .unwrap_or(RubyType::Unknown);
            match splat_type {
                RubyType::Shape(shape) if shape.is_exact() && shape.rest().is_none() => {
                    for field in shape.fields() {
                        fields.insert(field.key().clone(), field.clone());
                    }
                    let RubyType::Hash(keys, values) = shape.generic_hash_type() else {
                        panic!(
                            "INVARIANT VIOLATED: ShapeType::generic_hash_type did not return RubyType::Hash. This is a bug because literal splat inference relies on the canonical Hash projection. Fix: keep ShapeType projection exhaustive."
                        );
                    };
                    key_types.extend(keys);
                    value_types.extend(values);
                }
                RubyType::Hash(keys, values) => {
                    key_types.extend(keys);
                    value_types.extend(values);
                    shape_complete = false;
                }
                RubyType::Shape(shape) => {
                    let RubyType::Hash(keys, values) = shape.generic_hash_type() else {
                        panic!(
                            "INVARIANT VIOLATED: ShapeType::generic_hash_type did not return RubyType::Hash. This is a bug because literal splat inference relies on the canonical Hash projection. Fix: keep ShapeType projection exhaustive."
                        );
                    };
                    key_types.extend(keys);
                    value_types.extend(values);
                    shape_complete = false;
                }
                RubyType::Class(_)
                | RubyType::Module(_)
                | RubyType::ClassReference(_)
                | RubyType::ModuleReference(_)
                | RubyType::Literal(_)
                | RubyType::Array(_)
                | RubyType::Union(_)
                | RubyType::Unknown => {
                    key_types.push(RubyType::Unknown);
                    value_types.push(RubyType::Unknown);
                    shape_complete = false;
                }
            }
            continue;
        }

        key_types.push(RubyType::Unknown);
        value_types.push(RubyType::Unknown);
        shape_complete = false;
    }

    if shape_complete {
        return ShapeType::try_new(
            fields.into_values(),
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .map(|shape| RubyType::Shape(Box::new(shape)));
    }

    Ok(RubyType::Hash(
        RubyType::canonical_union_members(key_types),
        RubyType::canonical_union_members(value_types),
    ))
}

pub(crate) fn literal_key(node: &Node<'_>) -> Option<LiteralKey> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(LiteralKey::symbol(
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
        ));
    }
    node.as_string_node()
        .map(|string| LiteralKey::string(String::from_utf8_lossy(string.unescaped()).to_string()))
}

/// Convert the only construction failures reachable from canonical literal
/// inference into the stable proof-failure reason exposed by the engine.
pub(crate) fn literal_shape_construction_unknown_reason(
    error: ShapeConstructionError,
) -> UnknownReason {
    match error {
        ShapeConstructionError::FieldBoundExceeded { .. }
        | ShapeConstructionError::DepthBoundExceeded { .. } => {
            UnknownReason::ShapeBoundExceeded
        }
        ShapeConstructionError::DuplicateField(key) => panic!(
            "INVARIANT VIOLATED: canonical Hash literal inference produced duplicate field `{key}`. This is a bug because Ruby overwrite order is resolved in a BTreeMap before ShapeType construction. Fix: canonicalize every literal field before constructing the shape."
        ),
        ShapeConstructionError::ExactShapeHasRest => panic!(
            "INVARIANT VIOLATED: exact Hash literal inference produced a rest contract. This is a bug because a complete literal has no unlisted key contract. Fix: pass no rest type when constructing exact literal shapes."
        ),
        ShapeConstructionError::UnprovenField(key) => panic!(
            "INVARIANT VIOLATED: complete Hash literal inference retained unproven field `{key}`. This is a bug because any Unknown value must select the generic incomplete-Hash path before ShapeType construction. Fix: reject incomplete field evidence before constructing the shape."
        ),
        ShapeConstructionError::UnprovenRest => panic!(
            "INVARIANT VIOLATED: exact Hash literal inference retained an unproven rest contract. This is a bug because literal shapes are constructed without a rest contract. Fix: keep generic splat evidence out of exact ShapeType construction."
        ),
    }
}

fn literal_shape_value(node: &Node<'_>) -> Option<RubyType> {
    node.as_symbol_node().map(|symbol| {
        RubyType::Literal(Box::new(LiteralValue::symbol(
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
        )))
    })
}

fn get_literal_value(node: Node<'_>) -> Option<String> {
    LiteralAnalyzer::new().get_literal_value(&node)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_with_code<F>(source: &str, test_fn: F)
    where
        F: FnOnce(&LiteralAnalyzer, &Node),
    {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let analyzer = LiteralAnalyzer::new();

        if let Some(statements_node) = ast.as_statements_node() {
            if let Some(first_node) = statements_node.body().iter().next() {
                test_fn(&analyzer, &first_node);
            }
        } else {
            test_fn(&analyzer, &ast);
        }
    }

    #[test]
    fn test_string_literal() {
        test_with_code("\"hello\"", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::string());

            let value = analyzer.get_literal_value(node).unwrap();
            assert_eq!(value, "hello");
        });
    }

    #[test]
    fn test_integer_literal() {
        test_with_code("42", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::integer());
        });
    }

    #[test]
    fn test_float_literal() {
        test_with_code("3.14", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::float());
        });
    }

    #[test]
    fn test_boolean_literals() {
        test_with_code("true", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let true_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(true_type, RubyType::true_class());
        });

        test_with_code("false", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let false_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(false_type, RubyType::false_class());
        });
    }

    #[test]
    fn test_nil_literal() {
        test_with_code("nil", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(ruby_type, RubyType::nil_class());

            let value = analyzer.get_literal_value(node).unwrap();
            assert_eq!(value, "nil");
        });
    }

    #[test]
    fn test_array_literal() {
        test_with_code("[1, 2, 3]", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            assert_eq!(
                ruby_type,
                RubyType::Array(vec![RubyType::Class(
                    FullyQualifiedName::try_from("Integer").unwrap()
                )])
            );
        });
    }

    #[test]
    fn test_array_with_unresolved_element_has_unknown_element_type() {
        test_with_code("[1, dynamic_value]", |analyzer, node| {
            assert_eq!(
                analyzer.analyze_literal(node),
                Some(RubyType::Array(vec![RubyType::Unknown]))
            );
        });
    }

    #[test]
    fn test_hash_literal() {
        test_with_code("{a: 1, b: 2}", |analyzer, node| {
            assert!(analyzer.is_literal(node));
            let ruby_type = analyzer.analyze_literal(node).unwrap();
            let RubyType::Shape(shape) = ruby_type else {
                panic!(
                    "INVARIANT VIOLATED: a complete Symbol-keyed Hash literal did not infer an exact shape. This is a bug because Phase 2 requires local literal construction to preserve fields. Fix: keep infer_hash_literal_type on the complete path."
                );
            };
            assert_eq!(shape.to_string(), "{ a: Integer, b: Integer }");
            assert_eq!(shape.stability(), ShapeStability::TrackedMutable);
        });
    }

    #[test]
    fn nested_hash_literal_retains_exact_shapes() {
        test_with_code("{user: {name: \"Ada\", age: 42}}", |analyzer, node| {
            assert_eq!(
                analyzer.analyze_literal(node).unwrap().to_string(),
                "{ user: { age: Integer, name: String } }"
            );
        });
    }

    #[test]
    fn exact_hash_splat_uses_ruby_overwrite_order() {
        test_with_code(
            "{before: 1, **{after: 2, before: \"ready\"}}",
            |analyzer, node| {
                assert_eq!(
                    analyzer.analyze_literal(node).unwrap().to_string(),
                    "{ after: Integer, before: String }"
                );
            },
        );
    }

    #[test]
    fn test_hash_with_unresolved_splat_has_unknown_key_and_value_types() {
        test_with_code("{known: 1, **dynamic_hash}", |analyzer, node| {
            assert_eq!(
                analyzer.analyze_literal(node),
                Some(RubyType::Hash(
                    vec![RubyType::Unknown],
                    vec![RubyType::Unknown]
                ))
            );
        });
    }

    #[test]
    fn test_non_literal() {
        test_with_code("variable_name", |analyzer, node| {
            assert!(!analyzer.is_literal(node));
            assert!(analyzer.analyze_literal(node).is_none());
        });
    }
}
