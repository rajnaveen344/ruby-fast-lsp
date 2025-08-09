use crate::types::{
    fully_qualified_name::FullyQualifiedName,
    scope::{LVScope as Scope},
};

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
        context: &ConstantCompletionContext,
    ) {
        for candidate in candidates {
            candidate.is_accessible = self.is_accessible(&candidate.entry.fqn, context);

            // Update insert text based on accessibility and scope
            candidate.insert_text = self.calculate_insert_text(&candidate.entry.fqn, context);
        }
    }

    fn is_accessible(
        &self,
        constant_fqn: &FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> bool {
        // For qualified references, check if the namespace exists and is accessible
        if let Some(namespace_prefix) = &context.namespace_prefix {
            return self.is_namespace_accessible(namespace_prefix, context);
        }

        // For unqualified references, check if constant is accessible from current scope
        self.is_constant_accessible_from_scope(constant_fqn, &context.scope_stack)
    }

    fn is_namespace_accessible(
        &self,
        namespace: &FullyQualifiedName,
        _context: &ConstantCompletionContext,
    ) -> bool {
        // Check if the namespace is accessible from current scope
        // This involves checking if the namespace is defined and reachable
        // For now, allow all namespaces (will be refined)
        true
    }

    fn is_constant_accessible_from_scope(
        &self,
        _constant_fqn: &FullyQualifiedName,
        _scope_stack: &[Scope],
    ) -> bool {
        // Ruby constant lookup rules:
        // 1. Current scope and its ancestors
        // 2. Included/extended modules
        // 3. Top-level constants

        // For now, allow all constants (will be refined)
        true
    }

    fn calculate_insert_text(
        &self,
        constant_fqn: &FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> String {
        if context.is_qualified {
            // For qualified names, just insert the final component
            constant_fqn.name()
        } else {
            // For now, just return the constant name without qualification
            // TODO: Implement proper qualification logic based on scope analysis
            constant_fqn.name()
        }
    }

    fn get_current_namespace(&self, _scope_stack: &[Scope]) -> Option<FullyQualifiedName> {
        // Extract current namespace from scope stack
        // This would analyze the scope stack to determine the current namespace context
        // For now, return None (simplified implementation)
        None
    }

    fn needs_qualification(
        &self,
        _constant_fqn: &FullyQualifiedName,
        _current_namespace: &Option<FullyQualifiedName>,
    ) -> bool {
        // Determine if we need to qualify the constant name
        // This would check if the constant is accessible without qualification
        // For now, return false (simplified implementation)
        false
    }

    /// Check if a constant is accessible from the top level
    pub fn is_top_level_accessible(&self, constant_fqn: &FullyQualifiedName) -> bool {
        // Top-level constants are always accessible with full qualification
        !constant_fqn.is_empty()
    }

    /// Get the minimal qualification needed for a constant
    pub fn get_minimal_qualification(
        &self,
        constant_fqn: &FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> String {
        // Simplified implementation - just return the constant name
        constant_fqn.name()
    }

    fn is_top_level_constant(&self, constant_fqn: &FullyQualifiedName) -> bool {
        // Check if this is a top-level constant (no namespace)
        constant_fqn.namespace_parts().len() == 1
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
    use crate::types::{
        ruby_namespace::RubyConstant,
        scope::{LVScope, LVScopeKind},
    };
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn create_test_context(
        partial_name: &str,
        scope_stack: Vec<LVScope>,
    ) -> ConstantCompletionContext {
        ConstantCompletionContext::new(Position::new(0, 0), scope_stack, partial_name.to_string())
    }

    #[test]
    fn test_top_level_accessibility() {
        let resolver = ScopeResolver::new();
        let fqn = FullyQualifiedName::try_from("String").unwrap();

        assert!(resolver.is_top_level_accessible(&fqn));
    }

    #[test]
    fn test_qualified_constant_context() {
        let resolver = ScopeResolver::new();
        let context = create_test_context("Foo::Bar", vec![]);

        assert!(context.is_qualified);
        assert_eq!(context.partial_name, "Bar");
        assert!(context.namespace_prefix.is_some());
    }

    #[test]
    fn test_unqualified_constant_context() {
        let resolver = ScopeResolver::new();
        let context = create_test_context("String", vec![]);

        assert!(!context.is_qualified);
        assert_eq!(context.partial_name, "String");
        assert!(context.namespace_prefix.is_none());
    }

    #[test]
    fn test_scope_resolution_operator() {
        let resolver = ScopeResolver::new();
        let context = create_test_context("Foo::", vec![]);

        assert!(context.after_scope_resolution);
        assert_eq!(context.partial_name, "");
    }

    #[test]
    fn test_minimal_qualification() {
        let resolver = ScopeResolver::new();
        let fqn = FullyQualifiedName::try_from("Foo::Bar::Baz").unwrap();
        let context = create_test_context("Baz", vec![]);

        let qualification = resolver.get_minimal_qualification(&fqn, &context);
        // Since we're at top level, we might need full qualification
        assert!(!qualification.is_empty());
    }
}
