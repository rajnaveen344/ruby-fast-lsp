use log::warn;
use ruby_fast_lsp_extension_api::{DocumentContext, ExtensionEvent, ResponsePatch, SourceRange};
use tower_lsp::lsp_types::{CodeLens, Command, DocumentSymbol, Position, Range, SymbolKind};

use crate::extensions::patches::validation::validate_response_patch_provenance;
use crate::extensions::registry::handle::ExtensionRegistryHandle;

pub(super) fn document_symbols_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    handle_response_event(
        registry,
        "request.document_symbol",
        uri,
        text,
        project,
        |patch| match response_patch_to_document_symbol(patch) {
            Ok(Some(symbol)) => {
                symbols.push(symbol);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        },
    );
    symbols
}

pub(super) fn code_lenses_with_registry(
    registry: &ExtensionRegistryHandle,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    handle_response_event(registry, "request.code_lens", uri, text, project, |patch| {
        match response_patch_to_code_lens(patch) {
            Ok(Some(lens)) => {
                lenses.push(lens);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(err) => Err(err),
        }
    });
    lenses
}

fn handle_response_event(
    registry: &ExtensionRegistryHandle,
    event_name: &str,
    uri: &str,
    text: &str,
    project: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    mut handle_patch: impl FnMut(ResponsePatch) -> Result<(), String>,
) {
    let required_capability = match event_name {
        "request.document_symbol" => "document_symbol",
        "request.code_lens" => "code_lens",
        other => panic!(
            "INVARIANT VIOLATED: unsupported response event `{other}` reached extension dispatch. This is a host bug because response events must map to an explicit manifest capability. Fix: add the event-to-capability mapping before dispatching it."
        ),
    };
    let event = ExtensionEvent {
        event: event_name.to_string(),
        call: None,
        document: Some(DocumentContext {
            uri: uri.to_string(),
            text: text.to_string(),
            project: project.clone(),
        }),
        project: None,
        settings: None,
        files: None,
        process_results: None,
    };
    let extensions = registry.extensions();

    for loaded in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        if !loaded
            .metadata
            .capabilities
            .iter()
            .any(|capability| capability == required_capability)
        {
            continue;
        }
        if !loaded.applies_to_source(project.as_ref()) {
            continue;
        }

        let extension_output = match loaded.handle_event_for_project(&event, project.as_ref()) {
            Ok(extension_output) => extension_output,
            Err(err) => {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after event `{}` failure: {}",
                    loaded.metadata.id, event_name, err
                );
                let reason = err.to_string();
                loaded.fail(reason);
                continue;
            }
        };
        if let Err(spoofed_id) = validate_response_patch_provenance(
            &loaded.metadata.id,
            &extension_output.response_patches,
        ) {
            warn!(
                "Disabling Ruby Fast LSP extension `{}` after response patch provenance spoofed `{}` for `{}`",
                loaded.metadata.id, spoofed_id, event_name
            );
            loaded.reject(format!(
                "extension `{}` emitted response patch provenance for `{spoofed_id}`",
                loaded.metadata.id
            ));
            continue;
        }
        for patch in extension_output.response_patches {
            if let Err(err) = handle_patch(patch) {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after invalid response patch for `{}`: {}",
                    loaded.metadata.id, event_name, err
                );
                loaded.reject(err);
                break;
            }
        }
    }
}

pub(super) fn response_patch_to_document_symbol(
    patch: ResponsePatch,
) -> Result<Option<DocumentSymbol>, String> {
    let ResponsePatch::DocumentSymbol(symbol) = patch else {
        return Ok(None);
    };

    Ok(Some(DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: symbol_kind_from_extension(&symbol.kind)?,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: range_from_abi(symbol.range),
        selection_range: range_from_abi(symbol.selection_range),
        children: None,
    }))
}

fn response_patch_to_code_lens(patch: ResponsePatch) -> Result<Option<CodeLens>, String> {
    let ResponsePatch::CodeLens(lens) = patch else {
        return Ok(None);
    };

    Ok(Some(CodeLens {
        range: range_from_abi(lens.range),
        command: Some(Command {
            title: lens.title,
            command: lens.command,
            arguments: Some(
                lens.arguments
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        }),
        data: None,
    }))
}

pub(super) fn range_from_abi(range: SourceRange) -> Range {
    Range::new(
        Position::new(range.start.line, range.start.character),
        Position::new(range.end.line, range.end.character),
    )
}

fn symbol_kind_from_extension(kind: &str) -> Result<SymbolKind, String> {
    let symbol_kind = match kind {
        "File" => SymbolKind::FILE,
        "Module" => SymbolKind::MODULE,
        "Namespace" => SymbolKind::NAMESPACE,
        "Package" => SymbolKind::PACKAGE,
        "Class" => SymbolKind::CLASS,
        "Method" => SymbolKind::METHOD,
        "Property" => SymbolKind::PROPERTY,
        "Field" => SymbolKind::FIELD,
        "Constructor" => SymbolKind::CONSTRUCTOR,
        "Enum" => SymbolKind::ENUM,
        "Interface" => SymbolKind::INTERFACE,
        "Function" => SymbolKind::FUNCTION,
        "Variable" => SymbolKind::VARIABLE,
        "Constant" => SymbolKind::CONSTANT,
        "String" => SymbolKind::STRING,
        "Number" => SymbolKind::NUMBER,
        "Boolean" => SymbolKind::BOOLEAN,
        "Array" => SymbolKind::ARRAY,
        "Object" => SymbolKind::OBJECT,
        "Key" => SymbolKind::KEY,
        "Null" => SymbolKind::NULL,
        "EnumMember" => SymbolKind::ENUM_MEMBER,
        "Struct" => SymbolKind::STRUCT,
        "Event" => SymbolKind::EVENT,
        "Operator" => SymbolKind::OPERATOR,
        "TypeParameter" => SymbolKind::TYPE_PARAMETER,
        other => return Err(format!("unsupported document symbol kind `{}`", other)),
    };
    Ok(symbol_kind)
}
