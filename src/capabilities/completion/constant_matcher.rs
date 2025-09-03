use crate::indexer::entry::{entry_kind::EntryKind, Entry};

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
        indexer::entry::entry_kind::EntryKind, indexer::entry::Entry,
        types::fully_qualified_name::FullyQualifiedName,
    };
    use tower_lsp::lsp_types::{Location, Url};

    fn create_test_entry(name: &str, kind: EntryKind) -> Entry {
        Entry {
            fqn: FullyQualifiedName::try_from(name).unwrap(),
            kind,
            location: Location {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Default::default(),
            },
        }
    }

    #[test]
    fn test_prefix_match() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "String",
            EntryKind::Class {
                superclass: None,
                includes: vec![],
                extends: vec![],
                prepends: vec![],
            },
        );

        assert!(matcher.matches(&entry, "Str"));
        assert!(matcher.matches(&entry, "String"));
        assert!(!matcher.matches(&entry, "Array"));
    }

    #[test]
    fn test_case_insensitive_match() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "String",
            EntryKind::Class {
                superclass: None,
                includes: vec![],
                extends: vec![],
                prepends: vec![],
            },
        );

        assert!(matcher.matches(&entry, "str"));
        assert!(matcher.matches(&entry, "STRING"));
        assert!(matcher.matches(&entry, "StRiNg"));
    }

    #[test]
    fn test_fuzzy_match() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "ActiveRecord",
            EntryKind::Module {
                includes: vec![],
                extends: vec![],
                prepends: vec![],
            },
        );

        assert!(matcher.matches(&entry, "AcRe"));
        assert!(matcher.matches(&entry, "ActRec"));
        assert!(matcher.matches(&entry, "AR")); // This should work via camel case matching
    }

    #[test]
    fn test_camel_case_match() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "ActiveRecord",
            EntryKind::Module {
                includes: vec![],
                extends: vec![],
                prepends: vec![],
            },
        );

        assert!(matcher.matches(&entry, "AR"));
        assert!(matcher.matches(&entry, "A"));
        assert!(!matcher.matches(&entry, "B"));
    }

    #[test]
    fn test_empty_partial() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "String",
            EntryKind::Class {
                superclass: None,
                includes: vec![],
                extends: vec![],
                prepends: vec![],
            },
        );

        assert!(matcher.matches(&entry, ""));
    }

    #[test]
    fn test_constant_entry() {
        let matcher = ConstantMatcher::new();
        let entry = create_test_entry(
            "MY_CONSTANT",
            EntryKind::Constant {
                value: Some("42".to_string()),
                visibility: None,
            },
        );

        assert!(matcher.matches(&entry, "MY"));
        assert!(matcher.matches(&entry, "CONST"));
        assert!(matcher.matches(&entry, "MC")); // CamelCase matching
    }
}
