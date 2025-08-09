# Workspace Symbols Design Document

## Overview

This design document outlines the implementation of workspace symbols support for the Ruby Fast LSP, enabling project-wide symbol search and navigation. The implementation leverages the existing RubyIndex infrastructure to provide fast, accurate symbol search across all Ruby files in a workspace. The design focuses on performance, LSP protocol compliance, and seamless integration with the current architecture.

## Architecture

### Current Architecture Integration

The workspace symbols feature will integrate with existing components:
- **RubyIndex**: Primary data source using `definitions` and `methods_by_name` maps
- **Server**: Integrates with existing request handling pipeline
- **Capabilities**: Follows established capability module structure
- **Types**: Leverages existing Ruby type definitions (`FullyQualifiedName`, `Entry`, etc.)

### New Components

1. **Workspace Symbols Capability** (`src/capabilities/workspace_symbols.rs`)
2. **Symbol Search Engine** (`src/capabilities/workspace_symbols/search_engine.rs`)
3. **Symbol Matcher** (`src/capabilities/workspace_symbols/matcher.rs`)
4. **Symbol Ranker** (`src/capabilities/workspace_symbols/ranker.rs`)
5. **LSP Integration** (Server method and request handler)

## Core Data Structures

### WorkspaceSymbolQuery
```rust
#[derive(Debug, Clone)]
pub struct WorkspaceSymbolQuery {
    /// The search query string
    pub query: String,
    /// Optional symbol kind filter
    pub kind_filter: Option<SymbolKind>,
    /// Case sensitivity preference
    pub case_sensitive: bool,
    /// Maximum number of results to return
    pub limit: Option<usize>,
    /// Whether to use regex matching
    pub use_regex: bool,
}

impl WorkspaceSymbolQuery {
    pub fn new(query: String) -> Self {
        Self {
            query,
            kind_filter: None,
            case_sensitive: false,
            limit: Some(100), // Default limit
            use_regex: false,
        }
    }

    pub fn with_kind_filter(mut self, kind: SymbolKind) -> Self {
        self.kind_filter = Some(kind);
        self
    }

    pub fn case_sensitive(mut self) -> Self {
        self.case_sensitive = true;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}
```

### WorkspaceSymbolResult
```rust
#[derive(Debug, Clone)]
pub struct WorkspaceSymbolResult {
    /// The symbol information
    pub symbol: WorkspaceSymbol,
    /// Relevance score for ranking (higher = more relevant)
    pub relevance_score: f64,
    /// The entry from RubyIndex that this symbol represents
    pub entry: Entry,
}

impl WorkspaceSymbolResult {
    pub fn new(entry: Entry, relevance_score: f64) -> Self {
        let symbol = Self::entry_to_workspace_symbol(&entry);
        Self {
            symbol,
            relevance_score,
            entry,
        }
    }

    fn entry_to_workspace_symbol(entry: &Entry) -> WorkspaceSymbol {
        let name = Self::extract_symbol_name(entry);
        let kind = Self::entry_kind_to_symbol_kind(&entry.kind);
        let container_name = Self::extract_container_name(entry);
        
        WorkspaceSymbol {
            name,
            kind,
            tags: None,
            container_name,
            location: WorkspaceSymbolLocation::Location(entry.location.clone()),
        }
    }

    fn extract_symbol_name(entry: &Entry) -> String {
        match &entry.kind {
            EntryKind::Class { .. } | EntryKind::Module { .. } | EntryKind::Constant { .. } => {
                entry.fqn.name()
            }
            EntryKind::Method { name, .. } => name.to_string(),
            EntryKind::Variable { name } => name.to_string(),
        }
    }

    fn entry_kind_to_symbol_kind(kind: &EntryKind) -> SymbolKind {
        match kind {
            EntryKind::Class { .. } => SymbolKind::CLASS,
            EntryKind::Module { .. } => SymbolKind::MODULE,
            EntryKind::Method { .. } => SymbolKind::METHOD,
            EntryKind::Constant { .. } => SymbolKind::CONSTANT,
            EntryKind::Variable { .. } => SymbolKind::VARIABLE,
        }
    }

    fn extract_container_name(entry: &Entry) -> Option<String> {
        match &entry.kind {
            EntryKind::Method { owner, .. } => Some(owner.to_string()),
            _ => {
                if entry.fqn.parts().len() > 1 {
                    Some(entry.fqn.namespace().to_string())
                } else {
                    None
                }
            }
        }
    }
}
```

## Core Components

### 1. Symbol Search Engine

The main orchestrator for workspace symbol search:

```rust
use crate::indexer::index::RubyIndex;
use tower_lsp::lsp_types::{WorkspaceSymbol, SymbolKind};

pub struct SymbolSearchEngine {
    matcher: SymbolMatcher,
    ranker: SymbolRanker,
}

impl SymbolSearchEngine {
    pub fn new() -> Self {
        Self {
            matcher: SymbolMatcher::new(),
            ranker: SymbolRanker::new(),
        }
    }

    pub fn search(
        &self,
        index: &RubyIndex,
        query: &WorkspaceSymbolQuery,
    ) -> Vec<WorkspaceSymbolResult> {
        let mut results = Vec::new();

        // Search through all definitions in the index
        for (fqn, entries) in &index.definitions {
            for entry in entries {
                if let Some(relevance_score) = self.matcher.matches(entry, query) {
                    // Apply kind filter if specified
                    if let Some(kind_filter) = query.kind_filter {
                        let entry_kind = WorkspaceSymbolResult::entry_kind_to_symbol_kind(&entry.kind);
                        if entry_kind != kind_filter {
                            continue;
                        }
                    }

                    results.push(WorkspaceSymbolResult::new(entry.clone(), relevance_score));
                }
            }
        }

        // For method searches, also check methods_by_name for efficiency
        if query.kind_filter.is_none() || query.kind_filter == Some(SymbolKind::METHOD) {
            self.search_methods_by_name(index, query, &mut results);
        }

        // Rank and limit results
        self.ranker.rank_and_limit(&mut results, query);

        results
    }

    fn search_methods_by_name(
        &self,
        index: &RubyIndex,
        query: &WorkspaceSymbolQuery,
        results: &mut Vec<WorkspaceSymbolResult>,
    ) {
        for (method, entries) in &index.methods_by_name {
            if let Some(relevance_score) = self.matcher.matches_method_name(method, query) {
                for entry in entries {
                    // Avoid duplicates from the main definitions search
                    if !results.iter().any(|r| r.entry.location == entry.location) {
                        results.push(WorkspaceSymbolResult::new(entry.clone(), relevance_score));
                    }
                }
            }
        }
    }
}
```

### 2. Symbol Matcher

Handles pattern matching and relevance scoring:

```rust
use regex::Regex;
use crate::indexer::entry::{Entry, EntryKind};
use crate::types::ruby_method::RubyMethod;

pub struct SymbolMatcher {
    // Cache compiled regexes for performance
    regex_cache: std::collections::HashMap<String, Regex>,
}

impl SymbolMatcher {
    pub fn new() -> Self {
        Self {
            regex_cache: std::collections::HashMap::new(),
        }
    }

    /// Returns relevance score if the entry matches the query, None otherwise
    pub fn matches(&self, entry: &Entry, query: &WorkspaceSymbolQuery) -> Option<f64> {
        let symbol_name = self.extract_searchable_name(entry);
        self.match_string(&symbol_name, query)
    }

    pub fn matches_method_name(&self, method: &RubyMethod, query: &WorkspaceSymbolQuery) -> Option<f64> {
        let method_name = method.name();
        self.match_string(method_name, query)
    }

    fn match_string(&self, text: &str, query: &WorkspaceSymbolQuery) -> Option<f64> {
        if query.use_regex {
            self.match_regex(text, query)
        } else {
            self.match_substring(text, query)
        }
    }

    fn match_substring(&self, text: &str, query: &WorkspaceSymbolQuery) -> Option<f64> {
        let (text, query_str) = if query.case_sensitive {
            (text.to_string(), query.query.clone())
        } else {
            (text.to_lowercase(), query.query.to_lowercase())
        };

        if text.contains(&query_str) {
            // Calculate relevance score based on match quality
            let score = self.calculate_substring_score(&text, &query_str);
            Some(score)
        } else {
            None
        }
    }

    fn match_regex(&self, text: &str, query: &WorkspaceSymbolQuery) -> Option<f64> {
        // Implementation for regex matching
        // This would use the regex_cache for performance
        todo!("Implement regex matching")
    }

    fn calculate_substring_score(&self, text: &str, query: &str) -> f64 {
        // Exact match gets highest score
        if text == query {
            return 1.0;
        }

        // Prefix match gets high score
        if text.starts_with(query) {
            return 0.9;
        }

        // Word boundary match gets good score
        if self.is_word_boundary_match(text, query) {
            return 0.8;
        }

        // Camel case match gets decent score
        if self.is_camel_case_match(text, query) {
            return 0.7;
        }

        // General substring match gets lower score
        0.5
    }

    fn is_word_boundary_match(&self, text: &str, query: &str) -> bool {
        // Check if query matches at word boundaries (e.g., "user" matches "user_service")
        text.split('_').any(|word| word.starts_with(query))
    }

    fn is_camel_case_match(&self, text: &str, query: &str) -> bool {
        // Check if query matches camel case initials (e.g., "us" matches "UserService")
        let initials: String = text.chars()
            .filter(|c| c.is_uppercase())
            .collect::<String>()
            .to_lowercase();
        initials.starts_with(&query.to_lowercase())
    }

    fn extract_searchable_name(&self, entry: &Entry) -> String {
        match &entry.kind {
            EntryKind::Class { .. } | EntryKind::Module { .. } | EntryKind::Constant { .. } => {
                entry.fqn.name().to_string()
            }
            EntryKind::Method { name, .. } => name.name().to_string(),
            EntryKind::Variable { name } => name.to_string(),
        }
    }
}
```

### 3. Symbol Ranker

Handles result ranking and limiting:

```rust
pub struct SymbolRanker;

impl SymbolRanker {
    pub fn new() -> Self {
        Self
    }

    pub fn rank_and_limit(
        &self,
        results: &mut Vec<WorkspaceSymbolResult>,
        query: &WorkspaceSymbolQuery,
    ) {
        // Sort by relevance score (descending) and then by symbol type preference
        results.sort_by(|a, b| {
            // Primary sort: relevance score
            let score_cmp = b.relevance_score.partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal);
            
            if score_cmp != std::cmp::Ordering::Equal {
                return score_cmp;
            }

            // Secondary sort: symbol type preference
            let type_priority_a = self.get_symbol_type_priority(&a.entry.kind);
            let type_priority_b = self.get_symbol_type_priority(&b.entry.kind);
            type_priority_b.cmp(&type_priority_a)
        });

        // Apply limit if specified
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }
    }

    fn get_symbol_type_priority(&self, kind: &EntryKind) -> u8 {
        match kind {
            EntryKind::Class { .. } => 4,      // Highest priority
            EntryKind::Module { .. } => 3,
            EntryKind::Constant { .. } => 2,
            EntryKind::Method { .. } => 1,
            EntryKind::Variable { .. } => 0,   // Lowest priority
        }
    }
}
```

## LSP Integration

### Request Handler

```rust
// In src/handlers/request.rs

use tower_lsp::lsp_types::{WorkspaceSymbolParams, WorkspaceSymbol};
use crate::capabilities::workspace_symbols::WorkspaceSymbolsCapability;

impl RequestHandler {
    pub async fn workspace_symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<WorkspaceSymbol>>, tower_lsp::jsonrpc::Error> {
        let query = WorkspaceSymbolQuery::new(params.query);
        
        let index = self.server.get_index().await;
        let capability = WorkspaceSymbolsCapability::new();
        
        let results = capability.search_symbols(&index, &query);
        let symbols: Vec<WorkspaceSymbol> = results
            .into_iter()
            .map(|result| result.symbol)
            .collect();

        Ok(Some(symbols))
    }
}
```

### Server Integration

```rust
// In src/server.rs

impl LanguageServer for RubyLanguageServer {
    async fn workspace_symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<WorkspaceSymbol>>, tower_lsp::jsonrpc::Error> {
        self.request_handler.workspace_symbol(params).await
    }
}
```

### Capability Registration

```rust
// In src/capabilities/mod.rs

pub mod workspace_symbols;

// In server initialization
use tower_lsp::lsp_types::ServerCapabilities;

fn create_server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        workspace_symbol_provider: Some(OneOf::Left(true)),
        // ... other capabilities
        ..Default::default()
    }
}
```

## Performance Optimizations

### 1. Efficient Index Traversal
- Use iterator chains to avoid intermediate collections
- Implement early termination when limits are reached
- Cache frequently accessed data structures

### 2. String Matching Optimization
```rust
impl SymbolMatcher {
    fn optimized_search(&self, index: &RubyIndex, query: &WorkspaceSymbolQuery) -> Vec<WorkspaceSymbolResult> {
        // For exact matches, try direct lookup first
        if let Some(entries) = self.try_exact_lookup(index, query) {
            return entries;
        }

        // For prefix matches, use optimized prefix search
        if self.is_simple_prefix_query(query) {
            return self.prefix_search(index, query);
        }

        // Fall back to full search for complex patterns
        self.full_search(index, query)
    }

    fn try_exact_lookup(&self, index: &RubyIndex, query: &WorkspaceSymbolQuery) -> Option<Vec<WorkspaceSymbolResult>> {
        // Try to find exact FQN matches
        // This is O(1) lookup in the definitions map
        todo!()
    }
}
```

### 3. Memory Management
- Reuse existing Entry objects from RubyIndex
- Avoid unnecessary string allocations
- Implement result streaming for very large result sets

## Error Handling

### Graceful Degradation
```rust
impl SymbolSearchEngine {
    pub fn search_with_fallback(
        &self,
        index: &RubyIndex,
        query: &WorkspaceSymbolQuery,
    ) -> Vec<WorkspaceSymbolResult> {
        match self.search(index, query) {
            Ok(results) => results,
            Err(e) => {
                log::warn!("Symbol search failed: {}, falling back to simple search", e);
                self.simple_search(index, query)
            }
        }
    }

    fn simple_search(&self, index: &RubyIndex, query: &WorkspaceSymbolQuery) -> Vec<WorkspaceSymbolResult> {
        // Simplified search that's guaranteed to work
        // Even if regex or complex matching fails
        todo!()
    }
}
```

### Input Validation
```rust
impl WorkspaceSymbolQuery {
    pub fn validate(&self) -> Result<(), String> {
        if self.query.is_empty() {
            return Err("Query cannot be empty".to_string());
        }

        if self.query.len() > 1000 {
            return Err("Query too long".to_string());
        }

        if self.use_regex {
            // Validate regex syntax
            if let Err(e) = Regex::new(&self.query) {
                return Err(format!("Invalid regex: {}", e));
            }
        }

        Ok(())
    }
}
```

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match_search() {
        let mut index = RubyIndex::new();
        // Add test entries...
        
        let query = WorkspaceSymbolQuery::new("TestClass".to_string());
        let engine = SymbolSearchEngine::new();
        let results = engine.search(&index, &query);
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "TestClass");
    }

    #[test]
    fn test_partial_match_search() {
        // Test partial matching functionality
    }

    #[test]
    fn test_method_search() {
        // Test method-specific search using methods_by_name
    }

    #[test]
    fn test_ranking() {
        // Test that results are properly ranked by relevance
    }
}
```

### Integration Tests
```rust
#[test]
fn test_workspace_symbol_end_to_end() {
    // Test full LSP request/response cycle
    // Including server setup, index population, and symbol search
}
```

### Performance Tests
```rust
#[test]
fn test_large_workspace_performance() {
    // Test with workspace containing 10,000+ symbols
    // Verify search completes within 50ms
}
```

## Future Enhancements

### 1. Semantic Search
- Integration with type information for more accurate results
- Context-aware symbol suggestions

### 2. Cross-Reference Integration
- Include reference count in symbol information
- Show usage patterns in symbol details

### 3. Advanced Filtering
- Filter by file patterns
- Filter by symbol visibility
- Filter by modification time

### 4. Workspace-Wide Refactoring Support
- Symbol rename across workspace
- Symbol usage analysis

## Implementation Phases

### Phase 1: Basic Implementation
- Core search engine with substring matching
- Integration with RubyIndex.definitions
- Basic LSP protocol support

### Phase 2: Enhanced Matching
- Regex support
- Camel case matching
- Improved relevance scoring

### Phase 3: Performance Optimization
- Caching strategies
- Optimized data structures
- Memory usage optimization

### Phase 4: Advanced Features
- Symbol filtering
- Enhanced ranking
- Integration with other LSP features

This design provides a solid foundation for implementing workspace symbols that leverages the existing RubyIndex infrastructure while providing fast, accurate symbol search capabilities across the entire workspace.