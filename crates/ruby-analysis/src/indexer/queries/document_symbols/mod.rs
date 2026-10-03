use std::collections::HashMap;

use crate::core::{MethodVisibility, NamespaceKind, RubyConstant, SourceRange};
use crate::indexer::documents::scope_rules::{visibility_call, VisibilityCall};
use crate::indexer::{LVScopeKind, RubyDocument, ScopeTracker};
use ruby_prism::{
    visit_call_node, visit_class_node, visit_constant_write_node, visit_def_node,
    visit_module_node, visit_singleton_class_node, CallNode, ClassNode, ConstantWriteNode, DefNode,
    ModuleNode, SingletonClassNode, Visit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSymbolKind {
    Module,
    Class,
    Method,
    Constant,
    Property,
}

impl DocumentSymbolKind {
    const MODULE: Self = Self::Module;
    const CLASS: Self = Self::Class;
    const METHOD: Self = Self::Method;
    const CONSTANT: Self = Self::Constant;
    const PROPERTY: Self = Self::Property;
}

use DocumentSymbolKind as SymbolKind;

/// Internal representation of a Ruby symbol with additional context.
#[derive(Debug, Clone)]
pub struct RubySymbolContext {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: Option<String>,
    pub range: SourceRange,
    pub selection_range: SourceRange,
    pub children: Vec<RubySymbolContext>,
    pub visibility: Option<MethodVisibility>,
    pub namespace_kind: Option<NamespaceKind>,
}

pub struct DocumentSymbolsVisitor<'a> {
    flat_symbols: Vec<(RubySymbolContext, Option<usize>)>, // (symbol, parent_index)
    document: &'a RubyDocument,
    scope_tracker: ScopeTracker,
    scope_to_symbol_index: HashMap<usize, usize>, // scope_id -> symbol_index
    scope_id_stack: Vec<usize>,                   // Stack of scope_ids for hierarchy building
}

impl<'a> DocumentSymbolsVisitor<'a> {
    pub fn new(document: &'a RubyDocument) -> Self {
        Self {
            flat_symbols: Vec::new(),
            document,
            scope_tracker: ScopeTracker::new(),
            scope_to_symbol_index: std::collections::HashMap::new(),
            scope_id_stack: Vec::new(),
        }
    }

    pub fn symbols(&self) -> Vec<RubySymbolContext> {
        // Return a flat list of all symbols for backward compatibility with tests
        self.flat_symbols
            .iter()
            .map(|(symbol, _)| symbol.clone())
            .collect()
    }

    pub fn build_hierarchy(&self) -> Vec<RubySymbolContext> {
        // Build hierarchy from flat symbols
        let mut symbol_map: std::collections::HashMap<usize, RubySymbolContext> =
            std::collections::HashMap::new();

        // First pass: create all symbols with empty children
        for (index, (symbol, _parent_index)) in self.flat_symbols.iter().enumerate() {
            let mut symbol_copy = symbol.clone();
            symbol_copy.children.clear(); // Ensure children start empty
            symbol_map.insert(index, symbol_copy);
        }

        // Second pass: build hierarchy by adding children to parents
        // We need to do this in reverse order to ensure children are fully built before being added to parents
        for (index, (_symbol, parent_index)) in self.flat_symbols.iter().enumerate().rev() {
            if let Some(parent_idx) = parent_index {
                // Get the child symbol (with its own children) and add it to parent's children
                if let Some(child_symbol) = symbol_map.remove(&index) {
                    if let Some(parent_symbol) = symbol_map.get_mut(parent_idx) {
                        parent_symbol.children.push(child_symbol);
                    } else {
                        // Put the child back if parent not found
                        symbol_map.insert(index, child_symbol);
                    }
                }
            }
        }

        // Third pass: collect root symbols (those without parents)
        let mut root_symbols = Vec::new();
        for (index, (_symbol, parent_index)) in self.flat_symbols.iter().enumerate() {
            if parent_index.is_none() {
                if let Some(root_symbol) = symbol_map.get(&index) {
                    root_symbols.push(root_symbol.clone());
                }
            }
        }

        root_symbols
    }

    fn add_symbol_to_flat_list(&mut self, symbol: RubySymbolContext) -> usize {
        // Find parent index based on current scope
        let parent_index = self.find_current_parent_index();
        let symbol_index = self.flat_symbols.len();
        self.flat_symbols.push((symbol, parent_index));
        symbol_index
    }

    fn find_current_parent_index(&self) -> Option<usize> {
        // Use the scope_id_stack to find the current innermost scope
        let scope_id = self.scope_id_stack.last()?;
        self.scope_to_symbol_index.get(scope_id).copied()
    }

    fn create_symbol(
        &self,
        name: String,
        kind: SymbolKind,
        location: &ruby_prism::Location,
        namespace_kind: Option<NamespaceKind>,
    ) -> RubySymbolContext {
        let range = self.document.prism_location_to_source_range(location);

        RubySymbolContext {
            name,
            kind,
            detail: None, // Can be enhanced later with more detailed information
            range,
            selection_range: range,
            visibility: Some(self.scope_tracker.current_visibility()),
            namespace_kind,
            children: Vec::new(),
        }
    }

    fn is_attr_method(&self, method_name: &str) -> bool {
        matches!(method_name, "attr_reader" | "attr_writer" | "attr_accessor")
    }

    fn extract_attr_names(&self, _node: &CallNode) -> Vec<String> {
        // Simplified implementation - can be enhanced later
        Vec::new()
    }

    /// Push the namespace frame, with its public visibility, that a class or
    /// module body opens. Malformed source such as `class foo` has no
    /// constant name and opens no frame, so its exit must not pop one.
    fn push_namespace_scope(&mut self, name: &str) -> bool {
        let Ok(namespace) = RubyConstant::new(name) else {
            return false;
        };
        self.scope_tracker.push_ns_scope(namespace);
        true
    }

    // Process methods for scope tracking and symbol creation
    fn process_class_node_entry(&mut self, node: &ClassNode) -> bool {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Create and add symbol
        let symbol = self.create_symbol(name.clone(), SymbolKind::CLASS, &node.location(), None);
        let symbol_index = self.add_symbol_to_flat_list(symbol);

        let pushed = self.push_namespace_scope(&name);

        // Push local variable scope kind
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        // Map scope to symbol for hierarchy building
        let scope_id = node.location().start_offset() + 1;
        self.scope_to_symbol_index.insert(scope_id, symbol_index);
        self.scope_id_stack.push(scope_id);
        pushed
    }

    fn process_class_node_exit(&mut self, pushed: bool) {
        if pushed {
            self.scope_tracker.pop_ns_scope();
        }
        self.scope_tracker.pop_scope_kind();
        self.scope_id_stack.pop();
    }

    fn process_module_node_entry(&mut self, node: &ModuleNode) -> bool {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Create and add symbol
        let symbol = self.create_symbol(name.clone(), SymbolKind::MODULE, &node.location(), None);
        let symbol_index = self.add_symbol_to_flat_list(symbol);

        let pushed = self.push_namespace_scope(&name);

        // Push local variable scope kind
        self.scope_tracker.push_scope_kind(LVScopeKind::Constant);

        // Map scope to symbol for hierarchy building
        let scope_id = node.location().start_offset() + 1;
        self.scope_to_symbol_index.insert(scope_id, symbol_index);
        self.scope_id_stack.push(scope_id);
        pushed
    }

    fn process_module_node_exit(&mut self, pushed: bool) {
        if pushed {
            self.scope_tracker.pop_ns_scope();
        }
        self.scope_tracker.pop_scope_kind();
        self.scope_id_stack.pop();
    }

    fn process_singleton_class_node_entry(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
    }

    fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.exit_singleton();
    }

    fn process_def_node_entry(&mut self, node: &DefNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let namespace_kind = if node.receiver().is_some() || self.scope_tracker.in_singleton() {
            Some(NamespaceKind::Singleton)
        } else {
            Some(NamespaceKind::Instance)
        };

        // Create and add symbol - this will automatically find the current parent
        let symbol = self.create_symbol(name, SymbolKind::METHOD, &node.location(), namespace_kind);
        let _symbol_index = self.add_symbol_to_flat_list(symbol);

        // Push method scope kind
        let scope_kind = if node.receiver().is_some() || self.scope_tracker.in_singleton() {
            LVScopeKind::ClassMethod
        } else {
            LVScopeKind::InstanceMethod
        };
        self.scope_tracker.push_scope_kind(scope_kind);
    }

    fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_scope_kind();
    }

    fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let symbol = self.create_symbol(name, SymbolKind::CONSTANT, &node.location(), None);
        self.add_symbol_to_flat_list(symbol);
    }

    fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        if node.receiver().is_none() {
            if let Some(VisibilityCall::Default(visibility)) = visibility_call(node) {
                self.scope_tracker.set_current_visibility(visibility);
                return;
            }
        }
        if self.is_attr_method(&method_name) {
            let attr_names = self.extract_attr_names(node);
            for attr_name in attr_names {
                let symbol =
                    self.create_symbol(attr_name, SymbolKind::PROPERTY, &node.location(), None);
                self.add_symbol_to_flat_list(symbol);
            }
        }
    }

    /// Give the methods a visibility call names that visibility once its
    /// arguments, including a `def`, have been visited.
    fn process_call_node_exit(&mut self, node: &CallNode) {
        if node.receiver().is_some() {
            return;
        }
        let Some(VisibilityCall::Methods(visibility, methods)) = visibility_call(node) else {
            return;
        };
        let side = if self.scope_tracker.in_singleton() {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        };
        let parent = self.find_current_parent_index();
        for (symbol, parent_index) in &mut self.flat_symbols {
            if *parent_index == parent
                && symbol.kind == SymbolKind::METHOD
                && symbol.namespace_kind == Some(side)
                && methods.iter().any(|(name, _)| *name == symbol.name)
            {
                symbol.visibility = Some(visibility);
            }
        }
    }
}

impl<'a> Visit<'a> for DocumentSymbolsVisitor<'a> {
    fn visit_class_node(&mut self, node: &ClassNode<'a>) {
        let pushed = self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(pushed);
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'a>) {
        let pushed = self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(pushed);
    }

    fn visit_def_node(&mut self, node: &DefNode<'a>) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode<'a>) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
    }

    fn visit_call_node(&mut self, node: &CallNode<'a>) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode<'a>) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        self.process_singleton_class_node_exit(node);
    }
}

#[cfg(test)]
mod tests;
