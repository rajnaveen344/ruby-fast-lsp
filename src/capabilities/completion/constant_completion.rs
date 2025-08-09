use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation, Position,
};

use crate::{
    analyzer_prism::RubyPrismAnalyzer,
    indexer::{entry::Entry, entry::entry_kind::EntryKind, index::RubyIndex},
    types::{fully_qualified_name::FullyQualifiedName, scope::LVScope as Scope},
};

use super::{
    constant_matcher::ConstantMatcher,
    scope_resolver::ScopeResolver,
    completion_ranker::CompletionRanker,
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
            
            if let Ok(fqn) = FullyQualifiedName::try_from(namespace) {
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
                if let Some(parent) = superclass {
                    Some(format!("< {}", parent))
                } else {
                    Some("class".to_string())
                }
            }
            EntryKind::Module { .. } => Some("module".to_string()),
            EntryKind::Constant { value, .. } => {
                if let Some(val) = value {
                    Some(format!("= {}", val))
                } else {
                    Some("constant".to_string())
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
            documentation: self.documentation.as_ref().map(|doc| {
                Documentation::String(doc.clone())
            }),
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
        self.scope_resolver.resolve_accessibility(&mut candidates, &context);
        
        // Filter out inaccessible constants
        candidates.retain(|item| item.is_accessible);
        
        // Rank by relevance
        self.ranker.rank_by_relevance(&mut candidates, &context);
        
        // Convert to LSP completion items
        candidates.into_iter()
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

        // Search through all definitions in the index
        for (fqn, entries) in &index.definitions {
            for entry in entries {
                // Only consider constant-like entries
                if !self.is_constant_entry(entry) {
                    continue;
                }

                // Apply namespace filtering if qualified
                if let Some(namespace_prefix) = &context.namespace_prefix {
                    if !fqn.starts_with(namespace_prefix) {
                        continue;
                    }
                }

                // Check if the name matches the partial input
                if self.matcher.matches(entry, &context.partial_name) {
                    candidates.push(ConstantCompletionItem::new(entry.clone(), context));
                }
            }
        }

        candidates
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