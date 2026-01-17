use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation, Position,
};

use crate::{
    analyzer_prism::RubyPrismAnalyzer,
    indexer::{entry::entry_kind::EntryKind, entry::Entry, index::RubyIndex},
    types::{fully_qualified_name::FullyQualifiedName, scope::LVScopeId},
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
    /// Current scope ID from analyzer
    pub scope_id: LVScopeId,
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
    pub fn new(entry: Entry, fqn: FullyQualifiedName, context: &ConstantCompletionContext) -> Self {
        let name = fqn.name();
        let insert_text = Self::calculate_insert_text(&entry, fqn.clone(), context);
        let detail = Self::extract_detail(&entry, &fqn);
        let relevance_score = Self::calculate_relevance(&entry, &fqn, context);

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

    fn calculate_insert_text(
        _entry: &Entry,
        fqn: FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> String {
        if context.after_scope_resolution {
            // Already typed "Scope::", just insert "Constant"
            fqn.name()
        } else {
            // Use just the name for now, auto-import might be needed later
            fqn.name()
        }
    }

    fn extract_detail(entry: &Entry, fqn: &FullyQualifiedName) -> Option<String> {
        match &entry.kind {
            EntryKind::Class(info) => {
                let superclass = info
                    .superclass
                    .as_ref()
                    .map(|s| {
                        let fqn = FullyQualifiedName::from(s.parts.clone());
                        format!(" < {}", fqn)
                    })
                    .unwrap_or_default();
                Some(format!("class {}{}", fqn, superclass))
            }
            EntryKind::Module(_) => Some(format!("module {}", fqn)),
            EntryKind::Constant(info) => {
                let value = info
                    .value
                    .as_ref()
                    .map(|v| format!(" = {}", v))
                    .unwrap_or_default();
                Some(format!("{}{}", fqn, value))
            }
            _ => Some(fqn.to_string()),
        }
    }

    fn calculate_relevance(
        entry: &Entry,
        fqn: &FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> f64 {
        let mut score = 0.0;

        // Base score by entry type
        score += match &entry.kind {
            EntryKind::Class { .. } => 1.0,
            EntryKind::Module { .. } => 0.9,
            EntryKind::Constant(_) => 0.8,
            _ => 0.0,
        };

        // Boost for exact prefix matches
        let name = fqn.name();
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
            EntryKind::Constant(_) => CompletionItemKind::CONSTANT,
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
        let (_, _, _, scope_stack, _) = analyzer.get_identifier(position);

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
        use crate::types::ruby_namespace::RubyConstant;
        let mut candidates = Vec::new();
        // Use namespace_parts for deduplication to handle Constant vs Namespace variants
        let mut seen_parts: std::collections::HashSet<Vec<RubyConstant>> =
            std::collections::HashSet::new();

        // Search through all definitions in the index
        for (fqn, entries) in index.definitions() {
            // Skip if we've already processed this FQN (comparing namespace parts)
            let parts = fqn.namespace_parts();
            if seen_parts.contains(&parts) {
                continue;
            }

            // Find the best entry for this FQN (prefer the first constant-like entry)
            let best_entry = entries
                .iter()
                .cloned()
                .find(|entry| self.is_constant_entry(entry));

            if let Some(entry) = best_entry {
                // Handle qualified completion (e.g., "ActiveRecord::" or "ActiveRecord::B")
                if context.is_qualified {
                    if let Some(ref namespace_prefix) = context.namespace_prefix {
                        // Check if this constant is a direct child of the namespace
                        if !self.is_direct_child_of_namespace(fqn, namespace_prefix) {
                            continue;
                        }
                    } else {
                        // Handle the "::" case - show only top-level constants
                        let parts = fqn.namespace_parts();
                        if parts.len() > 1 {
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
                    if !self.matcher.matches(entry, fqn, &context.partial_name) {
                        continue;
                    }
                }

                candidates.push(ConstantCompletionItem::new(
                    entry.clone(),
                    fqn.clone(),
                    context,
                ));
                seen_parts.insert(parts);
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

        self.is_direct_match(&fqn_parts, &namespace_parts)
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
            EntryKind::Class(_) | EntryKind::Module(_) | EntryKind::Constant(_)
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
        types::{
            compact_location::CompactLocation, fully_qualified_name::FullyQualifiedName,
            ruby_namespace::RubyConstant,
        },
    };

    fn create_test_entry(index: &mut RubyIndex, name: &str, kind: EntryKind) -> Entry {
        let fqn = FullyQualifiedName::try_from(name).unwrap();
        let fqn_id = index.intern_fqn(fqn);
        Entry {
            fqn_id,
            kind,
            location: crate::types::compact_location::CompactLocation::default(),
        }
    }

    fn create_test_index() -> RubyIndex {
        let mut index = RubyIndex::new();

        // Add some test constants with nested namespaces
        let entries = vec![
            ("ActiveRecord", EntryKind::new_module()),
            ("ActiveRecord::Base", EntryKind::new_class(None)),
            ("ActiveRecord::Migration", EntryKind::new_class(None)),
            ("ActiveRecord::Base::Connection", EntryKind::new_class(None)),
            ("ActiveSupport", EntryKind::new_module()),
            ("ActiveSupport::Cache", EntryKind::new_module()),
            (
                "String",
                EntryKind::new_class(Some(MixinRef {
                    parts: vec![RubyConstant::new("Object").unwrap()],
                    absolute: false,
                    location: CompactLocation::default(),
                })),
            ),
            (
                "StringIO",
                EntryKind::new_class(Some(MixinRef {
                    parts: vec![RubyConstant::new("Object").unwrap()],
                    absolute: false,
                    location: CompactLocation::default(),
                })),
            ),
        ];

        for (name, kind) in entries {
            let entry = create_test_entry(&mut index, name, kind);
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
        let context =
            ConstantCompletionContext::new(Position::new(0, 0), 0, "ActiveRecord::".to_string());

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
        let context =
            ConstantCompletionContext::new(Position::new(0, 0), 0, "ActiveRecord::B".to_string());

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
        let context = ConstantCompletionContext::new(Position::new(0, 0), 0, "Str".to_string());

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
        let context = ConstantCompletionContext::new(Position::new(0, 0), 0, "::".to_string());

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
            ("A", EntryKind::new_module()),
            ("A::B", EntryKind::new_module()),
        ];

        for (name, kind) in entries {
            let entry = create_test_entry(&mut index, name, kind);
            index.add_entry(entry);
        }

        // Test completion for "A::" - should show nested module B
        let context = ConstantCompletionContext::new(Position::new(0, 0), 0, "A::".to_string());

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
