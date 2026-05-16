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
use crate::types::ruby_namespace::RubyConstant;
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
        let mut items = self
            .find_constant_completions_from_analysis(analyzer, position, &partial)
            .unwrap_or_default();
        if self.analysis_engine().is_some() {
            return dedupe_completion_items(items);
        }

        let index = self.index.lock();
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
        let query = ruby_analysis_engine::AnalysisQuery::new(&engine);
        let mut items = Vec::new();
        for namespace_fqn in receiver_type_to_namespaces(receiver_type, kind) {
            for fact in query.method_completion_facts(&namespace_fqn, partial_method) {
                let return_type = query.method_return_type(&fact);
                items.push(method_completion_item_from_analysis(&fact, return_type));
            }
        }
        items
    }

    pub fn find_top_level_method_completions(&self, partial_method: &str) -> Vec<CompletionItem> {
        let mut items = self.top_level_method_completions_from_analysis(partial_method);
        if self.analysis_engine().is_some() {
            return dedupe_completion_items(items);
        }

        items.extend(method::find_top_level_method_completions(
            &self.index,
            partial_method,
        ));
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
        let query = ruby_analysis_engine::AnalysisQuery::new(&engine);
        query
            .top_level_method_completion_facts(partial_method)
            .into_iter()
            .map(|fact| {
                let return_type = query.method_return_type(&fact);
                method_completion_item_from_analysis(&fact, return_type)
            })
            .collect()
    }
}

fn method_completion_item_from_analysis(
    fact: &ruby_analysis_core::MethodFact,
    return_type: Option<RubyType>,
) -> CompletionItem {
    let FullyQualifiedName::Method(_, method) = &fact.fqn else {
        panic!(
            "INVARIANT VIOLATED: analysis method completion fact has non-method FQN: {}. \
             This is a bug because MethodStore must only contain method facts. \
             Fix: reject non-method FQNs in MethodFact construction.",
            fact.fqn
        );
    };

    let name = method.get_name();
    let params = fact
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

fn receiver_type_to_namespaces(
    ruby_type: &RubyType,
    kind: NamespaceKind,
) -> Vec<FullyQualifiedName> {
    match ruby_type {
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                kind,
            )]
        }
        RubyType::Array(_) => namespace_for_builtin("Array", kind),
        RubyType::Hash(_, _) => namespace_for_builtin("Hash", kind),
        RubyType::Union(types) => types
            .iter()
            .flat_map(|ty| receiver_type_to_namespaces(ty, kind))
            .collect(),
        RubyType::Unknown => Vec::new(),
    }
}

fn namespace_for_builtin(name: &str, kind: NamespaceKind) -> Vec<FullyQualifiedName> {
    let Ok(constant) = RubyConstant::new(name) else {
        return Vec::new();
    };
    vec![FullyQualifiedName::namespace_with_kind(
        vec![constant],
        kind,
    )]
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
