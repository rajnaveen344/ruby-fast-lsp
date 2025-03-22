use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
};

use super::{constant::Constant, method::Method};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FullyQualifiedName {
    namespace: Vec<Constant>,
    method: Option<Method>,
}

impl FullyQualifiedName {
    pub fn new(namespace: Vec<Constant>, method: Option<Method>) -> Self {
        FullyQualifiedName { namespace, method }
    }

    pub fn push(&mut self, constant: Constant) {
        self.namespace.push(constant);
    }

    pub fn pop(&mut self) {
        self.namespace.pop();
    }
}

impl Display for FullyQualifiedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.namespace.iter().map(|c| c.to_string()).collect();
        let namespace = parts.join("::");
        let method = self.method.as_ref().map(|m| m.to_string());

        match method {
            Some(method) => write!(f, "{namespace}#{method}"),
            None => write!(f, "{namespace}"),
        }
    }
}

impl Hash for FullyQualifiedName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_string().hash(state);
    }
}
