//! Signature help: the overloads and active parameter of the call at the
//! cursor, from Ruby method facts or RBS signatures.

use crate::invariant::ExpectInvariant;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::core::{FullyQualifiedName, MethodFact, MethodParamFact, MethodParamKind};
use ruby_analysis::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use ruby_analysis::inference::method::return_type::rbs_method_signatures_for_type;
use ruby_analysis::inference::rbs::{RbsMethodSignature, RbsSignatureParameter};
use ruby_analysis::inference::semantics::ReceiverAccess;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureHelpParams, SignatureInformation, Url,
};

use crate::features::cursor::{method, Cursor, EngineQuery};
use crate::server::Server;

/// Handle `textDocument/signatureHelp`.
pub async fn handle(
    server: &Server,
    params: SignatureHelpParams,
) -> LspResult<Option<SignatureHelp>> {
    Ok(signature_help(server, params))
}

fn signature_help(server: &Server, params: SignatureHelpParams) -> Option<SignatureHelp> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let document = server.documents.read().get(&uri)?.clone();
    let help = EngineQuery::with_doc_and_project(document, server.project_for_uri(&uri))
        .with_view(|cursor| {
            let content = &cursor.document?.content;
            signature_help_at(cursor, &uri, position, content)
        })?;

    Some(SignatureHelp {
        signatures: help
            .signatures
            .into_iter()
            .map(|signature| SignatureInformation {
                label: signature.label,
                documentation: signature.documentation.map(|value| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value,
                    })
                }),
                parameters: Some(
                    signature
                        .parameters
                        .into_iter()
                        .map(|parameter| ParameterInformation {
                            label: ParameterLabel::Simple(parameter.label),
                            documentation: parameter.documentation.map(Documentation::String),
                        })
                        .collect(),
                ),
                active_parameter: Some(signature.active_parameter),
            })
            .collect(),
        active_signature: Some(help.active_signature),
        active_parameter: Some(help.active_parameter),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelpData {
    pub signatures: Vec<SignatureData>,
    pub active_signature: u32,
    pub active_parameter: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureData {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<SignatureParameterData>,
    pub active_parameter: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureParameterData {
    pub label: String,
    pub documentation: Option<String>,
}

/// The overloads and active parameter of the call enclosing `position`.
pub fn signature_help_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<SignatureHelpData> {
    let file_id = cursor.file_id()?;
    let byte_offset = cursor.offset(position)?;
    let target = cursor
        .analyzer(uri, content, position)
        .get_signature_help_target(byte_offset)?;
    let view = cursor.view;

    if matches!(target.receiver, MethodReceiver::Super) {
        return None;
    }

    let flow_sensitive_receiver = matches!(
        &target.receiver,
        MethodReceiver::LocalVariable(_)
            | MethodReceiver::InstanceVariable(_)
            | MethodReceiver::ClassVariable(_)
            | MethodReceiver::GlobalVariable(_)
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_)
            | MethodReceiver::Expression
    );
    if flow_sensitive_receiver
        && target.receiver_range.is_some_and(|(start, _end)| {
            view.expression_unknown_reason_at(file_id, start).is_some()
        })
    {
        return None;
    }

    let receiver_type = method::receiver_type(
        cursor,
        &target.receiver,
        &target.namespace,
        target.namespace_kind,
        position,
    );
    if flow_sensitive_receiver && receiver_type == ruby_analysis::core::RubyType::Unknown {
        return None;
    }
    let union_receiver = matches!(receiver_type, ruby_analysis::core::RubyType::Union(_));
    let namespace_fqn = if union_receiver {
        None
    } else {
        Some(method::receiver_namespace(
            cursor,
            &target.receiver,
            &target.namespace,
            target.namespace_kind,
            position,
        )?)
    };
    let caller_namespace =
        FullyQualifiedName::namespace_with_kind(target.namespace.clone(), target.namespace_kind);
    let signatures = |receiver, access| {
        let request =
            MethodRequest::new(receiver, target.method, MethodWant::Signatures).with_access(access);
        lookup::method(view, request).into_signature_vec()
    };
    let facts = match &target.receiver {
        MethodReceiver::None => signatures(
            LookupReceiver::Namespace(namespace_fqn.as_ref().expect_invariant(
                "an implicit receiver was classified as a union without a namespace",
                "implicit self has one lexical runtime namespace",
                "keep union receiver handling restricted to explicit typed expressions",
            )),
            ReceiverAccess::Any,
        ),
        MethodReceiver::SelfReceiver
        | MethodReceiver::Constant(_)
        | MethodReceiver::LocalVariable(_)
        | MethodReceiver::InstanceVariable(_)
        | MethodReceiver::ClassVariable(_)
        | MethodReceiver::GlobalVariable(_)
        | MethodReceiver::MethodCall { .. }
        | MethodReceiver::Literal(_)
        | MethodReceiver::Expression => {
            let receiver = if union_receiver {
                LookupReceiver::Type(&receiver_type)
            } else {
                LookupReceiver::Namespace(namespace_fqn.as_ref().expect_invariant(
                        "a non-union explicit receiver lost its resolved namespace before signature lookup",
                        "receiver classification and namespace resolution use the same immutable target",
                        "retain the resolved namespace through signature selection",
                    ))
            };
            signatures(
                receiver,
                ReceiverAccess::Protected {
                    caller: &caller_namespace,
                },
            )
        }
        MethodReceiver::Super => unreachable_invariant!(
            what = "super receiver reached ordinary signature resolution",
            why = "super calls are rejected before receiver resolution",
            fix = "keep the early super return above the receiver match",
        ),
    };
    let mut signatures = facts
        .iter()
        .map(|fact| {
            signature_data(
                target.method.as_str(),
                fact,
                target.active_parameter,
                target.active_keyword.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    if signatures.is_empty() {
        signatures = rbs_method_signatures_for_type(&receiver_type, target.method.as_str())
            .iter()
            .map(|signature| {
                rbs_signature_data(
                    target.method.as_str(),
                    signature,
                    target.active_parameter,
                    target.active_keyword.as_deref(),
                )
            })
            .collect();
    }
    if signatures.is_empty() {
        return None;
    }

    let active_parameter = signatures[0].active_parameter;
    Some(SignatureHelpData {
        signatures,
        active_signature: 0,
        active_parameter,
    })
}

fn signature_data(
    method_name: &str,
    fact: &MethodFact,
    positional_index: u32,
    active_keyword: Option<&str>,
) -> SignatureData {
    let parameters = fact
        .param_facts
        .iter()
        .map(|parameter| SignatureParameterData {
            label: parameter_label(parameter, parameter.type_label.as_deref()),
            documentation: parameter.documentation.clone(),
        })
        .collect::<Vec<_>>();
    let return_type = fact.return_type_label.as_ref();
    let parameter_labels = parameters
        .iter()
        .map(|parameter| parameter.label.as_str())
        .collect::<Vec<_>>();
    SignatureData {
        label: format!(
            "{}({}){}",
            method_name,
            parameter_labels.join(", "),
            return_type
                .as_ref()
                .map(|return_type| format!(" -> {return_type}"))
                .unwrap_or_default()
        ),
        documentation: fact.documentation.clone(),
        parameters,
        active_parameter: active_parameter_for_params(
            &fact.param_facts,
            positional_index,
            active_keyword,
        ),
    }
}

fn rbs_signature_data(
    method_name: &str,
    signature: &RbsMethodSignature,
    positional_index: u32,
    active_keyword: Option<&str>,
) -> SignatureData {
    let parameters = signature
        .parameters
        .iter()
        .map(|parameter| SignatureParameterData {
            label: rbs_parameter_label(parameter),
            documentation: None,
        })
        .collect::<Vec<_>>();
    let parameter_labels = parameters
        .iter()
        .map(|parameter| parameter.label.as_str())
        .collect::<Vec<_>>();
    SignatureData {
        label: format!(
            "{}({}) -> {}",
            method_name,
            parameter_labels.join(", "),
            signature.return_type
        ),
        documentation: None,
        active_parameter: active_parameter_for_rbs_params(
            &signature.parameters,
            positional_index,
            active_keyword,
        ),
        parameters,
    }
}

fn active_parameter_for_params(
    parameters: &[MethodParamFact],
    positional_index: u32,
    active_keyword: Option<&str>,
) -> u32 {
    if let Some(keyword) = active_keyword {
        if let Some(index) = parameters.iter().position(|parameter| {
            parameter.name == keyword
                && matches!(
                    parameter.kind,
                    MethodParamKind::RequiredKeyword | MethodParamKind::OptionalKeyword
                )
        }) {
            return index as u32;
        }
        if let Some(index) = parameters
            .iter()
            .position(|parameter| parameter.kind == MethodParamKind::KeywordRest)
        {
            return index as u32;
        }
    }
    active_positional_parameter(
        parameters.iter().map(|parameter| parameter.kind),
        positional_index,
    )
}

fn active_parameter_for_rbs_params(
    parameters: &[RbsSignatureParameter],
    positional_index: u32,
    active_keyword: Option<&str>,
) -> u32 {
    if let Some(keyword) = active_keyword {
        if let Some(index) = parameters.iter().position(|parameter| {
            parameter.name == keyword
                && matches!(
                    parameter.kind,
                    MethodParamKind::RequiredKeyword | MethodParamKind::OptionalKeyword
                )
        }) {
            return index as u32;
        }
        if let Some(index) = parameters
            .iter()
            .position(|parameter| parameter.kind == MethodParamKind::KeywordRest)
        {
            return index as u32;
        }
    }
    active_positional_parameter(
        parameters.iter().map(|parameter| parameter.kind),
        positional_index,
    )
}

fn active_positional_parameter(
    kinds: impl Iterator<Item = MethodParamKind>,
    positional_index: u32,
) -> u32 {
    let kinds = kinds.collect::<Vec<_>>();
    let positional = kinds
        .iter()
        .enumerate()
        .filter(|(_, kind)| {
            matches!(
                kind,
                MethodParamKind::Required
                    | MethodParamKind::Optional
                    | MethodParamKind::Rest
                    | MethodParamKind::AnonymousRest
                    | MethodParamKind::Forwarding
            )
        })
        .collect::<Vec<_>>();
    if let Some((index, _)) = positional.get(positional_index as usize) {
        return *index as u32;
    }
    if let Some((index, _)) = positional.iter().find(|(_, kind)| {
        matches!(
            kind,
            MethodParamKind::Rest | MethodParamKind::AnonymousRest | MethodParamKind::Forwarding
        )
    }) {
        return *index as u32;
    }
    positional
        .last()
        .map(|(index, _)| *index as u32)
        .unwrap_or(0)
}

fn parameter_label(parameter: &MethodParamFact, type_label: Option<&str>) -> String {
    let typed_name = type_label
        .map(|type_label| format!("{}: {}", parameter.name, type_label))
        .unwrap_or_else(|| parameter.name.clone());
    match parameter.kind {
        MethodParamKind::Required => typed_name,
        MethodParamKind::Optional => format!("{} = ...", typed_name),
        MethodParamKind::Rest => format!("*{}", typed_name),
        MethodParamKind::RequiredKeyword => type_label
            .map(|type_label| format!("{}: {}", parameter.name, type_label))
            .unwrap_or_else(|| format!("{}:", parameter.name)),
        MethodParamKind::OptionalKeyword => type_label
            .map(|type_label| format!("{}: {} = ...", parameter.name, type_label))
            .unwrap_or_else(|| format!("{}: ...", parameter.name)),
        MethodParamKind::KeywordRest => format!("**{}", typed_name),
        MethodParamKind::Block => format!("&{}", typed_name),
        MethodParamKind::Forwarding => "...".to_string(),
        MethodParamKind::AnonymousRest => "*".to_string(),
        MethodParamKind::AnonymousKeywordRest => "**".to_string(),
    }
}

fn rbs_parameter_label(parameter: &RbsSignatureParameter) -> String {
    match parameter.kind {
        MethodParamKind::Required => format!("{}: {}", parameter.name, parameter.type_label),
        MethodParamKind::Optional => {
            format!("{}: {} = ...", parameter.name, parameter.type_label)
        }
        MethodParamKind::Rest => format!("*{}: {}", parameter.name, parameter.type_label),
        MethodParamKind::RequiredKeyword => {
            format!("{}: {}", parameter.name, parameter.type_label)
        }
        MethodParamKind::OptionalKeyword => {
            format!("{}: {} = ...", parameter.name, parameter.type_label)
        }
        MethodParamKind::KeywordRest => format!("**{}: {}", parameter.name, parameter.type_label),
        MethodParamKind::Block => format!("&{}: {}", parameter.name, parameter.type_label),
        MethodParamKind::Forwarding => "...".to_string(),
        MethodParamKind::AnonymousRest => "*".to_string(),
        MethodParamKind::AnonymousKeywordRest => "**".to_string(),
    }
}
