use ruby_prism::Node;

use crate::{
    analyzer_prism::utils,
    indexer::entry::NamespaceKind,
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant, scope::LVScopeKind,
    },
};

/// Tracks namespace context and scope kinds during AST traversal.
///
/// Namespace tracking (frames) is used for FQN construction by all visitors.
/// Scope kind tracking is a lightweight stack used by `current_method_context()`
/// to determine instance vs class method context.
///
/// Note: Local variable scope *traversal* is handled by `VariableScopes`
/// (enter_scope/exit_scope/enter_child_scope). This struct only tracks the
/// *kind* of each scope for method context resolution.
#[derive(Debug, Clone)]
pub struct ScopeTracker {
    /// Ordered stack of namespace/singleton frames.
    frames: Vec<ScopeFrame>,

    /// Lightweight scope kind stack for `current_method_context()`
    scope_kind_stack: Vec<LVScopeKind>,

    /// Stack of enclosing method FQNs for call hierarchy tracking.
    /// None entries represent non-method scopes (class body, blocks).
    method_fqn_stack: Vec<Option<FullyQualifiedName>>,
}

/// Mixed scope frame – either a namespace or a `class << self` marker
#[derive(Debug, Clone)]
pub enum ScopeFrame {
    /// Stack of namespaces for each scope
    /// To support module/class definitions with ConstantPathNode
    /// we store the namespace stack for each scope as Vec<RubyConstant>
    /// Eg. module A; end
    /// namespace_stack = [A]
    /// Eg. module A::B::C; end;
    /// namespace_stack = [A, B, C]
    Namespace(Vec<RubyConstant>),

    /// Singleton frame for `class << self`
    Singleton,
}

impl ScopeTracker {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            // Start with a Constant scope kind (file-level)
            scope_kind_stack: vec![LVScopeKind::Constant],
            method_fqn_stack: Vec::new(),
        }
    }
}

impl ScopeTracker {
    // ---------- namespace helpers ----------
    pub fn push_ns_scope(&mut self, ns: RubyConstant) {
        self.frames.push(ScopeFrame::Namespace(vec![ns]));
    }

    pub fn push_ns_scopes(&mut self, v: Vec<RubyConstant>) {
        self.frames.push(ScopeFrame::Namespace(v));
    }

    pub fn pop_ns_scope(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Namespace(_))) {
            self.frames.pop();
        }
    }

    pub fn get_ns_stack(&self) -> Vec<RubyConstant> {
        self.frames
            .iter()
            .filter_map(|frame| match frame {
                ScopeFrame::Namespace(constants) => Some(constants.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Push namespace from a constant path node (from class/module definition).
    /// Handles both ConstantPathNode (e.g., `A::B::C`) and ConstantReadNode (e.g., `A`).
    ///
    /// # Arguments
    /// * `constant_path` - The constant_path() from ClassNode or ModuleNode
    /// * `fallback_name` - The name() bytes from the node (used if constant_path is a simple name)
    ///
    /// # Returns
    /// * `Ok(())` if namespace was successfully pushed
    /// * `Err(())` if the name is invalid (caller should skip processing)
    pub fn push_namespace_from_constant_path(
        &mut self,
        constant_path: &Node,
        fallback_name: &[u8],
    ) -> Result<(), ()> {
        if let Some(path_node) = constant_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.push_ns_scopes(namespace_parts);
            Ok(())
        } else {
            let name = String::from_utf8_lossy(fallback_name);
            match RubyConstant::new(&name) {
                Ok(constant) => {
                    self.push_ns_scope(constant);
                    Ok(())
                }
                Err(_) => Err(()),
            }
        }
    }

    // ---------- scope kind helpers ----------
    pub fn push_scope_kind(&mut self, kind: LVScopeKind) {
        self.scope_kind_stack.push(kind);
    }

    pub fn pop_scope_kind(&mut self) {
        self.scope_kind_stack.pop();
    }

    // ---------- method FQN helpers (call hierarchy) ----------
    pub fn push_method_fqn(&mut self, fqn: Option<FullyQualifiedName>) {
        self.method_fqn_stack.push(fqn);
    }

    pub fn pop_method_fqn(&mut self) {
        self.method_fqn_stack.pop();
    }

    /// Returns the FQN of the innermost enclosing method, if any.
    /// Walks the stack in reverse, skipping None entries (blocks, class bodies).
    pub fn current_method_fqn(&self) -> Option<&FullyQualifiedName> {
        self.method_fqn_stack
            .iter()
            .rev()
            .find_map(|entry| entry.as_ref())
    }

    // ---------- singleton helpers ----------
    pub fn enter_singleton(&mut self) {
        self.frames.push(ScopeFrame::Singleton);
    }

    pub fn exit_singleton(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Singleton)) {
            self.frames.pop();
        }
    }

    /// Returns true if the tracker is currently inside a singleton scope (i.e., a
    /// `class << self` block).
    pub fn in_singleton(&self) -> bool {
        matches!(self.frames.last(), Some(ScopeFrame::Singleton))
    }

    /// Returns the current method context based on the scope kind stack.
    /// This helps determine whether bare method calls should be treated as instance or singleton methods.
    pub fn current_method_context(&self) -> NamespaceKind {
        // Look for the most recent method scope in the kind stack
        for kind in self.scope_kind_stack.iter().rev() {
            match kind {
                LVScopeKind::InstanceMethod => return NamespaceKind::Instance,
                LVScopeKind::ClassMethod => return NamespaceKind::Singleton,
                LVScopeKind::Constant => break, // Hard scope boundary
                _ => continue,
            }
        }

        // If we're in a singleton context, default to singleton methods
        if self.in_singleton() {
            return NamespaceKind::Singleton;
        }

        // Class/module body (non-empty namespace) returns Singleton
        // Bare calls in class body are class methods
        if !self.get_ns_stack().is_empty() {
            return NamespaceKind::Singleton;
        }

        // Top-level returns Instance
        // Bare calls at top-level become Object instance methods
        NamespaceKind::Instance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ruby_namespace::RubyConstant;

    // ---------- constructor ----------
    #[test]
    fn test_new_initial_state() {
        let tracker = ScopeTracker::new();

        // Check namespace frames - should be empty at start
        assert_eq!(tracker.frames.len(), 0);
        assert!(tracker.get_ns_stack().is_empty());

        // Check scope kind stack starts with Constant
        assert_eq!(tracker.scope_kind_stack.len(), 1);
        assert_eq!(tracker.scope_kind_stack[0], LVScopeKind::Constant);
    }

    // ---------- namespace helpers ----------
    #[test]
    fn test_push_and_pop_ns_scope() {
        let mut tracker = ScopeTracker::new();

        // Initially empty namespace
        assert_eq!(tracker.frames.len(), 0);
        assert!(tracker.get_ns_stack().is_empty());

        // Push a single scope
        let const_a = RubyConstant::new("A").unwrap();
        tracker.push_ns_scope(const_a.clone());
        assert_eq!(tracker.frames.len(), 1);
        assert_eq!(tracker.get_ns_stack(), vec![const_a.clone()]);

        // Push multiple scopes
        let const_b = RubyConstant::new("B").unwrap();
        let const_c = RubyConstant::new("C").unwrap();
        tracker.push_ns_scopes(vec![const_b.clone(), const_c.clone()]);
        assert_eq!(tracker.frames.len(), 2);
        assert_eq!(
            tracker.get_ns_stack(),
            vec![const_a.clone(), const_b.clone(), const_c.clone()]
        );

        // Pop a namespace scope
        tracker.pop_ns_scope();
        assert_eq!(tracker.frames.len(), 1);
        assert_eq!(tracker.get_ns_stack(), vec![const_a.clone()]);

        // Try to pop a namespace scope when a singleton is on top (should not pop)
        tracker.enter_singleton();
        assert_eq!(tracker.frames.len(), 2);
        tracker.pop_ns_scope(); // This should be a no-op
        assert_eq!(tracker.frames.len(), 2);

        // Pop the singleton, then the namespace
        tracker.exit_singleton();
        tracker.pop_ns_scope();
        assert_eq!(tracker.frames.len(), 0);
        assert!(tracker.get_ns_stack().is_empty());
    }

    #[test]
    fn test_get_ns_stack() {
        let mut tracker = ScopeTracker::new();
        let const_a = RubyConstant::new("A").unwrap();
        let const_b = RubyConstant::new("B").unwrap();
        let const_c = RubyConstant::new("C").unwrap();

        tracker.push_ns_scope(const_a.clone());
        tracker.enter_singleton(); // This should be ignored by get_ns_stack
        tracker.push_ns_scopes(vec![const_b.clone(), const_c.clone()]);

        let expected_stack = vec![const_a, const_b, const_c];

        assert_eq!(tracker.get_ns_stack(), expected_stack);
    }

    // ---------- scope kind helpers ----------
    #[test]
    fn test_push_pop_scope_kind() {
        let mut tracker = ScopeTracker::new();

        // Initial state
        assert_eq!(tracker.scope_kind_stack.len(), 1);
        assert_eq!(tracker.scope_kind_stack[0], LVScopeKind::Constant);

        // Push a method scope kind
        tracker.push_scope_kind(LVScopeKind::InstanceMethod);
        assert_eq!(tracker.scope_kind_stack.len(), 2);

        // Push a block scope kind
        tracker.push_scope_kind(LVScopeKind::Block);
        assert_eq!(tracker.scope_kind_stack.len(), 3);

        // Pop block
        tracker.pop_scope_kind();
        assert_eq!(tracker.scope_kind_stack.len(), 2);

        // Pop method
        tracker.pop_scope_kind();
        assert_eq!(tracker.scope_kind_stack.len(), 1);
        assert_eq!(tracker.scope_kind_stack[0], LVScopeKind::Constant);
    }

    // ---------- singleton helpers ----------
    #[test]
    fn test_enter_and_exit_singleton() {
        let mut tracker = ScopeTracker::new();

        // Enter singleton
        tracker.enter_singleton();
        assert_eq!(tracker.frames.len(), 1);
        assert!(matches!(tracker.frames.last(), Some(ScopeFrame::Singleton)));

        // Should not affect namespace stack (empty at top level)
        assert!(tracker.get_ns_stack().is_empty());

        // Exit singleton
        tracker.exit_singleton();
        assert_eq!(tracker.frames.len(), 0);
        assert!(!matches!(
            tracker.frames.last(),
            Some(ScopeFrame::Singleton)
        ));

        // Exit again should be a no-op
        tracker.exit_singleton();
        assert_eq!(tracker.frames.len(), 0);
    }

    #[test]
    fn test_in_singleton_behavior() {
        let mut tracker = ScopeTracker::new();

        // Initially not in singleton
        assert!(!tracker.in_singleton());

        // Enter singleton
        tracker.enter_singleton();
        assert!(tracker.in_singleton());

        // Push a namespace, should not be in singleton as new class/module is not a singleton by default
        let const_a = RubyConstant::new("A").unwrap();
        tracker.push_ns_scope(const_a.clone());
        assert!(!tracker.in_singleton());

        // Pop the namespace, should be in singleton
        tracker.pop_ns_scope();
        assert!(tracker.in_singleton());

        // Exit singleton
        tracker.exit_singleton();
        assert!(!tracker.in_singleton());
    }
}
