use tower_lsp::lsp_types::{Location as LspLocation, Range};

use crate::types::{
    ruby_document::RubyDocument,
    ruby_namespace::RubyConstant,
    scope::{LVScope, LVScopeKind, LVScopeStack},
};
use crate::type_inference::typed_variable::{TypedVariable, VariableTypeContext};
use crate::type_inference::ruby_type::RubyType;
use crate::types::ruby_variable::{RubyVariable, RubyVariableType};

#[derive(Debug, Clone)]
pub struct ScopeTracker {
    /// Ordered stack of namespace/singleton frames.
    frames: Vec<ScopeFrame>,

    /// Local-variable scopes (method/block/rescue/lambda)
    lv_stack: LVScopeStack,

    /// Type information for variables in each scope
    /// Each element corresponds to a scope in lv_stack
    type_contexts: Vec<VariableTypeContext>,
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
    pub fn new(document: &RubyDocument) -> Self {
        let mut frames = Vec::new();
        let top_namespace = vec![RubyConstant::new("Object").unwrap()];
        frames.push(ScopeFrame::Namespace(top_namespace));
        let mut lv_stack = LVScopeStack::new();
        let top_lv_scope = LVScope::new(
            0,
            LspLocation {
                uri: document.uri.clone(),
                range: Range::new(
                    document.offset_to_position(0),
                    document.offset_to_position(document.content.len()),
                ),
            },
            LVScopeKind::Constant,
        );
        lv_stack.push(top_lv_scope);
        let mut type_contexts = Vec::new();
        type_contexts.push(VariableTypeContext::new());
        Self { frames, lv_stack, type_contexts }
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

    // ---------- lv-scope helpers ----------
    pub fn push_lv_scope(&mut self, scope: LVScope) {
        self.lv_stack.push(scope);
        self.type_contexts.push(VariableTypeContext::new());
    }

    pub fn pop_lv_scope(&mut self) {
        self.lv_stack.pop();
        self.type_contexts.pop();
    }

    pub fn current_lv_scope(&self) -> Option<&LVScope> {
        self.lv_stack.last()
    }

    pub fn get_lv_stack(&self) -> LVScopeStack {
        self.lv_stack.clone()
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

    /// Returns the current method context based on the local variable scope stack.
    /// This helps determine whether bare method calls should be treated as instance or class methods.
    pub fn current_method_context(&self) -> Option<crate::indexer::entry::MethodKind> {
        use crate::types::scope::LVScopeKind;
        use crate::indexer::entry::MethodKind;

        // Look for the most recent method scope in the LV stack
        for scope in self.lv_stack.iter().rev() {
            match scope.kind() {
                LVScopeKind::InstanceMethod => return Some(MethodKind::Instance),
                LVScopeKind::ClassMethod => return Some(MethodKind::Class),
                LVScopeKind::Constant => break, // Hard scope boundary
                _ => continue,
            }
        }

        // If we're in a singleton context, default to class methods
        if self.in_singleton() {
            return Some(MethodKind::Class);
        }

        None
    }

    // Type inference methods

    /// Add a typed variable to the current scope
    pub fn add_typed_variable(&mut self, typed_var: TypedVariable) {
        if let Some(current_context) = self.type_contexts.last_mut() {
            current_context.add_variable(typed_var);
        }
    }

    /// Find a typed variable in the current scope chain
    pub fn find_typed_variable(&self, name: &str, var_type: &RubyVariableType) -> Option<&TypedVariable> {
        // Search from current scope up to parent scopes
        for context in self.type_contexts.iter().rev() {
            if let Some(var) = context.find_variable(name, var_type) {
                return Some(var);
            }
        }
        None
    }

    /// Find any typed variable by name (regardless of type)
    pub fn find_any_typed_variable(&self, name: &str) -> Option<&TypedVariable> {
        // Search from current scope up to parent scopes
        for context in self.type_contexts.iter().rev() {
            if let Some(var) = context.find_any_variable(name) {
                return Some(var);
            }
        }
        None
    }

    /// Get the current variable type context
    pub fn current_type_context(&self) -> Option<&VariableTypeContext> {
        self.type_contexts.last()
    }

    /// Get all variables in the current scope
    pub fn current_scope_variables(&self) -> Vec<&TypedVariable> {
        if let Some(context) = self.type_contexts.last() {
            context.all_variables().collect()
        } else {
            Vec::new()
        }
    }

    /// Update or add a variable with type information
    pub fn set_variable_type(
        &mut self,
        name: &str,
        var_type: RubyVariableType,
        ruby_type: RubyType,
        source_location: Option<LspLocation>,
    ) {
        if let Ok(variable) = RubyVariable::new(name, var_type) {
            let typed_var = TypedVariable::new_inferred(variable, ruby_type, source_location);
            self.add_typed_variable(typed_var);
        }
    }

    /// Get the inferred type of a variable
    pub fn get_variable_type(&self, name: &str, var_type: &RubyVariableType) -> Option<&RubyType> {
        self.find_typed_variable(name, var_type)
            .map(|typed_var| &typed_var.ruby_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ruby_document::RubyDocument;

    // Helper that returns a minimal RubyDocument for constructing a ScopeTracker.
    fn dummy_document() -> RubyDocument {
        // The content is irrelevant for the scope-tracker tests; we only need a
        // valid, non-empty document so that `offset_to_position` works.
        let content = "# dummy\n".to_string();
        RubyDocument::new("file:///dummy.rb".parse().unwrap(), content, 1)
    }

    // ---------- constructor ----------
    #[test]
    fn test_new_initial_state() {
        // ensure that a fresh `ScopeTracker` contains:
        //   * exactly one `Namespace` frame with `Object`
        //   * an `LVScopeStack` with a single `Constant` scope spanning the file
        let doc = dummy_document();
        let tracker = ScopeTracker::new(&doc);

        // Check namespace frames
        assert_eq!(tracker.frames.len(), 1);
        match &tracker.frames[0] {
            ScopeFrame::Namespace(constants) => {
                assert_eq!(constants.len(), 1);
                assert_eq!(constants[0].to_string(), "Object");
            }
            _ => panic!("Expected a Namespace frame"),
        }

        // Check LV scope stack
        let lv_stack = tracker.get_lv_stack();
        assert_eq!(lv_stack.len(), 1);
        let top_scope = lv_stack.last().unwrap();
        assert_eq!(*top_scope.kind(), LVScopeKind::Constant);

        // Check that the scope spans the entire document
        let doc_content_len = doc.content.len();
        let expected_range = Range::new(
            doc.offset_to_position(0),
            doc.offset_to_position(doc_content_len),
        );
        assert_eq!(top_scope.location().range, expected_range);

        // Check type contexts
        assert_eq!(tracker.type_contexts.len(), 1);
        let context = tracker.current_type_context().unwrap();
        assert_eq!(context.all_variables().count(), 0);
    }

    // ---------- namespace helpers ----------
    #[test]
    fn test_push_and_pop_ns_scope() {
        let mut tracker = ScopeTracker::new(&dummy_document());

        // Push a single scope
        let const_a = RubyConstant::new("A").unwrap();
        tracker.push_ns_scope(const_a.clone());
        assert_eq!(tracker.frames.len(), 2);
        assert_eq!(
            tracker.get_ns_stack(),
            vec![RubyConstant::new("Object").unwrap(), const_a.clone()]
        );

        // Push multiple scopes
        let const_b = RubyConstant::new("B").unwrap();
        let const_c = RubyConstant::new("C").unwrap();
        tracker.push_ns_scopes(vec![const_b.clone(), const_c.clone()]);
        assert_eq!(tracker.frames.len(), 3);
        assert_eq!(
            tracker.get_ns_stack(),
            vec![
                RubyConstant::new("Object").unwrap(),
                const_a.clone(),
                const_b.clone(),
                const_c.clone()
            ]
        );

        // Pop a namespace scope
        tracker.pop_ns_scope();
        assert_eq!(tracker.frames.len(), 2);
        assert_eq!(
            tracker.get_ns_stack(),
            vec![RubyConstant::new("Object").unwrap(), const_a.clone()]
        );

        // Try to pop a namespace scope when a singleton is on top (should not pop)
        tracker.enter_singleton();
        assert_eq!(tracker.frames.len(), 3);
        tracker.pop_ns_scope(); // This should be a no-op
        assert_eq!(tracker.frames.len(), 3);

        // Pop the singleton, then the namespace
        tracker.exit_singleton();
        tracker.pop_ns_scope();
        assert_eq!(tracker.frames.len(), 1);
        assert_eq!(
            tracker.get_ns_stack(),
            vec![RubyConstant::new("Object").unwrap()]
        );
    }

    #[test]
    fn test_get_ns_stack() {
        let mut tracker = ScopeTracker::new(&dummy_document());
        let const_a = RubyConstant::new("A").unwrap();
        let const_b = RubyConstant::new("B").unwrap();
        let const_c = RubyConstant::new("C").unwrap();

        tracker.push_ns_scope(const_a.clone());
        tracker.enter_singleton(); // This should be ignored by get_ns_stack
        tracker.push_ns_scopes(vec![const_b.clone(), const_c.clone()]);

        let expected_stack = vec![
            RubyConstant::new("Object").unwrap(),
            const_a,
            const_b,
            const_c,
        ];

        assert_eq!(tracker.get_ns_stack(), expected_stack);
    }

    // ---------- lv-scope helpers ----------
    #[test]
    fn test_push_pop_current_lv_scope() {
        let doc = dummy_document();
        let mut tracker = ScopeTracker::new(&doc);

        // Initial state
        assert_eq!(tracker.get_lv_stack().len(), 1);
        assert_eq!(
            tracker.current_lv_scope().unwrap().kind(),
            &LVScopeKind::Constant
        );

        // Push a method scope
        let method_scope = LVScope::new(
            1,
            LspLocation::new(doc.uri.clone(), Range::default()),
            LVScopeKind::InstanceMethod,
        );
        tracker.push_lv_scope(method_scope.clone());
        assert_eq!(tracker.get_lv_stack().len(), 2);
        assert_eq!(tracker.current_lv_scope().unwrap(), &method_scope);

        // Push a block scope
        let block_scope = LVScope::new(
            2,
            LspLocation::new(doc.uri.clone(), Range::default()),
            LVScopeKind::Block,
        );
        tracker.push_lv_scope(block_scope.clone());
        assert_eq!(tracker.get_lv_stack().len(), 3);
        assert_eq!(tracker.current_lv_scope().unwrap(), &block_scope);

        // Pop block scope
        tracker.pop_lv_scope();
        assert_eq!(tracker.get_lv_stack().len(), 2);
        assert_eq!(tracker.current_lv_scope().unwrap(), &method_scope);

        // Pop method scope
        tracker.pop_lv_scope();
        assert_eq!(tracker.get_lv_stack().len(), 1);
        assert_eq!(
            tracker.current_lv_scope().unwrap().kind(),
            &LVScopeKind::Constant
        );
    }

    // ---------- singleton helpers ----------
    #[test]
    fn test_enter_and_exit_singleton() {
        let mut tracker = ScopeTracker::new(&dummy_document());

        // Enter singleton
        tracker.enter_singleton();
        assert_eq!(tracker.frames.len(), 2);
        assert!(matches!(tracker.frames.last(), Some(ScopeFrame::Singleton)));

        // Should not affect namespace stack
        assert_eq!(
            tracker.get_ns_stack(),
            vec![RubyConstant::new("Object").unwrap()]
        );

        // Exit singleton
        tracker.exit_singleton();
        assert_eq!(tracker.frames.len(), 1);
        assert!(!matches!(
            tracker.frames.last(),
            Some(ScopeFrame::Singleton)
        ));

        // Exit again should be a no-op
        tracker.exit_singleton();
        assert_eq!(tracker.frames.len(), 1);
    }

    #[test]
    fn test_in_singleton_behavior() {
        let mut tracker = ScopeTracker::new(&dummy_document());

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

    // ---------- type inference tests ----------
    #[test]
    fn test_typed_variable_management() {
        use crate::types::scope::LVScopeStack;
        use tower_lsp::lsp_types::{Position, Range, Url};
        
        let mut tracker = ScopeTracker::new(&dummy_document());
        
        // Add a local variable with type
        let lv_scope_stack = LVScopeStack::new();
        tracker.set_variable_type(
             "x",
             RubyVariableType::Local(lv_scope_stack),
             RubyType::integer(),
            Some(LspLocation {
                uri: Url::parse("file:///test.rb").unwrap(),
                range: Range::new(Position::new(1, 0), Position::new(1, 1)),
            }),
        );
        
        // Verify we can find the variable
        let found_var = tracker.find_any_typed_variable("x");
        assert!(found_var.is_some());
        let var = found_var.unwrap();
        assert_eq!(var.name(), "x");
        assert_eq!(var.ruby_type, RubyType::integer());
        
        // Test getting variable type directly
        let lv_scope_stack = LVScopeStack::new();
        let var_type = tracker.get_variable_type("x", &RubyVariableType::Local(lv_scope_stack));
        assert!(var_type.is_some());
        assert_eq!(*var_type.unwrap(), RubyType::integer());
        
        // Test scope isolation - push new scope
        let new_scope = LVScope::new(
            1,
            LspLocation {
                uri: tracker.lv_stack[0].location().uri.clone(),
                range: Range::new(
                    tracker.lv_stack[0].location().range.start,
                    tracker.lv_stack[0].location().range.end,
                ),
            },
            LVScopeKind::InstanceMethod,
        );
        tracker.push_lv_scope(new_scope);
        
        // Variable from parent scope should still be accessible
        let found_var = tracker.find_any_typed_variable("x");
        assert!(found_var.is_some());
        
        // Add variable in new scope
        let lv_scope_stack = LVScopeStack::new();
        tracker.set_variable_type(
             "y",
             RubyVariableType::Local(lv_scope_stack),
             RubyType::string(),
            None,
        );
        
        // Both variables should be accessible
        assert!(tracker.find_any_typed_variable("x").is_some());
        assert!(tracker.find_any_typed_variable("y").is_some());
        
        // Pop scope - y should no longer be accessible, x should still be
        tracker.pop_lv_scope();
        assert!(tracker.find_any_typed_variable("x").is_some());
        assert!(tracker.find_any_typed_variable("y").is_none());
    }
}
