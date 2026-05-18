use ruby_analysis_engine::{AnalysisEngine, AnalysisQuery};
use ruby_prism::CallNode;

use crate::analyzer_prism::utils;
use crate::{
    indexer::{
        entry::entry_kind::EntryKind,
        index::UnresolvedEntry,
        index_ref::{Index, Unlocked},
        symbol_table::SymbolTable,
    },
    inferrer::{method::resolver::MethodResolver, r#type::ruby::RubyType},
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_document::RubyDocument,
        ruby_method::RubyMethod, ruby_namespace::RubyConstant,
    },
};

/// Stdlib exception class names that are always valid `raise` arguments.
const EXCEPTION_WHITELIST: &[&str] = &[
    "Exception",
    "StandardError",
    "RuntimeError",
    "ArgumentError",
    "TypeError",
    "NameError",
    "NoMethodError",
    "IOError",
    "RangeError",
    "NotImplementedError",
    "ZeroDivisionError",
    "IndexError",
    "KeyError",
    "StopIteration",
    "SystemExit",
    "Interrupt",
    "ScriptError",
    "SyntaxError",
    "LoadError",
    "LocalJumpError",
    "FrozenError",
    "EncodingError",
    "RegexpError",
    "SystemCallError",
    "ThreadError",
    "FiberError",
    "SecurityError",
    "SignalException",
];

/// Stdlib non-exception types — provably unsafe to raise. Enumerated so we
/// never warn on unknown third-party types (conservative).
const NON_EXCEPTION_TYPES: &[&str] = &[
    "Integer",
    "Float",
    "Rational",
    "Complex",
    "Numeric",
    "Array",
    "Hash",
    "Symbol",
    "Regexp",
    "Range",
    "Proc",
    "Method",
    "UnboundMethod",
    "IO",
    "File",
    "Dir",
    "Time",
    "Struct",
    "Encoding",
    "Fiber",
    "Thread",
    "Mutex",
    "Queue",
    "TrueClass",
    "FalseClass",
    "NilClass",
    "Binding",
    "BasicObject",
    "Object",
];

/// Check whether `name` resolves to an Exception subclass.
///
/// Returns `true` (safe to raise) when:
/// - Name is in the stdlib whitelist, OR
/// - Name ends with "Error" / "Exception" (heuristic for unindexed user classes), OR
/// - Name is found in the user index and its ancestor chain includes a whitelist entry.
///
/// Returns `false` (warn) only when the class is in the user index but its ancestors
/// do NOT include Exception. Unknown classes not in the index are treated as safe
/// (conservative — avoid false positives on third-party gems).
fn is_exception_class(symbols: &dyn SymbolTable, name: &str) -> bool {
    if EXCEPTION_WHITELIST.contains(&name) {
        return true;
    }
    // Suffix heuristic: unindexed UserDefinedError / FooException → treat as safe.
    if name.ends_with("Error") || name.ends_with("Exception") {
        return true;
    }
    // User index walk.
    if let Ok(ruby_const) = RubyConstant::new(name) {
        let ns_fqn = FullyQualifiedName::namespace_with_kind(
            vec![ruby_const],
            crate::indexer::entry::NamespaceKind::Instance,
        );
        if symbols.contains_fqn(&ns_fqn) {
            for ancestor in symbols.get_ancestor_chain(&ns_fqn) {
                let last = ancestor.namespace_parts().last().map(|c| c.to_string());
                if let Some(n) = last {
                    if EXCEPTION_WHITELIST.contains(&n.as_str()) {
                        return true;
                    }
                }
            }
            // Class is indexed but no Exception ancestor found.
            return false;
        }
    }
    // Not in index and no suffix match → conservative, assume unknown/safe.
    true
}

fn is_exception_class_analysis(engine: &AnalysisEngine, name: &str) -> bool {
    if EXCEPTION_WHITELIST.contains(&name) {
        return true;
    }
    if name.ends_with("Error") || name.ends_with("Exception") {
        return true;
    }
    let Ok(ruby_const) = RubyConstant::new(name) else {
        return true;
    };
    let ns_fqn = FullyQualifiedName::namespace_with_kind(
        vec![ruby_const],
        crate::indexer::entry::NamespaceKind::Instance,
    );
    if engine.graph_nodes_for(&ns_fqn).is_empty() && engine.symbol_facts_for(&ns_fqn).is_empty() {
        return true;
    }

    let mut current = ns_fqn;
    let mut visited = std::collections::HashSet::new();
    while visited.insert(current.clone()) {
        for edge in engine.all_graph_edges() {
            if edge.kind != ruby_analysis_core::GraphEdgeKind::Superclass || edge.source != current
            {
                continue;
            }
            let last = edge.target.namespace_parts().last().map(|c| c.to_string());
            if let Some(n) = last {
                if EXCEPTION_WHITELIST.contains(&n.as_str()) {
                    return true;
                }
            }
            current = edge.target;
            break;
        }
    }

    false
}

/// Returns `true` when `ty` is safe to raise or uncertain (silent).
/// Returns `false` when `ty` is provably non-exception (warn).
///
/// Conservative: Union/Unknown → silent (avoid FPs).
/// String → silent (Ruby wraps in RuntimeError, mirrors V1 literal behaviour).
fn classify_raise_type(symbols: &dyn SymbolTable, ty: &RubyType) -> bool {
    match ty {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            let name = fqn
                .namespace_parts()
                .last()
                .map(|c| c.to_string())
                .unwrap_or_default();
            // String → Ruby wraps in RuntimeError, same as V1 string-literal path.
            if name == "String" {
                return true;
            }
            // Known stdlib non-exception types → provably not raiseable.
            if NON_EXCEPTION_TYPES.contains(&name.as_str()) {
                return false;
            }
            is_exception_class(symbols, &name)
        }
        // Modules can't be raised.
        RubyType::Module(_) | RubyType::ModuleReference(_) => false,
        // Union/Unknown → uncertain, skip.
        RubyType::Union(_) | RubyType::Unknown => true,
        // Everything else (Array, Hash, Integer, etc.) → warn.
        _ => false,
    }
}

fn classify_raise_type_analysis(engine: &AnalysisEngine, ty: &RubyType) -> bool {
    match ty {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            let name = fqn
                .namespace_parts()
                .last()
                .map(|c| c.to_string())
                .unwrap_or_default();
            if name == "String" {
                return true;
            }
            if NON_EXCEPTION_TYPES.contains(&name.as_str()) {
                return false;
            }
            is_exception_class_analysis(engine, &name)
        }
        RubyType::Module(_) | RubyType::ModuleReference(_) => false,
        RubyType::Union(_) | RubyType::Unknown => true,
        _ => false,
    }
}

/// Look up the return type of a bare (no-receiver) method call in the index.
///
/// Searches the current namespace and its ancestors for a matching method entry.
/// Returns `None` when the method is not found or has no inferred return type.
fn resolve_bare_call_return_type(
    symbols: &dyn SymbolTable,
    current_namespace: &[RubyConstant],
    method_name: &str,
) -> Option<RubyType> {
    let method = RubyMethod::new(method_name).ok()?;
    let entries = symbols.get_methods_by_name(&method)?;

    // Walk current namespace → parents → top-level (empty ns).
    let mut search_ns: Vec<Vec<RubyConstant>> = Vec::new();
    let mut ns = current_namespace.to_vec();
    loop {
        search_ns.push(ns.clone());
        if ns.is_empty() {
            break;
        }
        ns.pop();
    }

    for candidate_ns in &search_ns {
        for entry in entries.iter() {
            if let EntryKind::Method(data) = &entry.kind {
                if data.owner.namespace_parts() == *candidate_ns {
                    return Some(data.return_type.clone().unwrap_or(RubyType::Unknown));
                }
            }
        }
    }
    None
}

fn resolve_bare_call_return_type_analysis(
    engine: &AnalysisEngine,
    current_namespace: &[RubyConstant],
    method_name: &str,
) -> Option<RubyType> {
    let method = RubyMethod::new(method_name).ok()?;
    let query = AnalysisQuery::new(engine);
    let mut ns = current_namespace.to_vec();
    loop {
        let namespace_fqn = FullyQualifiedName::namespace_with_kind(
            ns.clone(),
            crate::indexer::entry::NamespaceKind::Instance,
        );
        if let Some(fact) = query.method_fact_for_receiver(&namespace_fqn, &method) {
            return query.method_return_type(&fact).or(Some(RubyType::Unknown));
        }
        if ns.is_empty() {
            break;
        }
        ns.pop();
    }
    None
}

/// Inspect the first argument of a bare `raise` call and return an
/// `UnresolvedEntry::RaiseNonException` when the argument is provably
/// not an Exception subclass. Returns `None` when uncertain.
// MUST be called with the index mutex NOT held. This function acquires
// short-lived read locks (including one for `MethodResolver::resolve_call_type`
// on call-arg receivers) — holding an outer guard causes a re-entrant deadlock
// (parking_lot `RwLock` is not reentrant). Callers in `process_call_node_entry`
// drop their guard before invoking.
pub fn check(
    node: &CallNode,
    index: &Index<Unlocked>,
    document: &RubyDocument,
    current_namespace: &[RubyConstant],
) -> Option<UnresolvedEntry> {
    let args = node.arguments()?;
    let first_arg = args.arguments().iter().next()?;

    let arg_loc = document.prism_location_to_lsp_location(&first_arg.location());
    let arg_repr = String::from_utf8_lossy(first_arg.location().as_slice()).to_string();

    // String → Ruby wraps in RuntimeError, always OK.
    if first_arg.as_string_node().is_some() {
        return None;
    }

    // Definite non-exception literals.
    if first_arg.as_integer_node().is_some()
        || first_arg.as_float_node().is_some()
        || first_arg.as_array_node().is_some()
        || first_arg.as_hash_node().is_some()
        || first_arg.as_symbol_node().is_some()
        || first_arg.as_true_node().is_some()
        || first_arg.as_false_node().is_some()
        || first_arg.as_nil_node().is_some()
        || first_arg.as_range_node().is_some()
    {
        return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
    }

    // Constant reference (e.g., `raise MyError`).
    if let Some(const_read) = first_arg.as_constant_read_node() {
        let name = utils::utf8_str(const_read.name().as_slice());
        let guard = index.read();
        let symbols: &dyn SymbolTable = &*guard;
        if !is_exception_class(symbols, name) {
            return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
        }
        return None;
    }

    // ConstantPath (e.g., `raise Foo::MyError`) — check last segment name.
    if let Some(_const_path) = first_arg.as_constant_path_node() {
        let full_name = crate::analyzer_prism::utils::build_constant_path_name(&first_arg);
        let last_segment = full_name.split("::").last().unwrap_or(&full_name);
        let guard = index.read();
        let symbols: &dyn SymbolTable = &*guard;
        if !is_exception_class(symbols, last_segment) {
            return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
        }
        return None;
    }

    // LocalVariableReadNode — look up inferred type via VariableScopes.
    if let Some(local) = first_arg.as_local_variable_read_node() {
        let var_name = utils::utf8_str(local.name().as_slice());
        let var_loc = first_arg.location();
        let var_pos = document.offset_to_position(var_loc.start_offset());
        let scopes = document.variable_scopes();
        let scope_id = scopes
            .find_scope_for_variable_at(var_name, var_pos)
            .or_else(|| scopes.scope_at_position(var_pos));
        if let Some(sid) = scope_id {
            if let Some(ty) = scopes.get_type_at_position(&var_name, sid, var_pos) {
                let guard = index.read();
                let symbols: &dyn SymbolTable = &*guard;
                if !classify_raise_type(symbols, ty) {
                    return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
                }
                return None;
            }
        }
        // Type unknown → uncertain, skip.
        return None;
    }

    // CallNode argument (e.g., `raise foo()` or `raise obj.method`) — resolve return type.
    if let Some(inner_call) = first_arg.as_call_node() {
        let ty = if inner_call.receiver().is_some() {
            // Has receiver — use MethodResolver chain. Must NOT hold the
            // index mutex here: MethodResolver locks internally.
            let resolver =
                MethodResolver::with_namespace(index.clone(), current_namespace.to_vec());
            resolver.resolve_call_type(&inner_call)
        } else {
            // Bare method call (no receiver) — look up by name in current namespace.
            let method_name = utils::utf8_str(inner_call.name().as_slice());
            let guard = index.read();
            let symbols: &dyn SymbolTable = &*guard;
            resolve_bare_call_return_type(symbols, current_namespace, method_name)
        };
        if let Some(ty) = ty {
            let guard = index.read();
            let symbols: &dyn SymbolTable = &*guard;
            if !classify_raise_type(symbols, &ty) {
                return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
            }
            return None;
        }
        // Return type unknown → uncertain, skip.
        return None;
    }

    // Anything else (interpolation, etc.) → uncertain, skip.
    None
}

pub fn check_with_engine(
    node: &CallNode,
    engine: &AnalysisEngine,
    document: &RubyDocument,
    current_namespace: &[RubyConstant],
) -> Option<UnresolvedEntry> {
    let args = node.arguments()?;
    let first_arg = args.arguments().iter().next()?;

    let arg_loc = document.prism_location_to_lsp_location(&first_arg.location());
    let arg_repr = String::from_utf8_lossy(first_arg.location().as_slice()).to_string();

    if first_arg.as_string_node().is_some() {
        return None;
    }

    if first_arg.as_integer_node().is_some()
        || first_arg.as_float_node().is_some()
        || first_arg.as_array_node().is_some()
        || first_arg.as_hash_node().is_some()
        || first_arg.as_symbol_node().is_some()
        || first_arg.as_true_node().is_some()
        || first_arg.as_false_node().is_some()
        || first_arg.as_nil_node().is_some()
        || first_arg.as_range_node().is_some()
    {
        return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
    }

    if let Some(const_read) = first_arg.as_constant_read_node() {
        let name = utils::utf8_str(const_read.name().as_slice());
        if !is_exception_class_analysis(engine, name) {
            return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
        }
        return None;
    }

    if first_arg.as_constant_path_node().is_some() {
        let full_name = crate::analyzer_prism::utils::build_constant_path_name(&first_arg);
        let last_segment = full_name.split("::").last().unwrap_or(&full_name);
        if !is_exception_class_analysis(engine, last_segment) {
            return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
        }
        return None;
    }

    if let Some(local) = first_arg.as_local_variable_read_node() {
        let var_name = utils::utf8_str(local.name().as_slice());
        let var_loc = first_arg.location();
        let var_pos = document.offset_to_position(var_loc.start_offset());
        let scopes = document.variable_scopes();
        let scope_id = scopes
            .find_scope_for_variable_at(var_name, var_pos)
            .or_else(|| scopes.scope_at_position(var_pos));
        if let Some(sid) = scope_id {
            if let Some(ty) = scopes.get_type_at_position(&var_name, sid, var_pos) {
                if !classify_raise_type_analysis(engine, ty) {
                    return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
                }
                return None;
            }
        }
        return None;
    }

    if let Some(inner_call) = first_arg.as_call_node() {
        if inner_call.receiver().is_none() {
            let method_name = utils::utf8_str(inner_call.name().as_slice());
            if let Some(ty) =
                resolve_bare_call_return_type_analysis(engine, current_namespace, method_name)
            {
                if !classify_raise_type_analysis(engine, &ty) {
                    return Some(UnresolvedEntry::raise_non_exception(arg_repr, arg_loc));
                }
                return None;
            }
        }
        return None;
    }

    None
}
