//! Entry Kind
//!
//! Defines the different kinds of Ruby entities that can be indexed,
//! along with their type-specific metadata.

use std::fmt::Display;

use tower_lsp::lsp_types::Position;

use crate::indexer::entry::MixinRef;
use crate::type_inference::ruby_type::RubyType;
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, scope::LVScopeStack,
};
use crate::yard::YardMethodDoc;

use super::{ConstVisibility, MethodKind, MethodOrigin, MethodVisibility};

// ============================================================================
// Method Parameter Info
// ============================================================================

/// The kind of method parameter for proper inlay hint formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Regular positional parameter: `def foo(name)`
    Required,
    /// Optional parameter with default: `def foo(name = "default")`
    Optional,
    /// Rest/splat parameter: `def foo(*args)`
    Rest,
    /// Keyword parameter: `def foo(name:)` or `def foo(name: "default")`
    /// Already has a colon, so inlay hint should NOT add another
    Keyword,
    /// Keyword rest/double-splat: `def foo(**kwargs)`
    KeywordRest,
    /// Block parameter: `def foo(&block)`
    Block,
}

/// Information about a method parameter including its position for inlay hints
#[derive(Debug, Clone, PartialEq)]
pub struct MethodParamInfo {
    /// Parameter name
    pub name: String,
    /// Position at the end of the parameter name (for inlay hints)
    pub end_position: Position,
    /// The kind of parameter (affects hint formatting)
    pub kind: ParamKind,
}

impl MethodParamInfo {
    pub fn new(name: String, end_position: Position, kind: ParamKind) -> Self {
        Self {
            name,
            end_position,
            kind,
        }
    }

    /// Returns true if this parameter already has a colon (keyword params)
    pub fn has_colon(&self) -> bool {
        self.kind == ParamKind::Keyword
    }
}

// ============================================================================
// EntryKind
// ============================================================================

/// Type-specific metadata for indexed Ruby entities
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
        /// Parameter info with names and positions for inlay hints
        params: Vec<MethodParamInfo>,
        owner: FullyQualifiedName,
        visibility: MethodVisibility,
        origin: MethodOrigin,
        origin_visibility: Option<MethodVisibility>,
        /// YARD documentation with type annotations
        yard_doc: Option<YardMethodDoc>,
        /// Position for return type hint (after closing paren or last param)
        return_type_position: Option<Position>,
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
    // ========================================================================
    // Constructors
    // ========================================================================

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

    // ========================================================================
    // Mixin Mutations
    // ========================================================================

    pub fn add_includes(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { includes, .. } | EntryKind::Module { includes, .. } => {
                includes.extend(mixin_refs);
            }
            _ => panic!("Cannot add includes to non-class/module entry"),
        }
    }

    pub fn add_extends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { extends, .. } | EntryKind::Module { extends, .. } => {
                extends.extend(mixin_refs);
            }
            _ => panic!("Cannot add extends to non-class/module entry"),
        }
    }

    pub fn add_prepends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class { prepends, .. } | EntryKind::Module { prepends, .. } => {
                prepends.extend(mixin_refs);
            }
            _ => panic!("Cannot add prepends to non-class/module entry"),
        }
    }

    pub fn set_superclass(&mut self, superclass_ref: MixinRef) {
        match self {
            EntryKind::Class { superclass, .. } => {
                *superclass = Some(superclass_ref);
            }
            _ => panic!("Cannot set superclass on non-class entry"),
        }
    }
}

// ============================================================================
// Display
// ============================================================================

impl Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::Class { .. } => write!(f, "Class"),
            EntryKind::Module { .. } => write!(f, "Module"),
            EntryKind::Method { name, .. } => {
                let kind_str = match name.get_kind() {
                    MethodKind::Instance => " (Instance)",
                    MethodKind::Class => " (Class)",
                    MethodKind::Unknown => " (Unknown)",
                };
                write!(f, "Method{}: {}", kind_str, name)
            }
            EntryKind::Constant { visibility, .. } => {
                let vis_str = if visibility.is_some() {
                    " (Private)"
                } else {
                    ""
                };
                write!(f, "Constant{}", vis_str)
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
