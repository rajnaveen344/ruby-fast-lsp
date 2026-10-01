//! Readers that extract names, constant paths, parameters, and ranges from
//! Prism nodes without consulting indexer state.

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, MethodParamFact, MethodParamKind, RubyConstant,
    SourceFileId, TextRange,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{AliasMethodNode, CallNode, ConstantPathNode, DefNode, Node};

use crate::indexer::constant_path_is_absolute;

pub(super) fn constant_parts(node: &Node<'_>) -> Option<Vec<RubyConstant>> {
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        return RubyConstant::new(&name).ok().map(|constant| vec![constant]);
    }
    if let Some(path) = node.as_constant_path_node() {
        return constant_path_parts(&path);
    }
    None
}

pub(super) fn attr_name_and_range(
    node: &Node<'_>,
    file_id: SourceFileId,
) -> Option<(String, TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &symbol.location()),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            text_range(file_id, &string.content_loc()),
        ));
    }
    None
}

pub(super) fn method_name_and_range(
    node: &Node<'_>,
    file_id: SourceFileId,
) -> Option<(String, TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &location),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            text_range(file_id, &string.content_loc()),
        ));
    }
    None
}

pub(super) fn included_hook_mixin_call_kind(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(GraphEdgeKind, usize)> {
    match node.name().as_slice() {
        b"include" => Some((GraphEdgeKind::Include, 0)),
        b"extend" => Some((GraphEdgeKind::Extend, 0)),
        b"send" | b"public_send" | b"__send__" => {
            let arguments = node.arguments()?;
            let first = arguments.arguments().iter().next()?;
            let (selector, _) = attr_name_and_range(&first, file_id)?;
            match selector.as_str() {
                "include" => Some((GraphEdgeKind::Include, 1)),
                "extend" => Some((GraphEdgeKind::Extend, 1)),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn define_method_name_and_range(
    node: &CallNode<'_>,
    file_id: SourceFileId,
    name_index: usize,
) -> Option<(String, TextRange)> {
    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().nth(name_index)?;
    if let Some(symbol) = arg.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &location),
        ));
    }
    attr_name_and_range(&arg, file_id)
}

pub(super) fn call_two_symbol_or_string_args(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(String, String)> {
    let arguments = node.arguments()?;
    let args = arguments.arguments();
    let mut iter = args.iter();
    let (new_name, _) = attr_name_and_range(&iter.next()?, file_id)?;
    let (old_name, _) = attr_name_and_range(&iter.next()?, file_id)?;
    Some((new_name, old_name))
}

pub(super) fn delegate_methods_and_receiver(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(Vec<String>, String)> {
    let arguments = node.arguments()?;
    let mut methods = Vec::new();
    let mut receiver = None;
    for arg in arguments.arguments().iter() {
        if let Some(keyword_hash) = arg.as_keyword_hash_node() {
            for element in keyword_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some((key, _)) = attr_name_and_range(&assoc.key(), file_id) else {
                    continue;
                };
                if key.trim_end_matches(':') == "to" {
                    receiver = attr_name_and_range(&assoc.value(), file_id).map(|(name, _)| name);
                }
            }
        } else if let Some((name, _range)) = attr_name_and_range(&arg, file_id) {
            methods.push(name);
        }
    }

    let receiver = receiver?;
    (!methods.is_empty()).then_some((methods, receiver))
}

pub(super) fn forwardable_delegates_and_receiver(
    node: &CallNode<'_>,
    file_id: SourceFileId,
) -> Option<(String, Vec<(String, String)>)> {
    let arguments = node.arguments()?;
    let mut args = arguments.arguments().iter();
    let (receiver, _) = attr_name_and_range(&args.next()?, file_id)?;
    let mut methods = Vec::new();

    match node.name().as_slice() {
        b"def_delegators" => {
            for arg in args {
                let Some((name, _)) = attr_name_and_range(&arg, file_id) else {
                    continue;
                };
                methods.push((name.clone(), name));
            }
        }
        b"def_delegator" => {
            let (target_name, _) = attr_name_and_range(&args.next()?, file_id)?;
            let defined_name = args
                .next()
                .and_then(|arg| attr_name_and_range(&arg, file_id).map(|(name, _)| name))
                .unwrap_or_else(|| target_name.clone());
            methods.push((defined_name, target_name));
        }
        _ => return None,
    }

    (!methods.is_empty()).then_some((receiver, methods))
}

pub(super) fn symbol_name_and_range(
    node: &Node<'_>,
    file_id: SourceFileId,
) -> Option<(String, TextRange)> {
    node.as_symbol_node().map(|symbol| {
        (
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            text_range(file_id, &symbol.location()),
        )
    })
}

pub(super) fn alias_method_names(node: &AliasMethodNode<'_>) -> Option<(String, String)> {
    let new_name = symbol_name(&node.new_name())?;
    let old_name = symbol_name(&node.old_name())?;
    Some((new_name, old_name))
}

pub(super) fn symbol_name(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}

pub(super) fn constant_parts_and_absolute(node: &Node<'_>) -> Option<(Vec<RubyConstant>, bool)> {
    if let Some(read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        return RubyConstant::new(&name)
            .ok()
            .map(|constant| (vec![constant], false));
    }
    if let Some(path) = node.as_constant_path_node() {
        let absolute = constant_path_is_absolute(&path);
        return constant_path_parts(&path).map(|parts| (parts, absolute));
    }
    None
}

pub(super) fn constant_path_parts(path: &ConstantPathNode<'_>) -> Option<Vec<RubyConstant>> {
    let mut parts = Vec::new();
    collect_constant_path_parts(path, &mut parts);
    (!parts.is_empty()).then_some(parts)
}

pub(super) fn method_param_facts(node: &DefNode<'_>) -> Vec<MethodParamFact> {
    let mut params = Vec::new();
    let Some(params_node) = node.parameters() else {
        return params;
    };

    for required in params_node.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice()).to_string(),
                MethodParamKind::Required,
            ));
        }
    }

    for optional in params_node.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice()).to_string(),
                MethodParamKind::Optional,
            ));
        }
    }

    if let Some(rest) = params_node.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                params.push(MethodParamFact::new(
                    String::from_utf8_lossy(name.as_slice()).to_string(),
                    MethodParamKind::Rest,
                ));
            } else {
                params.push(MethodParamFact::new("*", MethodParamKind::AnonymousRest));
            }
        } else if rest.as_forwarding_parameter_node().is_some() {
            params.push(MethodParamFact::new("...", MethodParamKind::Forwarding));
        }
    }

    for post in params_node.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice()).to_string(),
                MethodParamKind::Required,
            ));
        }
    }

    for keyword in params_node.keywords().iter() {
        if let Some(param) = keyword.as_required_keyword_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice())
                    .trim_end_matches(':')
                    .to_string(),
                MethodParamKind::RequiredKeyword,
            ));
        } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(param.name().as_slice())
                    .trim_end_matches(':')
                    .to_string(),
                MethodParamKind::OptionalKeyword,
            ));
        }
    }

    if let Some(kwrest) = params_node.keyword_rest() {
        if let Some(param) = kwrest.as_keyword_rest_parameter_node() {
            if let Some(name) = param.name() {
                params.push(MethodParamFact::new(
                    String::from_utf8_lossy(name.as_slice()).to_string(),
                    MethodParamKind::KeywordRest,
                ));
            } else {
                params.push(MethodParamFact::new(
                    "**",
                    MethodParamKind::AnonymousKeywordRest,
                ));
            }
        } else if kwrest.as_forwarding_parameter_node().is_some() {
            params.push(MethodParamFact::new("...", MethodParamKind::Forwarding));
        }
    }

    if let Some(block) = params_node.block() {
        if let Some(name) = block.name() {
            params.push(MethodParamFact::new(
                String::from_utf8_lossy(name.as_slice()).to_string(),
                MethodParamKind::Block,
            ));
        }
    }

    params
}

fn collect_constant_path_parts(path: &ConstantPathNode<'_>, parts: &mut Vec<RubyConstant>) {
    if let Some(parent) = path.parent() {
        if let Some(parent_path) = parent.as_constant_path_node() {
            collect_constant_path_parts(&parent_path, parts);
        } else if let Some(parent_read) = parent.as_constant_read_node() {
            let name = String::from_utf8_lossy(parent_read.name().as_slice()).to_string();
            if let Ok(constant) = RubyConstant::new(&name) {
                parts.push(constant);
            }
        }
    }
    if let Some(name) = path.name() {
        let name = String::from_utf8_lossy(name.as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            parts.push(constant);
        }
    }
}

pub(super) fn text_range(file_id: SourceFileId, location: &ruby_prism::Location<'_>) -> TextRange {
    TextRange::new(
        file_id,
        u32_offset(location.start_offset()),
        u32_offset(location.end_offset()),
    )
}

pub(super) fn terminal_name_range(
    file_id: SourceFileId,
    path: &ruby_prism::Location<'_>,
    name: &[u8],
) -> TextRange {
    let end = path.end_offset();
    let start = end.checked_sub(name.len()).expect_invariant(
        "constant name is longer than its Prism path location",
        "the terminal name must be contained in the constant path",
        "inspect Prism constant path locations before deriving declaration ranges",
    );
    TextRange::new(file_id, u32_offset(start), u32_offset(end))
}

pub(super) fn class_implicitly_inherits_object(fqn: &FullyQualifiedName) -> bool {
    let parts = fqn.namespace_parts();
    !matches!(
        parts.as_slice(),
        [name] if name.as_str() == "Object" || name.as_str() == "BasicObject"
    )
}

pub(super) fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect_invariant(
        "source byte offset exceeded u32",
        "analysis facts currently store u32 ranges",
        "widen TextRange offsets before indexing files larger than u32::MAX bytes",
    )
}
