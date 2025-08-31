use std::fmt::Display;

use crate::indexer::entry::MixinRef;
use crate::type_inference::ruby_type::RubyType;
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, ruby_variable::RubyVariable,
};

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class {
        superclass: Option<MixinRef>,
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
        r#type: RubyType,
    },
}

impl EntryKind {
    pub fn add_includes(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { includes, .. } => {
                includes.extend(mixin_refs);
            }
            EntryKind::Module { includes, .. } => {
                includes.extend(mixin_refs);
            }
            _ => {
                panic!("Cannot add includes to non-class/module entry");
            }
        }
    }

    pub fn add_extends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { extends, .. } => {
                extends.extend(mixin_refs);
            }
            EntryKind::Module { extends, .. } => {
                extends.extend(mixin_refs);
            }
            _ => {
                panic!("Cannot add extends to non-class/module entry");
            }
        }
    }

    pub fn add_prepends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { prepends, .. } => {
                prepends.extend(mixin_refs);
            }
            EntryKind::Module { prepends, .. } => {
                prepends.extend(mixin_refs);
            }
            _ => {
                panic!("Cannot add prepends to non-class/module entry");
            }
        }
    }

    pub fn set_superclass(&mut self, superclass_ref: MixinRef) {
        match self {
            EntryKind::Class { superclass, .. } => {
                *superclass = Some(superclass_ref);
            }
            _ => {
                panic!("Cannot set superclass on non-class entry");
            }
        }
    }
    pub fn new_class(superclass: Option<MixinRef>) -> Self {
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

    pub fn new_variable(name: RubyVariable, r#type: RubyType) -> Self {
        EntryKind::Variable { name, r#type }
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
                        MethodKind::Unknown => " (Unknown)",
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
            EntryKind::Variable { name, r#type, .. } => {
                write!(f, "Variable: {} ({})", name, r#type)
            }
        }
    }
}
