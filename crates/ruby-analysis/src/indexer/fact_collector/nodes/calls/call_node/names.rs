//! Symbol and string literal name extraction from call arguments.

use crate::indexer::fact_collector::FactCollector;
use ruby_prism::{CallNode, Node};

pub(super) fn direct_attr_name_and_range(
    visitor: &FactCollector,
    node: &Node<'_>,
) -> Option<(String, crate::core::TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&symbol.location()),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            visitor.direct_range(&string.content_loc()),
        ));
    }
    None
}

pub(super) fn direct_method_name_and_range(
    visitor: &FactCollector,
    node: &Node<'_>,
) -> Option<(String, crate::core::TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&location),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            visitor.direct_range(&string.content_loc()),
        ));
    }
    None
}

pub(super) fn define_method_name_and_range(
    visitor: &FactCollector,
    node: &CallNode<'_>,
    name_index: usize,
) -> Option<(String, crate::core::TextRange)> {
    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().nth(name_index)?;
    if let Some(symbol) = arg.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&location),
        ));
    }
    direct_attr_name_and_range(visitor, &arg)
}

pub(super) fn direct_symbol_name_and_range(
    visitor: &FactCollector,
    node: &Node<'_>,
) -> Option<(String, crate::core::TextRange)> {
    node.as_symbol_node().map(|symbol| {
        (
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&symbol.location()),
        )
    })
}
