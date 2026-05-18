use ruby_analysis_core::{MethodParamFact, MethodParamKind};
use ruby_analysis_engine::{AnalysisEngine, AnalysisQuery};
use ruby_prism::CallNode;
use tower_lsp::lsp_types::Location;

use crate::analyzer_prism::utils;

use crate::{
    analyzer_prism::diagnostics::ReceiverInfo,
    indexer::{
        entry::{
            entry_kind::{EntryKind, ParamKind},
            NamespaceKind,
        },
        index::UnresolvedEntry,
        symbol_table::SymbolTable,
    },
    inferrer::r#type::ruby::RubyType,
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_document::RubyDocument,
        ruby_method::RubyMethod, ruby_namespace::RubyConstant,
    },
};

/// Positional + keyword signature derived from a method's parameter list.
#[derive(Debug, Clone)]
struct MethodArity {
    required: usize,
    optional: usize,
    has_rest: bool,
    required_keywords: Vec<String>,
    optional_keywords: Vec<String>,
    has_kwrest: bool,
}

impl MethodArity {
    fn from_params(params: &[crate::indexer::entry::entry_kind::MethodParamInfo]) -> Self {
        let mut required = 0usize;
        let mut optional = 0usize;
        let mut has_rest = false;
        let mut required_keywords = Vec::new();
        let mut optional_keywords = Vec::new();
        let mut has_kwrest = false;
        for p in params {
            match p.kind {
                ParamKind::Required => required += 1,
                ParamKind::Optional => optional += 1,
                ParamKind::Rest => has_rest = true,
                ParamKind::RequiredKeyword => required_keywords.push(p.name.clone()),
                ParamKind::OptionalKeyword => optional_keywords.push(p.name.clone()),
                ParamKind::KeywordRest => has_kwrest = true,
                ParamKind::Block => {}
            }
        }
        Self {
            required,
            optional,
            has_rest,
            required_keywords,
            optional_keywords,
            has_kwrest,
        }
    }

    fn from_analysis_params(params: &[MethodParamFact]) -> Self {
        let mut required = 0usize;
        let mut optional = 0usize;
        let mut has_rest = false;
        let mut required_keywords = Vec::new();
        let mut optional_keywords = Vec::new();
        let mut has_kwrest = false;
        for p in params {
            match p.kind {
                MethodParamKind::Required => required += 1,
                MethodParamKind::Optional => optional += 1,
                MethodParamKind::Rest => has_rest = true,
                MethodParamKind::RequiredKeyword => required_keywords.push(p.name.clone()),
                MethodParamKind::OptionalKeyword => optional_keywords.push(p.name.clone()),
                MethodParamKind::KeywordRest => has_kwrest = true,
                MethodParamKind::Block => {}
            }
        }
        Self {
            required,
            optional,
            has_rest,
            required_keywords,
            optional_keywords,
            has_kwrest,
        }
    }
}

/// Extracts unknown kwargs from a callsite. Returns `(kwarg_name, name_loc)`
/// for each kwarg passed at `node` whose name is not declared on `arity` and
/// the method does not accept `**kwargs`. Skips when callsite uses `**opts`
/// splat (unknown keys). `name_loc` is the prism byte range of the key name.
fn collect_unknown_kwargs<'a>(
    node: &ruby_prism::CallNode<'a>,
    arity: &MethodArity,
) -> Option<Vec<(String, ruby_prism::Location<'a>)>> {
    if arity.has_kwrest {
        return Some(Vec::new());
    }
    let args = node.arguments()?;
    // The keyword args in Ruby calls are bundled into a single trailing
    // `KeywordHashNode` element. Find it.
    let mut kw_hash = None;
    for arg in args.arguments().iter() {
        if let Some(kh) = arg.as_keyword_hash_node() {
            kw_hash = Some(kh);
        }
    }
    let kw_hash = kw_hash?;
    let mut unknown = Vec::new();
    for elem in kw_hash.elements().iter() {
        // **opts splat in the kwarg position → unknown keys, punt entire check.
        if elem.as_assoc_splat_node().is_some() {
            return None;
        }
        let assoc = match elem.as_assoc_node() {
            Some(a) => a,
            None => continue,
        };
        let key = assoc.key();
        let sym = match key.as_symbol_node() {
            Some(s) => s,
            None => continue, // dynamic key → can't validate
        };
        let value_loc = match sym.value_loc() {
            Some(loc) => loc,
            None => continue,
        };
        let name = utils::utf8_str(value_loc.as_slice()).to_string();
        let declared =
            arity.required_keywords.contains(&name) || arity.optional_keywords.contains(&name);
        if !declared {
            unknown.push((name, value_loc));
        }
    }
    Some(unknown)
}

/// Extracts missing required kwargs for a callsite.
///
/// Returns `None` when the callsite uses a `**splat` (unknown keys — skip entire check).
/// Returns `Some(Vec<String>)` with sorted names of required kwargs not present at the callsite.
fn collect_missing_required_kwargs<'a>(
    node: &ruby_prism::CallNode<'a>,
    arity: &MethodArity,
) -> Option<Vec<String>> {
    if arity.required_keywords.is_empty() {
        return Some(Vec::new());
    }

    // Collect kwarg names supplied at the callsite.
    let mut supplied: Vec<String> = Vec::new();
    if let Some(args) = node.arguments() {
        for arg in args.arguments().iter() {
            if let Some(kh) = arg.as_keyword_hash_node() {
                for elem in kh.elements().iter() {
                    // **splat → unknown keys, skip entire check.
                    if elem.as_assoc_splat_node().is_some() {
                        return None;
                    }
                    if let Some(assoc) = elem.as_assoc_node() {
                        if let Some(sym) = assoc.key().as_symbol_node() {
                            if let Some(loc) = sym.value_loc() {
                                let name = utils::utf8_str(loc.as_slice()).to_string();
                                supplied.push(name);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut missing: Vec<String> = arity
        .required_keywords
        .iter()
        .filter(|kw| !supplied.contains(kw))
        .cloned()
        .collect();
    missing.sort();
    Some(missing)
}

/// Returns `Some((min, max, actual))` if callsite positional arity is outside
/// `[min, max]`. `max` is `None` when the method accepts `*args`. Returns `None`
/// when arity matches OR when the check is inconclusive.
///
/// Splat semantics: `*args` at the callsite expands to N >= 0 runtime args.
/// Too-few check is disabled (splat could supply the missing args).
/// Too-many check still fires when fixed args alone already exceed max — splat
/// can only ADD more args, making an overflow guaranteed.
fn compute_arity_mismatch(
    node: &CallNode,
    arity: &MethodArity,
) -> Option<(usize, Option<usize>, usize)> {
    let mut positional = 0usize;
    let mut has_splat_at_callsite = false;
    if let Some(args) = node.arguments() {
        for arg in args.arguments().iter() {
            if arg.as_splat_node().is_some() {
                has_splat_at_callsite = true;
                continue;
            }
            // Skip keyword hash and block-arg from positional count.
            if arg.as_keyword_hash_node().is_some() || arg.as_block_argument_node().is_some() {
                continue;
            }
            positional += 1;
        }
    }
    let min = arity.required;
    let max = if arity.has_rest {
        None
    } else {
        Some(arity.required + arity.optional)
    };
    if has_splat_at_callsite {
        let too_many = max.map(|m| positional > m).unwrap_or(false);
        if too_many {
            return Some((min, max, positional));
        }
        return None;
    }
    let too_few = positional < min;
    let too_many = max.map(|m| positional > m).unwrap_or(false);
    if too_few || too_many {
        Some((min, max, positional))
    } else {
        None
    }
}

/// Closest declared kwarg name within the suggestion threshold.
fn closest_kwarg(target: &str, declared: &[String]) -> Option<String> {
    let threshold = suggestion_threshold(target.len());
    if threshold == 0 {
        return None;
    }
    let mut best: Option<(String, usize)> = None;
    for cand in declared {
        let dist = crate::analyzer_prism::utils::levenshtein(cand, target);
        if dist > threshold {
            continue;
        }
        match &best {
            Some((_, d)) if *d <= dist => {}
            _ => best = Some((cand.clone(), dist)),
        }
    }
    best.map(|(s, _)| s)
}

/// Suggestion threshold scales with name length: 1 for tiny names, up to 3 for long.
fn suggestion_threshold(name_len: usize) -> usize {
    match name_len {
        0..=2 => 0,
        3..=8 => 2,
        _ => 3,
    }
}

/// Strict ancestor-walk lookup. Returns the method's arity if and only if a
/// single matching `MethodData` is found on the owner or one of its ancestors.
fn find_method_arity_strict(
    symbols: &dyn SymbolTable,
    method_name: &str,
    owner: &[RubyConstant],
    kind: NamespaceKind,
) -> Option<MethodArity> {
    let ruby_method = RubyMethod::new(method_name).ok()?;
    let entries = symbols.get_methods_by_name(&ruby_method)?;

    // Build owner + ancestors with namespace_kind, in resolution order.
    let mut search: Vec<FullyQualifiedName> = Vec::new();
    let owner_with_kind = FullyQualifiedName::namespace_with_kind(owner.to_vec(), kind);
    search.push(owner_with_kind.clone());
    for ancestor in symbols.get_ancestor_chain(&owner_with_kind) {
        let with_kind = FullyQualifiedName::namespace_with_kind(ancestor.namespace_parts(), kind);
        if !search.contains(&with_kind) {
            search.push(with_kind);
        }
    }

    for fqn in &search {
        for entry in &entries {
            if let EntryKind::Method(data) = &entry.kind {
                if &data.owner == fqn {
                    return Some(MethodArity::from_params(&data.params));
                }
            }
        }
    }
    None
}

fn find_method_arity_strict_analysis(
    engine: &AnalysisEngine,
    method_name: &str,
    owner: &[RubyConstant],
    kind: NamespaceKind,
) -> Option<MethodArity> {
    let ruby_method = RubyMethod::new(method_name).ok()?;
    let owner_fqn = FullyQualifiedName::namespace_with_kind(owner.to_vec(), kind);
    let fact = AnalysisQuery::new(engine).method_fact_for_receiver(&owner_fqn, &ruby_method)?;
    Some(MethodArity::from_analysis_params(&fact.param_facts))
}

/// Check wrong-arity, unknown-kwargs, and missing-kwargs for a call node.
/// Returns a `Vec<UnresolvedEntry>` — caller attaches URIs when staging.
/// Caller is responsible for the `track_unresolved` guard.
pub fn check(
    node: &CallNode,
    receiver_info: &ReceiverInfo,
    inferred_expr_type: Option<&RubyType>,
    method_name: &str,
    target_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    message_location: &Location,
    document: &RubyDocument,
    symbols: &dyn SymbolTable,
) -> Vec<UnresolvedEntry> {
    let mut out: Vec<UnresolvedEntry> = Vec::new();

    let owner_for_arity = match receiver_info {
        ReceiverInfo::NoReceiver | ReceiverInfo::ConstantReceiver(_) => {
            Some((target_namespace.to_vec(), namespace_kind))
        }
        ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
            if let Some(RubyType::Class(fqn) | RubyType::Module(fqn)) = inferred_expr_type {
                let known = fqn
                    .to_instance_namespace()
                    .as_ref()
                    .map(|ns_fqn| symbols.contains_fqn(ns_fqn))
                    .unwrap_or(false);
                if known {
                    Some((fqn.namespace_parts(), NamespaceKind::Instance))
                } else {
                    None
                }
            } else {
                None
            }
        }
        ReceiverInfo::SelfReceiver => None,
    };

    if let Some((owner, kind)) = owner_for_arity {
        if let Some(arity) = find_method_arity_strict(symbols, method_name, &owner, kind) {
            if let Some((min, max, actual)) = compute_arity_mismatch(node, &arity) {
                out.push(UnresolvedEntry::wrong_arity(
                    method_name.to_string(),
                    min,
                    max,
                    actual,
                    message_location.clone(),
                ));
            }
            if let Some(unknowns) = collect_unknown_kwargs(node, &arity) {
                let all_keyword_names: Vec<String> = arity
                    .required_keywords
                    .iter()
                    .chain(arity.optional_keywords.iter())
                    .cloned()
                    .collect();
                for (kwarg_name, kw_loc) in unknowns {
                    let suggestion = closest_kwarg(&kwarg_name, &all_keyword_names);
                    let kw_lsp_loc = document.prism_location_to_lsp_location(&kw_loc);
                    out.push(UnresolvedEntry::unknown_kwarg(
                        method_name.to_string(),
                        kwarg_name,
                        suggestion,
                        kw_lsp_loc,
                    ));
                }
            }
            if let Some(missing) = collect_missing_required_kwargs(node, &arity) {
                if !missing.is_empty() {
                    out.push(UnresolvedEntry::missing_kwarg(
                        method_name.to_string(),
                        missing,
                        message_location.clone(),
                    ));
                }
            }
        }
    }

    out
}

pub fn check_with_engine(
    node: &CallNode,
    receiver_info: &ReceiverInfo,
    inferred_expr_type: Option<&RubyType>,
    method_name: &str,
    target_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    message_location: &Location,
    document: &RubyDocument,
    engine: &AnalysisEngine,
) -> Vec<UnresolvedEntry> {
    let mut out: Vec<UnresolvedEntry> = Vec::new();

    let owner_for_arity = match receiver_info {
        ReceiverInfo::NoReceiver | ReceiverInfo::ConstantReceiver(_) => {
            Some((target_namespace.to_vec(), namespace_kind))
        }
        ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath => {
            if let Some(RubyType::Class(fqn) | RubyType::Module(fqn)) = inferred_expr_type {
                if let Some(ns_fqn) = fqn.to_instance_namespace() {
                    if !engine.graph_nodes_for(&ns_fqn).is_empty()
                        || !engine.symbol_facts_for(&ns_fqn).is_empty()
                    {
                        Some((fqn.namespace_parts(), NamespaceKind::Instance))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        ReceiverInfo::SelfReceiver => None,
    };

    if let Some((owner, kind)) = owner_for_arity {
        if let Some(arity) = find_method_arity_strict_analysis(engine, method_name, &owner, kind) {
            if let Some((min, max, actual)) = compute_arity_mismatch(node, &arity) {
                out.push(UnresolvedEntry::wrong_arity(
                    method_name.to_string(),
                    min,
                    max,
                    actual,
                    message_location.clone(),
                ));
            }
            if let Some(unknowns) = collect_unknown_kwargs(node, &arity) {
                let all_keyword_names: Vec<String> = arity
                    .required_keywords
                    .iter()
                    .chain(arity.optional_keywords.iter())
                    .cloned()
                    .collect();
                for (kwarg_name, kw_loc) in unknowns {
                    let suggestion = closest_kwarg(&kwarg_name, &all_keyword_names);
                    let kw_lsp_loc = document.prism_location_to_lsp_location(&kw_loc);
                    out.push(UnresolvedEntry::unknown_kwarg(
                        method_name.to_string(),
                        kwarg_name,
                        suggestion,
                        kw_lsp_loc,
                    ));
                }
            }
            if let Some(missing) = collect_missing_required_kwargs(node, &arity) {
                if !missing.is_empty() {
                    out.push(UnresolvedEntry::missing_kwarg(
                        method_name.to_string(),
                        missing,
                        message_location.clone(),
                    ));
                }
            }
        }
    }

    out
}
