use std::collections::HashMap;

use crate::{SourceFileId, TextRange};

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
        assert!(
            !code.is_empty(),
            "INVARIANT VIOLATED: diagnostic fact code is empty. \
             This is a bug because diagnostics must have stable machine-readable codes. \
             Fix: pass a non-empty diagnostic code when creating DiagnosticFact."
        );
        let message = message.into();
        assert!(
            !message.is_empty(),
            "INVARIANT VIOLATED: diagnostic fact message is empty. \
             This is a bug because diagnostics without messages cannot guide users. \
             Fix: pass a non-empty diagnostic message when creating DiagnosticFact."
        );
        Self {
            range,
            severity,
            code,
            message,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticStore {
    facts_by_file: HashMap<SourceFileId, Vec<DiagnosticFact>>,
}

impl DiagnosticStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: DiagnosticFact) {
        let file_id = fact.range.file_id;
        self.facts_by_file.entry(file_id).or_default().push(fact);
        self.sort_file(file_id);
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts_by_file
            .get(&file_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn all_facts(&self) -> Vec<DiagnosticFact> {
        self.facts_by_file
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.facts_by_file.remove(&file_id);
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.remove_file(file_id);
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement diagnostic fact belongs to a different file id. \
                 This is a bug because DiagnosticStore::replace_file must only receive facts for the target file. \
                 Fix: partition diagnostic facts by SourceFileId before replacing."
            );
            self.facts_by_file.entry(file_id).or_default().push(fact);
        }
        self.sort_file(file_id);
    }

    fn sort_file(&mut self, file_id: SourceFileId) {
        if let Some(facts) = self.facts_by_file.get_mut(&file_id) {
            facts.sort_by(|left, right| {
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
        let mut store = DiagnosticStore::new();
        store.add(DiagnosticFact::new(
            TextRange::new(file, 0, 1),
            DiagnosticSeverity::Warning,
            "old",
            "old message",
        ));

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
