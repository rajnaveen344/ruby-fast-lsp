use crate::core::storage::file_owned::{FileOwned, FileRow};
use crate::core::storage::memory_estimate::string_heap_bytes;
use crate::core::{SourceFileId, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticFact {
    pub range: TextRange,
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

impl DiagnosticFact {
    pub fn new(
        range: TextRange,
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let code = code.into();
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
            (
                left.range.start_byte,
                left.range.end_byte,
                severity_rank(left.severity),
                left.code.as_str(),
                left.message.as_str(),
            )
                .cmp(&(
                    right.range.start_byte,
                    right.range.end_byte,
                    severity_rank(right.severity),
                    right.code.as_str(),
                    right.message.as_str(),
                ))
        });
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.facts.estimated_heap_bytes(|fact| {
            string_heap_bytes(&fact.code) + string_heap_bytes(&fact.message)
        })
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
    }
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
}
