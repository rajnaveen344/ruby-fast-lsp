//! Method navigation over one request [`Cursor`]: resolve the call receiver
//! to a namespace, a union type, or a `super` owner, then ask the engine's
//! method lookup for ranked definition ranges. Shared by definition,
//! references, implementation, signature help, and call hierarchy.

mod analysis;

use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::core::RubyType;
use ruby_analysis::engine::View;
use ruby_analysis::indexer::{self, ReceiverResolutionContext};
use tower_lsp::lsp_types::{Location, Position};

use super::Cursor;

enum MethodLookupReceiver {
    Namespace(FullyQualifiedName),
    Type(RubyType),
    Super(FullyQualifiedName),
}

/// Definitions of `method` called on `receiver` from `namespace`, in the
/// engine's semantic order. Without `protected_caller` private targets are
/// allowed (implicit self, `super`, or a static `send`); with it, only public
/// targets and protected targets visible to that caller are.
pub(crate) fn definitions(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    method: &RubyMethod,
    namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<Vec<Location>> {
    let receiver = lookup_receiver(cursor, receiver, namespace, namespace_kind, position)?;
    analysis::definitions(cursor.view, &receiver, method, protected_caller)
}

fn lookup_receiver(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
) -> Option<MethodLookupReceiver> {
    if matches!(receiver, MethodReceiver::Super) {
        return Some(MethodLookupReceiver::Super(
            FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind),
        ));
    }
    if let Some(owner) = receiver_namespace(cursor, receiver, namespace, namespace_kind, position) {
        return Some(MethodLookupReceiver::Namespace(owner));
    }
    let receiver_type = receiver_type(cursor, receiver, namespace, namespace_kind, position);
    matches!(receiver_type, RubyType::Union(_)).then_some(MethodLookupReceiver::Type(receiver_type))
}

/// The namespace a method receiver denotes at `position`.
pub(crate) fn receiver_namespace(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    current_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
) -> Option<FullyQualifiedName> {
    indexer::resolve_receiver_to_namespace(
        receiver,
        &receiver_context(cursor, current_namespace, namespace_kind, position),
    )
}

/// The inferred type of a method receiver at `position`.
pub(crate) fn receiver_type(
    cursor: Cursor<'_>,
    receiver: &MethodReceiver,
    current_namespace: &[RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
) -> RubyType {
    indexer::resolve_receiver_type(
        receiver,
        &receiver_context(cursor, current_namespace, namespace_kind, position),
    )
}

fn receiver_context<'q>(
    cursor: Cursor<'q>,
    current_namespace: &'q [RubyConstant],
    namespace_kind: NamespaceKind,
    position: Position,
) -> ReceiverResolutionContext<'q, View<'q>> {
    ReceiverResolutionContext {
        query: Some(cursor.view),
        document: cursor.document,
        current_namespace,
        namespace_kind,
        byte_offset: cursor.offset(position).unwrap_or(0),
    }
}
