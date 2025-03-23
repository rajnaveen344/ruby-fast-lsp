use std::fmt::Display;

use crate::indexer::types::fully_qualified_name::FullyQualifiedName;

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class {
        superclass: Option<FullyQualifiedName>,
        is_singleton: bool,
    },
    Module,
    Method {
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
            EntryKind::Method { kind, .. } => {
                write!(
                    f,
                    "Method{}",
                    match kind {
                        MethodKind::Instance => " (Instance)",
                        MethodKind::Class => " (Class)",
                        MethodKind::Singleton => " (Singleton)",
                        MethodKind::ModuleFunc => " (ModuleFunc)",
                    }
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
        }
    }
}
