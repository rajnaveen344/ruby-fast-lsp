use std::fmt;

use crate::core::method_store::MethodVisibility;
use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant};
use ruby_prism::{ConstantPathNode, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamespacePushError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalScopeKind {
    Constant,
    InstanceMethod,
    ClassMethod,
    Block,
    FrameworkInstanceBlock,
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
            LocalScopeKind::FrameworkInstanceBlock => write!(f, "FrameworkInstanceBlock"),
            LocalScopeKind::Rescue => write!(f, "Rescue"),
            LocalScopeKind::ExplicitBlockLocal => write!(f, "ExplicitBlockLocal"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopeTracker {
    frames: Vec<ScopeFrame>,
    execution_context_stack: Vec<ExecutionContextFrame>,
    scope_kind_stack: Vec<LocalScopeKind>,
    method_fqn_stack: Vec<Option<FullyQualifiedName>>,
    module_function_mode_stack: Vec<bool>,
    visibility_stack: Vec<MethodVisibility>,
}

#[derive(Debug, Clone)]
struct ExecutionContextFrame {
    lexical_frame_depth: usize,
    local_scope_depth: usize,
    implicit_receiver: Vec<RubyConstant>,
    implicit_receiver_kind: NamespaceKind,
    method_definition_owner: Vec<RubyConstant>,
    method_definition_kind: NamespaceKind,
}

#[derive(Debug, Clone)]
pub enum ScopeFrame {
    Namespace {
        parts: Vec<RubyConstant>,
        absolute: bool,
    },
    Singleton,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinRef {
    pub parts: Vec<RubyConstant>,
    pub absolute: bool,
}

impl ScopeTracker {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            execution_context_stack: Vec::new(),
            scope_kind_stack: vec![LocalScopeKind::Constant],
            method_fqn_stack: Vec::new(),
            module_function_mode_stack: Vec::new(),
            visibility_stack: vec![MethodVisibility::Public],
        }
    }

    pub fn push_execution_context(
        &mut self,
        implicit_receiver: Vec<RubyConstant>,
        implicit_receiver_kind: NamespaceKind,
        method_definition_owner: Vec<RubyConstant>,
        method_definition_kind: NamespaceKind,
    ) {
        self.execution_context_stack.push(ExecutionContextFrame {
            lexical_frame_depth: self.frames.len(),
            local_scope_depth: self.scope_kind_stack.len(),
            implicit_receiver,
            implicit_receiver_kind,
            method_definition_owner,
            method_definition_kind,
        });
        self.module_function_mode_stack.push(false);
        self.visibility_stack.push(MethodVisibility::Public);
    }

    pub fn pop_execution_context(&mut self) {
        self.execution_context_stack.pop().expect(
            "INVARIANT VIOLATED: execution context stack underflow. This is a bug because every pushed block execution context must be popped exactly once. Fix: keep execution-context traversal balanced.",
        );
        self.module_function_mode_stack.pop().expect(
            "INVARIANT VIOLATED: module_function stack underflow after execution context. This is a bug because every execution context owns one module_function flag. Fix: keep execution-context traversal balanced.",
        );
        self.visibility_stack.pop().expect(
            "INVARIANT VIOLATED: visibility stack underflow after execution context. This is a bug because every execution context owns one visibility frame. Fix: keep execution-context traversal balanced.",
        );
    }

    fn current_execution_context(&self) -> Option<&ExecutionContextFrame> {
        let context = self.execution_context_stack.last()?;
        assert!(
            context.local_scope_depth <= self.scope_kind_stack.len(),
            "INVARIANT VIOLATED: execution context local-scope depth exceeds the active scope stack. This is a bug because scopes present when an execution context was pushed cannot disappear before that context is popped. Fix: keep execution-context and local-scope traversal balanced."
        );
        (context.lexical_frame_depth == self.frames.len()
            && !self.scope_kind_stack[context.local_scope_depth..]
                .iter()
                .any(|kind| {
                    matches!(
                        kind,
                        LocalScopeKind::Constant
                            | LocalScopeKind::InstanceMethod
                            | LocalScopeKind::ClassMethod
                            | LocalScopeKind::FrameworkInstanceBlock
                    )
                }))
        .then_some(context)
    }

    pub fn execution_context_active(&self) -> bool {
        self.current_execution_context().is_some()
    }

    pub fn implicit_receiver_context(&self) -> (Vec<RubyConstant>, NamespaceKind) {
        self.current_execution_context()
            .map(|context| {
                (
                    context.implicit_receiver.clone(),
                    context.implicit_receiver_kind,
                )
            })
            .unwrap_or_else(|| {
                (
                    self.get_ns_stack(),
                    self.current_method_context_without_execution(),
                )
            })
    }

    pub fn method_definition_context(&self) -> (Vec<RubyConstant>, NamespaceKind) {
        self.current_execution_context()
            .map(|context| {
                (
                    context.method_definition_owner.clone(),
                    context.method_definition_kind,
                )
            })
            .unwrap_or_else(|| {
                (
                    self.get_ns_stack(),
                    self.current_macro_definition_context_without_execution(),
                )
            })
    }

    pub fn push_ns_scope(&mut self, ns: RubyConstant) {
        self.frames.push(ScopeFrame::Namespace {
            parts: vec![ns],
            absolute: false,
        });
        self.module_function_mode_stack.push(false);
        self.visibility_stack.push(MethodVisibility::Public);
    }

    pub fn push_ns_scopes(&mut self, namespaces: Vec<RubyConstant>) {
        self.frames.push(ScopeFrame::Namespace {
            parts: namespaces,
            absolute: false,
        });
        self.module_function_mode_stack.push(false);
        self.visibility_stack.push(MethodVisibility::Public);
    }

    pub fn push_absolute_ns_scopes(&mut self, namespaces: Vec<RubyConstant>) {
        assert!(
            !namespaces.is_empty(),
            "INVARIANT VIOLATED: absolute namespace frame is empty. \
             This is a bug because an absolute class/module target must contain at least one Ruby constant. \
             Fix: validate the resolved namespace before pushing an absolute frame."
        );
        self.frames.push(ScopeFrame::Namespace {
            parts: namespaces,
            absolute: true,
        });
        self.module_function_mode_stack.push(false);
        self.visibility_stack.push(MethodVisibility::Public);
    }

    pub fn pop_ns_scope(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Namespace { .. })) {
            self.frames.pop();
            self.module_function_mode_stack.pop().expect(
                "INVARIANT VIOLATED: module_function mode stack underflow. \
                 This is a bug because every namespace frame must own one module_function mode flag. \
                 Fix: keep namespace frame push/pop balanced.",
            );
            self.visibility_stack.pop().expect(
                "INVARIANT VIOLATED: visibility stack underflow. \
                 This is a bug because every namespace frame must own one visibility flag. \
                 Fix: keep namespace frame push/pop balanced.",
            );
        }
    }

    pub fn get_ns_stack(&self) -> Vec<RubyConstant> {
        let mut namespaces = Vec::new();
        for frame in &self.frames {
            match frame {
                ScopeFrame::Namespace { parts, absolute } => {
                    if *absolute {
                        namespaces.clear();
                    }
                    namespaces.extend(parts.iter().cloned());
                }
                ScopeFrame::Singleton => {}
            }
        }
        namespaces
    }

    pub fn push_namespace_from_constant_path(
        &mut self,
        constant_path: &Node,
        fallback_name: &[u8],
    ) -> Result<(), NamespacePushError> {
        if let Some(path_node) = constant_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            collect_namespaces(&path_node, &mut namespace_parts);
            self.push_ns_scopes(namespace_parts);
            return Ok(());
        }

        let name = String::from_utf8_lossy(fallback_name);
        let constant = RubyConstant::new(&name).map_err(|_| NamespacePushError)?;
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

    pub fn enable_module_function_mode(&mut self) {
        let Some(mode) = self.module_function_mode_stack.last_mut() else {
            return;
        };
        *mode = true;
    }

    pub fn module_function_mode_enabled(&self) -> bool {
        self.module_function_mode_stack
            .last()
            .copied()
            .unwrap_or(false)
    }

    pub fn set_current_visibility(&mut self, visibility: MethodVisibility) {
        let Some(current) = self.visibility_stack.last_mut() else {
            panic!(
                "INVARIANT VIOLATED: visibility stack is empty. \
                 This is a bug because ScopeTracker always starts with public visibility. \
                 Fix: initialize ScopeTracker with a root visibility frame."
            );
        };
        *current = visibility;
    }

    pub fn current_visibility(&self) -> MethodVisibility {
        self.visibility_stack.last().copied().unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: visibility stack is empty. \
                 This is a bug because ScopeTracker always starts with public visibility. \
                 Fix: initialize ScopeTracker with a root visibility frame."
            )
        })
    }

    pub fn enter_singleton(&mut self) {
        self.frames.push(ScopeFrame::Singleton);
        self.visibility_stack.push(MethodVisibility::Public);
    }

    pub fn exit_singleton(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Singleton)) {
            self.frames.pop();
            self.visibility_stack.pop().expect(
                "INVARIANT VIOLATED: visibility stack underflow on singleton exit. \
                 This is a bug because every singleton frame must own one visibility flag. \
                 Fix: keep singleton enter/exit balanced.",
            );
        }
    }

    pub fn in_singleton(&self) -> bool {
        matches!(self.frames.last(), Some(ScopeFrame::Singleton))
    }

    pub fn current_method_context(&self) -> NamespaceKind {
        if let Some(context) = self.current_execution_context() {
            return context.implicit_receiver_kind;
        }
        self.current_method_context_without_execution()
    }

    fn current_method_context_without_execution(&self) -> NamespaceKind {
        for kind in self.scope_kind_stack.iter().rev() {
            match kind {
                LocalScopeKind::InstanceMethod => return NamespaceKind::Instance,
                LocalScopeKind::ClassMethod => return NamespaceKind::Singleton,
                LocalScopeKind::FrameworkInstanceBlock => return NamespaceKind::Instance,
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

    pub fn current_macro_definition_context(&self) -> NamespaceKind {
        if let Some(context) = self.current_execution_context() {
            return context.method_definition_kind;
        }
        self.current_macro_definition_context_without_execution()
    }

    fn current_macro_definition_context_without_execution(&self) -> NamespaceKind {
        for kind in self.scope_kind_stack.iter().rev() {
            match kind {
                LocalScopeKind::InstanceMethod => return NamespaceKind::Instance,
                LocalScopeKind::ClassMethod => return NamespaceKind::Singleton,
                LocalScopeKind::FrameworkInstanceBlock => return NamespaceKind::Instance,
                LocalScopeKind::Constant => break,
                LocalScopeKind::Block
                | LocalScopeKind::Rescue
                | LocalScopeKind::ExplicitBlockLocal => continue,
            }
        }

        if self.in_singleton() {
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

pub fn mixin_ref_from_node(node: &Node) -> Option<MixinRef> {
    if let Some(n) = node.as_constant_read_node() {
        let name = utf8_str(n.name().as_slice());
        let constant = RubyConstant::new(name).ok()?;
        return Some(MixinRef {
            parts: vec![constant],
            absolute: false,
        });
    }

    if let Some(n) = node.as_constant_path_node() {
        let mut parts = Vec::new();
        collect_namespaces(&n, &mut parts);
        return Some(MixinRef {
            parts,
            absolute: constant_path_is_absolute(&n),
        });
    }

    None
}

pub fn constant_path_is_absolute(path: &ConstantPathNode) -> bool {
    match path.parent() {
        None => true,
        Some(parent) => parent
            .as_constant_path_node()
            .is_some_and(|parent_path| constant_path_is_absolute(&parent_path)),
    }
}

pub fn build_constant_path_name(node: &Node) -> String {
    let mut parts = Vec::new();
    collect_constant_path_parts_for_name(node, &mut parts);
    parts.join("::")
}

fn collect_constant_path_parts_for_name(node: &Node, parts: &mut Vec<String>) {
    if let Some(constant_path) = node.as_constant_path_node() {
        if let Some(parent) = constant_path.parent() {
            collect_constant_path_parts_for_name(&parent, parts);
        }
        if let Some(name_bytes) = constant_path.name() {
            parts.push(utf8_str(name_bytes.as_slice()).to_string());
        }
    } else if let Some(constant_read) = node.as_constant_read_node() {
        parts.push(utf8_str(constant_read.name().as_slice()).to_string());
    }
}

#[cfg(test)]
mod tests {
    use crate::core::RubyMethod;

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
    fn nested_root_constant_path_is_absolute() {
        let parse = ruby_prism::parse(b"::Faraday::Middleware");
        let program = parse
            .node()
            .as_program_node()
            .expect("test source must parse as a program");
        let node = program
            .statements()
            .body()
            .iter()
            .next()
            .expect("test source must contain one statement");

        let reference = mixin_ref_from_node(&node).expect("constant path must be recognized");

        assert!(reference.absolute, "leading :: must remain absolute");
        assert_eq!(
            reference
                .parts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["Faraday", "Middleware"]
        );
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

    #[test]
    fn execution_context_separates_lexical_and_runtime_owners() {
        let mut tracker = ScopeTracker::new();
        let lexical = RubyConstant::new("Lexical").expect("test constant must be valid");
        let target = RubyConstant::new("Target").expect("test constant must be valid");
        tracker.push_ns_scope(lexical.clone());

        tracker.push_execution_context(
            vec![target.clone()],
            NamespaceKind::Instance,
            vec![target.clone()],
            NamespaceKind::Instance,
        );

        assert_eq!(tracker.get_ns_stack(), vec![lexical]);
        assert_eq!(
            tracker.implicit_receiver_context(),
            (vec![target.clone()], NamespaceKind::Instance)
        );
        assert_eq!(
            tracker.method_definition_context(),
            (vec![target], NamespaceKind::Instance)
        );
    }

    #[test]
    fn nested_ruby_namespace_suspends_outer_execution_context() {
        let mut tracker = ScopeTracker::new();
        let lexical = RubyConstant::new("Lexical").expect("test constant must be valid");
        let target = RubyConstant::new("Target").expect("test constant must be valid");
        let nested = RubyConstant::new("Nested").expect("test constant must be valid");
        tracker.push_ns_scope(lexical.clone());
        tracker.push_execution_context(
            vec![target],
            NamespaceKind::Instance,
            vec![RubyConstant::new("Target").expect("test constant must be valid")],
            NamespaceKind::Instance,
        );
        tracker.push_ns_scope(nested.clone());

        assert_eq!(
            tracker.method_definition_context(),
            (vec![lexical.clone(), nested], NamespaceKind::Instance)
        );

        tracker.pop_ns_scope();
        tracker.pop_execution_context();
        assert_eq!(
            tracker.implicit_receiver_context(),
            (vec![lexical], NamespaceKind::Singleton)
        );
    }

    #[test]
    fn ordinary_block_preserves_execution_context_but_method_scope_suspends_it() {
        let mut tracker = ScopeTracker::new();
        let lexical = RubyConstant::new("Lexical").expect("test constant must be valid");
        let target = RubyConstant::new("Target").expect("test constant must be valid");
        tracker.push_ns_scope(lexical.clone());
        tracker.push_execution_context(
            vec![target.clone()],
            NamespaceKind::Singleton,
            vec![target.clone()],
            NamespaceKind::Instance,
        );

        tracker.push_scope_kind(LocalScopeKind::Block);
        assert_eq!(
            tracker.implicit_receiver_context(),
            (vec![target], NamespaceKind::Singleton)
        );
        tracker.pop_scope_kind();

        tracker.push_scope_kind(LocalScopeKind::InstanceMethod);
        assert_eq!(
            tracker.implicit_receiver_context(),
            (vec![lexical.clone()], NamespaceKind::Instance)
        );
        assert_eq!(
            tracker.method_definition_context(),
            (vec![lexical], NamespaceKind::Instance)
        );
    }
}
