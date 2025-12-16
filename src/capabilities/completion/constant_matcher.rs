use crate::indexer::entry::{entry_kind::EntryKind, Entry};
use crate::types::fully_qualified_name::FullyQualifiedName;

/// Handles pattern matching for constant names
pub struct ConstantMatcher {
    /// Configuration for matching behavior
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

    /// Check if an entry matches the given partial name
    pub fn matches(&self, entry: &Entry, fqn: &FullyQualifiedName, partial: &str) -> bool {
        let constant_name = self.extract_name(&entry.kind, fqn);

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

    fn extract_name(&self, kind: &EntryKind, fqn: &FullyQualifiedName) -> String {
        match kind {
            EntryKind::Class(_) | EntryKind::Module(_) | EntryKind::Constant(_) => fqn.name(),
            _ => fqn.to_string(),
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
        let uppercase_chars: String = name.chars().filter(|c| c.is_uppercase()).collect();

        if self.case_sensitive {
            uppercase_chars.starts_with(partial)
        } else {
            uppercase_chars
                .to_lowercase()
                .starts_with(&partial.to_lowercase())
        }
    }

    /// Configure case sensitivity
    pub fn with_case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    /// Configure fuzzy matching
    pub fn with_fuzzy_matching(mut self, fuzzy_matching: bool) -> Self {
        self.fuzzy_matching = fuzzy_matching;
        self
    }

    /// Configure camel case matching
    pub fn with_camel_case_matching(mut self, camel_case_matching: bool) -> Self {
        self.camel_case_matching = camel_case_matching;
        self
    }
}

impl Default for ConstantMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        indexer::{
            entry::{entry_kind::EntryKind, Entry},
            index::FqnId,
        },
        types::fully_qualified_name::FullyQualifiedName,
    };

    fn create_test_entry(name: &str, kind: EntryKind) -> (Entry, FullyQualifiedName) {
        let fqn = FullyQualifiedName::try_from(name).unwrap();
        let entry = Entry {
            fqn_id: FqnId::default(),
            kind,
            location: crate::types::compact_location::CompactLocation::default(),
        };
        (entry, fqn)
    }

    #[test]
    fn test_prefix_match() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry("String", EntryKind::new_class(None));

        assert!(matcher.matches(&entry, &fqn, "Str"));
        assert!(matcher.matches(&entry, &fqn, "String"));
        assert!(!matcher.matches(&entry, &fqn, "Array"));
    }

    #[test]
    fn test_case_insensitive_match() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry("String", EntryKind::new_class(None));

        assert!(matcher.matches(&entry, &fqn, "str"));
        assert!(matcher.matches(&entry, &fqn, "STRING"));
        assert!(matcher.matches(&entry, &fqn, "StRiNg"));
    }

    #[test]
    fn test_fuzzy_match() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry("ActiveRecord", EntryKind::new_module());

        assert!(matcher.matches(&entry, &fqn, "AcRe"));
        assert!(matcher.matches(&entry, &fqn, "ActRec"));
        assert!(matcher.matches(&entry, &fqn, "AR")); // This should work via camel case matching
    }

    #[test]
    fn test_camel_case_match() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry("ActiveRecord", EntryKind::new_module());

        assert!(matcher.matches(&entry, &fqn, "AR"));
        assert!(matcher.matches(&entry, &fqn, "A"));
        assert!(!matcher.matches(&entry, &fqn, "B"));
    }

    #[test]
    fn test_empty_partial() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry("String", EntryKind::new_class(None));

        assert!(matcher.matches(&entry, &fqn, ""));
    }

    #[test]
    fn test_constant_entry() {
        let matcher = ConstantMatcher::new();
        let (entry, fqn) = create_test_entry(
            "MY_CONSTANT",
            EntryKind::new_constant(Some("42".to_string()), None),
        );

        assert!(matcher.matches(&entry, &fqn, "MY"));
        assert!(matcher.matches(&entry, &fqn, "CONST"));
        assert!(matcher.matches(&entry, &fqn, "MC")); // CamelCase matching
    }
}
