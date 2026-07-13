use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel, SignatureHelp,
    SignatureHelpParams, SignatureInformation,
};

use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;

pub async fn handle_signature_help(
    server: &RubyLanguageServer,
    params: SignatureHelpParams,
) -> Option<SignatureHelp> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let (content, document) = {
        let documents = server.docs.lock();
        let document = documents.get(&uri)?.clone();
        let content = document.read().content.clone();
        (content, document)
    };
    let query = EngineQuery::with_doc_and_engine(document, server.analysis_engine_for_uri(&uri));
    let help = query.signature_help_at_position(&uri, position, &content)?;

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
