//! Completion Query — Provides index-backed completion lookups.
//!
//! Wraps constant and method completion logic behind `IndexQuery`,
//! keeping lock management in one place.

use tower_lsp::lsp_types::{CompletionItem, Position};
use tower_lsp::lsp_types::{CompletionItemKind, CompletionItemLabelDetails};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::capabilities::completion::constant;
use crate::capabilities::completion::method;
use crate::indexer::entry::NamespaceKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_analysis_core::SymbolKind as AnalysisSymbolKind;

use super::IndexQuery;

impl IndexQuery {
    /// Find constant completions by locking the index and delegating
    /// to the existing constant completion engine.
    pub fn find_constant_completions(
        &self,
        analyzer: &RubyPrismAnalyzer,
        position: Position,
        partial: String,
    ) -> Vec<CompletionItem> {
        let index = self.index.lock();
        let mut items = self
            .find_constant_completions_from_analysis(analyzer, position, &partial)
            .unwrap_or_default();
        items.extend(constant::find_constant_completions(
            &index, analyzer, position, partial,
        ));
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
        if engine.all_symbol_facts().is_empty() {
            return None;
        }

        let (_, _, _, scope_stack, _) = analyzer.get_identifier(position);
        let context =
            crate::capabilities::completion::constant_completion::ConstantCompletionContext::new(
                position,
                scope_stack,
                partial.to_string(),
            );

        let mut seen = std::collections::HashSet::new();
        let mut items = engine
            .all_symbol_facts()
            .into_iter()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    AnalysisSymbolKind::Class
                        | AnalysisSymbolKind::Module
                        | AnalysisSymbolKind::Constant
                )
            })
            .filter(|fact| seen.insert(fact.fqn.namespace_parts()))
            .filter(|fact| constant_matches(&fact.fqn, &context))
            .map(|fact| constant_completion_item(&fact.fqn, fact.kind))
            .collect::<Vec<_>>();

        items.sort_by(|left, right| left.label.cmp(&right.label));
        items.truncate(50);
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
        method::find_method_completions(&self.index, receiver_type, partial_method, kind)
    }
}

fn constant_matches(
    fqn: &FullyQualifiedName,
    context: &crate::capabilities::completion::constant_completion::ConstantCompletionContext,
) -> bool {
    if context.is_qualified {
        if let Some(namespace_prefix) = &context.namespace_prefix {
            let fqn_parts = fqn.namespace_parts();
            let namespace_parts = namespace_prefix.namespace_parts();
            if fqn_parts.len() != namespace_parts.len() + 1 {
                return false;
            }
            if !fqn_parts.starts_with(&namespace_parts) {
                return false;
            }
        } else if fqn.namespace_parts().len() > 1 {
            return false;
        }
    }

    fqn.name()
        .to_lowercase()
        .starts_with(&context.partial_name.to_lowercase())
}

fn constant_completion_item(fqn: &FullyQualifiedName, kind: AnalysisSymbolKind) -> CompletionItem {
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
        AnalysisSymbolKind::Class => format!("class {}", fqn),
        AnalysisSymbolKind::Module => format!("module {}", fqn),
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
