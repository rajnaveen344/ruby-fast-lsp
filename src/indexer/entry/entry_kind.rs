//! Entry Kind
//!
//! Defines the different kinds of Ruby entities that can be indexed,
//! along with their type-specific metadata.

use std::fmt::Display;

use tower_lsp::lsp_types::Position;

use crate::indexer::entry::MixinRef;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::{
    fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod, scope::LVScopeId,
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

/// Data for Class entries
#[derive(Debug, Clone, PartialEq)]
pub struct ClassData {
    pub superclass: Option<MixinRef>,
    pub includes: Vec<MixinRef>,
    pub prepends: Vec<MixinRef>,
    pub extends: Vec<MixinRef>,
}

/// Data for Module entries
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleData {
    pub includes: Vec<MixinRef>,
    pub prepends: Vec<MixinRef>,
    pub extends: Vec<MixinRef>,
}

/// Data for Method entries
#[derive(Debug, Clone, PartialEq)]
pub struct MethodData {
    pub name: RubyMethod,
    /// Parameter info with names and positions for inlay hints
    pub params: Vec<MethodParamInfo>,
    pub owner: FullyQualifiedName,
    pub visibility: MethodVisibility,
    pub origin: MethodOrigin,
    pub origin_visibility: Option<MethodVisibility>,
    /// YARD documentation with type annotations (raw strings for display)
    pub yard_doc: Option<YardMethodDoc>,
    /// Position for return type hint (after closing paren or last param)
    pub return_type_position: Option<Position>,
    /// Inferred or documented return type as RubyType (for type inference)
    pub return_type: Option<RubyType>,
    /// Parameter types as RubyType (for type inference within method body)
    /// Maps parameter name to its type
    pub param_types: Vec<(String, RubyType)>,
}

/// Data for Constant entries
#[derive(Debug, Clone, PartialEq)]
pub struct ConstantData {
    pub value: Option<String>,
    pub visibility: Option<ConstVisibility>,
}

/// Assignment or narrowing for a local variable, valid for a specific range
#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariableAssignment {
    /// The range where this type assignment is valid
    pub range: tower_lsp::lsp_types::Range,
    /// The type assigned or narrowed to
    pub r#type: RubyType,
}

/// Data for LocalVariable entries
#[derive(Debug, Clone, PartialEq)]
pub struct LocalVariableData {
    pub name: String,
    pub scope_id: LVScopeId,
    /// Ordered list of assignments/narrowings
    pub assignments: Vec<LocalVariableAssignment>,
}

/// Data for InstanceVariable entries
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceVariableData {
    pub name: String,
    pub r#type: RubyType,
}

/// Data for ClassVariable entries
#[derive(Debug, Clone, PartialEq)]
pub struct ClassVariableData {
    pub name: String,
    pub r#type: RubyType,
}

/// Data for GlobalVariable entries
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVariableData {
    pub name: String,
    pub r#type: RubyType,
}

/// Type-specific metadata for indexed Ruby entities
/// Values are boxed to prevent enum size from growing with large data just on single variant
#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Class(Box<ClassData>),
    Module(Box<ModuleData>),
    Method(Box<MethodData>),
    Constant(Box<ConstantData>),
    LocalVariable(Box<LocalVariableData>),
    InstanceVariable(Box<InstanceVariableData>),
    ClassVariable(Box<ClassVariableData>),
    GlobalVariable(Box<GlobalVariableData>),
    Reference,
}

impl EntryKind {
    // ========================================================================
    // Constructors
    // ========================================================================

    pub fn new_class(superclass: Option<MixinRef>) -> Self {
        Self::Class(Box::new(ClassData {
            superclass,
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
        }))
    }

    pub fn new_constant(value: Option<String>, visibility: Option<ConstVisibility>) -> Self {
        Self::Constant(Box::new(ConstantData { value, visibility }))
    }

    pub fn new_reference() -> Self {
        Self::Reference
    }

    pub fn new_module() -> Self {
        Self::Module(Box::new(ModuleData {
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
        }))
    }

    pub fn new_method(
        name: RubyMethod,
        params: Vec<MethodParamInfo>,
        owner: FullyQualifiedName,
        visibility: MethodVisibility,
        origin: MethodOrigin,
        origin_visibility: Option<MethodVisibility>,
        yard_doc: Option<YardMethodDoc>,
        return_type_position: Option<Position>,
        return_type: Option<RubyType>,
        param_types: Vec<(String, RubyType)>,
    ) -> Self {
        Self::Method(Box::new(MethodData {
            name,
            params,
            owner,
            visibility,
            origin,
            origin_visibility,
            yard_doc,
            return_type_position,
            return_type,
            param_types,
        }))
    }

    pub fn new_local_variable(
        name: String,
        scope_id: LVScopeId,
        r#type: RubyType,
        assignment_range: tower_lsp::lsp_types::Range,
    ) -> Self {
        let assignment = LocalVariableAssignment {
            range: assignment_range,
            r#type,
        };

        EntryKind::LocalVariable(Box::new(LocalVariableData {
            name,
            scope_id,
            assignments: vec![assignment],
        }))
    }

    pub fn new_instance_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::InstanceVariable(Box::new(InstanceVariableData { name, r#type }))
    }

    pub fn new_class_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::ClassVariable(Box::new(ClassVariableData { name, r#type }))
    }

    pub fn new_global_variable(name: String, r#type: RubyType) -> Self {
        EntryKind::GlobalVariable(Box::new(GlobalVariableData { name, r#type }))
    }

    // ========================================================================
    // Mixin Mutations
    // ========================================================================

    pub fn add_includes(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class(data) => data.includes.extend(mixin_refs),
            EntryKind::Module(data) => data.includes.extend(mixin_refs),
            _ => panic!("Cannot add includes to non-class/module entry"),
        }
    }

    pub fn add_extends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class(data) => data.extends.extend(mixin_refs),
            EntryKind::Module(data) => data.extends.extend(mixin_refs),
            _ => panic!("Cannot add extends to non-class/module entry"),
        }
    }

    pub fn add_prepends(&mut self, mixin_refs: Vec<MixinRef>) {
        match self {
            EntryKind::Class(data) => data.prepends.extend(mixin_refs),
            EntryKind::Module(data) => data.prepends.extend(mixin_refs),
            _ => panic!("Cannot add prepends to non-class/module entry"),
        }
    }

    pub fn set_superclass(&mut self, superclass_ref: MixinRef) {
        match self {
            EntryKind::Class(data) => {
                data.superclass = Some(superclass_ref);
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
            EntryKind::Class(_) => write!(f, "Class"),
            EntryKind::Module(_) => write!(f, "Module"),
            EntryKind::Method(data) => {
                let kind_str = match data.name.get_kind() {
                    MethodKind::Instance => " (Instance)",
                    MethodKind::Class => " (Class)",
                };
                write!(f, "Method{}: {}", kind_str, data.name)
            }
            EntryKind::Constant(data) => {
                let vis_str = if data.visibility.is_some() {
                    " (Private)"
                } else {
                    ""
                };
                write!(f, "Constant{}", vis_str)
            }
            EntryKind::LocalVariable(data) => {
                let count = data.assignments.len();
                let type_info = if count == 1 {
                    format!("{}", data.assignments[0].r#type)
                } else {
                    format!("{} assignments", count)
                };
                write!(f, "Local Variable: {} ({})", data.name, type_info)
            }
            EntryKind::InstanceVariable(data) => {
                write!(f, "Instance Variable: {} ({})", data.name, data.r#type)
            }
            EntryKind::ClassVariable(data) => {
                write!(f, "Class Variable: {} ({})", data.name, data.r#type)
            }
            EntryKind::GlobalVariable(data) => {
                write!(f, "Global Variable: {} ({})", data.name, data.r#type)
            }
            EntryKind::Reference => {
                write!(f, "Reference")
            }
        }
    }
}
