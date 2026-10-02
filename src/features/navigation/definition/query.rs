//! Definition lookup for the identifier at a cursor position.

use crate::invariant::ExpectInvariant;
use log::info;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::{FullyQualifiedName, SymbolKind};
use ruby_analysis::engine::View;
use ruby_analysis::indexer::yard::parser::YardParser;
use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use crate::features::cursor::{method, Cursor};
use crate::utils::lsp::{lsp_text_location, source_position};
use crate::utils::parser::position_to_offset;

/// Definitions for the identifier at `position`: YARD type references,
/// constants, methods, and instance, class, global, and local variables.
pub fn definitions_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    let byte_offset = u32::try_from(position_to_offset(content, position)).expect_invariant(
        "definition position exceeded u32 byte offsets",
        "analysis TextRange offsets are u32",
        "widen domain offsets before accepting larger source files",
    );
    if let Some(yard_type) = YardParser::find_type_at_position(content, source_position(position)) {
        info!("Found YARD type at position: {}", yard_type.type_name);
        let ancestors = RubyPrismAnalyzer::new(uri.clone(), content.to_string())
            .get_namespace_at_offset(byte_offset);
        info!("YARD type namespace context: {:?}", ancestors);
        return yard_type_definitions(cursor.view, &yard_type.type_name, &ancestors);
    }

    let (identifier, _, ancestors, _scope_stack, namespace_kind) = cursor
        .analyzer(uri, content, position)
        .get_identifier(byte_offset);

    // Lexical bindings own their tokens even at the trailing cursor boundary
    // of an adjacent method reference, such as the `[` in table[key].
    // Neither a resolved enclosing call nor an Unknown dispatch barrier may
    // replace that binding with method navigation.
    if let Some(Identifier::RubyLocalVariable { name, .. }) = &identifier {
        return local_variable_definitions(cursor, name, position);
    }
    if let Some(locations) = resolved_reference_definitions(cursor, position) {
        return Some(locations);
    }
    let Some(identifier) = identifier else {
        info!("No identifier found at position {:?}", position);
        return None;
    };
    info!(
        "Looking for definition of: {}->{}",
        FullyQualifiedName::from(ancestors.clone()),
        identifier,
    );

    let view = cursor.view;
    let ranges = match &identifier {
        Identifier::RubyConstant { iden, .. } => view.constant_definition_ranges(iden, &ancestors),
        Identifier::RubyMethod {
            namespace,
            receiver,
            iden,
        } => {
            let caller = FullyQualifiedName::namespace_with_kind(ancestors.clone(), namespace_kind);
            let protected_caller =
                (!method_receiver_allows_private(receiver, content, position)).then_some(&caller);
            return method::definitions(
                cursor,
                receiver,
                iden,
                namespace,
                namespace_kind,
                position,
                protected_caller,
            );
        }
        Identifier::RubyInstanceVariable { name, .. } => {
            view.instance_variable_definition_ranges(name)
        }
        Identifier::RubyClassVariable { name, .. } => view.class_variable_definition_ranges(name),
        Identifier::RubyGlobalVariable { name, .. } => view.global_variable_definition_ranges(name),
        Identifier::RubyLocalVariable { name, .. } => {
            return local_variable_definitions(cursor, name, position);
        }
        // The YARD path above resolves comment types in their namespace.
        Identifier::YardType { type_name, .. } => {
            return yard_type_definitions(view, type_name, &[]);
        }
    };
    non_empty_locations(locations_for_ranges(view, ranges))
}

/// An empty result records an engine dispatch barrier; lexical local-variable
/// lookup remains valid even when the binding's value type is unknown.
fn resolved_reference_definitions(cursor: Cursor<'_>, position: Position) -> Option<Vec<Location>> {
    let file_id = cursor.file_id()?;
    let byte_offset = cursor.offset(position)?;
    let resolved = cursor
        .view
        .resolved_reference_definition_ranges_at(file_id, byte_offset);
    if cursor
        .view
        .navigation_must_fail_closed_at(file_id, byte_offset, !resolved.is_empty())
    {
        return Some(Vec::new());
    }
    non_empty_locations(locations_for_ranges(cursor.view, resolved))
}

/// The open document's variable scopes first, then the nearest earlier
/// local-variable fact in the same file.
fn local_variable_definitions(
    cursor: Cursor<'_>,
    name: &str,
    position: Position,
) -> Option<Vec<Location>> {
    let document = cursor.document?;
    let byte_offset = document.position_to_analysis_offset(source_position(position));
    if let Some(range) = document.local_variable_definition_range_before(name, byte_offset) {
        return Some(vec![lsp_text_location(document, range)]);
    }
    let fqn = FullyQualifiedName::local_variable(name.to_string()).ok()?;
    let file_id = document.analysis_file_id();
    let range = cursor
        .view
        .symbol_facts_for(&fqn)
        .into_iter()
        .filter(|fact| fact.kind == SymbolKind::LocalVariable)
        .filter(|fact| fact.range.file_id == file_id)
        .filter(|fact| fact.range.start_byte < byte_offset)
        .max_by_key(|fact| fact.range.start_byte)
        .map(|fact| fact.range)?;
    non_empty_locations(locations_for_ranges(cursor.view, vec![range]))
}

/// A YARD type reference string (e.g. `String`, `Foo::Bar`) resolved relative
/// to the enclosing namespace.
fn yard_type_definitions(
    view: &View<'_>,
    type_name: &str,
    ancestors: &[RubyConstant],
) -> Option<Vec<Location>> {
    non_empty_locations(locations_for_ranges(
        view,
        view.yard_type_definition_ranges(type_name, ancestors),
    ))
}

/// The constant a path names from `ancestors`, or the literal path when the
/// engine cannot resolve it.
pub(crate) fn constant_fqn(
    view: &View<'_>,
    constant_path: &[RubyConstant],
    ancestors: &[RubyConstant],
) -> FullyQualifiedName {
    view.resolve_constant_in_context(constant_path, ancestors)
        .unwrap_or_else(|| FullyQualifiedName::constant(constant_path.to_vec()))
}

fn method_receiver_allows_private(
    receiver: &MethodReceiver,
    content: &str,
    position: Position,
) -> bool {
    matches!(receiver, MethodReceiver::None | MethodReceiver::Super)
        || static_send_symbol_at_position(content, position)
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
