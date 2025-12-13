use crate::indexer::entry::Entry;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use log::info;
use std::time::Instant;
use tower_lsp::lsp_types::{SymbolInformation, SymbolKind, WorkspaceSymbolParams};

/// Handle workspace symbol requests
///
/// This function processes workspace symbol requests by searching through the Ruby index
/// for symbols that match the given query pattern. It supports multiple matching strategies
/// including exact matches, prefix matches, camel case matching, and fuzzy subsequence matching.
/// Results are ranked by relevance with exact matches scoring highest.
///
/// **Fuzzy Search Support:**
/// - Subsequence matching: "showthemehelper" matches "showthemeshelper"
/// - Camel case abbreviations: "AppCtrl" matches "ApplicationController"
/// - Partial word matching: "userauth" matches "user_authentication"
///
/// **Included symbol types:**
/// - Classes
/// - Modules  
/// - Methods
/// - Constants
/// - Class variables (@@var)
/// - Instance variables (@var)
/// - Global variables ($var)
///
/// **Excluded symbol types:**
/// - Local variables (to reduce noise and improve performance)
pub async fn handle_workspace_symbols(
    lang_server: &RubyLanguageServer,
    params: WorkspaceSymbolParams,
) -> Option<Vec<SymbolInformation>> {
    let query = params.query;
    info!("Workspace symbols request for query: '{}'", query);

    let start_time = Instant::now();

    // If query is empty, return a limited set of top-level symbols
    if query.is_empty() {
        let symbols = get_top_level_symbols(lang_server).await;
        info!(
            "Empty query - returned {} top-level symbols in {:?}",
            symbols.len(),
            start_time.elapsed()
        );
        return Some(symbols);
    }

    // Parse the query
    let search_query = WorkspaceSymbolQuery::new(&query);

    // Perform the search
    let search_engine = SymbolSearchEngine::new();
    let symbols = search_engine.search(lang_server, &search_query).await;

    info!(
        "Workspace symbols search completed in {:?} - found {} symbols",
        start_time.elapsed(),
        symbols.len()
    );

    Some(symbols)
}

/// Get a limited set of top-level symbols for empty queries
async fn get_top_level_symbols(lang_server: &RubyLanguageServer) -> Vec<SymbolInformation> {
    let index = lang_server.index.lock();
    let mut symbols = Vec::new();

    // Limit to 50 top-level symbols to avoid overwhelming the client
    let mut count = 0;
    const MAX_SYMBOLS: usize = 50;

    for (fqn, entries) in index.definitions() {
        if count >= MAX_SYMBOLS {
            break;
        }

        // Only include top-level classes and modules
        if let FullyQualifiedName::Constant(parts) = fqn {
            if parts.len() == 1 {
                if let Some(entry) = entries.first() {
                    if let Some(symbol) = convert_entry_to_symbol_information(entry) {
                        symbols.push(symbol);
                        count += 1;
                    }
                }
            }
        }
    }

    symbols
}

/// Query structure for workspace symbol search
#[derive(Debug)]
struct WorkspaceSymbolQuery {
    pattern: String,
}

impl WorkspaceSymbolQuery {
    fn new(query: &str) -> Self {
        Self {
            pattern: query.to_string(),
        }
    }
}

/// Result structure for workspace symbol search
#[derive(Debug)]
struct WorkspaceSymbolResult {
    symbol: SymbolInformation,
    relevance: f64,
}

/// Main search engine for workspace symbols
struct SymbolSearchEngine;

impl SymbolSearchEngine {
    fn new() -> Self {
        Self
    }

    async fn search(
        &self,
        lang_server: &RubyLanguageServer,
        query: &WorkspaceSymbolQuery,
    ) -> Vec<SymbolInformation> {
        let index = lang_server.index.lock();
        let mut results = Vec::new();
        let matcher = SymbolMatcher::new();

        // Search through definitions
        for entries in index.definitions().map(|(_, e)| e) {
            for entry in entries {
                if let Some(symbol) = convert_entry_to_symbol_information(entry) {
                    if let Some(relevance) =
                        matcher.calculate_relevance(&symbol.name, &query.pattern)
                    {
                        results.push(WorkspaceSymbolResult { symbol, relevance });
                    }
                }
            }
        }

        // Search through methods by name
        for (method, entries) in index.methods_by_name() {
            for entry in entries {
                if let Some(symbol) = convert_entry_to_symbol_information(entry) {
                    let method_name = method.get_name();
                    if let Some(relevance) =
                        matcher.calculate_relevance(&method_name, &query.pattern)
                    {
                        results.push(WorkspaceSymbolResult { symbol, relevance });
                    }
                }
            }
        }

        // Sort by relevance (highest first) and limit results
        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(100); // Limit to 100 results

        results.into_iter().map(|r| r.symbol).collect()
    }
}

/// Symbol matcher for calculating relevance scores
struct SymbolMatcher;

impl SymbolMatcher {
    fn new() -> Self {
        Self
    }

    fn calculate_relevance(&self, symbol_name: &str, pattern: &str) -> Option<f64> {
        if pattern.is_empty() {
            return Some(0.1); // Low relevance for empty pattern
        }

        let symbol_lower = symbol_name.to_lowercase();
        let pattern_lower = pattern.to_lowercase();

        // Exact match (highest score)
        if symbol_name == pattern {
            return Some(1.0);
        }

        // Case-insensitive exact match
        if symbol_lower == pattern_lower {
            return Some(0.9);
        }

        // Prefix match
        if symbol_lower.starts_with(&pattern_lower) {
            return Some(0.8);
        }

        // Camel case match
        if let Some(score) = self.camel_case_match(symbol_name, pattern) {
            return Some(score);
        }

        // Fuzzy subsequence match (e.g., "showthemehelper" matches "showthemeshelper")
        if let Some(score) = self.fuzzy_match(&symbol_lower, &pattern_lower) {
            return Some(score);
        }

        // Word boundary match
        if self.word_boundary_match(&symbol_lower, &pattern_lower) {
            return Some(0.6);
        }

        // Substring match
        if symbol_lower.contains(&pattern_lower) {
            return Some(0.4);
        }

        None
    }

    fn camel_case_match(&self, symbol_name: &str, pattern: &str) -> Option<f64> {
        let symbol_caps: String = symbol_name.chars().filter(|c| c.is_uppercase()).collect();
        let pattern_caps: String = pattern.chars().filter(|c| c.is_uppercase()).collect();

        if !pattern_caps.is_empty() && symbol_caps.starts_with(&pattern_caps) {
            Some(0.7)
        } else {
            None
        }
    }

    fn word_boundary_match(&self, symbol_lower: &str, pattern_lower: &str) -> bool {
        symbol_lower
            .split('_')
            .any(|word| word.starts_with(pattern_lower))
    }

    /// Fuzzy matching using subsequence algorithm
    /// Returns a score based on how well the pattern matches as a subsequence
    fn fuzzy_match(&self, symbol: &str, pattern: &str) -> Option<f64> {
        let symbol_chars: Vec<char> = symbol.chars().collect();
        let pattern_chars: Vec<char> = pattern.chars().collect();

        if pattern_chars.is_empty() {
            return Some(0.1);
        }

        if pattern_chars.len() > symbol_chars.len() {
            return None;
        }

        // Check if pattern is a subsequence of symbol
        let mut pattern_idx = 0;
        let mut symbol_idx = 0;
        let mut matches = Vec::new();

        while pattern_idx < pattern_chars.len() && symbol_idx < symbol_chars.len() {
            if pattern_chars[pattern_idx] == symbol_chars[symbol_idx] {
                matches.push(symbol_idx);
                pattern_idx += 1;
            }
            symbol_idx += 1;
        }

        // If we didn't match all pattern characters, it's not a subsequence
        if pattern_idx < pattern_chars.len() {
            return None;
        }

        // Calculate score based on match quality
        let score = self.calculate_fuzzy_score(&matches, symbol_chars.len(), pattern_chars.len());

        // Only return scores that are meaningful (above a threshold)
        if score > 0.2 {
            Some(score)
        } else {
            None
        }
    }

    /// Calculate fuzzy match score based on match positions
    fn calculate_fuzzy_score(
        &self,
        matches: &[usize],
        symbol_len: usize,
        pattern_len: usize,
    ) -> f64 {
        if matches.is_empty() {
            return 0.0;
        }

        // Base score based on pattern coverage
        let coverage_score = pattern_len as f64 / symbol_len as f64;

        // Bonus for consecutive matches
        let mut consecutive_bonus = 0.0;
        let mut consecutive_count = 1;

        for i in 1..matches.len() {
            if matches[i] == matches[i - 1] + 1 {
                consecutive_count += 1;
            } else {
                if consecutive_count > 1 {
                    consecutive_bonus += (consecutive_count as f64 - 1.0) * 0.1;
                }
                consecutive_count = 1;
            }
        }

        // Final bonus for any remaining consecutive sequence
        if consecutive_count > 1 {
            consecutive_bonus += (consecutive_count as f64 - 1.0) * 0.1;
        }

        // Bonus for early matches (matches at the beginning are better)
        let early_match_bonus = if matches[0] == 0 { 0.2 } else { 0.0 };

        // Penalty for gaps between matches
        let mut gap_penalty = 0.0;
        for i in 1..matches.len() {
            let gap = matches[i] - matches[i - 1] - 1;
            gap_penalty += gap as f64 * 0.01;
        }

        // Combine all factors, ensuring we stay within fuzzy match range (0.3-0.75)
        let raw_score = coverage_score + consecutive_bonus + early_match_bonus - gap_penalty;

        // Clamp to fuzzy match range
        (raw_score * 0.45 + 0.3).clamp(0.3, 0.75)
    }
}

/// Convert an Entry to SymbolInformation, filtering out unwanted symbol types
fn convert_entry_to_symbol_information(entry: &Entry) -> Option<SymbolInformation> {
    use crate::indexer::entry::entry_kind::EntryKind;

    // Filter out local variables - only include class/modules/methods/constants/class_var/instance_var/global_var
    if let EntryKind::LocalVariable { .. } = &entry.kind {
        return None; // Exclude local variables
    }

    let name = extract_display_name(&entry.fqn);
    let kind = entry_kind_to_symbol_kind(&entry.kind);
    let container_name = extract_container_name(&entry.fqn);

    Some(SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: Some(false),
        location: entry.location.clone(),
        container_name,
    })
}

/// Convert EntryKind to LSP SymbolKind
fn entry_kind_to_symbol_kind(kind: &crate::indexer::entry::entry_kind::EntryKind) -> SymbolKind {
    use crate::indexer::entry::entry_kind::EntryKind;

    match kind {
        EntryKind::Class { .. } => SymbolKind::CLASS,
        EntryKind::Module { .. } => SymbolKind::MODULE,
        EntryKind::Method { .. } => SymbolKind::METHOD,
        EntryKind::Constant { .. } => SymbolKind::CONSTANT,
        EntryKind::LocalVariable { .. }
        | EntryKind::InstanceVariable { .. }
        | EntryKind::ClassVariable { .. }
        | EntryKind::GlobalVariable { .. } => SymbolKind::VARIABLE,
        EntryKind::Reference { .. } => SymbolKind::KEY, // References use KEY symbol
    }
}

/// Extract display name from FullyQualifiedName
fn extract_display_name(fqn: &FullyQualifiedName) -> String {
    match fqn {
        FullyQualifiedName::Constant(parts) => {
            parts.last().map(|c| c.to_string()).unwrap_or_default()
        }
        FullyQualifiedName::Method(_, method) => method.get_name(),
        FullyQualifiedName::LocalVariable(name, _) => name.to_string(),
        FullyQualifiedName::InstanceVariable(name) => name.to_string(),
        FullyQualifiedName::ClassVariable(name) => name.to_string(),
        FullyQualifiedName::GlobalVariable(name) => name.to_string(),
    }
}

/// Extract container name from FullyQualifiedName
fn extract_container_name(fqn: &FullyQualifiedName) -> Option<String> {
    match fqn {
        FullyQualifiedName::Constant(parts) => {
            if parts.len() > 1 {
                let container_parts: Vec<String> = parts[..parts.len() - 1]
                    .iter()
                    .map(|c| c.to_string())
                    .collect();
                Some(container_parts.join("::"))
            } else {
                None
            }
        }
        FullyQualifiedName::Method(namespace, _) => {
            if !namespace.is_empty() {
                let container_parts: Vec<String> =
                    namespace.iter().map(|c| c.to_string()).collect();
                Some(container_parts.join("::"))
            } else {
                None
            }
        }
        FullyQualifiedName::LocalVariable(_, _) => None,
        FullyQualifiedName::InstanceVariable(_) => None,
        FullyQualifiedName::ClassVariable(_) => None,
        FullyQualifiedName::GlobalVariable(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_symbol_query_creation() {
        let query = WorkspaceSymbolQuery::new("test");
        assert_eq!(query.pattern, "test");

        let query = WorkspaceSymbolQuery::new("TestClass");
        assert_eq!(query.pattern, "TestClass");
    }

    #[test]
    fn test_symbol_matcher_relevance() {
        let matcher = SymbolMatcher::new();

        // Exact match
        assert_eq!(matcher.calculate_relevance("test", "test"), Some(1.0));

        // Case insensitive exact match
        assert_eq!(matcher.calculate_relevance("Test", "test"), Some(0.9));

        // Prefix match
        assert_eq!(matcher.calculate_relevance("testing", "test"), Some(0.8));

        // No match
        assert_eq!(matcher.calculate_relevance("foo", "bar"), None);
    }

    #[test]
    fn test_fuzzy_matching() {
        let matcher = SymbolMatcher::new();

        // Test the specific case mentioned: "showthemehelper" should match "showthemeshelper"
        let result = matcher.calculate_relevance("showthemeshelper", "showthemehelper");
        assert!(result.is_some());
        assert!(result.unwrap() > 0.3); // Should be in fuzzy match range

        // Test other fuzzy cases
        assert!(matcher
            .calculate_relevance("ApplicationController", "AppCtrl")
            .is_some());
        assert!(matcher
            .calculate_relevance("user_authentication", "userauth")
            .is_some());
        assert!(matcher
            .calculate_relevance("get_user_by_id", "getuid")
            .is_some());

        // Test cases that shouldn't match
        assert!(matcher
            .calculate_relevance("completely_different", "xyz")
            .is_none());
        assert!(matcher
            .calculate_relevance("short", "verylongpattern")
            .is_none());
    }

    #[test]
    fn test_fuzzy_match_scoring() {
        let matcher = SymbolMatcher::new();

        // Consecutive matches should score higher than scattered matches
        let consecutive = matcher.fuzzy_match("abcdef", "abc").unwrap();
        let scattered = matcher.fuzzy_match("azbycx", "abc").unwrap();
        assert!(consecutive > scattered);

        // Early matches should score higher
        let early = matcher.fuzzy_match("abcxyz", "abc").unwrap();
        let late = matcher.fuzzy_match("xyzabc", "abc").unwrap();
        assert!(early > late);
    }
}
