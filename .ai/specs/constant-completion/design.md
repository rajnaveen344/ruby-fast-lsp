# Constant Completion Design Document

## Overview

This design document outlines the implementation of constant completion support for the Ruby Fast LSP, enabling intelligent auto completion of Ruby classes, modules, and constants. The implementation leverages the existing RubyIndex infrastructure and integrates seamlessly with the current completion system to provide fast, context-aware constant suggestions. The design focuses on performance, accuracy, and LSP protocol compliance while maintaining the existing architectural patterns.

## Architecture

### Current Architecture Integration

The constant completion feature will integrate with existing components:
- **RubyIndex**: Primary data source using `definitions` map filtered by constant entry types
- **Completion System**: Extends existing `handle_completion` function in `src/capabilities/completion.rs`
- **Analyzer**: Uses existing `RubyPrismAnalyzer` for scope and context analysis
- **Types**: Leverages existing Ruby type definitions (`FullyQualifiedName`, `Entry`, etc.)

### New Components

1. **Constant Completion Engine** (`src/capabilities/completion/constant_completion.rs`)
2. **Constant Matcher** (`src/capabilities/completion/constant_matcher.rs`)
3. **Scope Resolver** (`src/capabilities/completion/scope_resolver.rs`)
4. **Completion Ranker** (`src/capabilities/completion/completion_ranker.rs`)

## Core Data Structures

### ConstantCompletionContext
```rust
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
```

### ConstantCompletionItem
```rust
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
```

## Core Components

### 1. Constant Completion Engine

The main orchestrator for constant completion:

```rust
use crate::indexer::index::RubyIndex;
use crate::analyzer_prism::RubyPrismAnalyzer;
use tower_lsp::lsp_types::{CompletionItem, Position};

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
                if self.matcher.matches(&entry, &context.partial_name) {
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
```

### 2. Constant Matcher

Handles pattern matching for constant names:

```rust
pub struct ConstantMatcher {
    // Configuration for matching behavior
    case_sensitive: bool,
    fuzzy_matching: bool,
    camel_case_matching: bool,
}

impl ConstantMatcher {
    pub fn new() -> Self {
        Self {
            case_sensitive: false,
            fuzzy_matching: true,
            camel_case_matching: true,
        }
    }

    pub fn matches(&self, entry: &Entry, partial: &str) -> bool {
        let constant_name = self.extract_name(entry);
        
        // Empty partial matches everything
        if partial.is_empty() {
            return true;
        }

        // Exact prefix match (highest priority)
        if self.prefix_match(&constant_name, partial) {
            return true;
        }

        // Fuzzy matching
        if self.fuzzy_matching && self.fuzzy_match(&constant_name, partial) {
            return true;
        }

        // CamelCase abbreviation matching (e.g., "AR" matches "ActiveRecord")
        if self.camel_case_matching && self.camel_case_match(&constant_name, partial) {
            return true;
        }

        false
    }

    fn extract_name(&self, entry: &Entry) -> String {
        match &entry.kind {
            EntryKind::Class { .. } | EntryKind::Module { .. } | EntryKind::Constant { .. } => {
                entry.fqn.name()
            }
            _ => entry.fqn.to_string(),
        }
    }

    fn prefix_match(&self, name: &str, partial: &str) -> bool {
        if self.case_sensitive {
            name.starts_with(partial)
        } else {
            name.to_lowercase().starts_with(&partial.to_lowercase())
        }
    }

    fn fuzzy_match(&self, name: &str, partial: &str) -> bool {
        // Simple fuzzy matching - all characters of partial must appear in order
        let name_chars: Vec<char> = if self.case_sensitive {
            name.chars().collect()
        } else {
            name.to_lowercase().chars().collect()
        };
        
        let partial_chars: Vec<char> = if self.case_sensitive {
            partial.chars().collect()
        } else {
            partial.to_lowercase().chars().collect()
        };

        let mut partial_idx = 0;
        for &ch in &name_chars {
            if partial_idx < partial_chars.len() && ch == partial_chars[partial_idx] {
                partial_idx += 1;
            }
        }

        partial_idx == partial_chars.len()
    }

    fn camel_case_match(&self, name: &str, partial: &str) -> bool {
        // Extract uppercase letters from name for abbreviation matching
        let uppercase_chars: String = name.chars()
            .filter(|c| c.is_uppercase())
            .collect();

        if self.case_sensitive {
            uppercase_chars.starts_with(partial)
        } else {
            uppercase_chars.to_lowercase().starts_with(&partial.to_lowercase())
        }
    }
}
```

### 3. Scope Resolver

Determines constant accessibility based on Ruby's scoping rules:

```rust
use crate::types::scope::Scope;

pub struct ScopeResolver;

impl ScopeResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve_accessibility(
        &self,
        candidates: &mut Vec<ConstantCompletionItem>,
        context: &ConstantCompletionContext,
    ) {
        for candidate in candidates {
            candidate.is_accessible = self.is_accessible(&candidate.entry, context);
            
            // Update insert text based on accessibility and scope
            candidate.insert_text = self.calculate_insert_text(&candidate.entry, context);
        }
    }

    fn is_accessible(&self, entry: &Entry, context: &ConstantCompletionContext) -> bool {
        // For qualified references, check if the namespace exists and is accessible
        if let Some(namespace_prefix) = &context.namespace_prefix {
            return self.is_namespace_accessible(namespace_prefix, context);
        }

        // For unqualified references, check if constant is accessible from current scope
        self.is_constant_accessible_from_scope(&entry.fqn, &context.scope_stack)
    }

    fn is_namespace_accessible(
        &self,
        namespace: &FullyQualifiedName,
        context: &ConstantCompletionContext,
    ) -> bool {
        // Check if the namespace is accessible from current scope
        // This involves checking if the namespace is defined and reachable
        true // Simplified for now
    }

    fn is_constant_accessible_from_scope(
        &self,
        constant_fqn: &FullyQualifiedName,
        scope_stack: &[Scope],
    ) -> bool {
        // Ruby constant lookup rules:
        // 1. Current scope and its ancestors
        // 2. Included/extended modules
        // 3. Top-level constants
        
        // For now, allow all constants (will be refined)
        true
    }

    fn calculate_insert_text(&self, entry: &Entry, context: &ConstantCompletionContext) -> String {
        if context.is_qualified {
            // For qualified names, just insert the final component
            entry.fqn.name()
        } else {
            // For unqualified names, determine if we need the full path
            let current_namespace = self.get_current_namespace(&context.scope_stack);
            
            if self.needs_qualification(&entry.fqn, &current_namespace) {
                entry.fqn.to_string()
            } else {
                entry.fqn.name()
            }
        }
    }

    fn get_current_namespace(&self, scope_stack: &[Scope]) -> Option<FullyQualifiedName> {
        // Extract current namespace from scope stack
        None // Simplified for now
    }

    fn needs_qualification(
        &self,
        constant_fqn: &FullyQualifiedName,
        current_namespace: &Option<FullyQualifiedName>,
    ) -> bool {
        // Determine if we need to qualify the constant name
        false // Simplified for now
    }
}
```

### 4. Completion Ranker

Ranks completion items by relevance:

```rust
pub struct CompletionRanker;

impl CompletionRanker {
    pub fn new() -> Self {
        Self
    }

    pub fn rank_by_relevance(
        &self,
        candidates: &mut Vec<ConstantCompletionItem>,
        context: &ConstantCompletionContext,
    ) {
        // Calculate detailed relevance scores
        for candidate in candidates.iter_mut() {
            candidate.relevance_score = self.calculate_detailed_relevance(candidate, context);
        }

        // Sort by relevance score (descending)
        candidates.sort_by(|a, b| {
            b.relevance_score.partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    fn calculate_detailed_relevance(
        &self,
        candidate: &ConstantCompletionItem,
        context: &ConstantCompletionContext,
    ) -> f64 {
        let mut score = candidate.relevance_score;

        // Boost for exact matches
        if candidate.name == context.partial_name {
            score += 5.0;
        }

        // Boost for prefix matches
        if candidate.name.starts_with(&context.partial_name) {
            score += 3.0;
        }

        // Boost for same namespace
        score += self.namespace_proximity_score(&candidate.fqn, &context.scope_stack);

        // Boost for shorter names (prefer simpler constants)
        score += 1.0 / (candidate.name.len() as f64).max(1.0);

        // Boost for common patterns
        if self.is_common_constant(&candidate.name) {
            score += 1.0;
        }

        score
    }

    fn namespace_proximity_score(&self, fqn: &FullyQualifiedName, scope_stack: &[Scope]) -> f64 {
        // Calculate how "close" this constant is to the current scope
        // Higher score for constants in the same or nearby namespaces
        0.0 // Simplified for now
    }

    fn is_common_constant(&self, name: &str) -> bool {
        // Boost common Ruby constants
        matches!(name, "String" | "Array" | "Hash" | "Integer" | "Float" | "Object" | "Class" | "Module")
    }
}
```

## Integration with Existing Completion System

### Extending handle_completion Function

```rust
// In src/capabilities/completion.rs

use crate::capabilities::completion::constant_completion::ConstantCompletionEngine;

pub async fn handle_completion(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> CompletionResponse {
    let document = server.get_doc(&uri).unwrap();
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), document.content.clone());
    
    // Get context at position
    let (identifier, _, lv_stack_at_pos) = analyzer.get_identifier(position);
    
    let mut completions = vec![];

    // Existing local variable completion
    let mut seen_variables = HashSet::new();
    for scope in lv_stack_at_pos.iter().rev() {
        let scope_id = scope.scope_id();
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::Variable { name } = &entry.kind {
                    let var_name = name.name().to_string();
                    if seen_variables.insert(var_name.clone()) {
                        completions.push(CompletionItem {
                            label: var_name,
                            label_details: Some(CompletionItemLabelDetails {
                                detail: None,
                                description: Some("local_variable".to_string()),
                            }),
                            kind: Some(CompletionItemKind::VARIABLE),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        if scope.kind().is_hard_scope_boundary() {
            break;
        }
    }

    // NEW: Constant completion
    if let Some(partial_constant) = extract_partial_constant(&identifier) {
        let constant_engine = ConstantCompletionEngine::new();
        let index = server.get_index().await;
        
        let constant_completions = constant_engine.complete_constants(
            &index,
            &analyzer,
            position,
            partial_constant,
        );
        
        completions.extend(constant_completions);
    }

    CompletionResponse::Array(completions)
}

fn extract_partial_constant(identifier: &Option<String>) -> Option<String> {
    identifier.as_ref().and_then(|id| {
        // Check if this looks like a constant (starts with uppercase or contains ::)
        if id.chars().next().map_or(false, |c| c.is_uppercase()) || id.contains("::") {
            Some(id.clone())
        } else {
            None
        }
    })
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::fixtures::*;

    #[test]
    fn test_basic_class_completion() {
        let index = create_test_index_with_classes();
        let engine = ConstantCompletionEngine::new();
        
        let completions = engine.complete_constants(
            &index,
            &create_test_analyzer(),
            Position::new(0, 5),
            "Str".to_string(),
        );
        
        assert!(completions.iter().any(|c| c.label == "String"));
    }

    #[test]
    fn test_namespace_qualified_completion() {
        let index = create_test_index_with_namespaces();
        let engine = ConstantCompletionEngine::new();
        
        let completions = engine.complete_constants(
            &index,
            &create_test_analyzer(),
            Position::new(0, 10),
            "Foo::Bar".to_string(),
        );
        
        assert!(completions.iter().any(|c| c.label == "BarClass"));
    }

    #[test]
    fn test_fuzzy_matching() {
        let index = create_test_index_with_classes();
        let engine = ConstantCompletionEngine::new();
        
        let completions = engine.complete_constants(
            &index,
            &create_test_analyzer(),
            Position::new(0, 5),
            "AR".to_string(),
        );
        
        assert!(completions.iter().any(|c| c.label == "ActiveRecord"));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_constant_completion_integration() {
    let server = create_test_server().await;
    
    // Index a file with constants
    let ruby_content = r#"
        class MyClass
        end
        
        module MyModule
        end
        
        MY_CONSTANT = 42
    "#;
    
    server.index_file("test.rb", ruby_content).await;
    
    // Test completion
    let completions = handle_completion(
        &server,
        Url::parse("file://test.rb").unwrap(),
        Position::new(5, 2), // After "My"
    ).await;
    
    let completion_labels: Vec<String> = completions.into_iter()
        .map(|c| c.label)
        .collect();
    
    assert!(completion_labels.contains(&"MyClass".to_string()));
    assert!(completion_labels.contains(&"MyModule".to_string()));
    assert!(completion_labels.contains(&"MY_CONSTANT".to_string()));
}
```

## Performance Considerations

### Optimization Strategies

1. **Index Filtering**: Pre-filter constants by type to avoid processing non-constant entries
2. **Lazy Evaluation**: Only calculate detailed relevance scores for top candidates
3. **Caching**: Cache frequent completion results and scope resolutions
4. **Limits**: Implement reasonable limits on result count and processing time
5. **Incremental Updates**: Update completion data incrementally as the index changes

### Memory Management

1. **Efficient Data Structures**: Use appropriate data structures for fast lookups
2. **String Interning**: Consider interning frequently used constant names
3. **Scope Cleanup**: Clean up temporary completion data after requests
4. **Index Reuse**: Reuse existing index data without duplication

## Future Enhancements

### Phase 2 Features
- **Documentation Integration**: Include YARD/RDoc documentation in completion details
- **Type Information**: Show type signatures and return types where available
- **Usage Statistics**: Rank by actual usage frequency in the codebase
- **Import Suggestions**: Suggest adding require/include statements for external constants

### Phase 3 Features
- **Semantic Completion**: Use semantic analysis for more accurate suggestions
- **Cross-Reference Integration**: Show where constants are used and defined
- **Refactoring Support**: Integration with constant renaming and moving
- **AI-Powered Ranking**: Use machine learning for better relevance ranking

## Implementation Timeline

### Week 1: Core Infrastructure
- Implement `ConstantCompletionEngine` and basic matching
- Create `ConstantCompletionContext` and `ConstantCompletionItem`
- Basic integration with existing completion system

### Week 2: Matching and Ranking
- Implement `ConstantMatcher` with fuzzy and camelCase matching
- Create `CompletionRanker` with relevance scoring
- Add comprehensive unit tests

### Week 3: Scope Resolution
- Implement `ScopeResolver` for Ruby constant lookup rules
- Handle qualified and unqualified constant references
- Add namespace-aware completion

### Week 4: Integration and Testing
- Complete integration with LSP protocol
- Add integration tests and performance benchmarks
- Documentation and code review

This design provides a comprehensive foundation for implementing constant completion while maintaining consistency with the existing Ruby Fast LSP architecture and ensuring high performance and accuracy.