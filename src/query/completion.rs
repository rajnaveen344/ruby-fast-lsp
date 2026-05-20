//! Completion Query — Provides engine-backed completion lookups.
//!
//! Wraps constant and method completion logic behind `EngineQuery`,
//! keeping lock management in one place.

use tower_lsp::lsp_types::{CompletionItem, Position};
use tower_lsp::lsp_types::{CompletionItemKind, CompletionItemLabelDetails};

use crate::capabilities::completion::method;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis::engine::{ConstantLookupRequest, ConstantMatch, MethodMatch};
use ruby_analysis::indexer::RubyPrismAnalyzer;
use ruby_analysis::inference::RubyType;

use super::EngineQuery;

impl EngineQuery {
    /// Find constant completions by locking the analysis engine.
    pub fn find_constant_completions(
        &self,
        analyzer: &RubyPrismAnalyzer,
        position: Position,
        partial: String,
    ) -> Vec<CompletionItem> {
        let items = self
            .find_constant_completions_from_analysis(analyzer, position, &partial)
            .unwrap_or_default();
        dedupe_completion_items(items)
    }

    fn find_constant_completions_from_analysis(
        &self,
        analyzer: &RubyPrismAnalyzer,
        position: Position,
        partial: &str,
    ) -> Option<Vec<CompletionItem>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        if !query.has_symbols() {
            return None;
        }

        let (_, _, _, scope_stack, _) = analyzer.get_identifier(position);
        let context =
            crate::capabilities::completion::constant_completion::ConstantCompletionContext::new(
                position,
                scope_stack,
                partial.to_string(),
            );

        let mut items = query
            .constant_matches(&ConstantLookupRequest {
                partial_name: context.partial_name,
                namespace_prefix: context.namespace_prefix,
                is_qualified: context.is_qualified,
                limit: 50,
            })
            .into_iter()
            .map(constant_completion_item)
            .collect::<Vec<_>>();

        items.sort_by(|left, right| left.label.cmp(&right.label));
        Some(items)
    }

    /// Find method completions for a receiver type.
    ///
    /// Delegates to `method::find_method_completions` which handles
    /// both RBS (built-in) and index (user-defined) methods.
    pub fn find_method_completions(
        &self,
        receiver_type: &RubyType,
        partial_method: &str,
        kind: NamespaceKind,
    ) -> Vec<CompletionItem> {
        let mut items = self.method_completions_from_analysis(receiver_type, partial_method, kind);
        items.extend(method::find_rbs_method_completions(
            receiver_type,
            partial_method,
            kind,
        ));
        dedupe_completion_items(items)
    }

    fn method_completions_from_analysis(
        &self,
        receiver_type: &RubyType,
        partial_method: &str,
        kind: NamespaceKind,
    ) -> Vec<CompletionItem> {
        let Some(engine) = self.analysis_engine() else {
            return Vec::new();
        };
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        query
            .method_matches_for_type(receiver_type, partial_method, kind)
            .into_iter()
            .map(method_completion_item_from_analysis)
            .collect()
    }

    pub fn find_top_level_method_completions(&self, partial_method: &str) -> Vec<CompletionItem> {
        let items = self.top_level_method_completions_from_analysis(partial_method);
        dedupe_completion_items(items)
    }

    fn top_level_method_completions_from_analysis(
        &self,
        partial_method: &str,
    ) -> Vec<CompletionItem> {
        let Some(engine) = self.analysis_engine() else {
            return Vec::new();
        };
        let engine = engine.lock();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        query
            .top_level_method_matches(partial_method)
            .into_iter()
            .map(method_completion_item_from_analysis)
            .collect()
    }
}

fn method_completion_item_from_analysis(candidate: MethodMatch) -> CompletionItem {
    let name = candidate.name;
    let params = candidate
        .params
        .iter()
        .filter(|param| !param.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let params = if params.is_empty() {
        String::new()
    } else {
        format!("({})", params.join(", "))
    };
    let return_type = candidate
        .return_type
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
