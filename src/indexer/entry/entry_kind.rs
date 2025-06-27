use std::fmt::Display;

use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, ruby_variable::RubyVariable,
};

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class {
        superclass: Option<FullyQualifiedName>,
        includes: Vec<FullyQualifiedName>,
        prepends: Vec<FullyQualifiedName>,
        extends: Vec<FullyQualifiedName>,
    },
    Module {
        includes: Vec<FullyQualifiedName>,
        prepends: Vec<FullyQualifiedName>,
        extends: Vec<FullyQualifiedName>,
    },
    Method {
        name: RubyMethod,
        parameters: Vec<String>,
        owner: FullyQualifiedName,
        visibility: MethodVisibility,
        origin: MethodOrigin,
        origin_visibility: Option<MethodVisibility>,
    },
    Constant {
        value: Option<String>,
        visibility: Option<ConstVisibility>,
    },
    Variable {
        name: RubyVariable,
    },
}

impl EntryKind {
    pub fn new_class(
        superclass: Option<FullyQualifiedName>,
        includes: Vec<FullyQualifiedName>,
        prepends: Vec<FullyQualifiedName>,
        extends: Vec<FullyQualifiedName>,
    ) -> Self {
        Self::Class {
            superclass,
            includes,
            prepends,
            extends,
        }
    }

    pub fn new_module(
        includes: Vec<FullyQualifiedName>,
        prepends: Vec<FullyQualifiedName>,
        extends: Vec<FullyQualifiedName>,
    ) -> Self {
        Self::Module {
            includes,
            prepends,
            extends,
        }
    }
}

impl Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::Class { .. } => write!(f, "Class"),
            EntryKind::Module { .. } => write!(f, "Module"),
            EntryKind::Method { name, .. } => {
                write!(
                    f,
                    "Method{}: {}",
                    match name.get_kind() {
                        MethodKind::Instance => " (Instance)",
                        MethodKind::Class => " (Class)",
                    },
                    name
                )
            }
            EntryKind::Constant { visibility, .. } => {
                write!(
                    f,
                    "Constant{}",
                    if visibility.is_some() {
                        " (Private)"
                    } else {
                        ""
                    }
                )
            }
            EntryKind::Variable { name, .. } => write!(f, "Variable: {}", name),
        }
    }
}
