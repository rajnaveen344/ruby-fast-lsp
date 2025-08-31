use std::fmt::Display;

use crate::indexer::entry::MixinRef;
use crate::type_inference::ruby_type::RubyType;
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, scope::LVScopeStack,
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
    LocalVariable {
        name: String,
        scope_stack: LVScopeStack,
        r#type: RubyType,
    },
    InstanceVariable {
        name: String,
        r#type: RubyType,
    },
    ClassVariable {
        name: String,
        r#type: RubyType,
    },
    GlobalVariable {
        name: String,
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

    pub fn new_local_variable(name: String, scope_stack: LVScopeStack, r#type: RubyType) -> Self {
        EntryKind::LocalVariable {
            name,
            scope_stack,
            r#type,
        }
    }

    pub fn new_instance_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::InstanceVariable { name, r#type }
    }

    pub fn new_class_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::ClassVariable { name, r#type }
    }

    pub fn new_global_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::GlobalVariable { name, r#type }
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
            EntryKind::LocalVariable { name, r#type, .. } => {
                write!(f, "Local Variable: {} ({})", name, r#type)
            }
            EntryKind::InstanceVariable { name, r#type, .. } => {
                write!(f, "Instance Variable: {} ({})", name, r#type)
            }
            EntryKind::ClassVariable { name, r#type, .. } => {
                write!(f, "Class Variable: {} ({})", name, r#type)
            }
            EntryKind::GlobalVariable { name, r#type, .. } => {
                write!(f, "Global Variable: {} ({})", name, r#type)
            }
        }
    }
}
