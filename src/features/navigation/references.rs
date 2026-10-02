//! Find references: every usage of the symbol at the cursor, plus the
//! same-document locations that document highlights reuse.

use crate::invariant::ExpectInvariant;
use parking_lot::RwLock;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::core::MethodVisibility;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::core::SourceFileId;
use ruby_analysis::core::TextRange;
use ruby_analysis::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use ruby_analysis::engine::View;
use ruby_analysis::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use ruby_analysis::indexer::yard::converter::YardTypeConverter;
use ruby_analysis::indexer::{Identifier, RubyDocument};
use ruby_analysis::inference::semantics::ReceiverAccess;
use ruby_prism::Visit;
use std::path::Path;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{Location, Position, Range, ReferenceParams, Url};

use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use crate::features::cursor::{method, Cursor, EngineQuery};
use crate::server::{ProjectHandle, RubyLanguageServer};
use crate::utils::lsp::{deduplicate_locations, lsp_text_location, source_position};
use crate::utils::parser::position_to_offset;

/// Handle `textDocument/references`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: ReferenceParams,
) -> LspResult<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    Ok(find_references_at_position(server, &uri, position).await)
}

/// Find all references to the symbol at `position` in an open document.
pub async fn find_references_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    read_open_document(server, uri, |cursor| references_at(cursor, position))
}

/// Read one cursor over the open document at `uri`, then rebuild stale local
/// variable scopes after both guards are released when the read asks for it.
pub(crate) fn read_open_document(
    server: &RubyLanguageServer,
    uri: &Url,
    read: impl FnOnce(Cursor<'_>) -> Answer,
) -> Option<Vec<Location>> {
    let document = server.documents.read().get(uri)?.clone();
    let project = server.project_for_uri(uri);
    EngineQuery::with_doc_and_project(document.clone(), project.clone())
        .with_view(read)
        .finish(&document, &project)
}

/// A cursor read's result. A local variable without reference ranges in the
/// open document's scopes is answered after those scopes are rebuilt, which
/// writes the document and so cannot happen under the read guards.
pub(crate) enum Answer {
    Locations(Option<Vec<Location>>),
    StaleLocalScopes { name: String, byte_offset: u32 },
}

impl Answer {
    fn finish(
        self,
        document: &Arc<RwLock<RubyDocument>>,
        project: &ProjectHandle,
    ) -> Option<Vec<Location>> {
        let (name, byte_offset) = match self {
            Answer::Locations(locations) => return locations,
            Answer::StaleLocalScopes { name, byte_offset } => (name, byte_offset),
        };
        rebuild_local_variable_scopes(document, project);
        let document = document.read();
        let ranges = document.local_variable_reference_ranges_at(&name, byte_offset);
        (!ranges.is_empty()).then(|| document_locations(&document, ranges))
    }
}

/// All references to the symbol at `position` in the cursor's document.
pub(crate) fn references_at(cursor: Cursor<'_>, position: Position) -> Answer {
    let Some(document) = cursor.document else {
        return Answer::Locations(None);
    };
    let content = document.content.as_str();
    if let Some(locations) = module_call_locations(cursor, position, content, false) {
        return Answer::Locations(Some(locations));
    }
    let (identifier, _, ancestors, _scope_stack, namespace_kind) = cursor
        .analyzer(&document.uri, content, position)
        .get_identifier_at_position(source_position(position));
    let Some(identifier) = identifier else {
        return Answer::Locations(None);
    };

    let view = cursor.view;
    let locations = |ranges| non_empty_locations(locations_for_ranges(view, ranges));
    Answer::Locations(match &identifier {
        Identifier::RubyConstant { iden, .. } => {
            locations(view.constant_reference_ranges(iden, &ancestors))
        }
        Identifier::RubyMethod { receiver, iden, .. } => {
            method_references(cursor, receiver, iden, &ancestors, namespace_kind, position)
        }
        Identifier::RubyInstanceVariable { name, .. } => {
            FullyQualifiedName::instance_variable(name.clone())
                .ok()
                .and_then(|fqn| locations(view.variable_reference_ranges(&fqn)))
        }
        Identifier::RubyClassVariable { name, .. } => {
            FullyQualifiedName::class_variable(name.clone())
                .ok()
                .and_then(|fqn| locations(view.variable_reference_ranges(&fqn)))
        }
        Identifier::RubyGlobalVariable { name, .. } => {
            FullyQualifiedName::global_variable(name.clone())
                .ok()
                .and_then(|fqn| locations(view.variable_reference_ranges(&fqn)))
        }
        Identifier::RubyLocalVariable { name, .. } => {
            return local_variable_references(document, name, position);
        }
        Identifier::YardType { type_name, .. } => {
            YardTypeConverter::parse_type_name_to_fqn_public(type_name)
                .and_then(|fqn| locations(view.reference_ranges_for_fqn(&fqn)))
        }
    })
}

/// Same-document highlight locations for the symbol at `position`.
///
/// Resolves identity the same way as find-references, then collects only
/// ranges in the open file. Does not run project-wide
/// `method_reference_ranges*` (or its full-workspace private-source scan).
pub(crate) fn highlights_at(cursor: Cursor<'_>, position: Position) -> Answer {
    let (Some(document), Some(file_id)) = (cursor.document, cursor.file_id()) else {
        return Answer::Locations(None);
    };
    let content = document.content.as_str();
    if let Some(locations) = module_call_locations(cursor, position, content, true) {
        return Answer::Locations(Some(locations));
    }
    let (identifier, _, ancestors, _scope_stack, namespace_kind) = cursor
        .analyzer(&document.uri, content, position)
        .get_identifier_at_position(source_position(position));
    let Some(identifier) = identifier else {
        return Answer::Locations(None);
    };

    let view = cursor.view;
    let same_file =
        |fqn: FullyQualifiedName| unique_locations(view, same_file_ranges(view, &fqn, file_id));
    Answer::Locations(match &identifier {
        Identifier::RubyConstant { iden, .. } => unique_locations(
            view,
            view.constant_reference_ranges(iden, &ancestors)
                .into_iter()
                .filter(|range| range.file_id == file_id)
                .collect(),
        ),
        Identifier::RubyMethod { receiver, iden, .. } => method_highlights(
            cursor,
            receiver,
            iden,
            &ancestors,
            namespace_kind,
            position,
            file_id,
        ),
        Identifier::RubyInstanceVariable { name, .. } => {
            FullyQualifiedName::instance_variable(name.clone())
                .ok()
                .and_then(same_file)
        }
        Identifier::RubyClassVariable { name, .. } => {
            FullyQualifiedName::class_variable(name.clone())
                .ok()
                .and_then(same_file)
        }
        Identifier::RubyGlobalVariable { name, .. } => {
            FullyQualifiedName::global_variable(name.clone())
                .ok()
                .and_then(same_file)
        }
        // Locals are already document-scoped via VariableScopes.
        Identifier::RubyLocalVariable { name, .. } => {
            return local_variable_references(document, name, position);
        }
        Identifier::YardType { type_name, .. } => {
            YardTypeConverter::parse_type_name_to_fqn_public(type_name).and_then(same_file)
        }
    })
}

fn module_call_locations(
    cursor: Cursor<'_>,
    position: Position,
    content: &str,
    same_file: bool,
) -> Option<Vec<Location>> {
    let file_id = cursor.file_id()?;
    let byte_offset = u32::try_from(position_to_offset(content, position)).expect_invariant(
        "reference position exceeded u32 offsets",
        "engine ranges use u32",
        "bound source input sizes",
    );
    let ranges = if same_file {
        cursor
            .view
            .module_call_highlight_ranges_at(file_id, byte_offset)?
    } else {
        cursor
            .view
            .module_call_reference_ranges_at(file_id, byte_offset)?
    };
    Some(deduplicate_locations(locations_for_ranges(
        cursor.view,
        ranges,
    )))
}

/// References to a method. Expression receivers resolve through the same
/// type inference as go-to-definition; an uninferred receiver returns `None`
/// rather than a guess.
fn method_references(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    method: &RubyMethod,
    ancestors: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
) -> Option<Vec<Location>> {
    let view = cursor.view;
    // `def initialize` is indexed as `new` (singleton).
    if method.as_str() == "initialize" {
        if let Ok(new_method) = RubyMethod::new("new") {
            let singleton = FullyQualifiedName::namespace_with_kind(
                ancestors.to_vec(),
                NamespaceKind::Singleton,
            );
            return unique_locations(view, view.method_reference_ranges(&singleton, &new_method));
        }
    }

    let scope = FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
    let ranges = match receiver {
        MethodReceiver::Constant(receiver_path) => view
            .method_reference_ranges_for_constant_receiver_public(receiver_path, ancestors, method),
        MethodReceiver::Super => view.super_method_reference_ranges(&scope, method),
        MethodReceiver::None => view.method_reference_ranges(&scope, method),
        MethodReceiver::SelfReceiver => {
            view.method_reference_ranges_protected_receiver(&scope, method, &scope)
        }
        _ => {
            let owner =
                method::receiver_namespace(cursor, receiver, ancestors, namespace_kind, position)?;
            let content = cursor.document.map_or("", |document| &document.content);
            if static_send_symbol_at_position(content, position) {
                view.method_reference_ranges(&owner, method)
            } else {
                view.method_reference_ranges_protected_receiver(&owner, method, &scope)
            }
        }
    };
    let locations = unique_locations(view, ranges)?;
    Some(without_invalid_private_receivers(cursor, method, locations))
}

fn method_highlights(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    method: &RubyMethod,
    ancestors: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
    file_id: SourceFileId,
) -> Option<Vec<Location>> {
    let view = cursor.view;
    // `def initialize` is indexed as `new` (singleton).
    if method.as_str() == "initialize" {
        if let Ok(new_method) = RubyMethod::new("new") {
            let singleton = FullyQualifiedName::namespace_with_kind(
                ancestors.to_vec(),
                NamespaceKind::Singleton,
            );
            return method_target_highlights(view, &singleton, &new_method, file_id, false);
        }
    }

    let scope = FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
    let (owner, super_only) = match receiver {
        MethodReceiver::Constant(receiver_path) => (
            view.resolve_constant_receiver(receiver_path, ancestors),
            false,
        ),
        MethodReceiver::Super => (scope, true),
        MethodReceiver::None | MethodReceiver::SelfReceiver => (scope, false),
        _ => (
            method::receiver_namespace(cursor, receiver, ancestors, namespace_kind, position)?,
            false,
        ),
    };
    method_target_highlights(view, &owner, method, file_id, super_only)
}

fn method_target_highlights(
    view: &View<'_>,
    owner: &FullyQualifiedName,
    method: &RubyMethod,
    file_id: SourceFileId,
    super_only: bool,
) -> Option<Vec<Location>> {
    let targets = if super_only {
        view.super_method_reference_target(owner, method)
            .into_iter()
            .collect()
    } else {
        view.method_reference_targets(owner, method)
    };
    let ranges = targets
        .iter()
        .flat_map(|target| same_file_ranges(view, target, file_id))
        .collect();
    unique_locations(view, ranges)
}

fn local_variable_references(document: &RubyDocument, name: &str, position: Position) -> Answer {
    let byte_offset = document.position_to_analysis_offset(source_position(position));
    let ranges = document.local_variable_reference_ranges_at(name, byte_offset);
    if ranges.is_empty() {
        return Answer::StaleLocalScopes {
            name: name.to_string(),
            byte_offset,
        };
    }
    Answer::Locations(Some(document_locations(document, ranges)))
}

fn document_locations(document: &RubyDocument, ranges: Vec<TextRange>) -> Vec<Location> {
    ranges
        .into_iter()
        .map(|range| lsp_text_location(document, range))
        .collect()
}

fn rebuild_local_variable_scopes(document: &Arc<RwLock<RubyDocument>>, project: &ProjectHandle) {
    let snapshot = document.read().clone();
    let content = snapshot.content.clone();
    let parse_result = ruby_prism::parse(content.as_bytes());
    let mut collector = FactCollector::analysis_only(
        snapshot,
        Arc::new(NullFactCollectorExtensionHost),
        project.semantics(),
    )
    .without_analysis_method_return_resolution()
    .without_expression_receiver_inference()
    .without_diagnostics();
    collector.visit(&parse_result.node());
    document.write().variable_scopes = collector.into_document().variable_scopes;
}

fn unique_locations(view: &View<'_>, ranges: Vec<TextRange>) -> Option<Vec<Location>> {
    non_empty_locations(deduplicate_locations(locations_for_ranges(view, ranges)))
}

fn same_file_ranges(
    view: &View<'_>,
    fqn: &FullyQualifiedName,
    file_id: SourceFileId,
) -> Vec<TextRange> {
    view.reference_facts_for(fqn)
        .iter()
        .map(|fact| fact.range)
        .filter(|range| range.file_id == file_id)
        .collect()
}

/// Drop explicit-receiver calls of a private method unless a public
/// visibility override makes the receiver's target callable.
fn without_invalid_private_receivers(
    cursor: Cursor<'_>,
    method: &RubyMethod,
    locations: Vec<Location>,
) -> Vec<Location> {
    let Some(document) = cursor.document else {
        return locations;
    };
    if !document_declares_private_method(&document.content, method.as_str())
        && !has_private_method(cursor.view, method)
    {
        return locations;
    }
    locations
        .into_iter()
        .filter(|location| {
            let Some(content) =
                location_content(cursor.view, location, &document.uri, &document.content)
            else {
                return true;
            };
            !range_uses_invalid_private_receiver(content, location.range)
                || explicit_receiver_constant_parts(content, location.range).is_none()
                || public_receiver_target_exists(cursor.view, method, content, location.range)
        })
        .collect()
}

fn public_receiver_target_exists(
    view: &View<'_>,
    method: &RubyMethod,
    content: &str,
    range: Range,
) -> bool {
    let Some((receiver_parts, receiver_kind)) = explicit_receiver_constant_parts(content, range)
    else {
        return false;
    };
    let receiver_fqn = FullyQualifiedName::namespace_with_kind(receiver_parts, receiver_kind);
    let request = MethodRequest::new(
        LookupReceiver::Namespace(&receiver_fqn),
        *method,
        MethodWant::Callees,
    )
    .with_access(ReceiverAccess::Public);
    lookup::method(view, request)
        .into_callees()
        .is_some_and(|callees| {
            callees
                .iter()
                .any(|callee| !callee.definition_ranges.is_empty())
        })
}

fn has_private_method(view: &View<'_>, method: &RubyMethod) -> bool {
    view.all_method_facts().iter().any(|fact| {
        let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
            return false;
        };
        *fact_method == *method && fact.visibility == MethodVisibility::Private
    }) || view
        .all_method_visibility_overrides()
        .iter()
        .any(|fact| fact.method == *method && fact.visibility == MethodVisibility::Private)
}

fn location_content<'a>(
    view: &View<'a>,
    location: &Location,
    current_uri: &Url,
    current_content: &'a str,
) -> Option<&'a str> {
    if &location.uri == current_uri {
        return Some(current_content);
    }
    let path = location.uri.to_file_path().ok()?;
    if let Some(file_id) = view.file_id(&path) {
        return view.file(file_id)?.source.as_deref();
    }
    let relative = path.strip_prefix(Path::new("/")).ok()?;
    if let Some(file_id) = view.file_id(relative) {
        return view.file(file_id)?.source.as_deref();
    }
    view.files()
        .find(|file| file.path.ends_with(relative))
        .and_then(|file| file.source.as_deref())
}

fn document_declares_private_method(content: &str, method: &str) -> bool {
    let mut visibility_private = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        match trimmed {
            "private" => {
                visibility_private = true;
                continue;
            }
            "protected" | "public" => {
                visibility_private = false;
                continue;
            }
            _ => {}
        }
        let Some(rest) = trimmed.strip_prefix("def ") else {
            if visibility_line_mentions_method(trimmed, "private", method) {
                return true;
            }
            continue;
        };
        let name = rest
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ';'))
            .next()
            .unwrap_or("");
        if name == method && visibility_private {
            return true;
        }
    }
    false
}

fn visibility_line_mentions_method(line: &str, keyword: &str, method: &str) -> bool {
    let Some(rest) = line.strip_prefix(keyword) else {
        return false;
    };
    rest.split(',')
        .map(|part| {
            part.trim()
                .trim_start_matches(':')
                .trim_matches('"')
                .trim_matches('\'')
        })
        .any(|name| name == method)
}

fn range_uses_invalid_private_receiver(content: &str, range: Range) -> bool {
    let Some(line) = content.lines().nth(range.start.line as usize) else {
        return false;
    };
    let before = line
        .chars()
        .take(range.start.character as usize)
        .collect::<String>();
    let trimmed = before.trim_end();
    trimmed.ends_with('.')
        || trimmed.ends_with("public_send(:")
        || trimmed.ends_with("public_send(\"")
}

/// The constant an explicit receiver names, and which side of it receives the
/// call: `Shapes.call` reaches the module object, `Shapes.new.call` an instance.
fn explicit_receiver_constant_parts(
    content: &str,
    range: Range,
) -> Option<(Vec<RubyConstant>, NamespaceKind)> {
    let line = content.lines().nth(range.start.line as usize)?;
    let before = line
        .chars()
        .take(range.start.character as usize)
        .collect::<String>();
    let receiver = before.trim_end().strip_suffix('.')?.trim_end();
    let (receiver, kind) = match receiver.strip_suffix(".new") {
        Some(instance) => (instance, NamespaceKind::Instance),
        None => (receiver, NamespaceKind::Singleton),
    };
    let token = receiver.split_whitespace().last()?;
    let mut parts = Vec::new();
    for part in token.split("::") {
        parts.push(RubyConstant::new(part).ok()?);
    }
    (!parts.is_empty()).then_some((parts, kind))
}

fn static_send_symbol_at_position(content: &str, position: Position) -> bool {
    let Some(line) = content.lines().nth(position.line as usize) else {
        return false;
    };
    line.contains(".send(:")
        || line.contains(".__send__(:")
        || line.contains(".send(\"")
        || line.contains(".__send__(\"")
}
