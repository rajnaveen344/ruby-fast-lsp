use std::fmt::Display;

use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, ruby_variable::RubyVariable,
};

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class {
        superclass: Option<FullyQualifiedName>,
        is_singleton: bool,
    },
    Module,
    Method {
        name: RubyMethod,
        kind: MethodKind,
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

impl Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::Class { is_singleton, .. } => {
                write!(
                    f,
                    "Class{}",
                    if *is_singleton { " (Singleton)" } else { "" }
                )
            }
            EntryKind::Module { .. } => write!(f, "Module"),
            EntryKind::Method { name, kind, .. } => {
                write!(
                    f,
                    "Method{}: {}",
                    match kind {
                        MethodKind::Instance => " (Instance)",
                        MethodKind::Class => " (Class)",
                        MethodKind::Singleton => " (Singleton)",
                        MethodKind::ModuleFunc => " (ModuleFunc)",
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
