use crate::capabilities::formatting::full_document_range;
use crate::config::LinterKind;
use crate::linter::fix_document;
use crate::server::RubyLanguageServer;
use log::warn;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic, TextEdit,
    WorkspaceEdit,
};

pub async fn handle_code_actions(
    server: &RubyLanguageServer,
    params: CodeActionParams,
) -> Option<Vec<CodeActionOrCommand>> {
    if params
        .context
        .only
        .as_ref()
        .is_some_and(|kinds| !kinds.iter().any(|kind| *kind == CodeActionKind::QUICKFIX))
    {
        return None;
    }
    let config = server.config.lock().clone();
    if config.linter == LinterKind::None {
        return None;
    }
    let matching = params
        .context
        .diagnostics
        .into_iter()
        .filter(|diagnostic| is_correctable_linter_diagnostic(diagnostic, config.linter))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return None;
    }

    let uri = params.text_document.uri;
    let document = server.get_doc(&uri)?;
    let content = document.content;
    let file_path = uri.to_file_path().ok()?;
    let workspace_root = server
        .workspace_for_uri(&uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| file_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    let fixed = match fix_document(
        &config,
        server.indexing_resources.clone(),
        &workspace_root,
        &file_path,
        &content,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(fixed) => fixed,
        Err(error) => {
            warn!(
                "Safe linter quick fix unavailable for {}: {error:#}. \
                 No workspace edit was returned.",
                file_path.display()
            );
            return None;
        }
    };
    if fixed == content {
        return None;
    }

    let edit = TextEdit::new(full_document_range(&content), fixed);
    let mut changes = HashMap::new();
    changes.insert(uri, vec![edit]);
    Some(vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: format!(
            "Fix safe {} offenses",
            config.linter.diagnostic_source().expect(
                "INVARIANT VIOLATED: enabled linter has no display name. \
                 This is a bug because quick fixes require a user-facing title. \
                 Fix: add diagnostic_source for every enabled LinterKind."
            )
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(matching),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })])
}

fn is_correctable_linter_diagnostic(diagnostic: &Diagnostic, linter: LinterKind) -> bool {
    let Some(data) = diagnostic
        .data
        .as_ref()
        .and_then(serde_json::Value::as_object)
    else {
        return false;
    };
    data.get("linter").and_then(serde_json::Value::as_str) == linter.data_name()
        && data.get("correctable").and_then(serde_json::Value::as_bool) == Some(true)
}
