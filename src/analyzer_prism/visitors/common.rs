use crate::types::{
    ruby_method::RubyMethod,
    ruby_namespace::RubyConstant,
    scope::{LVScope, LVScopeStack},
};

/// Mixed stack frame – either a namespace or a `class << self` marker
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

#[derive(Default, Debug, Clone)]
pub struct ScopeTracker {
    /// Ordered stack of namespace/singleton frames.
    frames: Vec<ScopeFrame>,

    /// Local-variable scopes (method/block/rescue/lambda)
    lv_stack: LVScopeStack,

    /// Current Ruby method we are inside (used by Identifier / Index visitors)
    current_method: Option<RubyMethod>,
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

    // ---------- method helpers ----------
    pub fn enter_method(&mut self, m: RubyMethod) {
        self.current_method = Some(m);
    }

    pub fn exit_method(&mut self) {
        self.current_method = None;
    }

    pub fn current_method(&self) -> Option<&RubyMethod> {
        self.current_method.as_ref()
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

    /// Returns true if there is a `Singleton` frame above the last `Namespace`.
    pub fn in_singleton(&self) -> bool {
        for frame in self.frames.iter().rev() {
            match frame {
                ScopeFrame::Singleton => return true,
                ScopeFrame::Namespace(_) => return false,
            }
        }
        false
    }
}
