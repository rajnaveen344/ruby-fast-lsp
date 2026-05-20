use ruby_prism::{CallNode, Node};
use tower_lsp::lsp_types::Location;

use crate::{analyzer_prism::utils, types::ruby_document::RubyDocument};
use ruby_analysis_inference::RubyType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadSplatCandidate {
    pub operator: String,
    pub arg_repr: String,
    pub expected: String,
    pub location: Location,
}

/// Check *splat and **splat arguments at a callsite for type mismatches.
///
/// *expr must be Array-like; **expr must be Hash-like.
/// Conservative: silent on Union/Unknown/user-defined classes.
pub fn check(node: &CallNode, document: &RubyDocument) -> Vec<BadSplatCandidate> {
    let mut entries = Vec::new();
    let args = match node.arguments() {
        Some(a) => a,
        None => return entries,
    };
    for arg in args.arguments().iter() {
        // *expr — positional splat
        if let Some(splat) = arg.as_splat_node() {
            if let Some(expr) = splat.expression() {
                if is_definitely_non_array(&expr, document) {
                    let arg_repr = String::from_utf8_lossy(expr.location().as_slice()).to_string();
                    let loc = document.prism_location_to_lsp_location(&splat.location());
                    entries.push(BadSplatCandidate {
                        operator: "*".to_string(),
                        arg_repr,
                        expected: "Array".to_string(),
                        location: loc,
                    });
                }
            }
        }
        // **expr — keyword splat lives inside KeywordHashNode elements
        if let Some(kw_hash) = arg.as_keyword_hash_node() {
            for elem in kw_hash.elements().iter() {
                if let Some(assoc_splat) = elem.as_assoc_splat_node() {
                    if let Some(expr) = assoc_splat.value() {
                        if is_definitely_non_hash(&expr, document) {
                            let arg_repr =
                                String::from_utf8_lossy(expr.location().as_slice()).to_string();
                            let loc =
                                document.prism_location_to_lsp_location(&assoc_splat.location());
                            entries.push(BadSplatCandidate {
                                operator: "**".to_string(),
                                arg_repr,
                                expected: "Hash".to_string(),
                                location: loc,
                            });
                        }
                    }
                }
            }
        }
    }
    entries
}

/// Returns `true` when `expr` is provably NOT array-like.
/// nil → OK (Ruby treats as []). Literals whose type cannot respond to `to_a`
/// in the normal sense → warn. Unknown/Union/user-class → silent.
fn is_definitely_non_array(expr: &Node, document: &RubyDocument) -> bool {
    if expr.as_nil_node().is_some() {
        return false;
    }
    if expr.as_integer_node().is_some()
        || expr.as_float_node().is_some()
        || expr.as_string_node().is_some()
        || expr.as_symbol_node().is_some()
        || expr.as_true_node().is_some()
        || expr.as_false_node().is_some()
        || expr.as_hash_node().is_some()
        || expr.as_range_node().is_some()
    {
        return true;
    }
    if let Some(local) = expr.as_local_variable_read_node() {
        let var_name = utils::utf8_str(local.name().as_slice());
        let byte_offset = u32::try_from(expr.location().start_offset()).expect(
            "INVARIANT VIOLATED: Prism location offset exceeded u32. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let file_id = document.analysis_file_id();
        let scopes = document.variable_scopes();
        let sid = scopes
            .find_scope_for_variable_at(var_name, file_id, byte_offset)
            .or_else(|| scopes.scope_at_position(file_id, byte_offset));
        if let Some(sid) = sid {
            if let Some(ty) = scopes.get_type_at_position(var_name, sid, file_id, byte_offset) {
                return is_type_definitely_non_array(ty);
            }
        }
    }
    false
}

/// Returns `true` when `expr` is provably NOT hash-like.
/// nil → OK (Ruby treats as {}). Hash literal → OK.
fn is_definitely_non_hash(expr: &Node, document: &RubyDocument) -> bool {
    if expr.as_nil_node().is_some() {
        return false;
    }
    if expr.as_hash_node().is_some() {
        return false;
    }
    if expr.as_integer_node().is_some()
        || expr.as_float_node().is_some()
        || expr.as_string_node().is_some()
        || expr.as_symbol_node().is_some()
        || expr.as_true_node().is_some()
        || expr.as_false_node().is_some()
        || expr.as_array_node().is_some()
        || expr.as_range_node().is_some()
    {
        return true;
    }
    if let Some(local) = expr.as_local_variable_read_node() {
        let var_name = utils::utf8_str(local.name().as_slice());
        let byte_offset = u32::try_from(expr.location().start_offset()).expect(
            "INVARIANT VIOLATED: Prism location offset exceeded u32. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let file_id = document.analysis_file_id();
        let scopes = document.variable_scopes();
        let sid = scopes
            .find_scope_for_variable_at(var_name, file_id, byte_offset)
            .or_else(|| scopes.scope_at_position(file_id, byte_offset));
        if let Some(sid) = sid {
            if let Some(ty) = scopes.get_type_at_position(var_name, sid, file_id, byte_offset) {
                return is_type_definitely_non_hash(ty);
            }
        }
    }
    false
}

/// Returns `true` when the type is a known scalar stdlib type — provably not Array.
/// Conservative: user-defined classes might define `to_a`, so only warn on stdlib scalars.
fn is_type_definitely_non_array(ty: &RubyType) -> bool {
    match ty {
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            let name = fqn
                .namespace_parts()
                .last()
                .map(|c| c.to_string())
                .unwrap_or_default();
            matches!(
                name.as_str(),
                "Integer"
                    | "Float"
                    | "String"
                    | "Symbol"
                    | "TrueClass"
                    | "FalseClass"
                    | "Hash"
                    | "Range"
                    | "NilClass"
            )
        }
        _ => false,
    }
}

/// Returns `true` when the type is a known stdlib type — provably not Hash.
fn is_type_definitely_non_hash(ty: &RubyType) -> bool {
    match ty {
        RubyType::Hash(_, _) => false,
        RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
            let name = fqn
                .namespace_parts()
                .last()
                .map(|c| c.to_string())
                .unwrap_or_default();
            matches!(
                name.as_str(),
                "Integer"
                    | "Float"
                    | "String"
                    | "Symbol"
                    | "TrueClass"
                    | "FalseClass"
                    | "Array"
                    | "Range"
                    | "NilClass"
            )
        }
        _ => false,
    }
}
