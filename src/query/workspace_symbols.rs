//! Workspace Symbol Query — Searches the index for symbols matching a query.
//!
//! Supports multiple matching strategies: exact, prefix, camel case, and fuzzy
//! subsequence matching. Results are ranked by relevance.

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::Entry;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_analysis_core::{SymbolFact, SymbolKind as AnalysisSymbolKind};
use tower_lsp::lsp_types::{SymbolInformation, SymbolKind};

use super::analysis_location::location_for_range;
use super::IndexQuery;

// ============================================================================
// IndexQuery entry points
// ============================================================================

impl IndexQuery {
    pub fn has_analysis_symbols(&self) -> bool {
        let Some(engine) = self.analysis_engine() else {
            return false;
        };
        !engine.lock().all_symbol_facts().is_empty()
    }

    /// Return a limited set of top-level symbols (for empty queries).
    pub fn get_top_level_symbols(&self) -> Vec<SymbolInformation> {
        if let Some(symbols) = self.top_level_symbols_from_analysis() {
            return symbols;
        }

        let index = self.index.lock();
        let mut symbols = Vec::new();
        let mut count = 0;
        const MAX_SYMBOLS: usize = 50;

        for (fqn, entries) in index.definitions() {
            if count >= MAX_SYMBOLS {
                break;
            }

            match fqn {
                FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
                    if parts.len() == 1 {
                        if let Some(entry) = entries.first() {
                            if let Some(symbol) = convert_entry_to_symbol_information(entry, &index)
                            {
                                symbols.push(symbol);
                                count += 1;
                            }
                        }
                    }
                }
                FullyQualifiedName::Method(_, _)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => {}
            }
        }

        symbols
    }

    /// Search the index for symbols matching the given query string.
    ///
    /// Supports exact, prefix, camel case, and fuzzy subsequence matching.
    /// Results are ranked by relevance and limited to 100.
    pub fn search_workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        if let Some(symbols) = self.search_workspace_symbols_from_analysis(query) {
            return symbols;
        }

        let index = self.index.lock();
        let matcher = SymbolMatcher::new();
        let mut results = Vec::new();

        // Search through definitions.
        for entries in index.definitions().map(|(_, e)| e) {
            for entry in entries {
                if let Some(symbol) = convert_entry_to_symbol_information(entry, &index) {
                    if let Some(relevance) = matcher.calculate_relevance(&symbol.name, query) {
                        results.push(WorkspaceSymbolResult { symbol, relevance });
                    }
                }
            }
        }

        // Search through methods by name.
        for (method, entries) in index.methods_by_name() {
            for entry in entries {
                if let Some(symbol) = convert_entry_to_symbol_information(entry, &index) {
                    let method_name = method.get_name();
                    if let Some(relevance) = matcher.calculate_relevance(&method_name, query) {
                        results.push(WorkspaceSymbolResult { symbol, relevance });
                    }
                }
            }
        }

        // Sort by relevance (highest first) and limit results.
        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(100);

        results.into_iter().map(|r| r.symbol).collect()
    }

    fn top_level_symbols_from_analysis(&self) -> Option<Vec<SymbolInformation>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let mut symbols = Vec::new();
        const MAX_SYMBOLS: usize = 50;

        for fact in engine.all_symbol_facts() {
            if symbols.len() >= MAX_SYMBOLS {
                break;
            }
            match &fact.fqn {
                FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
                    if parts.len() == 1 {
                        if let Some(symbol) =
                            convert_symbol_fact_to_symbol_information(&fact, &engine)
                        {
                            symbols.push(symbol);
                        }
                    }
                }
                FullyQualifiedName::Method(_, _)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => {}
            }
        }

        Some(symbols)
    }

    fn search_workspace_symbols_from_analysis(
        &self,
        query: &str,
    ) -> Option<Vec<SymbolInformation>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let matcher = SymbolMatcher::new();
        let mut results = Vec::new();

        for fact in engine.all_symbol_facts() {
            let name = extract_display_name(&fact.fqn);
            let match_name = match &fact.fqn {
                FullyQualifiedName::Method(_, method) => method.get_name(),
                FullyQualifiedName::Namespace(_, _)
                | FullyQualifiedName::Constant(_)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => name.clone(),
            };
            if let Some(symbol) = convert_symbol_fact_to_symbol_information(&fact, &engine) {
                if let Some(relevance) = matcher.calculate_relevance(&match_name, query) {
                    results.push(WorkspaceSymbolResult { symbol, relevance });
                }
            }
        }

        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(100);

        Some(results.into_iter().map(|r| r.symbol).collect())
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Intermediate result for sorting by relevance.
struct WorkspaceSymbolResult {
    symbol: SymbolInformation,
    relevance: f64,
}

/// Convert an Entry to SymbolInformation, filtering out local variables.
fn convert_entry_to_symbol_information(
    entry: &Entry,
    index: &RubyIndex,
) -> Option<SymbolInformation> {
    if let EntryKind::LocalVariable(_) = &entry.kind {
        return None;
    }

    let fqn = index.get_fqn(entry.fqn_id)?;
    let name = extract_display_name(fqn);
    let kind = entry_kind_to_symbol_kind(&entry.kind);
    let container_name = extract_container_name(fqn);
    let location = index.to_lsp_location(&entry.location)?;

    Some(SymbolInformation {
        name,
        kind,
        tags: None,
        #[allow(deprecated)]
        deprecated: Some(false),
        location,
        container_name,
    })
}

fn convert_symbol_fact_to_symbol_information(
    fact: &SymbolFact,
    engine: &ruby_analysis_engine::AnalysisEngine,
) -> Option<SymbolInformation> {
    if matches!(fact.kind, AnalysisSymbolKind::LocalVariable) {
        return None;
    }

    Some(SymbolInformation {
        name: extract_display_name(&fact.fqn),
        kind: analysis_symbol_kind_to_lsp_kind(fact.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: Some(false),
        location: location_for_range(engine, fact.range)?,
        container_name: extract_container_name(&fact.fqn),
    })
}

fn analysis_symbol_kind_to_lsp_kind(kind: AnalysisSymbolKind) -> SymbolKind {
    match kind {
        AnalysisSymbolKind::Class => SymbolKind::CLASS,
        AnalysisSymbolKind::Module => SymbolKind::MODULE,
        AnalysisSymbolKind::Method => SymbolKind::METHOD,
        AnalysisSymbolKind::Constant => SymbolKind::CONSTANT,
        AnalysisSymbolKind::LocalVariable
        | AnalysisSymbolKind::InstanceVariable
        | AnalysisSymbolKind::ClassVariable
        | AnalysisSymbolKind::GlobalVariable => SymbolKind::VARIABLE,
    }
}

fn entry_kind_to_symbol_kind(kind: &EntryKind) -> SymbolKind {
    match kind {
        EntryKind::Class(_) => SymbolKind::CLASS,
        EntryKind::Module(_) => SymbolKind::MODULE,
        EntryKind::Method(_) => SymbolKind::METHOD,
        EntryKind::Constant(_) => SymbolKind::CONSTANT,
        EntryKind::LocalVariable(_)
        | EntryKind::InstanceVariable(_)
        | EntryKind::ClassVariable(_)
        | EntryKind::GlobalVariable(_) => SymbolKind::VARIABLE,
        EntryKind::Reference(_) => SymbolKind::KEY,
    }
}

fn extract_display_name(fqn: &FullyQualifiedName) -> String {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
            parts.last().map(|c| c.to_string()).unwrap_or_default()
        }
        FullyQualifiedName::Method(_, method) => method.get_name(),
        FullyQualifiedName::LocalVariable(name) => name.to_string(),
        FullyQualifiedName::InstanceVariable(name) => name.to_string(),
        FullyQualifiedName::ClassVariable(name) => name.to_string(),
        FullyQualifiedName::GlobalVariable(name) => name.to_string(),
    }
}

fn extract_container_name(fqn: &FullyQualifiedName) -> Option<String> {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
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
        FullyQualifiedName::LocalVariable(_) => None,
        FullyQualifiedName::InstanceVariable(_) => None,
        FullyQualifiedName::ClassVariable(_) => None,
        FullyQualifiedName::GlobalVariable(_) => None,
    }
}

// ============================================================================
// Symbol matcher
// ============================================================================

/// Calculates relevance scores for symbol matches using multiple strategies.
struct SymbolMatcher;

impl SymbolMatcher {
    fn new() -> Self {
        Self
    }

    fn calculate_relevance(&self, symbol_name: &str, pattern: &str) -> Option<f64> {
        if pattern.is_empty() {
            return Some(0.1);
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

        // Fuzzy subsequence match
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

    /// Fuzzy matching using subsequence algorithm.
    fn fuzzy_match(&self, symbol: &str, pattern: &str) -> Option<f64> {
        let symbol_chars: Vec<char> = symbol.chars().collect();
        let pattern_chars: Vec<char> = pattern.chars().collect();

        if pattern_chars.is_empty() {
            return Some(0.1);
        }

        if pattern_chars.len() > symbol_chars.len() {
            return None;
        }

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

        if pattern_idx < pattern_chars.len() {
            return None;
        }

        let score = self.calculate_fuzzy_score(&matches, symbol_chars.len(), pattern_chars.len());

        if score > 0.2 {
            Some(score)
        } else {
            None
        }
    }

    fn calculate_fuzzy_score(
        &self,
        matches: &[usize],
        symbol_len: usize,
        pattern_len: usize,
    ) -> f64 {
        if matches.is_empty() {
            return 0.0;
        }

        let coverage_score = pattern_len as f64 / symbol_len as f64;

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

        if consecutive_count > 1 {
            consecutive_bonus += (consecutive_count as f64 - 1.0) * 0.1;
        }

        let early_match_bonus = if matches[0] == 0 { 0.2 } else { 0.0 };

        let mut gap_penalty = 0.0;
        for i in 1..matches.len() {
            let gap = matches[i] - matches[i - 1] - 1;
            gap_penalty += gap as f64 * 0.01;
        }

        let raw_score = coverage_score + consecutive_bonus + early_match_bonus - gap_penalty;
        (raw_score * 0.45 + 0.3).clamp(0.3, 0.75)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::{Mutex, RwLock};
    use ruby_analysis_core::{
        FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SymbolFact,
        SymbolKind as AnalysisSymbolKind, TextRange,
    };
    use ruby_analysis_engine::AnalysisEngine;

    use super::*;
    use crate::indexer::index_ref::Index;

    fn query_with_analysis_symbols() -> IndexQuery {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("/tmp/user.rb", source);
        assert_eq!(
            file_id,
            SourceFileId(0),
            "INVARIANT VIOLATED: first test analysis file id changed. \
             This is a bug because this test assumes a fresh AnalysisEngine. \
             Fix: update the expected file id or avoid asserting it."
        );

        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::namespace(vec![user.clone()]),
            AnalysisSymbolKind::Class,
            TextRange::new(file_id, 6, 10),
        ));
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::method(
                vec![user],
                RubyMethod::new("name").expect("test method must be valid"),
            ),
            AnalysisSymbolKind::Method,
            TextRange::new(file_id, 17, 21),
        ));

        IndexQuery::with_engine(
            Index::new(Arc::new(RwLock::new(RubyIndex::new()))),
            Arc::new(Mutex::new(engine)),
        )
    }

    #[test]
    fn workspace_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.search_workspace_symbols("name");

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::METHOD);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_can_read_analysis_engine_without_index_entries() {
        let query = query_with_analysis_symbols();

        let symbols = query.get_top_level_symbols();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::CLASS);
    }

    #[test]
    fn test_symbol_matcher_relevance() {
        let matcher = SymbolMatcher::new();

        assert_eq!(matcher.calculate_relevance("test", "test"), Some(1.0));
        assert_eq!(matcher.calculate_relevance("Test", "test"), Some(0.9));
        assert_eq!(matcher.calculate_relevance("testing", "test"), Some(0.8));
        assert_eq!(matcher.calculate_relevance("foo", "bar"), None);
    }

    #[test]
    fn test_fuzzy_matching() {
        let matcher = SymbolMatcher::new();

        let result = matcher.calculate_relevance("showthemeshelper", "showthemehelper");
        assert!(result.is_some());
        assert!(result.unwrap() > 0.3);

        assert!(matcher
            .calculate_relevance("ApplicationController", "AppCtrl")
            .is_some());
        assert!(matcher
            .calculate_relevance("user_authentication", "userauth")
            .is_some());
        assert!(matcher
            .calculate_relevance("get_user_by_id", "getuid")
            .is_some());

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

        let consecutive = matcher.fuzzy_match("abcdef", "abc").unwrap();
        let scattered = matcher.fuzzy_match("azbycx", "abc").unwrap();
        assert!(consecutive > scattered);

        let early = matcher.fuzzy_match("abcxyz", "abc").unwrap();
        let late = matcher.fuzzy_match("xyzabc", "abc").unwrap();
        assert!(early > late);
    }
}
