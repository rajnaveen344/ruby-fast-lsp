//! Type guards for control flow analysis.
//!
//! Type guards are conditions that narrow the type of a variable
//! within a specific branch of control flow.

use crate::type_inference::ruby_type::RubyType;

/// Type guard extracted from conditionals
#[derive(Debug, Clone, PartialEq)]
pub enum TypeGuard {
    /// x.is_a?(Type) or x.kind_of?(Type) or x.instance_of?(Type)
    IsA {
        variable: String,
        target_type: RubyType,
    },
    /// x.nil?
    IsNil { variable: String },
    /// !x.nil? or x (truthy check)
    NotNil { variable: String },
    /// x.respond_to?(:method)
    RespondsTo { variable: String, method: String },
    /// case x; when Type
    CaseMatch {
        variable: String,
        pattern_type: RubyType,
    },
    /// x == value or x === value (for case/when)
    Equality {
        variable: String,
        value_type: RubyType,
    },
    /// Negation of another guard
    Not(Box<TypeGuard>),
    /// Conjunction of guards (&&)
    And(Vec<TypeGuard>),
    /// Disjunction of guards (||)
    Or(Vec<TypeGuard>),
    /// A guard we couldn't analyze (preserves types)
    Unknown,
}

impl TypeGuard {
    /// Create an is_a? guard
    pub fn is_a(variable: impl Into<String>, target_type: RubyType) -> Self {
        Self::IsA {
            variable: variable.into(),
            target_type,
        }
    }

    /// Create a nil? guard
    pub fn is_nil(variable: impl Into<String>) -> Self {
        Self::IsNil {
            variable: variable.into(),
        }
    }

    /// Create a not-nil guard (truthy check)
    pub fn not_nil(variable: impl Into<String>) -> Self {
        Self::NotNil {
            variable: variable.into(),
        }
    }

    /// Create a respond_to? guard
    pub fn responds_to(variable: impl Into<String>, method: impl Into<String>) -> Self {
        Self::RespondsTo {
            variable: variable.into(),
            method: method.into(),
        }
    }

    /// Create a case/when pattern match guard
    pub fn case_match(variable: impl Into<String>, pattern_type: RubyType) -> Self {
        Self::CaseMatch {
            variable: variable.into(),
            pattern_type,
        }
    }

    /// Create a conjunction of guards
    pub fn and(guards: Vec<TypeGuard>) -> Self {
        // Flatten nested Ands
        let flattened: Vec<TypeGuard> = guards
            .into_iter()
            .flat_map(|g| match g {
                TypeGuard::And(inner) => inner,
                other => vec![other],
            })
            .collect();

        match flattened.len() {
            0 => TypeGuard::Unknown,
            1 => flattened.into_iter().next().unwrap(),
            _ => TypeGuard::And(flattened),
        }
    }

    /// Create a disjunction of guards
    pub fn or(guards: Vec<TypeGuard>) -> Self {
        // Flatten nested Ors
        let flattened: Vec<TypeGuard> = guards
            .into_iter()
            .flat_map(|g| match g {
                TypeGuard::Or(inner) => inner,
                other => vec![other],
            })
            .collect();

        match flattened.len() {
            0 => TypeGuard::Unknown,
            1 => flattened.into_iter().next().unwrap(),
            _ => TypeGuard::Or(flattened),
        }
    }

    /// Get the inverse of this guard (for else branches)
    pub fn negate(&self) -> TypeGuard {
        match self {
            TypeGuard::Not(inner) => (**inner).clone(),
            TypeGuard::IsNil { variable } => TypeGuard::NotNil {
                variable: variable.clone(),
            },
            TypeGuard::NotNil { variable } => TypeGuard::IsNil {
                variable: variable.clone(),
            },
            TypeGuard::And(guards) => {
                // De Morgan: !(A && B) = !A || !B
                TypeGuard::Or(guards.iter().map(|g| g.negate()).collect())
            }
            TypeGuard::Or(guards) => {
                // De Morgan: !(A || B) = !A && !B
                TypeGuard::And(guards.iter().map(|g| g.negate()).collect())
            }
            TypeGuard::Unknown => TypeGuard::Unknown,
            other => TypeGuard::Not(Box::new(other.clone())),
        }
    }

    /// Get the variable(s) affected by this guard
    pub fn affected_variables(&self) -> Vec<&str> {
        match self {
            TypeGuard::IsA { variable, .. }
            | TypeGuard::IsNil { variable }
            | TypeGuard::NotNil { variable }
            | TypeGuard::RespondsTo { variable, .. }
            | TypeGuard::CaseMatch { variable, .. }
            | TypeGuard::Equality { variable, .. } => vec![variable.as_str()],
            TypeGuard::Not(inner) => inner.affected_variables(),
            TypeGuard::And(guards) | TypeGuard::Or(guards) => {
                guards.iter().flat_map(|g| g.affected_variables()).collect()
            }
            TypeGuard::Unknown => vec![],
        }
    }

    /// Check if this guard affects a specific variable
    pub fn affects_variable(&self, var_name: &str) -> bool {
        self.affected_variables().contains(&var_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_negate() {
        let guard = TypeGuard::is_nil("x");
        let negated = guard.negate();
        assert_eq!(
            negated,
            TypeGuard::NotNil {
                variable: "x".to_string()
            }
        );

        // Double negation
        let double_negated = negated.negate();
        assert_eq!(double_negated, guard);
    }

    #[test]
    fn test_guard_negate_is_a() {
        let guard = TypeGuard::is_a("x", RubyType::string());
        let negated = guard.negate();

        match negated {
            TypeGuard::Not(inner) => {
                assert_eq!(
                    *inner,
                    TypeGuard::IsA {
                        variable: "x".to_string(),
                        target_type: RubyType::string()
                    }
                );
            }
            _ => panic!("Expected Not variant"),
        }
    }

    #[test]
    fn test_guard_de_morgan() {
        // !(A && B) = !A || !B
        let guard = TypeGuard::And(vec![TypeGuard::is_nil("x"), TypeGuard::is_nil("y")]);

        let negated = guard.negate();

        match negated {
            TypeGuard::Or(guards) => {
                assert_eq!(guards.len(), 2);
                assert_eq!(
                    guards[0],
                    TypeGuard::NotNil {
                        variable: "x".to_string()
                    }
                );
                assert_eq!(
                    guards[1],
                    TypeGuard::NotNil {
                        variable: "y".to_string()
                    }
                );
            }
            _ => panic!("Expected Or variant"),
        }
    }

    #[test]
    fn test_affected_variables() {
        let guard = TypeGuard::And(vec![
            TypeGuard::is_nil("x"),
            TypeGuard::is_a("y", RubyType::string()),
        ]);

        let vars = guard.affected_variables();
        assert!(vars.contains(&"x"));
        assert!(vars.contains(&"y"));
    }

    #[test]
    fn test_flatten_and() {
        let guard = TypeGuard::and(vec![
            TypeGuard::And(vec![TypeGuard::is_nil("x"), TypeGuard::is_nil("y")]),
            TypeGuard::is_nil("z"),
        ]);

        match guard {
            TypeGuard::And(guards) => {
                assert_eq!(guards.len(), 3);
            }
            _ => panic!("Expected And variant"),
        }
    }
}
