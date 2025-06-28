use std::fmt::Display;

use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, ruby_variable::RubyVariable,
};
use super::mixin_ref::MixinRef;

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class {
        superclass: Option<FullyQualifiedName>,
        includes: Vec<MixinRef>,
        prepends: Vec<MixinRef>,
        extends: Vec<MixinRef>,
    },
    Module {
        includes: Vec<MixinRef>,
        prepends: Vec<MixinRef>,
        extends: Vec<MixinRef>,
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
    pub fn add_includes(&mut self, fqns: Vec<MixinRef>) {
        match self {
            EntryKind::Class { includes, .. } => {
                includes.extend(fqns);
            }
            EntryKind::Module { includes, .. } => {
                includes.extend(fqns);
            }
            _ => {
                panic!("Cannot add includes to non-class/module entry");
            }
        }
    }

    pub fn add_extends(&mut self, fqns: Vec<MixinRef>) {
        match self {
            EntryKind::Class { extends, .. } => {
                extends.extend(fqns);
            }
            EntryKind::Module { extends, .. } => {
                extends.extend(fqns);
            }
            _ => {
                panic!("Cannot add extends to non-class/module entry");
            }
        }
    }

    pub fn add_prepends(&mut self, fqns: Vec<MixinRef>) {
        match self {
            EntryKind::Class { prepends, .. } => {
                prepends.extend(fqns);
            }
            EntryKind::Module { prepends, .. } => {
                prepends.extend(fqns);
            }
            _ => {
                panic!("Cannot add prepends to non-class/module entry");
            }
        }
    }
    pub fn new_class(superclass: Option<FullyQualifiedName>) -> Self {
        Self::Class {
            superclass,
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
        }
    }

    pub fn new_module() -> Self {
        Self::Module {
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
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
