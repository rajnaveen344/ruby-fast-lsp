//! Scope rules every declaration walk shares: lexical constant lookup and the
//! namespace a static receiver names. Walks differ only in which namespaces
//! they know, so each rule takes that knowledge as a predicate.

use crate::core::{FullyQualifiedName, RubyConstant};
use ruby_prism::Node;

use super::scope_tracker::mixin_ref_from_node;

/// Lexical lookup candidates for `parts`, innermost enclosing namespace first
/// and the top level last. An absolute path has the single candidate `parts`.
pub fn lexical_candidates<'a>(
    parts: &'a [RubyConstant],
    absolute: bool,
    lexical_context: &'a [RubyConstant],
) -> impl Iterator<Item = Vec<RubyConstant>> + 'a {
    let depth = if absolute { 0 } else { lexical_context.len() };
    (0..=depth).rev().map(move |len| {
        let mut candidate = lexical_context[..len].to_vec();
        candidate.extend_from_slice(parts);
        candidate
    })
}

/// Candidates a class or module declaration name reopens: the name inside the
/// enclosing namespace, then a qualified name from the top level.
pub fn declaration_candidates(
    parts: &[RubyConstant],
    absolute: bool,
    lexical_context: &[RubyConstant],
) -> Vec<Vec<RubyConstant>> {
    let mut exact = if absolute {
        Vec::new()
    } else {
        lexical_context.to_vec()
    };
    exact.extend_from_slice(parts);
    let mut candidates = vec![exact];
    if !absolute && parts.len() > 1 && !lexical_context.is_empty() {
        candidates.push(parts.to_vec());
    }
    candidates
}

/// The first known namespace that `parts` names from `lexical_context`.
pub fn resolve_lexical_namespace(
    parts: &[RubyConstant],
    absolute: bool,
    lexical_context: &[RubyConstant],
    is_known: impl Fn(&FullyQualifiedName) -> bool,
) -> Option<FullyQualifiedName> {
    if parts.is_empty() {
        return None;
    }
    lexical_candidates(parts, absolute, lexical_context)
        .map(FullyQualifiedName::namespace)
        .find(|fqn| is_known(fqn))
}

/// The known namespace a constant or `Const.const_get(:Name)` receiver names.
pub fn resolve_receiver_namespace(
    receiver: &Node<'_>,
    lexical_context: &[RubyConstant],
    is_known: &impl Fn(&FullyQualifiedName) -> bool,
) -> Option<Vec<RubyConstant>> {
    if let Some(call) = receiver.as_call_node() {
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        let base = resolve_receiver_namespace(&call.receiver()?, lexical_context, is_known)?;
        let arguments = call.arguments()?;
        let name = static_name(&arguments.arguments().iter().next()?)?;
        let mut namespace = base;
        namespace.push(RubyConstant::new(&name).ok()?);
        return is_known(&FullyQualifiedName::namespace(namespace.clone())).then_some(namespace);
    }
    let receiver = mixin_ref_from_node(receiver)?;
    resolve_lexical_namespace(
        &receiver.parts,
        receiver.absolute,
        lexical_context,
        is_known,
    )
    .map(|fqn| fqn.namespace_parts())
}

/// The text of a symbol or string literal.
pub fn static_name(node: &Node<'_>) -> Option<String> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    let string = node.as_string_node()?;
    Some(String::from_utf8_lossy(string.unescaped()).to_string())
}
