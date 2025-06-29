use std::{
    fmt,
    hash::{Hash, Hasher},
};

use lsp_types::Location;

pub type LVScopeStack = Vec<LVScope>;
pub type LVScopeId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LVScope {
    id: LVScopeId,
    location: Location,
    kind: LVScopeKind,
}

impl LVScope {
    pub fn new(id: LVScopeId, location: Location, kind: LVScopeKind) -> Self {
        Self { id, location, kind }
    }

    pub fn scope_id(&self) -> LVScopeId {
        self.id
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn kind(&self) -> &LVScopeKind {
        &self.kind
    }
}

impl Hash for LVScope {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.location.uri.hash(state);
        self.location.range.start.line.hash(state);
        self.location.range.start.character.hash(state);
        self.location.range.end.line.hash(state);
        self.location.range.end.character.hash(state);
        self.kind.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LVScopeKind {
    /// Scope for local variables defined in class/module bodies.
    ///
    /// # Examples
    /// ```ruby
    /// class Example
    ///   x = 1  # Class body local
    ///
    ///   class Nested
    ///     puts x  # NameError: undefined local variable or method `x'
    ///   end
    ///
    ///   def instance_method
    ///     puts x  # NameError: undefined local variable or method `x'
    ///   end
    ///   
    ///   class Nested
    ///     puts x  # NameError: undefined local variable or method `x'
    ///   end
    /// end
    /// ```
    Constant,

    /// Scope for local variables in method definitions.
    ///
    /// # Examples
    /// ```ruby
    /// def example
    ///   x = 1  # Method local
    ///   if true
    ///     y = 2  # Also method local
    ///   end
    ///   puts y  # => 2 (accessible in entire method)
    /// end
    /// ```
    Method,

    /// Scope for blocks, procs, and lambdas.
    ///
    /// # Examples
    /// ```ruby
    /// x = 1
    /// [1].each do |y|  # y is block-local
    ///   z = 2          # Also block-local
    ///   x = 3          # Can modify outer x
    /// end
    /// puts x  # => 3
    /// puts z  # NameError: undefined local variable or method `z'
    /// ```
    Block,

    /// Special scope for exception variables in rescue clauses.
    ///
    /// # Examples
    /// ```ruby
    /// begin
    ///   raise "error"
    /// rescue => e  # e is in Rescue scope
    ///   puts e.message  # "error"
    /// end
    /// puts e  # NameError: undefined local variable or method `e'
    /// ```
    Rescue,

    /// Scope for explicitly declared block-local variables.
    ///
    /// # Examples
    /// ```ruby
    /// x = 1
    /// [1].each do |y; x|  # x is explicitly block-local
    ///   x = 2  # Doesn't affect outer x
    ///   y = 3
    /// end
    /// puts x  # => 1 (unchanged)
    /// ```
    ExplicitBlockLocal,
}

impl LVScopeKind {
    pub fn is_hard_scope_boundary(&self) -> bool {
        matches!(self, LVScopeKind::Method | LVScopeKind::Constant)
    }
}

impl fmt::Display for LVScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LVScopeKind::Constant => write!(f, "Constant"),
            LVScopeKind::Method => write!(f, "Method"),
            LVScopeKind::Block => write!(f, "Block"),
            LVScopeKind::Rescue => write!(f, "Rescue"),
            LVScopeKind::ExplicitBlockLocal => write!(f, "ExplicitBlockLocal"),
        }
    }
}
