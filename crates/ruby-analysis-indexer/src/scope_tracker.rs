use std::fmt;

use ruby_analysis_core::{FullyQualifiedName, NamespaceKind, RubyConstant};
use ruby_prism::{ConstantPathNode, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalScopeKind {
    Constant,
    InstanceMethod,
    ClassMethod,
    Block,
    Rescue,
    ExplicitBlockLocal,
}

impl LocalScopeKind {
    pub fn is_hard_scope_boundary(&self) -> bool {
        matches!(
            self,
            LocalScopeKind::InstanceMethod | LocalScopeKind::ClassMethod | LocalScopeKind::Constant
        )
    }
}

impl fmt::Display for LocalScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalScopeKind::Constant => write!(f, "Constant"),
            LocalScopeKind::InstanceMethod => write!(f, "InstanceMethod"),
            LocalScopeKind::ClassMethod => write!(f, "ClassMethod"),
            LocalScopeKind::Block => write!(f, "Block"),
            LocalScopeKind::Rescue => write!(f, "Rescue"),
            LocalScopeKind::ExplicitBlockLocal => write!(f, "ExplicitBlockLocal"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopeTracker {
    frames: Vec<ScopeFrame>,
    scope_kind_stack: Vec<LocalScopeKind>,
    method_fqn_stack: Vec<Option<FullyQualifiedName>>,
}

#[derive(Debug, Clone)]
pub enum ScopeFrame {
    Namespace(Vec<RubyConstant>),
    Singleton,
}

impl ScopeTracker {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            scope_kind_stack: vec![LocalScopeKind::Constant],
            method_fqn_stack: Vec::new(),
        }
    }

    pub fn push_ns_scope(&mut self, ns: RubyConstant) {
        self.frames.push(ScopeFrame::Namespace(vec![ns]));
    }

    pub fn push_ns_scopes(&mut self, namespaces: Vec<RubyConstant>) {
        self.frames.push(ScopeFrame::Namespace(namespaces));
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
                ScopeFrame::Singleton => None,
            })
            .flatten()
            .collect()
    }

    pub fn push_namespace_from_constant_path(
        &mut self,
        constant_path: &Node,
        fallback_name: &[u8],
    ) -> Result<(), ()> {
        if let Some(path_node) = constant_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            collect_namespaces(&path_node, &mut namespace_parts);
            self.push_ns_scopes(namespace_parts);
            return Ok(());
        }

        let name = String::from_utf8_lossy(fallback_name);
        let constant = RubyConstant::new(&name).map_err(|_| ())?;
        self.push_ns_scope(constant);
        Ok(())
    }

    pub fn push_scope_kind(&mut self, kind: LocalScopeKind) {
        self.scope_kind_stack.push(kind);
    }

    pub fn pop_scope_kind(&mut self) {
        self.scope_kind_stack.pop();
    }

    pub fn push_method_fqn(&mut self, fqn: Option<FullyQualifiedName>) {
        self.method_fqn_stack.push(fqn);
    }

    pub fn pop_method_fqn(&mut self) {
        self.method_fqn_stack.pop();
    }

    pub fn current_method_fqn(&self) -> Option<&FullyQualifiedName> {
        self.method_fqn_stack
            .iter()
            .rev()
            .find_map(|entry| entry.as_ref())
    }

    pub fn enter_singleton(&mut self) {
        self.frames.push(ScopeFrame::Singleton);
    }

    pub fn exit_singleton(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Singleton)) {
            self.frames.pop();
        }
    }

    pub fn in_singleton(&self) -> bool {
        matches!(self.frames.last(), Some(ScopeFrame::Singleton))
    }

    pub fn current_method_context(&self) -> NamespaceKind {
        for kind in self.scope_kind_stack.iter().rev() {
            match kind {
                LocalScopeKind::InstanceMethod => return NamespaceKind::Instance,
                LocalScopeKind::ClassMethod => return NamespaceKind::Singleton,
                LocalScopeKind::Constant => break,
                LocalScopeKind::Block
                | LocalScopeKind::Rescue
                | LocalScopeKind::ExplicitBlockLocal => continue,
            }
        }

        if self.in_singleton() || !self.get_ns_stack().is_empty() {
            return NamespaceKind::Singleton;
        }

        NamespaceKind::Instance
    }
}

impl Default for ScopeTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub fn collect_namespaces(node: &ConstantPathNode, acc: &mut Vec<RubyConstant>) {
    if let Some(parent) = node.parent() {
        if let Some(parent_const_path) = parent.as_constant_path_node() {
            collect_namespaces(&parent_const_path, acc);
        } else if let Some(parent_const_read) = parent.as_constant_read_node() {
            let parent_name = String::from_utf8_lossy(parent_const_read.name().as_slice());
            if let Ok(constant) = RubyConstant::new(&parent_name) {
                acc.push(constant);
            }
        }
    }

    if let Some(name_node) = node.name() {
        let name = String::from_utf8_lossy(name_node.as_slice());
        if let Ok(constant) = RubyConstant::new(&name) {
            acc.push(constant);
        }
    }
}

pub fn get_method_namespace_kind(
    receiver: Option<Node>,
    current_namespace: &[RubyConstant],
    in_singleton: bool,
) -> (NamespaceKind, bool) {
    let mut namespace_kind = NamespaceKind::Instance;
    let mut skip_method = false;

    if let Some(receiver) = receiver {
        if receiver.as_self_node().is_some() {
            namespace_kind = NamespaceKind::Singleton;
        } else if let Some(read_node) = receiver.as_constant_read_node() {
            let recv_name = utf8_str(read_node.name().as_slice());
            if current_namespace
                .last()
                .is_some_and(|last| last.as_str() == recv_name)
            {
                namespace_kind = NamespaceKind::Singleton;
            } else {
                skip_method = true;
            }
        } else if receiver.as_constant_path_node().is_some() {
            namespace_kind = NamespaceKind::Singleton;
        } else {
            skip_method = true;
        }
    } else if in_singleton {
        namespace_kind = NamespaceKind::Singleton;
    }

    (namespace_kind, skip_method)
}

pub fn utf8_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use ruby_analysis_core::RubyMethod;

    use super::*;

    #[test]
    fn starts_at_file_scope() {
        let tracker = ScopeTracker::new();

        assert!(tracker.get_ns_stack().is_empty());
        assert_eq!(tracker.current_method_context(), NamespaceKind::Instance);
    }

    #[test]
    fn tracks_nested_namespaces() {
        let mut tracker = ScopeTracker::new();
        let a = RubyConstant::new("A").expect("test constant must be valid");
        let b = RubyConstant::new("B").expect("test constant must be valid");
        let c = RubyConstant::new("C").expect("test constant must be valid");

        tracker.push_ns_scope(a.clone());
        tracker.push_ns_scopes(vec![b.clone(), c.clone()]);

        assert_eq!(tracker.get_ns_stack(), vec![a, b, c]);
        assert_eq!(tracker.current_method_context(), NamespaceKind::Singleton);
    }

    #[test]
    fn singleton_frame_does_not_pop_as_namespace() {
        let mut tracker = ScopeTracker::new();
        let user = RubyConstant::new("User").expect("test constant must be valid");

        tracker.push_ns_scope(user.clone());
        tracker.enter_singleton();
        tracker.pop_ns_scope();

        assert!(tracker.in_singleton());
        assert_eq!(tracker.get_ns_stack(), vec![user]);

        tracker.exit_singleton();
        tracker.pop_ns_scope();
        assert!(tracker.get_ns_stack().is_empty());
    }

    #[test]
    fn method_scope_kind_controls_context() {
        let mut tracker = ScopeTracker::new();

        tracker.push_scope_kind(LocalScopeKind::InstanceMethod);
        assert_eq!(tracker.current_method_context(), NamespaceKind::Instance);

        tracker.push_scope_kind(LocalScopeKind::Block);
        assert_eq!(tracker.current_method_context(), NamespaceKind::Instance);

        tracker.pop_scope_kind();
        tracker.pop_scope_kind();
        tracker.push_scope_kind(LocalScopeKind::ClassMethod);
        assert_eq!(tracker.current_method_context(), NamespaceKind::Singleton);
    }

    #[test]
    fn tracks_current_method_fqn() {
        let mut tracker = ScopeTracker::new();
        let user = RubyConstant::new("User").expect("test constant must be valid");
        let name = RubyMethod::new("name").expect("test method must be valid");
        let fqn = FullyQualifiedName::method(vec![user], name);

        tracker.push_method_fqn(None);
        assert_eq!(tracker.current_method_fqn(), None);

        tracker.push_method_fqn(Some(fqn.clone()));
        assert_eq!(tracker.current_method_fqn(), Some(&fqn));

        tracker.pop_method_fqn();
        assert_eq!(tracker.current_method_fqn(), None);
    }
}
