//! Classes built by `Name = Struct.new(:member, ...)`.
//!
//! The write declares a class whose members are readers and writers, and a
//! block on the call is the class body. Only the form whose members are all
//! written as symbols is recognized; any other form stays an ordinary
//! constant, because its members are not known from syntax.

use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant};
use crate::invariant::ExpectInvariant;
use ruby_prism::{CallNode, Node};

use super::scope_rules::{resolve_lexical_namespace, BlockExecution};
use super::scope_tracker::ScopeTracker;

/// A recognized `Struct.new(:member, ...)` value of a constant write.
pub struct StructDefinition<'pr> {
    pub call: CallNode<'pr>,
    /// The constant path of the parent class, `Struct`.
    pub parent: Vec<RubyConstant>,
    /// Whether the parent was written `::Struct`.
    pub parent_is_absolute: bool,
}

/// The struct class `name = value` declares. `is_known` is the walk's
/// knowledge of declared namespaces: a `Struct` that lexical lookup finds
/// anywhere but the top level is another class, and proves nothing.
pub fn struct_definition<'pr>(
    name: &[u8],
    value: &Node<'pr>,
    lexical_context: &[RubyConstant],
    is_known: impl Fn(&FullyQualifiedName) -> bool,
) -> Option<StructDefinition<'pr>> {
    let call = value.as_call_node()?;
    if call.name().as_slice() != b"new" || call.is_safe_navigation() || name == b"Struct" {
        return None;
    }
    let parent_is_absolute = match call.receiver()? {
        receiver if receiver.as_constant_read_node().is_some() => {
            (receiver.as_constant_read_node()?.name().as_slice() == b"Struct").then_some(false)?
        }
        receiver => {
            let path = receiver.as_constant_path_node()?;
            (path.parent().is_none() && path.name()?.as_slice() == b"Struct").then_some(true)?
        }
    };
    if let Some(block) = call.block() {
        block.as_block_node()?;
    }
    let arguments: Vec<_> = call.arguments()?.arguments().iter().collect();
    let (last, leading) = arguments.split_last()?;
    let members = if is_keyword_init_option(last) {
        leading
    } else {
        arguments.as_slice()
    };
    if members.is_empty() || !members.iter().all(is_plain_symbol) {
        return None;
    }
    let parent = vec![RubyConstant::new("Struct").expect_invariant(
        "Struct is not a valid Ruby constant",
        "the parent of a struct class must be representable",
        "update RubyConstant validation or struct parent construction",
    )];
    let core = FullyQualifiedName::namespace(parent.clone());
    let found = resolve_lexical_namespace(&parent, parent_is_absolute, lexical_context, is_known);
    if found.is_some_and(|found| found != core) {
        return None;
    }
    Some(StructDefinition {
        call,
        parent,
        parent_is_absolute,
    })
}

fn is_plain_symbol(node: &Node<'_>) -> bool {
    node.as_symbol_node()
        .is_some_and(|symbol| symbol.value_loc().is_some())
}

/// A trailing `keyword_init: true|false|nil` option.
fn is_keyword_init_option(node: &Node<'_>) -> bool {
    let Some(hash) = node.as_keyword_hash_node() else {
        return false;
    };
    let elements: Vec<_> = hash.elements().iter().collect();
    let [element] = elements.as_slice() else {
        return false;
    };
    let Some(assoc) = element.as_assoc_node() else {
        return false;
    };
    let key_is_option = assoc
        .key()
        .as_symbol_node()
        .is_some_and(|key| key.unescaped() == b"keyword_init");
    let value = assoc.value();
    key_is_option
        && (value.as_true_node().is_some()
            || value.as_false_node().is_some()
            || value.as_nil_node().is_some())
}

/// The context of the members and of the block of a struct definition:
/// `self` is the class and a `def` defines an instance method on it.
pub fn struct_class_execution(namespace: Vec<RubyConstant>) -> BlockExecution {
    BlockExecution {
        implicit_namespace: namespace.clone(),
        implicit_kind: NamespaceKind::Singleton,
        definition_namespace: namespace,
        definition_kind: NamespaceKind::Instance,
    }
}

/// The class body context of `node`'s block, when the enclosing constant
/// write recorded `node` as a struct definition.
pub fn struct_definition_block(
    node: &CallNode<'_>,
    scope_tracker: &ScopeTracker,
) -> Option<BlockExecution> {
    let namespace = scope_tracker.struct_definition_block_at(node.location().start_offset())?;
    node.block()?;
    Some(struct_class_execution(namespace.to_vec()))
}
