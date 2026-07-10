use ruby_analysis::core::{FullyQualifiedName, MethodFact, MethodParamFact, MethodParamKind};
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::{MethodReceiver, RubyPrismAnalyzer};
use ruby_analysis::inference::method::rbs_method_signatures_for_type;
use ruby_analysis::inference::rbs::{RbsMethodSignature, RbsSignatureParameter};
use tower_lsp::lsp_types::{Position, Url};

use super::EngineQuery;

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

impl EngineQuery {
    pub fn signature_help_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<SignatureHelpData> {
        let document = self.doc.as_ref()?.read();
        let byte_offset = document.position_to_analysis_offset(position);
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let target = analyzer.get_signature_help_target(byte_offset)?;
        drop(document);

        if matches!(target.receiver, MethodReceiver::Super) {
            return None;
        }

        let namespace_fqn = self.resolve_receiver_to_namespace(
            &target.receiver,
            &target.namespace,
            target.namespace_kind,
            position,
        )?;
        let caller_namespace = FullyQualifiedName::namespace_with_kind(
            target.namespace.clone(),
            target.namespace_kind,
        );
        let engine = self.analysis_engine()?.read();
        let query = AnalysisQuery::new(&engine);
        let facts = match &target.receiver {
            MethodReceiver::None => {
                query.resolve_method_signature_facts(&namespace_fqn, &target.method)
            }
            MethodReceiver::SelfReceiver
            | MethodReceiver::Constant(_)
            | MethodReceiver::LocalVariable(_)
            | MethodReceiver::InstanceVariable(_)
            | MethodReceiver::ClassVariable(_)
            | MethodReceiver::GlobalVariable(_)
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_)
            | MethodReceiver::Expression => query.resolve_protected_method_signature_facts(
                &namespace_fqn,
                &target.method,
                &caller_namespace,
            ),
            MethodReceiver::Super => unreachable!(
                "INVARIANT VIOLATED: super receiver reached ordinary signature resolution. \
                 This is a bug because super calls are rejected before receiver resolution. \
                 Fix: keep the early super return above the receiver match."
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
            let receiver_type = self.resolve_receiver_type(
                &target.receiver,
                &target.namespace,
                target.namespace_kind,
                position,
            );
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
                MethodParamKind::Required | MethodParamKind::Optional | MethodParamKind::Rest
            )
        })
        .collect::<Vec<_>>();
    if let Some((index, _)) = positional.get(positional_index as usize) {
        return *index as u32;
    }
    if let Some((index, _)) = positional
        .iter()
        .find(|(_, kind)| **kind == MethodParamKind::Rest)
    {
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
    }
}
