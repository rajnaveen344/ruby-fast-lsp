use crate::core::{FullyQualifiedName, RubyType, SymbolKind, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantLookupRequest {
    pub partial_name: String,
    pub namespace_prefix: Option<FullyQualifiedName>,
    pub is_qualified: bool,
    pub limit: usize,
}

impl ConstantLookupRequest {
    pub fn new(partial_name: impl Into<String>, limit: usize) -> Self {
        let partial_name = partial_name.into();
        let is_qualified = partial_name.contains("::");
        let (namespace_prefix, partial_name) = if is_qualified {
            parse_qualified_constant_name(&partial_name)
        } else {
            (None, partial_name)
        };

        Self {
            partial_name,
            namespace_prefix,
            is_qualified,
            limit,
        }
    }
}

fn parse_qualified_constant_name(name: &str) -> (Option<FullyQualifiedName>, String) {
    let Some(last_scope) = name.rfind("::") else {
        return (None, name.to_string());
    };

    let namespace = &name[..last_scope];
    let partial = &name[last_scope + 2..];

    if namespace.is_empty() {
        return (None, partial.to_string());
    }

    match FullyQualifiedName::try_from(namespace) {
        Ok(fqn) => (Some(fqn), partial.to_string()),
        Err(_) => (None, name.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantMatch {
    pub fqn: FullyQualifiedName,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodMatch {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<RubyType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantHover {
    pub name: String,
    pub kind: ConstantHoverKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantHoverKind {
    Class,
    Module,
    Value(RubyType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MixinUsageKind {
    Include,
    Prepend,
    Extend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinUsage {
    pub kind: MixinUsageKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableTypeKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
}

#[cfg(test)]
mod tests {
    use super::ConstantLookupRequest;

    #[test]
    fn qualified_constant_request() {
        let request = ConstantLookupRequest::new("Foo::Bar", 50);

        assert!(request.is_qualified);
        assert_eq!(request.partial_name, "Bar");
        assert_eq!(
            request.namespace_prefix.map(|fqn| fqn.to_string()),
            Some("Foo".to_string())
        );
        assert_eq!(request.limit, 50);
    }

    #[test]
    fn unqualified_constant_request() {
        let request = ConstantLookupRequest::new("String", 50);

        assert!(!request.is_qualified);
        assert_eq!(request.partial_name, "String");
        assert!(request.namespace_prefix.is_none());
    }

    #[test]
    fn scope_resolution_request() {
        let request = ConstantLookupRequest::new("Foo::", 50);

        assert!(request.is_qualified);
        assert_eq!(request.partial_name, "");
        assert_eq!(
            request.namespace_prefix.map(|fqn| fqn.to_string()),
            Some("Foo".to_string())
        );
    }
}
