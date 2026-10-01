//! The robustness checks. Each one compares the server with itself.

use tower_lsp::lsp_types::{Diagnostic, NumberOrString};

use super::corpus::Corpus;
use super::edits::{edited_lines, erase_and_retype};
use super::observe::{differences, exercise, Snapshot};
use super::workspace::{ProjectDir, Session};

/// Requests are issued on every Nth keystroke; every keystroke is still indexed.
const REQUEST_EVERY_KEYSTROKES: usize = 4;

/// Diagnostics the server reports on the built-in corpus although the corpus
/// runs cleanly under Ruby. Each entry is a false positive to fix. When one is
/// fixed, delete its line; the check fails until the list matches exactly.
const KNOWN_FALSE_POSITIVES: &[&str] = &[];

/// A fresh session on a new copy of the corpus with `session`'s buffers open.
async fn fresh_copy<'a>(
    corpus: &Corpus,
    dir: &'a ProjectDir,
    session: &Session<'_>,
    names: &[&str],
) -> Session<'a> {
    let mut fresh = dir.open(corpus, &[]).await;
    for &name in names {
        let filename = fresh.filename(name);
        fresh.editor.open(&filename, session.content(name)).await;
    }
    fresh
}

fn assert_same(context: &str, expected: &Snapshot, actual: &Snapshot) {
    let diff = differences(expected, actual);
    assert!(
        diff.is_empty(),
        "{context}: {} observation(s) differ from a fresh index\n{}",
        diff.len(),
        diff.join("\n")
    );
}

/// Erase and retype lines in every file one character at a time. Requests on
/// the broken intermediate buffers must not panic, a half-typed state must
/// match a fresh index of the same buffers, and the restored project must
/// match the original fresh index.
#[tokio::test]
async fn typing_through_every_file_matches_a_fresh_index() {
    let corpus = Corpus::selected();
    let names = corpus.names();
    let dir = ProjectDir::write(&corpus);
    let fresh_dir = ProjectDir::write(&corpus);
    let mut session = dir.open(&corpus, &names).await;
    let baseline = session.snapshot(&names).await;

    for &name in &names {
        let filename = session.filename(name);
        for line in edited_lines(corpus.content(name)) {
            let steps = erase_and_retype(session.content(name), line);
            let partly_erased = steps.len() / 4;
            for (index, step) in steps.iter().enumerate() {
                session.editor.set(&filename, &step.content).await;
                if index % REQUEST_EVERY_KEYSTROKES == 0 {
                    exercise(&session.editor, &filename, step.line, step.character).await;
                }
                if index == partly_erased {
                    let fresh = fresh_copy(&corpus, &fresh_dir, &session, &names).await;
                    assert_same(
                        &format!("{name} line {} partly erased", line + 1),
                        &fresh.snapshot(&names).await,
                        &session.snapshot(&names).await,
                    );
                }
            }
        }
        assert_same(
            &format!("{name} retyped"),
            &baseline,
            &session.snapshot(&names).await,
        );
    }
}

/// Opening the same files in a different order yields the same observations.
#[tokio::test]
async fn open_order_does_not_change_observations() {
    let corpus = Corpus::selected();
    let forward = corpus.names();
    let reverse = forward.iter().rev().copied().collect::<Vec<_>>();
    let interleaved = forward
        .iter()
        .step_by(2)
        .chain(forward.iter().skip(1).step_by(2))
        .copied()
        .collect::<Vec<_>>();

    let dir = ProjectDir::write(&corpus);
    let expected = dir.open(&corpus, &forward).await.snapshot(&forward).await;
    for (label, order) in [("reverse", reverse), ("interleaved", interleaved)] {
        let actual = dir.open(&corpus, &order).await.snapshot(&forward).await;
        assert_same(&format!("{label} open order"), &expected, &actual);
    }
}

/// The built-in corpus runs cleanly under Ruby, so every diagnostic on it is a
/// false positive. Only the reviewed known entries are allowed.
#[tokio::test]
async fn clean_code_has_no_unexpected_diagnostics() {
    let corpus = Corpus::builtin();
    assert!(corpus.is_builtin);
    let names = corpus.names();
    let dir = ProjectDir::write(&corpus);
    let session = dir.open(&corpus, &names).await;

    let mut reported = Vec::new();
    for &name in &names {
        for diagnostic in session.editor.diagnostics(&session.filename(name)).await {
            reported.push(describe(name, &diagnostic));
        }
    }
    reported.sort();
    let mut known = KNOWN_FALSE_POSITIVES
        .iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>();
    known.sort();

    let new = reported
        .iter()
        .filter(|entry| !known.contains(entry))
        .collect::<Vec<_>>();
    let fixed = known
        .iter()
        .filter(|entry| !reported.contains(entry))
        .collect::<Vec<_>>();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "diagnostics on clean Ruby changed\nnew false positives:\n  {}\nno longer reported (delete from KNOWN_FALSE_POSITIVES):\n  {}",
        new.iter().map(|entry| entry.as_str()).collect::<Vec<_>>().join("\n  "),
        fixed.iter().map(|entry| entry.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}

fn describe(file: &str, diagnostic: &Diagnostic) -> String {
    let code = match &diagnostic.code {
        Some(NumberOrString::String(code)) => code.clone(),
        Some(NumberOrString::Number(code)) => code.to_string(),
        None => "-".to_string(),
    };
    format!(
        "{file}:{}:{} {code}: {}",
        diagnostic.range.start.line + 1,
        diagnostic.range.start.character + 1,
        diagnostic.message
    )
}
