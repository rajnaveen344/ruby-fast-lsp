use super::constant_completion::{ConstantCompletionContext, ConstantCompletionItem};

/// Determines constant accessibility based on Ruby's scoping rules
pub struct ScopeResolver;

impl ScopeResolver {
    pub fn new() -> Self {
        Self
    }

    /// Resolve accessibility for all candidates and update their insert text
    pub fn resolve_accessibility(
        &self,
        candidates: &mut Vec<ConstantCompletionItem>,
        _context: &ConstantCompletionContext,
    ) {
        for candidate in candidates {
            // For now, all constants are considered accessible
            candidate.is_accessible = true;

            // Insert text is just the constant name
            candidate.insert_text = candidate.entry.fqn.name();
        }
    }
}

impl Default for ScopeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::entry::{entry_kind::EntryKind, Entry},
        types::{fully_qualified_name::FullyQualifiedName, scope::LVScopeId},
    };
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn create_test_context(partial_name: &str, scope_id: LVScopeId) -> ConstantCompletionContext {
        ConstantCompletionContext::new(Position::new(0, 0), scope_id, partial_name.to_string())
    }

    fn create_test_item(name: &str) -> ConstantCompletionItem {
        let entry = Entry {
            fqn: FullyQualifiedName::try_from(name).unwrap(),
            kind: EntryKind::new_class(None),
            location: Location {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Range::default(),
            },
        };

        let context = create_test_context("", 0);
        ConstantCompletionItem::new(entry, &context)
    }

    #[test]
    fn test_resolve_accessibility() {
        let resolver = ScopeResolver::new();
        let context = create_test_context("String", 0);
        let mut candidates = vec![create_test_item("String"), create_test_item("Array")];

        resolver.resolve_accessibility(&mut candidates, &context);

        // All candidates should be accessible
        assert!(candidates.iter().all(|c| c.is_accessible));

        // Insert text should be the constant name
        assert_eq!(candidates[0].insert_text, "String");
        assert_eq!(candidates[1].insert_text, "Array");
    }

    #[test]
    fn test_qualified_constant_context() {
        let context = create_test_context("Foo::Bar", 0);

        assert!(context.is_qualified);
        assert_eq!(context.partial_name, "Bar");
        assert!(context.namespace_prefix.is_some());
    }

    #[test]
    fn test_unqualified_constant_context() {
        let context = create_test_context("String", 0);

        assert!(!context.is_qualified);
        assert_eq!(context.partial_name, "String");
        assert!(context.namespace_prefix.is_none());
    }

    #[test]
    fn test_scope_resolution_operator() {
        let context = create_test_context("Foo::", 0);

        assert!(context.after_scope_resolution);
        assert_eq!(context.partial_name, "");
    }
}
