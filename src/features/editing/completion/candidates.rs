//! Completion candidates: constant and method matches read from one engine
//! view and converted to protocol items.

use ruby_analysis::core::RubyType;
use tower_lsp::lsp_types::CompletionItem;
use tower_lsp::lsp_types::{CompletionItemKind, CompletionItemLabelDetails};

use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis::engine::completion::rbs_method_matches_for_type;
use ruby_analysis::engine::{ConstantLookupRequest, ConstantMatch, MethodMatch, View};

/// Constants whose names match `partial`, sorted by label.
pub(super) fn constant_completions(view: &View<'_>, partial: &str) -> Vec<CompletionItem> {
    if !view.has_symbols() {
        return Vec::new();
    }
    let mut items = view
        .constant_matches(&ConstantLookupRequest::new(partial, 50))
        .into_iter()
        .map(constant_completion_item)
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    dedupe_completion_items(items)
}

/// Methods on `receiver_type`: indexed definitions, then RBS built-ins.
pub(super) fn method_completions(
    view: &View<'_>,
    receiver_type: &RubyType,
    partial_method: &str,
    kind: NamespaceKind,
) -> Vec<CompletionItem> {
    let mut items = view
        .method_matches_for_type(receiver_type, partial_method, kind)
        .into_iter()
        .map(method_completion_item_from_analysis)
        .collect::<Vec<_>>();
    items.extend(
        rbs_method_matches_for_type(receiver_type, partial_method, kind)
            .into_iter()
            .map(|candidate| {
                method_completion_item(candidate.name, candidate.params, candidate.return_type)
            }),
    );
    dedupe_completion_items(items)
}

/// Methods defined outside any class or module.
pub(super) fn top_level_method_completions(
    view: &View<'_>,
    partial_method: &str,
) -> Vec<CompletionItem> {
    dedupe_completion_items(
        view.top_level_method_matches(partial_method)
            .into_iter()
            .map(method_completion_item_from_analysis)
            .collect(),
    )
}

fn method_completion_item_from_analysis(candidate: MethodMatch) -> CompletionItem {
    method_completion_item(candidate.name, candidate.params, candidate.return_type)
}

fn method_completion_item(
    name: String,
    params: Vec<String>,
    return_type: Option<RubyType>,
) -> CompletionItem {
    let params = params
        .iter()
        .filter(|param| !param.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let params = if params.is_empty() {
        String::new()
    } else {
        format!("({})", params.join(", "))
    };
    let return_type = return_type
        .map(|ruby_type| format!(" -> {ruby_type}"))
        .unwrap_or_default();
    let detail = format!("{name}{params}{return_type}");

    CompletionItem {
        label: name.clone(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(detail),
        insert_text: Some(name),
        ..Default::default()
    }
}

fn constant_completion_item(candidate: ConstantMatch) -> CompletionItem {
    let fqn = candidate.fqn;
    let kind = candidate.kind;
    let item_kind = match kind {
        AnalysisSymbolKind::Class => CompletionItemKind::CLASS,
        AnalysisSymbolKind::Module => CompletionItemKind::MODULE,
        AnalysisSymbolKind::Constant => CompletionItemKind::CONSTANT,
        AnalysisSymbolKind::Method
        | AnalysisSymbolKind::LocalVariable
        | AnalysisSymbolKind::InstanceVariable
        | AnalysisSymbolKind::ClassVariable
        | AnalysisSymbolKind::GlobalVariable => CompletionItemKind::VALUE,
    };
    let detail = match kind {
        AnalysisSymbolKind::Class => format!("class {fqn}"),
        AnalysisSymbolKind::Module => format!("module {fqn}"),
        AnalysisSymbolKind::Constant => fqn.to_string(),
        AnalysisSymbolKind::Method
        | AnalysisSymbolKind::LocalVariable
        | AnalysisSymbolKind::InstanceVariable
        | AnalysisSymbolKind::ClassVariable
        | AnalysisSymbolKind::GlobalVariable => fqn.to_string(),
    };

    CompletionItem {
        label: fqn.name(),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(detail.clone()),
            description: Some(fqn.to_string()),
        }),
        kind: Some(item_kind),
        detail: Some(detail),
        insert_text: Some(fqn.name()),
        ..Default::default()
    }
}

fn dedupe_completion_items(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for item in items {
        if seen.insert(item.label.clone()) {
            deduped.push(item);
        }
    }
    deduped
}
