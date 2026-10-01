use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{MethodCalleeResolution, NamespaceKind, RubyConstant, RubyMethod};
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_analysis::indexer::MethodReceiver as CoreMethodReceiver;
use ruby_fast_lsp_extension_api::{
    Argument, ArgumentValue, CallContext, Keyword, NamespaceKind as AbiNamespaceKind, Receiver,
    ResolvedCall, ResolvedCallee, SourcePosition, SourceRange,
};
use ruby_prism::{CallNode, Node};

pub(super) fn call_context(
    visitor: &FactCollector,
    node: &CallNode,
    include_project: bool,
) -> CallContext {
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    CallContext {
        project: include_project
            .then(|| visitor.extension_project_context().cloned())
            .flatten(),
        method_name: utils::utf8_str(node.name().as_slice()).to_string(),
        receiver: receiver.clone(),
        arguments: node
            .arguments()
            .map(|args| {
                args.arguments()
                    .iter()
                    .flat_map(|arg| arguments_from_node(visitor, &arg))
                    .collect()
            })
            .unwrap_or_default(),
        current_namespace: visitor
            .scope_tracker()
            .get_ns_stack()
            .iter()
            .map(ToString::to_string)
            .collect(),
        namespace_kind: namespace_kind_to_abi(visitor.scope_tracker().current_method_context()),
        call_range: source_range(visitor, &node.location()),
        block_range: node
            .block()
            .map(|block| source_range(visitor, &block.location())),
        message_range: ruby_analysis::indexer::call_reference_location(node)
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
        resolved_callees: resolved_callees_for_call(visitor, node),
        enclosing_calls: visitor.enclosing_extension_calls().to_vec(),
    }
}

pub fn resolved_call_for_stack(visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
    let method_name = utils::utf8_str(node.name().as_slice()).to_string();
    let receiver = node
        .receiver()
        .map(|receiver| receiver_from_node(&receiver))
        .unwrap_or(Receiver::None);
    let resolved_callees = resolved_callees_for_call(visitor, node);
    ResolvedCall {
        method_name,
        receiver: receiver.clone(),
        arguments: node
            .arguments()
            .map(|args| {
                args.arguments()
                    .iter()
                    .flat_map(|arg| arguments_from_node(visitor, &arg))
                    .collect()
            })
            .unwrap_or_default(),
        resolved_callees,
        call_range: source_range(visitor, &node.location()),
        message_range: ruby_analysis::indexer::call_reference_location(node)
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or_else(|| source_range(visitor, &node.location())),
        frame_extension_ids: Vec::new(),
    }
}

fn resolved_callees_for_call(visitor: &FactCollector, node: &CallNode) -> Vec<ResolvedCallee> {
    resolved_core_callees_for_call(visitor, node)
        .into_iter()
        .map(resolved_callee_to_abi)
        .collect()
}

pub(in crate::environment::extensions) fn resolved_core_callees_for_call(
    visitor: &FactCollector,
    node: &CallNode,
) -> Vec<ruby_analysis::core::ResolvedMethodCallee> {
    let method_name = utils::utf8_str(node.name().as_slice());
    let Ok(method) = RubyMethod::new(method_name) else {
        return Vec::new();
    };
    let core_receiver = node
        .receiver()
        .map(|receiver| core_method_receiver_from_node(visitor, &receiver))
        .unwrap_or(CoreMethodReceiver::None);

    visitor.extension_call_callees(&core_receiver, &method)
}

fn resolved_callee_to_abi(callee: ruby_analysis::core::ResolvedMethodCallee) -> ResolvedCallee {
    let owner_kind = callee.owner.namespace_kind().unwrap_or_else(|| {
        unreachable_invariant!(
            what = "analysis resolved extension callee owner `{}` is not a namespace",
            why = "extension callee owners must be namespaces",
            fix = "keep AnalysisQuery::resolve_method_callees returning namespace owners",
            callee.owner,
        )
    });
    ResolvedCallee {
        owner: callee
            .owner
            .namespace_parts()
            .iter()
            .map(ToString::to_string)
            .collect(),
        owner_kind: namespace_kind_to_abi(owner_kind),
        method: callee.method.to_string(),
        resolution: callee_resolution_to_abi(callee.resolution),
    }
}

fn core_method_receiver_from_node(visitor: &FactCollector, node: &Node) -> CoreMethodReceiver {
    if node.as_self_node().is_some() {
        CoreMethodReceiver::SelfReceiver
    } else if let Some(constant) = node.as_constant_read_node() {
        CoreMethodReceiver::Constant(vec![RubyConstant::new(utils::utf8_str(
            constant.name().as_slice(),
        ))
        .expect_invariant(
            "Prism returned an invalid constant-read name",
            "prism constant names must be valid Ruby constants",
            "inspect constant receiver conversion",
        )])
    } else if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        CoreMethodReceiver::Constant(parts)
    } else if let Some(local) = node.as_local_variable_read_node() {
        CoreMethodReceiver::LocalVariable(utils::utf8_str(local.name().as_slice()).to_string())
    } else if let Some(ivar) = node.as_instance_variable_read_node() {
        CoreMethodReceiver::InstanceVariable(utils::utf8_str(ivar.name().as_slice()).to_string())
    } else if let Some(cvar) = node.as_class_variable_read_node() {
        CoreMethodReceiver::ClassVariable(utils::utf8_str(cvar.name().as_slice()).to_string())
    } else if let Some(gvar) = node.as_global_variable_read_node() {
        CoreMethodReceiver::GlobalVariable(utils::utf8_str(gvar.name().as_slice()).to_string())
    } else if let Some(call) = node.as_call_node() {
        CoreMethodReceiver::MethodCall {
            inner_receiver: Box::new(
                call.receiver()
                    .map(|receiver| core_method_receiver_from_node(visitor, &receiver))
                    .unwrap_or(CoreMethodReceiver::None),
            ),
            method_name: utils::utf8_str(call.name().as_slice()).to_string(),
        }
    } else if let Some(ruby_type) =
        ruby_analysis::inference::r#type::literal::LiteralAnalyzer::new().analyze_literal(node)
    {
        CoreMethodReceiver::Literal(ruby_type)
    } else {
        CoreMethodReceiver::Expression
    }
}

fn callee_resolution_to_abi(
    resolution: MethodCalleeResolution,
) -> ruby_fast_lsp_extension_api::CalleeResolution {
    match resolution {
        MethodCalleeResolution::Exact | MethodCalleeResolution::MethodMissing => {
            ruby_fast_lsp_extension_api::CalleeResolution::Exact
        }
        MethodCalleeResolution::ReceiverOnly => {
            ruby_fast_lsp_extension_api::CalleeResolution::ReceiverOnly
        }
    }
}

fn receiver_from_node(node: &Node) -> Receiver {
    if node.as_self_node().is_some() {
        Receiver::SelfReceiver
    } else if let Some(constant) = node.as_constant_read_node() {
        Receiver::Constant(vec![utils::utf8_str(constant.name().as_slice()).to_string()])
    } else if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        Receiver::Constant(parts.iter().map(ToString::to_string).collect())
    } else if let Some(local) = node.as_local_variable_read_node() {
        Receiver::LocalVariable(utils::utf8_str(local.name().as_slice()).to_string())
    } else if let Some(ivar) = node.as_instance_variable_read_node() {
        Receiver::InstanceVariable(utils::utf8_str(ivar.name().as_slice()).to_string())
    } else if let Some(cvar) = node.as_class_variable_read_node() {
        Receiver::ClassVariable(utils::utf8_str(cvar.name().as_slice()).to_string())
    } else if let Some(gvar) = node.as_global_variable_read_node() {
        Receiver::GlobalVariable(utils::utf8_str(gvar.name().as_slice()).to_string())
    } else if let Some(call) = node.as_call_node() {
        Receiver::MethodCall {
            method_name: utils::utf8_str(call.name().as_slice()).to_string(),
        }
    } else if is_literal(node) {
        Receiver::Literal
    } else {
        Receiver::Expression
    }
}

fn arguments_from_node(visitor: &FactCollector, node: &Node) -> Vec<Argument> {
    if let Some(keyword_hash) = node.as_keyword_hash_node() {
        return keyword_hash
            .elements()
            .iter()
            .filter_map(|element| {
                let assoc = element.as_assoc_node()?;
                let symbol = assoc.key().as_symbol_node()?;
                Some(Argument {
                    keyword: Some(Keyword {
                        name: String::from_utf8_lossy(symbol.unescaped()).to_string(),
                        range: source_range(visitor, &symbol.location()),
                    }),
                    value: argument_value_from_node(&assoc.value()),
                    range: argument_value_range(visitor, &assoc.value()),
                })
            })
            .collect();
    }

    vec![argument_from_node(visitor, node)]
}

fn argument_from_node(visitor: &FactCollector, node: &Node) -> Argument {
    Argument {
        keyword: None,
        value: argument_value_from_node(node),
        range: argument_value_range(visitor, node),
    }
}

fn argument_value_from_node(node: &Node) -> ArgumentValue {
    if let Some(symbol) = node.as_symbol_node() {
        return ArgumentValue::Symbol(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    if let Some(string) = node.as_string_node() {
        return ArgumentValue::String(String::from_utf8_lossy(string.unescaped()).to_string());
    }
    if let Some(constant) = node.as_constant_read_node() {
        return ArgumentValue::Constant(vec![
            utils::utf8_str(constant.name().as_slice()).to_string()
        ]);
    }
    if let Some(path) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        utils::collect_namespaces(&path, &mut parts);
        return ArgumentValue::Constant(parts.iter().map(ToString::to_string).collect());
    }
    if node.as_true_node().is_some() {
        ArgumentValue::Boolean(true)
    } else if node.as_false_node().is_some() {
        ArgumentValue::Boolean(false)
    } else if node.as_nil_node().is_some() {
        ArgumentValue::Nil
    } else {
        ArgumentValue::Unsupported
    }
}

fn argument_value_range(visitor: &FactCollector, node: &Node) -> SourceRange {
    if let Some(string) = node.as_string_node() {
        source_range(visitor, &string.content_loc())
    } else {
        source_range(visitor, &node.location())
    }
}

fn source_range(visitor: &FactCollector, location: &ruby_prism::Location) -> SourceRange {
    let range = visitor.document().prism_location_to_source_range(location);
    SourceRange {
        start: SourcePosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: SourcePosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

fn namespace_kind_to_abi(kind: NamespaceKind) -> AbiNamespaceKind {
    match kind {
        NamespaceKind::Instance => AbiNamespaceKind::Instance,
        NamespaceKind::Singleton => AbiNamespaceKind::Singleton,
    }
}

fn is_literal(node: &Node) -> bool {
    node.as_string_node().is_some()
        || node.as_interpolated_string_node().is_some()
        || node.as_integer_node().is_some()
        || node.as_float_node().is_some()
        || node.as_symbol_node().is_some()
        || node.as_array_node().is_some()
        || node.as_hash_node().is_some()
        || node.as_true_node().is_some()
        || node.as_false_node().is_some()
        || node.as_nil_node().is_some()
}
