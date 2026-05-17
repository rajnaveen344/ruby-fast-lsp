use tower_lsp::lsp_types::Position;

use crate::types::{fully_qualified_name::FullyQualifiedName, scope::LVScopeId};

/// Context information for constant completion.
#[derive(Debug, Clone)]
pub struct ConstantCompletionContext {
    /// Current position in the document.
    pub position: Position,
    /// Current scope ID from analyzer.
    pub scope_id: LVScopeId,
    /// Partial constant name being typed.
    pub partial_name: String,
    /// Whether this is a qualified constant reference (contains ::).
    pub is_qualified: bool,
    /// The namespace prefix if qualified (e.g., "Foo::Bar" -> "Foo").
    pub namespace_prefix: Option<FullyQualifiedName>,
    /// Whether completion was triggered after "::".
    pub after_scope_resolution: bool,
}

impl ConstantCompletionContext {
    pub fn new(position: Position, scope_id: LVScopeId, partial_name: String) -> Self {
        let is_qualified = partial_name.contains("::");
        let (namespace_prefix, clean_partial) = if is_qualified {
            Self::parse_qualified_name(&partial_name)
        } else {
            (None, partial_name.clone())
        };

        Self {
            position,
            scope_id,
            partial_name: clean_partial,
            is_qualified,
            namespace_prefix,
            after_scope_resolution: partial_name.ends_with("::"),
        }
    }

    fn parse_qualified_name(name: &str) -> (Option<FullyQualifiedName>, String) {
        if let Some(last_scope) = name.rfind("::") {
            let namespace = &name[..last_scope];
            let partial = &name[last_scope + 2..];

            if namespace.is_empty() {
                (None, partial.to_string())
            } else if let Ok(fqn) = FullyQualifiedName::try_from(namespace) {
                (Some(fqn), partial.to_string())
            } else {
                (None, name.to_string())
            }
        } else {
            (None, name.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(partial_name: &str) -> ConstantCompletionContext {
        ConstantCompletionContext::new(Position::new(0, 0), 0, partial_name.to_string())
    }

    #[test]
    fn qualified_constant_context() {
        let context = context("Foo::Bar");

        assert!(context.is_qualified);
        assert_eq!(context.partial_name, "Bar");
        assert_eq!(
            context.namespace_prefix.map(|fqn| fqn.to_string()),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn unqualified_constant_context() {
        let context = context("String");

        assert!(!context.is_qualified);
        assert_eq!(context.partial_name, "String");
        assert!(context.namespace_prefix.is_none());
    }

    #[test]
    fn scope_resolution_context() {
        let context = context("Foo::");

        assert!(context.is_qualified);
        assert!(context.after_scope_resolution);
        assert_eq!(context.partial_name, "");
    }
}
