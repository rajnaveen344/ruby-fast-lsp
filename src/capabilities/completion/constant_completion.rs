use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation, Position,
};

use crate::{
    analyzer_prism::RubyPrismAnalyzer,
    indexer::{entry::entry_kind::EntryKind, entry::Entry, index::RubyIndex},
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant,
        scope::LVScope as Scope,
    },
};

use super::{
    completion_ranker::CompletionRanker, constant_matcher::ConstantMatcher,
    scope_resolver::ScopeResolver,
};

/// Context information for constant completion
#[derive(Debug, Clone)]
pub struct ConstantCompletionContext {
    /// Current position in the document
    pub position: Position,
    /// Current scope stack from analyzer
    pub scope_stack: Vec<Scope>,
    /// Partial constant name being typed
    pub partial_name: String,
    /// Whether this is a qualified constant reference (contains ::)
    pub is_qualified: bool,
    /// The namespace prefix if qualified (e.g., "Foo::Bar" -> "Foo")
    pub namespace_prefix: Option<FullyQualifiedName>,
    /// Whether completion was triggered after "::"
    pub after_scope_resolution: bool,
}

impl ConstantCompletionContext {
    pub fn new(position: Position, scope_stack: Vec<Scope>, partial_name: String) -> Self {
        let is_qualified = partial_name.contains("::");
        let (namespace_prefix, clean_partial) = if is_qualified {
            Self::parse_qualified_name(&partial_name)
        } else {
            (None, partial_name.clone())
        };

        Self {
            position,
            scope_stack,
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

            // Special case: if namespace is empty, this is top-level scope resolution (::)
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

/// A completion item for a Ruby constant
#[derive(Debug, Clone)]
pub struct ConstantCompletionItem {
    /// The constant name to complete
    pub name: String,
    /// The fully qualified name
    pub fqn: FullyQualifiedName,
    /// The entry from RubyIndex
    pub entry: Entry,
    /// Relevance score for ranking
    pub relevance_score: f64,
    /// Whether this constant is accessible from current scope
    pub is_accessible: bool,
    /// The completion text to insert
    pub insert_text: String,
    /// Additional details for display
    pub detail: Option<String>,
    /// Documentation if available
    pub documentation: Option<String>,
}

impl ConstantCompletionItem {
    pub fn new(entry: Entry, context: &ConstantCompletionContext) -> Self {
        let name = Self::extract_constant_name(&entry);
        let fqn = entry.fqn.clone();
        let insert_text = Self::calculate_insert_text(&entry, context);
        let detail = Self::extract_detail(&entry);
        let relevance_score = Self::calculate_relevance(&entry, context);

        Self {
            name,
            fqn,
            entry,
            relevance_score,
            is_accessible: true, // Will be calculated by scope resolver
            insert_text,
            detail,
            documentation: None, // Will be populated if available
        }
    }

    fn extract_constant_name(entry: &Entry) -> String {
        match &entry.kind {
            EntryKind::Class { .. } | EntryKind::Module { .. } | EntryKind::Constant { .. } => {
                entry.fqn.name()
            }
            _ => entry.fqn.to_string(),
        }
    }

    fn calculate_insert_text(entry: &Entry, context: &ConstantCompletionContext) -> String {
        if context.is_qualified || context.namespace_prefix.is_some() {
            // For qualified names, just insert the final component
            entry.fqn.name()
        } else {
            // For unqualified names, might need full path depending on scope
            // This will be refined by the scope resolver
            entry.fqn.name()
        }
    }

    fn extract_detail(entry: &Entry) -> Option<String> {
        match &entry.kind {
            EntryKind::Class { superclass, .. } => {
                if let Some(_parent) = superclass {
                    // TODO: Resolve MixinRef to display superclass name
                    Some(" class".to_string())
                } else {
                    Some(" class".to_string())
                }
            }
            EntryKind::Module { .. } => Some(" module".to_string()),
            EntryKind::Constant { value, .. } => {
                if let Some(val) = value {
                    Some(format!(" = {}", val))
                } else {
                    Some(" constant".to_string())
                }
            }
            _ => None,
        }
    }

    fn calculate_relevance(entry: &Entry, context: &ConstantCompletionContext) -> f64 {
        let mut score = 0.0;

        // Base score by entry type
        score += match &entry.kind {
            EntryKind::Class { .. } => 1.0,
            EntryKind::Module { .. } => 0.9,
            EntryKind::Constant { .. } => 0.8,
            _ => 0.0,
        };

        // Boost for exact prefix matches
        let name = Self::extract_constant_name(entry);
        if name.starts_with(&context.partial_name) {
            score += 2.0;
        }

        // Boost for same namespace
        // This will be refined by the scope resolver

        score
    }

    pub fn to_completion_item(&self) -> CompletionItem {
        let kind = match &self.entry.kind {
            EntryKind::Class { .. } => CompletionItemKind::CLASS,
            EntryKind::Module { .. } => CompletionItemKind::MODULE,
            EntryKind::Constant { .. } => CompletionItemKind::CONSTANT,
            _ => CompletionItemKind::VALUE,
        };

        CompletionItem {
            label: self.name.clone(),
            label_details: Some(CompletionItemLabelDetails {
                detail: self.detail.clone(),
                description: Some(self.fqn.to_string()),
            }),
            kind: Some(kind),
            detail: self.detail.clone(),
            documentation: self
                .documentation
                .as_ref()
                .map(|doc| Documentation::String(doc.clone())),
            insert_text: Some(self.insert_text.clone()),
            ..Default::default()
        }
    }
}

/// Main engine for constant completion
pub struct ConstantCompletionEngine {
    matcher: ConstantMatcher,
    scope_resolver: ScopeResolver,
    ranker: CompletionRanker,
}

impl ConstantCompletionEngine {
    pub fn new() -> Self {
        Self {
            matcher: ConstantMatcher::new(),
            scope_resolver: ScopeResolver::new(),
            ranker: CompletionRanker::new(),
        }
    }

    pub fn complete_constants(
        &self,
        index: &RubyIndex,
        analyzer: &RubyPrismAnalyzer,
        position: Position,
        partial_name: String,
    ) -> Vec<CompletionItem> {
        // Get scope context from analyzer
        let (_, _, scope_stack) = analyzer.get_identifier(position);

        // Create completion context
        let context = ConstantCompletionContext::new(position, scope_stack, partial_name);

        // Find matching constants
        let mut candidates = self.find_constant_candidates(index, &context);

        // Resolve scope accessibility
        self.scope_resolver
            .resolve_accessibility(&mut candidates, &context);

        // Filter out inaccessible constants
        candidates.retain(|item| item.is_accessible);

        // Rank by relevance
        self.ranker.rank_by_relevance(&mut candidates, &context);

        // Convert to LSP completion items
        candidates
            .into_iter()
            .take(50) // Limit results
            .map(|item| item.to_completion_item())
            .collect()
    }

    fn find_constant_candidates(
        &self,
        index: &RubyIndex,
        context: &ConstantCompletionContext,
    ) -> Vec<ConstantCompletionItem> {
        let mut candidates = Vec::new();
        let mut seen_fqns = std::collections::HashSet::new();

        // Search through all definitions in the index
        for (fqn, entries) in &index.definitions {
            // Skip if we've already processed this FQN
            if seen_fqns.contains(fqn) {
                continue;
            }

            // Find the best entry for this FQN (prefer the first constant-like entry)
            let best_entry = entries.iter().find(|entry| self.is_constant_entry(entry));

            if let Some(entry) = best_entry {
                // Handle qualified completion (e.g., "ActiveRecord::" or "ActiveRecord::B")
                if context.is_qualified {
                    if let Some(ref namespace_prefix) = context.namespace_prefix {
                        // Check if this constant is a direct child of the namespace
                        // We need to check both the direct namespace and the Object-prefixed version
                        let is_direct_child =
                            self.is_direct_child_of_namespace(fqn, namespace_prefix);

                        // Also check if the namespace might be under Object
                        let is_object_prefixed_child =
                            if namespace_prefix.namespace_parts().len() == 1 {
                                // Try creating Object::namespace_prefix and check if fqn is a child of that
                                let object_prefixed_namespace = {
                                    let mut parts = vec![RubyConstant::new("Object").unwrap()];
                                    parts.extend(namespace_prefix.namespace_parts());
                                    FullyQualifiedName::Constant(parts)
                                };
                                self.is_direct_child_of_namespace(fqn, &object_prefixed_namespace)
                            } else {
                                false
                            };

                        if !is_direct_child && !is_object_prefixed_child {
                            continue;
                        }
                    } else {
                        // Handle the "::" case - show only top-level constants
                        // In Ruby, top-level constants are typically under Object namespace
                        let parts = fqn.namespace_parts();
                        if parts.len() == 2 && parts[0].to_string() == "Object" {
                            // This is a top-level constant like Object::String -> show as String
                        } else if parts.len() > 1 {
                            // This is a nested constant, skip it for top-level completion
                            continue;
                        }
                    }

                    // For qualified completion, check if the final component matches
                    let final_component = fqn.name();
                    if !final_component
                        .to_lowercase()
                        .starts_with(&context.partial_name.to_lowercase())
                    {
                        continue;
                    }
                } else {
                    // Handle unqualified completion
                    if !self.matcher.matches(entry, &context.partial_name) {
                        continue;
                    }
                }

                candidates.push(ConstantCompletionItem::new(entry.clone(), context));
                seen_fqns.insert(fqn.clone());
            }
        }

        candidates
    }

    /// Check if the given FQN is a direct child of the specified namespace
    fn is_direct_child_of_namespace(
        &self,
        fqn: &FullyQualifiedName,
        namespace: &FullyQualifiedName,
    ) -> bool {
        let fqn_parts = fqn.namespace_parts();
        let namespace_parts = namespace.namespace_parts();

        // Try direct match first
        if self.is_direct_match(&fqn_parts, &namespace_parts) {
            return true;
        }

        // Try Object-prefixed match: if namespace doesn't start with Object,
        // try matching against Object::namespace
        if !namespace_parts.is_empty() && namespace_parts[0].to_string() != "Object" {
            // Check if FQN starts with Object and then matches the namespace
            if fqn_parts.len() == namespace_parts.len() + 2 && fqn_parts[0].to_string() == "Object"
            {
                // Check if the rest of the FQN matches the namespace
                let mut matches = true;
                for (i, namespace_part) in namespace_parts.iter().enumerate() {
                    if fqn_parts.get(i + 1) != Some(namespace_part) {
                        matches = false;
                        break;
                    }
                }

                if matches {
                    return true;
                }
            }
        }

        false
    }

    /// Helper function to check direct namespace match
    fn is_direct_match(
        &self,
        fqn_parts: &[crate::types::ruby_namespace::RubyConstant],
        namespace_parts: &[crate::types::ruby_namespace::RubyConstant],
    ) -> bool {
        // The FQN should have exactly one more part than the namespace
        if fqn_parts.len() != namespace_parts.len() + 1 {
            return false;
        }

        // All namespace parts should match
        for (i, namespace_part) in namespace_parts.iter().enumerate() {
            if fqn_parts.get(i) != Some(namespace_part) {
                return false;
            }
        }

        true
    }

    fn is_constant_entry(&self, entry: &Entry) -> bool {
        matches!(
            entry.kind,
            EntryKind::Class { .. } | EntryKind::Module { .. } | EntryKind::Constant { .. }
        )
    }
}

impl Default for ConstantCompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::{
            entry::{Entry, MixinRef},
            index::RubyIndex,
        },
        types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant},
    };
    use tower_lsp::lsp_types::{Location, Range, Url};

    fn create_test_entry(name: &str, kind: EntryKind) -> Entry {
        Entry {
            fqn: FullyQualifiedName::try_from(name).unwrap(),
            kind,
            location: Location {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Range::default(),
            },
        }
    }

    fn create_test_index() -> RubyIndex {
        let mut index = RubyIndex::new();

        // Add some test constants with nested namespaces
        let entries = vec![
            (
                "ActiveRecord",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "ActiveRecord::Base",
                EntryKind::Class {
                    superclass: None,
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "ActiveRecord::Migration",
                EntryKind::Class {
                    superclass: None,
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "ActiveRecord::Base::Connection",
                EntryKind::Class {
                    superclass: None,
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "ActiveSupport",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "ActiveSupport::Cache",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "String",
                EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "StringIO",
                EntryKind::Class {
                    superclass: Some(MixinRef {
                        parts: vec![RubyConstant::new("Object").unwrap()],
                        absolute: false,
                    }),
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
        ];

        for (name, kind) in entries {
            let entry = create_test_entry(name, kind);
            index.add_entry(entry);
        }

        index
    }

    #[test]
    fn test_is_direct_child_of_namespace() {
        let engine = ConstantCompletionEngine::new();

        let namespace = FullyQualifiedName::try_from("ActiveRecord").unwrap();
        let direct_child = FullyQualifiedName::try_from("ActiveRecord::Base").unwrap();
        let nested_child = FullyQualifiedName::try_from("ActiveRecord::Base::Connection").unwrap();
        let unrelated = FullyQualifiedName::try_from("ActiveSupport::Cache").unwrap();

        assert!(engine.is_direct_child_of_namespace(&direct_child, &namespace));
        assert!(!engine.is_direct_child_of_namespace(&nested_child, &namespace));
        assert!(!engine.is_direct_child_of_namespace(&unrelated, &namespace));
    }

    #[test]
    fn test_scope_resolution_completion() {
        let engine = ConstantCompletionEngine::new();
        let index = create_test_index();

        // Test completion for "ActiveRecord::" - should only show direct children
        let context = ConstantCompletionContext::new(
            Position::new(0, 0),
            vec![],
            "ActiveRecord::".to_string(),
        );

        let candidates = engine.find_constant_candidates(&index, &context);

        // Should find Base and Migration, but not Base::Connection
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"Base".to_string()));
        assert!(names.contains(&"Migration".to_string()));
        assert!(!names.contains(&"Connection".to_string())); // This is nested under Base

        // Should not contain ActiveRecord itself or unrelated constants
        assert!(!names.contains(&"ActiveRecord".to_string()));
        assert!(!names.contains(&"ActiveSupport".to_string()));
        assert!(!names.contains(&"String".to_string()));
    }

    #[test]
    fn test_scope_resolution_with_partial_name() {
        let engine = ConstantCompletionEngine::new();
        let index = create_test_index();

        // Test completion for "ActiveRecord::B" - should only show Base
        let context = ConstantCompletionContext::new(
            Position::new(0, 0),
            vec![],
            "ActiveRecord::B".to_string(),
        );

        let candidates = engine.find_constant_candidates(&index, &context);

        // Should find only Base (starts with "B")
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"Base".to_string()));
        assert!(!names.contains(&"Migration".to_string())); // Doesn't start with "B"
    }

    #[test]
    fn test_unqualified_completion() {
        let engine = ConstantCompletionEngine::new();
        let index = create_test_index();

        // Test completion for "Str" - should show String and StringIO
        let context =
            ConstantCompletionContext::new(Position::new(0, 0), vec![], "Str".to_string());

        let candidates = engine.find_constant_candidates(&index, &context);

        // Should find String and StringIO
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"String".to_string()));
        assert!(names.contains(&"StringIO".to_string()));

        // Should not contain ActiveRecord constants
        assert!(!names.contains(&"ActiveRecord".to_string()));
        assert!(!names.contains(&"Base".to_string()));
    }

    #[test]
    fn test_empty_namespace_completion() {
        let engine = ConstantCompletionEngine::new();
        let index = create_test_index();

        // Test completion for "::" - should show top-level constants
        let context = ConstantCompletionContext::new(Position::new(0, 0), vec![], "::".to_string());

        let candidates = engine.find_constant_candidates(&index, &context);

        // Should find top-level constants only
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"ActiveRecord".to_string()));
        assert!(names.contains(&"ActiveSupport".to_string()));
        assert!(names.contains(&"String".to_string()));
        assert!(names.contains(&"StringIO".to_string()));

        // Should not contain nested constants
        assert!(!names.contains(&"Base".to_string()));
        assert!(!names.contains(&"Migration".to_string()));
        assert!(!names.contains(&"Cache".to_string()));
    }

    #[test]
    fn test_simple_nested_module_completion() {
        let engine = ConstantCompletionEngine::new();
        let mut index = RubyIndex::new();

        // Add the exact scenario from the user's issue: module A with nested module B
        let entries = vec![
            (
                "A",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
            (
                "A::B",
                EntryKind::Module {
                    includes: vec![],
                    extends: vec![],
                    prepends: vec![],
                },
            ),
        ];

        for (name, kind) in entries {
            let entry = create_test_entry(name, kind);
            index.add_entry(entry);
        }

        // Test completion for "A::" - should show nested module B
        let context =
            ConstantCompletionContext::new(Position::new(0, 0), vec![], "A::".to_string());

        let candidates = engine.find_constant_candidates(&index, &context);

        // Should find B as a direct child of A
        let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();

        assert!(
            names.contains(&"B".to_string()),
            "Expected to find module B in A:: completion, but found: {:?}",
            names
        );

        // Should not contain A itself
        assert!(!names.contains(&"A".to_string()));
    }
}
