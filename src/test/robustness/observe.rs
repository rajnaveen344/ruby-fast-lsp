//! Read-only snapshots of what an editor would show for a set of files.

use std::collections::BTreeMap;

use crate::test::harness::FakeEditor;

/// At most this many cursor positions are probed per file.
const MAX_PROBES_PER_FILE: usize = 120;

/// Every observation keyed by a readable description of the request.
pub type Snapshot = BTreeMap<String, String>;

/// Requests every per-file feature, plus hover and definition at word starts.
pub async fn snapshot(editor: &FakeEditor, files: &[&str]) -> Snapshot {
    let mut out = Snapshot::new();
    for &file in files {
        out.insert(
            format!("{file} diagnostics"),
            sorted_json(editor.diagnostics(file).await),
        );
        out.insert(
            format!("{file} inlay hints"),
            sorted_json(editor.inlay_hints(file).await),
        );
        out.insert(
            format!("{file} symbols"),
            json(&editor.document_symbols(file).await),
        );
        for (line, character) in probe_positions(editor.content(file)) {
            let at = format!("{file}:{}:{}", line + 1, character + 1);
            out.insert(
                format!("{at} hover"),
                json(&editor.hover_at(file, line, character).await),
            );
            out.insert(
                format!("{at} definition"),
                sorted_json(editor.goto_def_at(file, line, character).await),
            );
        }
    }
    out
}

/// Issues every request without recording results, for use on broken buffers.
pub async fn exercise(editor: &FakeEditor, file: &str, line: u32, character: u32) {
    editor.diagnostics(file).await;
    editor.inlay_hints(file).await;
    editor.hover_at(file, line, character).await;
    editor.goto_def_at(file, line, character).await;
    editor.complete_at(file, line, character).await;
}

/// The first UTF-16 column of each identifier, capped per file.
pub fn probe_positions(content: &str) -> Vec<(u32, u32)> {
    let mut positions = Vec::new();
    for (line, text) in content.lines().enumerate() {
        let mut column = 0u32;
        let mut previous_is_word = false;
        for ch in text.chars() {
            let is_word = ch.is_alphanumeric() || ch == '_';
            if is_word && !previous_is_word {
                positions.push((line as u32, column));
            }
            previous_is_word = is_word;
            column += ch.len_utf16() as u32;
        }
    }
    let stride = positions.len().div_ceil(MAX_PROBES_PER_FILE).max(1);
    positions.into_iter().step_by(stride).collect()
}

/// Lists the differences between two snapshots, one line per key.
pub fn differences(expected: &Snapshot, actual: &Snapshot) -> Vec<String> {
    let mut keys = expected.keys().chain(actual.keys()).collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| expected.get(*key) != actual.get(*key))
        .map(|key| {
            format!(
                "{key}\n    expected: {}\n    actual:   {}",
                expected.get(key).map_or("<missing>", String::as_str),
                actual.get(key).map_or("<missing>", String::as_str)
            )
        })
        .collect()
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("LSP response types serialize to JSON")
}

fn sorted_json<T: serde::Serialize>(values: Vec<T>) -> String {
    let mut items = values.iter().map(json).collect::<Vec<_>>();
    items.sort();
    format!("[{}]", items.join(","))
}
