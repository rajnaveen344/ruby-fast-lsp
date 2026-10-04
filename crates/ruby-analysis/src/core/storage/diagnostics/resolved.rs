use crate::core::storage::file_owned::{FileOwned, FileRow};
use crate::core::{SourceFileId, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Codes come from fixed producer tables, so a fact borrows its code and
/// owns only its exact-length message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFact {
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: Box<str>,
}

impl DiagnosticFact {
    pub fn new(
        range: TextRange,
        severity: DiagnosticSeverity,
        code: &'static str,
        message: impl Into<Box<str>>,
    ) -> Self {
        invariant!(
            !code.is_empty(),
            what = "diagnostic fact code is empty",
            why = "diagnostics must have stable machine-readable codes",
            fix = "pass a non-empty diagnostic code when creating DiagnosticFact",
        );
        let message = message.into();
        invariant!(
            !message.is_empty(),
            what = "diagnostic fact message is empty",
            why = "diagnostics without messages cannot guide users",
            fix = "pass a non-empty diagnostic message when creating DiagnosticFact",
        );
        Self {
            range,
            severity,
            code,
            message,
        }
    }
}

impl FileRow for DiagnosticFact {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticStore {
    facts: FileOwned<DiagnosticFact>,
}

impl DiagnosticStore {
    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts.rows(file_id).to_vec()
    }

    pub fn all_facts(&self) -> Vec<DiagnosticFact> {
        self.facts.iter().cloned().collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.len()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.facts.remove(file_id);
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.facts.replace(file_id, facts, |left, right| {
            fact_order_key(left).cmp(&fact_order_key(right))
        });
    }

    /// Replace the facts of one file that `replaced` selects with `facts`,
    /// keeping every other fact. Only the kept rows are cloned, and a file
    /// with nothing to drop or add stays untouched.
    pub fn replace_matching(
        &mut self,
        file_id: SourceFileId,
        replaced: impl Fn(&DiagnosticFact) -> bool,
        facts: Vec<DiagnosticFact>,
    ) {
        let current = self.facts.rows(file_id);
        if facts.is_empty() && !current.iter().any(&replaced) {
            return;
        }
        let kept = current.iter().filter(|fact| !replaced(fact)).cloned();
        let rows = kept.chain(facts).collect::<Vec<_>>();
        self.replace_file(file_id, rows);
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.facts.estimated_heap_bytes(|fact| fact.message.len())
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
    }
}

fn fact_order_key(fact: &DiagnosticFact) -> (u32, u32, u8, &str, &str) {
    (
        fact.range.start_byte,
        fact.range.end_byte,
        severity_rank(fact.severity),
        fact.code,
        &fact.message,
    )
}

fn severity_rank(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Error => 0,
        DiagnosticSeverity::Warning => 1,
        DiagnosticSeverity::Information => 2,
        DiagnosticSeverity::Hint => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_file_drops_stale_diagnostics() {
        let file = SourceFileId(1);
        let mut store = DiagnosticStore::default();
        store.replace_file(
            file,
            [DiagnosticFact::new(
                TextRange::new(file, 0, 1),
                DiagnosticSeverity::Warning,
                "old",
                "old message",
            )],
        );

        store.replace_file(
            file,
            [DiagnosticFact::new(
                TextRange::new(file, 2, 3),
                DiagnosticSeverity::Error,
                "new",
                "new message",
            )],
        );

        let facts = store.facts_in_file(file);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].code, "new");
    }

    fn fact(file: SourceFileId, start: u32, code: &'static str, message: &str) -> DiagnosticFact {
        DiagnosticFact::new(
            TextRange::new(file, start, start + 1),
            DiagnosticSeverity::Warning,
            code,
            message,
        )
    }

    #[test]
    fn replace_matching_swaps_only_selected_codes_in_source_order() {
        let file = SourceFileId(1);
        let other = SourceFileId(2);
        let mut store = DiagnosticStore::default();
        store.replace_file(
            file,
            [
                fact(file, 0, "kept", "kept first"),
                fact(file, 4, "derived", "stale"),
                fact(file, 8, "kept", "kept last"),
            ],
        );
        store.replace_file(other, [fact(other, 0, "derived", "other file")]);

        store.replace_matching(
            file,
            |fact| fact.code == "derived",
            vec![fact(file, 6, "derived", "fresh")],
        );

        let messages = store
            .facts_in_file(file)
            .iter()
            .map(|fact| fact.message.to_string())
            .collect::<Vec<_>>();
        assert_eq!(messages, ["kept first", "fresh", "kept last"]);
        assert_eq!(store.facts_in_file(other)[0].message.as_ref(), "other file");
    }

    #[test]
    fn replace_matching_without_changes_keeps_the_file_rows() {
        let file = SourceFileId(1);
        let mut store = DiagnosticStore::default();
        store.replace_file(file, [fact(file, 0, "kept", "kept")]);
        let before = store.facts.rows(file).as_ptr();

        store.replace_matching(file, |fact| fact.code == "derived", Vec::new());

        assert_eq!(store.facts.rows(file).as_ptr(), before);
        assert_eq!(store.fact_count(), 1);
    }

    #[test]
    fn fact_borrows_its_code_and_owns_an_exact_message() {
        let file = SourceFileId(1);
        let mut store = DiagnosticStore::default();
        let empty = store.estimated_heap_bytes();
        store.replace_file(file, [fact(file, 0, "code", "twelve bytes")]);
        assert!(std::mem::size_of::<DiagnosticFact>() <= 48);
        assert!(
            store.estimated_heap_bytes()
                >= empty + std::mem::size_of::<DiagnosticFact>() + "twelve bytes".len()
        );
    }
}
