//! Type definitions for representing RBS AST nodes.
//!
//! These types represent the parsed structure of RBS files and can be used
//! for type checking, code completion, and other IDE features.

use std::fmt;

/// A parse error from the RBS parser
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub location: Option<Location>,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: None,
        }
    }

    pub fn with_location(message: impl Into<String>, location: Location) -> Self {
        Self {
            message: message.into(),
            location: Some(location),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = &self.location {
            write!(f, "{}:{}: {}", loc.start_row, loc.start_col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

/// Source location information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

impl Location {
    pub fn new(start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> Self {
        Self {
            start_row,
            start_col,
            end_row,
            end_col,
        }
    }
}

/// Top-level declaration in an RBS file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    Class(ClassDecl),
    Module(ModuleDecl),
    Interface(InterfaceDecl),
    TypeAlias(TypeAliasDecl),
    Constant(ConstantDecl),
    Global(GlobalDecl),
}

/// A class declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub superclass: Option<RbsType>,
    pub members: Vec<Member>,
    pub methods: Vec<MethodDecl>,
    pub location: Option<Location>,
}

impl ClassDecl {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_params: Vec::new(),
            superclass: None,
            members: Vec::new(),
            methods: Vec::new(),
            location: None,
        }
    }
}

/// A module declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub self_types: Vec<RbsType>,
    pub members: Vec<Member>,
    pub methods: Vec<MethodDecl>,
    pub location: Option<Location>,
}

impl ModuleDecl {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_params: Vec::new(),
            self_types: Vec::new(),
            members: Vec::new(),
            methods: Vec::new(),
            location: None,
        }
    }
}

/// An interface declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<MethodDecl>,
    pub location: Option<Location>,
}

/// A type alias declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub r#type: RbsType,
    pub location: Option<Location>,
}

/// A constant declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantDecl {
    pub name: String,
    pub r#type: RbsType,
    pub location: Option<Location>,
}

/// A global variable declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDecl {
    pub name: String,
    pub r#type: RbsType,
    pub location: Option<Location>,
}

/// A type parameter (e.g., `T` in `Array[T]`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: String,
    pub variance: Variance,
    pub bound: Option<RbsType>,
}

impl TypeParam {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variance: Variance::Invariant,
            bound: None,
        }
    }
}

/// Type parameter variance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variance {
    #[default]
    Invariant,
    Covariant,     // out
    Contravariant, // in
}

/// Members of a class or module
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    Include(RbsType),
    Extend(RbsType),
    Prepend(RbsType),
    Attr(AttrDecl),
    Alias(AliasDecl),
    Public,
    Private,
}

/// An attribute declaration (attr_reader, attr_writer, attr_accessor)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrDecl {
    pub name: String,
    pub kind: AttrKind,
    pub r#type: RbsType,
    pub is_singleton: bool,
    pub location: Option<Location>,
}

/// Attribute kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrKind {
    Reader,
    Writer,
    Accessor,
}

/// An alias declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasDecl {
    pub new_name: String,
    pub old_name: String,
    pub is_singleton: bool,
    pub location: Option<Location>,
}

/// A method declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDecl {
    pub name: String,
    pub kind: MethodKind,
    pub overloads: Vec<MethodType>,
    pub visibility: Visibility,
    pub location: Option<Location>,
}

impl MethodDecl {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: MethodKind::Instance,
            overloads: Vec::new(),
            visibility: Visibility::Public,
            location: None,
        }
    }

    /// Get the first (primary) return type
    pub fn return_type(&self) -> Option<&RbsType> {
        self.overloads.first().map(|o| &o.return_type)
    }
}

/// Method kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MethodKind {
    #[default]
    Instance,
    Singleton, // self.method
}

/// Method visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
}

/// A method type signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodType {
    pub type_params: Vec<TypeParam>,
    pub params: Vec<MethodParam>,
    pub return_type: RbsType,
    pub block: Option<Block>,
}

impl MethodType {
    pub fn new(return_type: RbsType) -> Self {
        Self {
            type_params: Vec::new(),
            params: Vec::new(),
            return_type,
            block: None,
        }
    }
}

/// A method parameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParam {
    pub name: Option<String>,
    pub r#type: RbsType,
    pub kind: ParamKind,
}

impl MethodParam {
    pub fn required(r#type: RbsType) -> Self {
        Self {
            name: None,
            r#type,
            kind: ParamKind::Required,
        }
    }

    pub fn optional(r#type: RbsType) -> Self {
        Self {
            name: None,
            r#type,
            kind: ParamKind::Optional,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Parameter kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    Required,
    Optional,
    Rest,        // *args
    Keyword,     // key: Type
    KeywordOpt,  // ?key: Type
    KeywordRest, // **kwargs
    Block,       // &block
}

/// A block type
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub params: Vec<MethodParam>,
    pub return_type: RbsType,
    pub required: bool,
}

/// RBS type representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RbsType {
    /// A simple class type like `String` or `Integer`
    Class(String),

    /// A namespaced class like `::String` or `Foo::Bar`
    ClassInstance { name: String, args: Vec<RbsType> },

    /// An interface type like `_ToS`
    Interface(String),

    /// A type variable like `T` or `Elem`
    TypeVar(String),

    /// A union type like `String | Integer`
    Union(Vec<RbsType>),

    /// An intersection type like `_ToS & _ToI`
    Intersection(Vec<RbsType>),

    /// An optional type like `String?`
    Optional(Box<RbsType>),

    /// A tuple type like `[String, Integer]`
    Tuple(Vec<RbsType>),

    /// A record type like `{ name: String, age: Integer }`
    Record(Vec<(String, RbsType)>),

    /// A proc type
    Proc(Box<MethodType>),

    /// A literal type
    Literal(Literal),

    /// The `self` type
    SelfType,

    /// The `instance` type
    Instance,

    /// The `class` type
    ClassType,

    /// The `void` type
    Void,

    /// The `nil` type
    Nil,

    /// The `bool` type
    Bool,

    /// The `untyped` type
    Untyped,

    /// The `top` type
    Top,

    /// The `bot` type (bottom)
    Bot,
}

impl RbsType {
    /// Create a simple class type
    pub fn class(name: impl Into<String>) -> Self {
        RbsType::Class(name.into())
    }

    /// Create a generic class instance
    pub fn generic(name: impl Into<String>, args: Vec<RbsType>) -> Self {
        RbsType::ClassInstance {
            name: name.into(),
            args,
        }
    }

    /// Create a union type
    pub fn union(types: Vec<RbsType>) -> Self {
        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            RbsType::Union(types)
        }
    }

    /// Create an optional type
    pub fn optional(inner: RbsType) -> Self {
        RbsType::Optional(Box::new(inner))
    }
}

impl fmt::Display for RbsType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RbsType::Class(name) => write!(f, "{}", name),
            RbsType::ClassInstance { name, args } => {
                write!(f, "{}", name)?;
                if !args.is_empty() {
                    write!(f, "[")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", arg)?;
                    }
                    write!(f, "]")?;
                }
                Ok(())
            }
            RbsType::Interface(name) => write!(f, "{}", name),
            RbsType::TypeVar(name) => write!(f, "{}", name),
            RbsType::Union(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            RbsType::Intersection(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " & ")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            RbsType::Optional(inner) => write!(f, "{}?", inner),
            RbsType::Tuple(types) => {
                write!(f, "[")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, "]")
            }
            RbsType::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, t)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", name, t)?;
                }
                write!(f, " }}")
            }
            RbsType::Proc(method_type) => {
                write!(f, "^(")?;
                for (i, p) in method_type.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p.r#type)?;
                }
                write!(f, ") -> {}", method_type.return_type)
            }
            RbsType::Literal(lit) => write!(f, "{}", lit),
            RbsType::SelfType => write!(f, "self"),
            RbsType::Instance => write!(f, "instance"),
            RbsType::ClassType => write!(f, "class"),
            RbsType::Void => write!(f, "void"),
            RbsType::Nil => write!(f, "nil"),
            RbsType::Bool => write!(f, "bool"),
            RbsType::Untyped => write!(f, "untyped"),
            RbsType::Top => write!(f, "top"),
            RbsType::Bot => write!(f, "bot"),
        }
    }
}

/// A literal type value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
    Integer(i64),
    Symbol(String),
    True,
    False,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "\"{}\"", s),
            Literal::Integer(n) => write!(f, "{}", n),
            Literal::Symbol(s) => write!(f, ":{}", s),
            Literal::True => write!(f, "true"),
            Literal::False => write!(f, "false"),
        }
    }
}
