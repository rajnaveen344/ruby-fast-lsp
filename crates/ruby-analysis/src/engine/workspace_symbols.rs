use crate::core::{FullyQualifiedName, SymbolFact, SymbolKind};
use crate::engine::query::AnalysisQuery;
use crate::engine::workspace_symbol_types::WorkspaceSymbolMatch;

impl<'a> AnalysisQuery<'a> {
    pub fn top_level_symbols(&self, limit: usize) -> Vec<WorkspaceSymbolMatch> {
        let mut symbols = Vec::new();

        for fact in self.engine.all_symbol_facts() {
            if symbols.len() >= limit {
                break;
            }
            match &fact.fqn {
                FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
                    if parts.len() == 1 {
                        if let Some(symbol) = workspace_symbol_match(fact, 0.1) {
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

        symbols
    }

    pub fn search_workspace_symbols(&self, query: &str, limit: usize) -> Vec<WorkspaceSymbolMatch> {
        let matcher = SymbolMatcher::new();
        let mut results = Vec::new();

        for fact in self.engine.all_symbol_facts() {
            let name = display_name(&fact.fqn);
            let match_name = match &fact.fqn {
                FullyQualifiedName::Method(_, method) => method.get_name(),
                FullyQualifiedName::Namespace(_, _)
                | FullyQualifiedName::Constant(_)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => name.clone(),
            };
            if let Some(relevance) = matcher.calculate_relevance(&match_name, query) {
                if let Some(symbol) = workspace_symbol_match(fact, relevance) {
                    results.push(symbol);
                }
            }
        }

        results.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }
}

fn workspace_symbol_match(fact: SymbolFact, relevance: f64) -> Option<WorkspaceSymbolMatch> {
    if matches!(fact.kind, SymbolKind::LocalVariable) {
        return None;
    }

    Some(WorkspaceSymbolMatch {
        name: display_name(&fact.fqn),
        kind: fact.kind,
        range: fact.range,
        container_name: container_name(&fact.fqn),
        relevance,
    })
}

fn display_name(fqn: &FullyQualifiedName) -> String {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => parts
            .last()
            .map(|part| part.to_string())
            .unwrap_or_default(),
        FullyQualifiedName::Method(_, method) => method.get_name(),
        FullyQualifiedName::LocalVariable(name)
        | FullyQualifiedName::InstanceVariable(name)
        | FullyQualifiedName::ClassVariable(name)
        | FullyQualifiedName::GlobalVariable(name) => name.to_string(),
    }
}

fn container_name(fqn: &FullyQualifiedName) -> Option<String> {
    match fqn {
        FullyQualifiedName::Namespace(parts, _) | FullyQualifiedName::Constant(parts) => {
            if parts.len() <= 1 {
                return None;
            }
            Some(
                parts[..parts.len() - 1]
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        FullyQualifiedName::Method(namespace, _) => {
            if namespace.is_empty() {
                return None;
            }
            Some(
                namespace
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        FullyQualifiedName::LocalVariable(_)
        | FullyQualifiedName::InstanceVariable(_)
        | FullyQualifiedName::ClassVariable(_)
        | FullyQualifiedName::GlobalVariable(_) => None,
    }
}

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

        if symbol_name == pattern {
            return Some(1.0);
        }
        if symbol_lower == pattern_lower {
            return Some(0.9);
        }
        if symbol_lower.starts_with(&pattern_lower) {
            return Some(0.8);
        }
        if let Some(score) = self.camel_case_match(symbol_name, pattern) {
            return Some(score);
        }
        if let Some(score) = self.fuzzy_match(&symbol_lower, &pattern_lower) {
            return Some(score);
        }
        if self.word_boundary_match(&symbol_lower, &pattern_lower) {
            return Some(0.6);
        }
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

#[cfg(test)]
mod tests {
    use crate::core::{
        FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SymbolFact, SymbolKind,
        TextRange,
    };
    use crate::engine::AnalysisQuery;
    use crate::AnalysisEngine;

    use super::*;

    fn query_with_symbols() -> (AnalysisEngine, SourceFileId) {
        let source = "class User\n  def name\n  end\nend";
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("/tmp/user.rb", source);
        let user = RubyConstant::new("User").expect("test constant must be valid");
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::namespace(vec![user.clone()]),
            SymbolKind::Class,
            TextRange::new(file_id, 6, 10),
        ));
        engine.add_symbol_fact(SymbolFact::new(
            FullyQualifiedName::method(
                vec![user],
                RubyMethod::new("name").expect("test method must be valid"),
            ),
            SymbolKind::Method,
            TextRange::new(file_id, 17, 21),
        ));
        (engine, file_id)
    }

    #[test]
    fn workspace_symbol_search_returns_domain_matches() {
        let (engine, file_id) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        let symbols = query.search_workspace_symbols("name", 100);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[0].kind, SymbolKind::Method);
        assert_eq!(symbols[0].range.file_id, file_id);
        assert_eq!(symbols[0].container_name.as_deref(), Some("User"));
    }

    #[test]
    fn top_level_symbols_return_only_top_level_namespaces() {
        let (engine, _) = query_with_symbols();
        let query = AnalysisQuery::new(&engine);

        let symbols = query.top_level_symbols(50);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
    }

    #[test]
    fn symbol_matcher_relevance() {
        let matcher = SymbolMatcher::new();

        assert_eq!(matcher.calculate_relevance("test", "test"), Some(1.0));
        assert_eq!(matcher.calculate_relevance("Test", "test"), Some(0.9));
        assert_eq!(matcher.calculate_relevance("testing", "test"), Some(0.8));
        assert_eq!(matcher.calculate_relevance("foo", "bar"), None);
    }

    #[test]
    fn fuzzy_matching() {
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
    fn fuzzy_match_scoring() {
        let matcher = SymbolMatcher::new();

        let consecutive = matcher.fuzzy_match("abcdef", "abc").unwrap();
        let scattered = matcher.fuzzy_match("azbycx", "abc").unwrap();
        assert!(consecutive > scattered);

        let early = matcher.fuzzy_match("abcxyz", "abc").unwrap();
        let late = matcher.fuzzy_match("xyzabc", "abc").unwrap();
        assert!(early > late);
    }
}
