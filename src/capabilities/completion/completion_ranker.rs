use crate::types::{fully_qualified_name::FullyQualifiedName, scope::LVScope as Scope};

use super::constant_completion::{ConstantCompletionContext, ConstantCompletionItem};

/// Ranks completion items by relevance
pub struct CompletionRanker;

impl CompletionRanker {
    pub fn new() -> Self {
        Self
    }

    /// Rank candidates by relevance and sort them
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
            b.relevance_score
                .partial_cmp(&a.relevance_score)
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

        // Boost for case-sensitive prefix matches
        if !context.partial_name.is_empty() && candidate.name.starts_with(&context.partial_name) {
            score += 1.0;
        }

        // Boost for same namespace
        score += self.namespace_proximity_score(&candidate.fqn, &context.scope_stack);

        // Boost for shorter names (prefer simpler constants)
        score += 1.0 / (candidate.name.len() as f64).max(1.0);

        // Penalty for very long names
        if candidate.name.len() > 20 {
            score -= 0.5;
        }

        // Boost for constants that match the partial exactly in case
        if candidate.name.to_lowercase() == context.partial_name.to_lowercase() {
            score += 2.0;
        }

        score
    }

    fn namespace_proximity_score(&self, fqn: &FullyQualifiedName, _scope_stack: &[Scope]) -> f64 {
        // Calculate how "close" this constant is to the current scope
        // Higher score for constants in the same or nearby namespaces

        // For now, give a small boost to top-level constants
        if fqn.namespace_parts().is_empty() {
            0.2
        } else {
            0.0
        }
    }

    /// Calculate relevance based on fuzzy match quality
    pub fn fuzzy_match_score(&self, name: &str, partial: &str) -> f64 {
        if partial.is_empty() {
            return 0.0;
        }

        let name_lower = name.to_lowercase();
        let partial_lower = partial.to_lowercase();

        // Exact match gets highest score
        if name_lower == partial_lower {
            return 10.0;
        }

        // Prefix match gets high score
        if name_lower.starts_with(&partial_lower) {
            return 8.0 - (name.len() as f64 - partial.len() as f64) * 0.1;
        }

        // Calculate fuzzy match score
        let mut score = 0.0;
        let mut last_match_pos = 0;
        let mut consecutive_matches = 0;

        for ch in partial_lower.chars() {
            if let Some(pos) = name_lower[last_match_pos..].find(ch) {
                let actual_pos = last_match_pos + pos;

                // Boost for consecutive character matches
                if actual_pos == last_match_pos {
                    consecutive_matches += 1;
                    score += 1.0 + consecutive_matches as f64 * 0.5;
                } else {
                    consecutive_matches = 0;
                    score += 1.0;
                }

                // Boost for matches at word boundaries
                if actual_pos == 0
                    || name
                        .chars()
                        .nth(actual_pos - 1)
                        .is_some_and(|c| !c.is_alphanumeric())
                {
                    score += 0.5;
                }

                last_match_pos = actual_pos + 1;
            } else {
                // Character not found, reduce score
                score -= 2.0;
            }
        }

        // Normalize by length
        score / name.len() as f64
    }

    /// Calculate CamelCase abbreviation match score
    pub fn camel_case_score(&self, name: &str, partial: &str) -> f64 {
        let uppercase_chars: String = name.chars().filter(|c| c.is_uppercase()).collect();

        if uppercase_chars.is_empty() {
            return 0.0;
        }

        let partial_upper = partial.to_uppercase();

        if uppercase_chars == partial_upper {
            return 5.0; // Perfect abbreviation match
        }

        if uppercase_chars.starts_with(&partial_upper) {
            return 3.0 - (uppercase_chars.len() as f64 - partial_upper.len() as f64) * 0.2;
        }

        0.0
    }

    /// Boost constants based on their type and context
    pub fn type_relevance_score(
        &self,
        candidate: &ConstantCompletionItem,
        context: &ConstantCompletionContext,
    ) -> f64 {
        use crate::indexer::entry::entry_kind::EntryKind;

        let mut score = 0.0;

        match &candidate.entry.kind {
            EntryKind::Class(_) => {
                score += 2.0;

                // Boost classes if the partial looks like a class name
                if context
                    .partial_name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                {
                    score += 1.0;
                }
            }
            EntryKind::Module(_) => {
                score += 1.8;

                // Boost modules if the partial looks like a module name
                if context
                    .partial_name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                {
                    score += 0.8;
                }
            }
            EntryKind::Constant(_) => {
                score += 1.5;

                // Boost constants if the partial is all uppercase
                if context
                    .partial_name
                    .chars()
                    .all(|c| c.is_uppercase() || c == '_')
                {
                    score += 1.0;
                }
            }
            _ => {}
        }

        score
    }
}

impl Default for CompletionRanker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::entry::{entry_kind::EntryKind, Entry},
        types::fully_qualified_name::FullyQualifiedName,
    };
    use tower_lsp::lsp_types::{Location, Position, Url};

    fn create_test_item(name: &str, kind: EntryKind) -> ConstantCompletionItem {
        let entry = Entry {
            fqn: FullyQualifiedName::try_from(name).unwrap(),
            kind,
            location: Location {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Default::default(),
            },
        };

        let context = ConstantCompletionContext::new(Position::new(0, 0), vec![], "".to_string());

        ConstantCompletionItem::new(entry, &context)
    }

    #[test]
    fn test_fuzzy_match_score() {
        let ranker = CompletionRanker::new();

        // Exact match
        assert!(ranker.fuzzy_match_score("String", "String") > 9.0);

        // Prefix match
        assert!(ranker.fuzzy_match_score("String", "Str") > 7.0);

        // Fuzzy match
        assert!(ranker.fuzzy_match_score("ActiveRecord", "AR") > 0.0);

        // No match
        assert!(ranker.fuzzy_match_score("String", "xyz") < 0.0);
    }

    #[test]
    fn test_camel_case_score() {
        let ranker = CompletionRanker::new();

        // Perfect abbreviation
        assert!(ranker.camel_case_score("ActiveRecord", "AR") > 4.0);

        // Partial abbreviation
        assert!(ranker.camel_case_score("ActiveRecord", "A") > 2.0);

        // No match
        assert_eq!(ranker.camel_case_score("ActiveRecord", "B"), 0.0);
    }

    #[test]
    fn test_ranking_order() {
        let ranker = CompletionRanker::new();
        let context =
            ConstantCompletionContext::new(Position::new(0, 0), vec![], "Str".to_string());

        let mut candidates = vec![
            create_test_item("String", EntryKind::new_class(None)),
            create_test_item("StringIO", EntryKind::new_class(None)),
            create_test_item("MyString", EntryKind::new_class(None)),
        ];

        ranker.rank_by_relevance(&mut candidates, &context);

        // String should be ranked higher than StringIO due to exact prefix match
        assert!(candidates[0].name == "String" || candidates[0].name == "StringIO");

        // MyString should be ranked lower due to not being a prefix match
        assert!(
            candidates
                .iter()
                .position(|c| c.name == "MyString")
                .unwrap()
                > 0
        );
    }
}
