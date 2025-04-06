use std::fmt::{self, Display, Formatter};

use super::{ruby_constant::RubyConstant, ruby_method::RubyMethod, ruby_namespace::RubyNamespace};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum FullyQualifiedName {
    /// Represents a class or module (namespace).
    /// Example: `Foo::Bar` → `Namespace(vec!["Foo", "Bar"])`
    Namespace(Vec<RubyNamespace>),

    /// Instance method, e.g., `Foo#bar` → `InstanceMethod(vec!["Foo"], RubyMethod::new("bar"))`
    InstanceMethod(Vec<RubyNamespace>, RubyMethod),

    /// Class/singleton method, e.g., `Foo.bar` → `ClassMethod(vec!["Foo"], RubyMethod::new("bar"))`
    ClassMethod(Vec<RubyNamespace>, RubyMethod),

    /// Module function, e.g., `Foo::bar` → `ModuleMethod(vec!["Foo"], RubyMethod::new("bar"))`
    /// These are methods created with `module_function` and have a dual nature:
    /// - Public class method on the module
    /// - Private instance method when included in other classes
    ModuleMethod(Vec<RubyNamespace>, RubyMethod),

    /// Constant, e.g., `Foo::CONST` → `Constant(vec!["Foo"], RubyConstant::new("CONST"))`
    Constant(Vec<RubyNamespace>, RubyConstant),
}

impl FullyQualifiedName {
    // Constructor helpers
    pub fn namespace(namespace: Vec<RubyNamespace>) -> Self {
        FullyQualifiedName::Namespace(namespace)
    }

    pub fn instance_method(namespace: Vec<RubyNamespace>, method: RubyMethod) -> Self {
        FullyQualifiedName::InstanceMethod(namespace, method)
    }

    pub fn class_method(namespace: Vec<RubyNamespace>, method: RubyMethod) -> Self {
        FullyQualifiedName::ClassMethod(namespace, method)
    }

    pub fn constant(namespace: Vec<RubyNamespace>, constant: RubyConstant) -> Self {
        FullyQualifiedName::Constant(namespace, constant)
    }

    // Common accessor for namespace parts
    pub fn namespace_parts(&self) -> &[RubyNamespace] {
        match self {
            FullyQualifiedName::Namespace(ns) => ns,
            FullyQualifiedName::InstanceMethod(ns, _) => ns,
            FullyQualifiedName::ClassMethod(ns, _) => ns,
            FullyQualifiedName::ModuleMethod(ns, _) => ns,
            FullyQualifiedName::Constant(ns, _) => ns,
        }
    }

    // Constructor helper for module methods
    pub fn module_method(namespace: Vec<RubyNamespace>, method: RubyMethod) -> Self {
        FullyQualifiedName::ModuleMethod(namespace, method)
    }
}

impl From<Vec<RubyNamespace>> for FullyQualifiedName {
    fn from(value: Vec<RubyNamespace>) -> Self {
        FullyQualifiedName::namespace(value)
    }
}

impl Display for FullyQualifiedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let namespace = self
            .namespace_parts()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");

        match self {
            FullyQualifiedName::Namespace(_) => write!(f, "{namespace}"),
            FullyQualifiedName::InstanceMethod(_, method) => write!(f, "{namespace}#{method}"),
            FullyQualifiedName::ClassMethod(_, method) => write!(f, "{namespace}.{method}"),
            FullyQualifiedName::ModuleMethod(_, method) => write!(f, "{namespace}::{method}"),
            FullyQualifiedName::Constant(_, constant) => write!(f, "{namespace}::{constant}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fully_qualified_name() {
        let fqn = FullyQualifiedName::namespace(vec![
            RubyNamespace::new("Foo").unwrap(),
            RubyNamespace::new("Bar").unwrap(),
        ]);
        assert_eq!(fqn.to_string(), "Foo::Bar");
    }

    #[test]
    fn test_instance_method() {
        let fqn = FullyQualifiedName::instance_method(
            vec![
                RubyNamespace::new("Foo").unwrap(),
                RubyNamespace::new("Bar").unwrap(),
            ],
            RubyMethod::new("baz").unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar#baz");
    }

    #[test]
    fn test_class_method() {
        let fqn = FullyQualifiedName::class_method(
            vec![
                RubyNamespace::new("Foo").unwrap(),
                RubyNamespace::new("Bar").unwrap(),
            ],
            RubyMethod::new("baz").unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar.baz");
    }

    #[test]
    fn test_constant() {
        let fqn = FullyQualifiedName::constant(
            vec![
                RubyNamespace::new("Foo").unwrap(),
                RubyNamespace::new("Bar").unwrap(),
            ],
            RubyConstant::new("BAZ").unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar::BAZ");
    }

    // invalid
    #[test]
    fn test_invalid_namespace() {
        let fqn = FullyQualifiedName::namespace(vec![]);
        assert_eq!(fqn.to_string(), "");
    }

    #[test]
    fn test_namespace_parts() {
        let fqn = FullyQualifiedName::namespace(vec![
            RubyNamespace::new("Foo").unwrap(),
            RubyNamespace::new("Bar").unwrap(),
        ]);

        assert_eq!(fqn.to_string(), "Foo::Bar");
    }

    #[test]
    fn test_module_method() {
        let fqn = FullyQualifiedName::module_method(
            vec![
                RubyNamespace::new("Foo").unwrap(),
                RubyNamespace::new("Bar").unwrap(),
            ],
            RubyMethod::new("baz").unwrap(),
        );
        assert_eq!(fqn.to_string(), "Foo::Bar::baz");
    }
}
