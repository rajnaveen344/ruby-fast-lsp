use lsp_types::{Location as LspLocation, Range};

use crate::types::{
    ruby_document::RubyDocument,
    ruby_namespace::RubyConstant,
    scope::{LVScope, LVScopeKind, LVScopeStack},
};

#[derive(Debug, Clone)]
pub struct ScopeTracker {
    /// Ordered stack of namespace/singleton frames.
    frames: Vec<ScopeFrame>,

    /// Local-variable scopes (method/block/rescue/lambda)
    lv_stack: LVScopeStack,
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
        Self { frames, lv_stack }
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
    }

    pub fn pop_lv_scope(&mut self) {
        self.lv_stack.pop();
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
        self.frames
            .iter()
            .any(|frame| matches!(frame, ScopeFrame::Singleton))
    }
}

#[cfg(test)]
mod tests {
    // NOTE: These tests are currently placeholders that outline the required test
    // scenarios for `ScopeTracker`.  We will fill in the assertions and sample
    // documents in subsequent commits.

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
            LVScopeKind::Method,
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

        // Push a namespace, should still be in singleton
        let const_a = RubyConstant::new("A").unwrap();
        tracker.push_ns_scope(const_a.clone());
        assert!(tracker.in_singleton());

        // Pop the namespace, should still be in singleton
        tracker.pop_ns_scope();
        assert!(tracker.in_singleton());

        // Exit singleton
        tracker.exit_singleton();
        assert!(!tracker.in_singleton());
    }
}
