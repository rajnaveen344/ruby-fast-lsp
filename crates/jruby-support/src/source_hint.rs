//! Catalog-independent JRuby evidence kept from a project's first pass.

/// Compact, catalog-independent evidence retained by the first project pass.
///
/// The exact JRuby catalog may still be under construction while ordinary Ruby
/// facts are collected. Keeping only definite Java DSL/canonical-proxy markers
/// and dotted receiver roots lets the owning project later replay the bounded
/// subset whose semantics depend on that catalog without retaining source
/// buffers or reading every project file a second time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticJavaSourceHint {
    definite_catalog_semantics: bool,
    dotted_roots: Vec<String>,
}

impl StaticJavaSourceHint {
    pub fn from_source(source: &str) -> Self {
        let mut definite_catalog_semantics = [
            "java_import",
            "include_package",
            "java_implements",
            "java_package",
            "java_alias",
            "java_send",
            "java_method",
            "to_java",
        ]
        .iter()
        .any(|marker| source.contains(marker));

        let mut dotted_roots = Vec::new();
        let mut characters = source.char_indices().peekable();
        while let Some((start, character)) = characters.next() {
            if !(character.is_alphabetic() || character == '_' || character == '$') {
                continue;
            }
            let mut end = start + character.len_utf8();
            while let Some(&(offset, next)) = characters.peek() {
                if !(next.is_alphanumeric() || next == '_' || next == '$') {
                    break;
                }
                characters.next();
                end = offset + next.len_utf8();
            }
            let identifier = &source[start..end];
            let suffix = source[end..].trim_start_matches(char::is_whitespace);
            if identifier == "Java" && suffix.starts_with("::") {
                definite_catalog_semantics = true;
            }
            if suffix.starts_with('.') {
                dotted_roots.push(source[start..end].to_string());
            }
        }
        dotted_roots.sort();
        dotted_roots.dedup();
        Self {
            definite_catalog_semantics,
            dotted_roots,
        }
    }
    /// The source names a Java DSL call, a canonical `Java::` constant, or
    /// another form whose meaning always depends on the catalog.
    pub fn definite_catalog_semantics(&self) -> bool {
        self.definite_catalog_semantics
    }

    /// Sorted, distinct identifiers that start a dotted call chain.
    pub fn dotted_roots(&self) -> &[String] {
        &self.dotted_roots
    }
}
