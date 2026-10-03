use crate::core::{FullyQualifiedName, NamespaceKind, RubyMethod, RubyType, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodCalleeResolution {
    Exact,
    MethodMissing,
    ReceiverOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMethodCallee {
    pub owner: FullyQualifiedName,
    pub method: RubyMethod,
    pub resolution: MethodCalleeResolution,
    pub definition_ranges: Vec<TextRange>,
}

/// The namespaces a receiver value type dispatches method lookups through:
/// instance namespaces for values, singleton namespaces for class and module
/// references, every member of a union, and the widened type of a literal.
/// Collection and unknown receivers have no project namespace here.
pub fn receiver_type_to_method_namespaces(ruby_type: &RubyType) -> Vec<FullyQualifiedName> {
    match ruby_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            let mut namespaces = vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Instance,
            )];
            if fqn.name() == "Object" {
                namespaces.push(FullyQualifiedName::namespace_with_kind(
                    Vec::new(),
                    NamespaceKind::Instance,
                ));
            }
            namespaces
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            vec![FullyQualifiedName::namespace_with_kind(
                fqn.namespace_parts(),
                NamespaceKind::Singleton,
            )]
        }
        RubyType::Union(types) => types
            .iter()
            .flat_map(receiver_type_to_method_namespaces)
            .collect(),
        RubyType::Literal(value) => receiver_type_to_method_namespaces(&value.widened_type()),
        RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_) | RubyType::Unknown => {
            Vec::new()
        }
    }
}
