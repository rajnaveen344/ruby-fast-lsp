//! Scope Tree - Tree structure for local variable scope tracking.
//!
//! This module provides a tree-based representation of scopes in a Ruby document,
//! enabling proper handling of variable capture in blocks and proper rename refactoring.
//!
//! # Key Concepts
//!
//! - **ScopeNode**: Represents a single scope (method, block, class body, etc.)
//! - **VariableNode**: Represents a local variable with its full def-use chain
//! - **Capture**: References to variables from outer scopes (in blocks)
//!
//! # Scope Hierarchy
//!
//! - **Hard boundaries** (Constant, InstanceMethod, ClassMethod): Inner scopes CANNOT access outer variables
//! - **Soft boundaries** (Block): Inner scopes CAN capture outer variables

use tower_lsp::lsp_types::{Location, Range};

use crate::types::scope::LVScopeId;
pub use crate::types::scope::LVScopeKind;

/// A tree structure representing the nesting of local variable scopes.
/// Each node represents a scope with its variables and relationships.
#[derive(Clone)]
pub struct ScopeTree {
    /// All scope nodes indexed by LVScopeId
    scopes: Vec<ScopeNode>,
    /// Root scope id
    root: LVScopeId,
    /// Current active scope during building
    current: Option<LVScopeId>,
}

impl ScopeTree {
    /// Create a new empty scope tree
    pub fn new() -> Self {
        let mut scopes = Vec::new();

        // Create root scope (file-level Constant scope)
        let root = ScopeNode {
            id: 0,
            parent: None,
            children: Vec::new(),
            kind: LVScopeKind::Constant,
            range: Range::default(),
            local_variables: Vec::new(),
            captured_variables: Vec::new(),
            name: None,
        };
        scopes.push(root);

        Self {
            scopes,
            root: 0,
            current: Some(0),
        }
    }

    /// Get the root scope id
    pub fn root(&self) -> LVScopeId {
        self.root
    }

    /// Get the current scope id
    pub fn current_scope(&self) -> Option<LVScopeId> {
        self.current
    }

    /// Enter a new scope (called when entering method, block, etc.)
    pub fn enter_scope(
        &mut self,
        kind: LVScopeKind,
        range: Range,
        name: Option<String>,
    ) -> LVScopeId {
        let parent = self.current;
        let id = self.scopes.len();

        let node = ScopeNode {
            id,
            parent,
            children: Vec::new(),
            kind,
            range,
            local_variables: Vec::new(),
            captured_variables: Vec::new(),
            name,
        };

        self.scopes.push(node);

        // Register as child of parent
        if let Some(p) = parent {
            self.scopes[p].children.push(id);
        }

        self.current = Some(id);
        id
    }

    /// Exit the current scope (called when exiting method, block, etc.)
    pub fn exit_scope(&mut self) {
        if let Some(current) = self.current {
            if let Some(node) = self.scopes.get(current) {
                self.current = node.parent;
            }
        }
    }

    /// Check if current scope can access variables from parent (not a hard boundary)
    pub fn can_access_outer_vars(&self) -> bool {
        if let Some(current) = self.current {
            if let Some(node) = self.scopes.get(current) {
                return !node.kind.is_hard_scope_boundary();
            }
        }
        false
    }

    /// Define a new variable in the current scope
    /// Returns the index of the variable in the scope's local_variables vector
    pub fn define_variable(&mut self, name: &str, location: Location) -> Option<usize> {
        let current = self.current?;
        let scope = self.scopes.get_mut(current)?;

        let name_key = ustr::ustr(name);
        let idx = scope.local_variables.len();

        scope.local_variables.push(VariableNode {
            name: name_key,
            definition_location: location,
            read_locations: Vec::new(),
            write_locations: Vec::new(),
        });

        Some(idx)
    }

    /// Record a reference to a variable. Walks up parent scopes to find the definition.
    /// Returns (scope_id, variable_index) if found, and whether it was captured from an outer scope.
    /// Also records the read location.
    pub fn reference_variable(
        &mut self,
        name: &str,
        location: Location,
    ) -> Option<(LVScopeId, usize, bool)> {
        let current = self.current?;
        let name_key = ustr::ustr(name);

        // Try to find the variable in current or parent scopes
        let result = self.reference_variable_from_scope(current, name_key);

        // If found, record the read location
        if let Some((scope_id, var_idx, _)) = result {
            if let Some(scope) = self.scopes.get_mut(scope_id) {
                if let Some(var) = scope.local_variables.get_mut(var_idx) {
                    var.read_locations.push(location);
                }
            }
        }

        result
    }

    fn reference_variable_from_scope(
        &mut self,
        scope_id: LVScopeId,
        name: ustr::Ustr,
    ) -> Option<(LVScopeId, usize, bool)> {
        // Walk up the scope chain iteratively
        let mut current_scope = Some(scope_id);

        while let Some(sid) = current_scope {
            // Get scope info (immutable)
            let (is_hard_boundary, parent) = {
                let scope = self.scopes.get(sid)?;

                // Check if variable is in this scope
                for (idx, var) in scope.local_variables.iter().enumerate() {
                    if var.name == name {
                        return Some((sid, idx, false));
                    }
                }

                (scope.kind.is_hard_scope_boundary(), scope.parent)
            };

            // If hard boundary, can't access outer vars
            if is_hard_boundary {
                return None;
            }

            // Move to parent
            current_scope = parent;
        }

        // Variable not found - don't create a capture for undefined variables
        None
    }

    /// Record a read location for a variable at a specific scope
    pub fn record_read(&mut self, scope_id: LVScopeId, var_index: usize, location: Location) {
        if let Some(scope) = self.scopes.get_mut(scope_id) {
            if let Some(var) = scope.local_variables.get_mut(var_index) {
                var.read_locations.push(location);
            }
        }
    }

    /// Record a write location for a variable at a specific scope
    pub fn record_write(&mut self, scope_id: LVScopeId, var_index: usize, location: Location) {
        if let Some(scope) = self.scopes.get_mut(scope_id) {
            if let Some(var) = scope.local_variables.get_mut(var_index) {
                var.write_locations.push(location);
            }
        }
    }

    /// Find all rename targets for a variable by name, starting from a given scope
    pub fn find_rename_targets(&self, name: &str, from_scope: LVScopeId) -> Vec<RenameTarget> {
        let mut targets = Vec::new();
        let name_key = ustr::ustr(name);

        self.collect_targets_from_scope(from_scope, name_key, &mut targets);

        targets
    }

    fn collect_targets_from_scope(
        &self,
        scope_id: LVScopeId,
        name: ustr::Ustr,
        targets: &mut Vec<RenameTarget>,
    ) {
        let scope = match self.scopes.get(scope_id) {
            Some(s) => s,
            None => return,
        };

        // Check variables defined in this scope
        for var in &scope.local_variables {
            if var.name == name {
                // Add definition
                targets.push(RenameTarget {
                    location: var.definition_location.clone(),
                    kind: RenameTargetKind::Definition,
                });

                // Add reads
                for loc in &var.read_locations {
                    targets.push(RenameTarget {
                        location: loc.clone(),
                        kind: RenameTargetKind::Read,
                    });
                }

                // Add writes
                for loc in &var.write_locations {
                    targets.push(RenameTarget {
                        location: loc.clone(),
                        kind: RenameTargetKind::Write,
                    });
                }
            }
        }

        // If not a hard boundary, also check parent scopes for captured variables
        if !scope.kind.is_hard_scope_boundary() {
            if let Some(parent) = scope.parent {
                self.collect_targets_from_scope(parent, name, targets);
            }
        }
    }

    /// Find the scope at a given position
    pub fn scope_at_position(&self, position: tower_lsp::lsp_types::Position) -> Option<LVScopeId> {
        self.find_scope_at(self.root, position)
    }

    fn find_scope_at(
        &self,
        scope_id: LVScopeId,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<LVScopeId> {
        let scope = self.scopes.get(scope_id)?;

        // Check children first (more specific scopes)
        for &child in &scope.children {
            if let Some(found) = self.find_scope_at(child, position) {
                return Some(found);
            }
        }

        // Check if position is within this scope's range
        let range = &scope.range;
        let after_start = position.line > range.start.line
            || (position.line == range.start.line && position.character >= range.start.character);
        let before_end = position.line < range.end.line
            || (position.line == range.end.line && position.character <= range.end.character);

        if after_start && before_end {
            Some(scope_id)
        } else {
            None
        }
    }

    /// Get a scope's kind
    pub fn scope_kind(&self, scope_id: LVScopeId) -> Option<LVScopeKind> {
        self.scopes.get(scope_id).map(|s| s.kind)
    }

    /// Get all local variable definitions across all scopes
    pub fn get_all_definitions(&self) -> Vec<(LVScopeId, &VariableNode)> {
        let mut results = Vec::new();
        for scope in &self.scopes {
            for var in &scope.local_variables {
                results.push((scope.id, var));
            }
        }
        results
    }

    /// Find a local variable definition by name in a scope or parent scopes
    pub fn find_variable(&self, name: &str, scope_id: LVScopeId) -> Option<(LVScopeId, &VariableNode)> {
        let name_key = ustr::ustr(name);
        let mut current = Some(scope_id);
        
        while let Some(sid) = current {
            if let Some(scope) = self.scopes.get(sid) {
                for var in &scope.local_variables {
                    if var.name == name_key {
                        return Some((sid, var));
                    }
                }
                
                // If hard boundary, stop
                if scope.kind.is_hard_scope_boundary() {
                    return None;
                }
                
                current = scope.parent;
            } else {
                return None;
            }
        }
        
        None
    }
}

    /// Find a child scope that matches the given range
    pub fn find_child_scope_by_range(
        &self,
        parent_id: LVScopeId,
        range: Range,
    ) -> Option<LVScopeId> {
        let scope = self.scopes.get(parent_id)?;
        for &child_id in &scope.children {
            if let Some(child) = self.scopes.get(child_id) {
                if child.range == range {
                    return Some(child_id);
                }
            }
        }
        None
    }
}

impl Default for ScopeTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single scope in the scope tree
#[derive(Clone)]
pub struct ScopeNode {
    pub id: LVScopeId,
    pub parent: Option<LVScopeId>,
    pub children: Vec<LVScopeId>,
    pub kind: LVScopeKind,
    pub range: Range,
    /// Optional name (e.g., method name, block info)
    pub name: Option<String>,
    /// Variables defined in this scope
    pub local_variables: Vec<VariableNode>,
    /// References to variables from outer scopes (captured in blocks)
    pub captured_variables: Vec<CaptureRef>,
}

/// A single local variable with its full def-use chain
#[derive(Clone)]
pub struct VariableNode {
    pub name: ustr::Ustr,
    pub definition_location: Location,
    pub read_locations: Vec<Location>,
    pub write_locations: Vec<Location>,
}

/// A reference to a variable from an outer scope (captured in a block)
#[derive(Clone)]
pub struct CaptureRef {
    pub variable_scope: LVScopeId,
    pub variable_index: usize,
    pub captured_by_scope: LVScopeId,
    pub capture_location: Location,
}

/// A location that would be renamed
#[derive(Clone)]
pub struct RenameTarget {
    pub location: Location,
    pub kind: RenameTargetKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenameTargetKind {
    Definition,
    Read,
    Write,
}
