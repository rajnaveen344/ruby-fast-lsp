use std::collections::HashMap;

use ruby_prism::{
    visit_call_node, visit_class_node, visit_constant_write_node, visit_def_node,
    visit_module_node, visit_singleton_class_node, CallNode, ClassNode, ConstantWriteNode, DefNode,
    ModuleNode, SingletonClassNode, Visit,
};
use tower_lsp::lsp_types::SymbolKind;

use crate::{
    analyzer_prism::scope_tracker::ScopeTracker,
    capabilities::document_symbols::RubySymbolContext,
    indexer::entry::{MethodVisibility, NamespaceKind},
    types::ruby_document::RubyDocument,
};

pub struct DocumentSymbolsVisitor<'a> {
    flat_symbols: Vec<(RubySymbolContext, Option<usize>)>, // (symbol, parent_index)
    document: &'a RubyDocument,
    visibility_stack: Vec<MethodVisibility>, // Stack of visibility states for nested scopes
    scope_tracker: ScopeTracker,
    scope_to_symbol_index: HashMap<usize, usize>, // scope_id -> symbol_index
}

impl<'a> DocumentSymbolsVisitor<'a> {
    pub fn new(document: &'a RubyDocument) -> Self {
        Self {
            flat_symbols: Vec::new(),
            document,
            visibility_stack: vec![MethodVisibility::Public], // Start with public visibility
            scope_tracker: ScopeTracker::new(document),
            scope_to_symbol_index: std::collections::HashMap::new(),
        }
    }

    /// Get the current visibility (top of the stack)
    fn current_visibility(&self) -> MethodVisibility {
        *self
            .visibility_stack
            .last()
            .unwrap_or(&MethodVisibility::Public)
    }

    /// Push a new visibility scope (when entering a class/module)
    fn push_visibility_scope(&mut self) {
        // New scopes start with public visibility
        self.visibility_stack.push(MethodVisibility::Public);
    }

    /// Pop the current visibility scope (when exiting a class/module)
    fn pop_visibility_scope(&mut self) {
        if self.visibility_stack.len() > 1 {
            self.visibility_stack.pop();
        }
    }

    /// Set the visibility in the current scope
    fn set_current_visibility(&mut self, visibility: MethodVisibility) {
        if let Some(current) = self.visibility_stack.last_mut() {
            *current = visibility;
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
        // Look for the most recent container symbol in the current scope
        if let Some(current_scope) = self.scope_tracker.current_lv_scope() {
            let scope_id = current_scope.scope_id();
            self.scope_to_symbol_index.get(&scope_id).copied()
        } else {
            None
        }
    }

    fn create_symbol(
        &self,
        name: String,
        kind: SymbolKind,
        location: &ruby_prism::Location,
        namespace_kind: Option<NamespaceKind>,
    ) -> RubySymbolContext {
        let range = self.document.prism_location_to_lsp_range(location);

        RubySymbolContext {
            name,
            kind,
            detail: None, // Can be enhanced later with more detailed information
            range,
            selection_range: range,
            visibility: Some(self.current_visibility()),
            namespace_kind,
            children: Vec::new(),
        }
    }

    fn is_attr_method(&self, method_name: &str) -> bool {
        matches!(method_name, "attr_reader" | "attr_writer" | "attr_accessor")
    }

    fn is_visibility_modifier(&self, node: &CallNode) -> bool {
        let method_name = String::from_utf8_lossy(node.name().as_slice());
        matches!(method_name.as_ref(), "private" | "protected" | "public")
    }

    fn extract_attr_names(&self, _node: &CallNode) -> Vec<String> {
        // Simplified implementation - can be enhanced later
        Vec::new()
    }

    // Process methods for scope tracking and symbol creation
    fn process_class_node_entry(&mut self, node: &ClassNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Create and add symbol
        let symbol = self.create_symbol(name.clone(), SymbolKind::CLASS, &node.location(), None);
        let symbol_index = self.add_symbol_to_flat_list(symbol);

        // Handle scope tracking similar to IndexVisitor
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        // Push namespace scope
        if let Ok(namespace) = crate::types::ruby_namespace::RubyConstant::new(&name) {
            self.scope_tracker.push_ns_scope(namespace);
        }

        // Push local variable scope
        // Use the class node's start offset + 1 as the scope_id to avoid collision
        // with the top-level scope (which has scope_id = 0)
        let scope_id = node.location().start_offset() + 1;
        let lv_scope = crate::types::scope::LVScope::new(
            scope_id,
            body_loc,
            crate::types::scope::LVScopeKind::Constant,
        );
        self.scope_tracker.push_lv_scope(lv_scope);

        // Map scope to symbol for hierarchy building
        self.scope_to_symbol_index.insert(scope_id, symbol_index);

        // Push new visibility scope (classes start with public visibility)
        self.push_visibility_scope();
    }

    fn process_class_node_exit(&mut self, _node: &ClassNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
        // Pop visibility scope when exiting class
        self.pop_visibility_scope();
    }

    fn process_module_node_entry(&mut self, node: &ModuleNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Create and add symbol
        let symbol = self.create_symbol(name.clone(), SymbolKind::MODULE, &node.location(), None);
        let symbol_index = self.add_symbol_to_flat_list(symbol);

        // Handle scope tracking
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        // Push namespace scope
        if let Ok(namespace) = crate::types::ruby_namespace::RubyConstant::new(&name) {
            self.scope_tracker.push_ns_scope(namespace);
        }

        // Push local variable scope
        // Use the module node's start offset + 1 as the scope_id to avoid collision
        // with the top-level scope (which has scope_id = 0)
        let scope_id = node.location().start_offset() + 1;
        let lv_scope = crate::types::scope::LVScope::new(
            scope_id,
            body_loc,
            crate::types::scope::LVScopeKind::Constant,
        );
        self.scope_tracker.push_lv_scope(lv_scope);

        // Map scope to symbol for hierarchy building
        self.scope_to_symbol_index.insert(scope_id, symbol_index);

        // Push new visibility scope (modules start with public visibility)
        self.push_visibility_scope();
    }

    fn process_module_node_exit(&mut self, _node: &ModuleNode) {
        self.scope_tracker.pop_ns_scope();
        self.scope_tracker.pop_lv_scope();
        // Pop visibility scope when exiting module
        self.pop_visibility_scope();
    }

    fn process_singleton_class_node_entry(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
        // Push new visibility scope for singleton class (starts with public visibility)
        self.push_visibility_scope();
    }

    fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.exit_singleton();
        // Pop visibility scope when exiting singleton class
        self.pop_visibility_scope();
    }

    fn process_def_node_entry(&mut self, node: &DefNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let namespace_kind = if node.receiver().is_some() {
            Some(NamespaceKind::Singleton)
        } else if self.scope_tracker.in_singleton() {
            Some(NamespaceKind::Singleton)
        } else {
            Some(NamespaceKind::Instance)
        };

        // Create and add symbol - this will automatically find the current parent
        let symbol = self.create_symbol(name, SymbolKind::METHOD, &node.location(), namespace_kind);
        let _symbol_index = self.add_symbol_to_flat_list(symbol);

        // Push method scope
        let method_loc = self
            .document
            .prism_location_to_lsp_location(&node.location());
        let scope_id = self.document.position_to_offset(method_loc.range.start);
        let scope_kind = if node.receiver().is_some() || self.scope_tracker.in_singleton() {
            crate::types::scope::LVScopeKind::ClassMethod
        } else {
            crate::types::scope::LVScopeKind::InstanceMethod
        };
        let lv_scope = crate::types::scope::LVScope::new(scope_id, method_loc, scope_kind);
        self.scope_tracker.push_lv_scope(lv_scope);
    }

    fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_lv_scope();
    }

    fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let symbol = self.create_symbol(name, SymbolKind::CONSTANT, &node.location(), None);
        self.add_symbol_to_flat_list(symbol);
    }

    fn process_call_node_entry(&mut self, node: &CallNode) {
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        if self.is_visibility_modifier(node) {
            match method_name.as_str() {
                "private" => self.set_current_visibility(MethodVisibility::Private),
                "protected" => self.set_current_visibility(MethodVisibility::Protected),
                "public" => self.set_current_visibility(MethodVisibility::Public),
                _ => {}
            }
        } else if self.is_attr_method(&method_name) {
            let attr_names = self.extract_attr_names(node);
            for attr_name in attr_names {
                let symbol =
                    self.create_symbol(attr_name, SymbolKind::PROPERTY, &node.location(), None);
                self.add_symbol_to_flat_list(symbol);
            }
        }
    }
}

impl<'a> Visit<'a> for DocumentSymbolsVisitor<'a> {
    fn visit_class_node(&mut self, node: &ClassNode<'a>) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode<'a>) {
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
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
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode<'a>) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        self.process_singleton_class_node_exit(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    fn create_test_document(content: &str) -> RubyDocument {
        let uri = Url::parse("file:///test.rb").unwrap();
        RubyDocument::new(uri, content.to_string(), 1)
    }

    fn extract_symbols_from_content(content: &str) -> Vec<RubySymbolContext> {
        let document = create_test_document(content);
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = DocumentSymbolsVisitor::new(&document);
        visitor.visit(&node);
        visitor.symbols()
    }

    #[test]
    fn test_class_symbol_extraction() {
        let content = "class MyClass\nend";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "MyClass");
        assert_eq!(symbol.kind, SymbolKind::CLASS);
        assert_eq!(symbol.visibility, Some(MethodVisibility::Public));
        assert_eq!(symbol.namespace_kind, None);
        assert!(symbol.children.is_empty());
    }

    #[test]
    fn test_module_symbol_extraction() {
        let content = "module MyModule\nend";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "MyModule");
        assert_eq!(symbol.kind, SymbolKind::MODULE);
        assert_eq!(symbol.visibility, Some(MethodVisibility::Public));
        assert_eq!(symbol.namespace_kind, None);
        assert!(symbol.children.is_empty());
    }

    #[test]
    fn test_instance_method_symbol_extraction() {
        let content = "def my_method\nend";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "my_method");
        assert_eq!(symbol.kind, SymbolKind::METHOD);
        assert_eq!(symbol.visibility, Some(MethodVisibility::Public));
        assert_eq!(symbol.namespace_kind, Some(NamespaceKind::Instance));
        assert!(symbol.children.is_empty());
    }

    #[test]
    fn test_class_method_symbol_extraction() {
        let content = "def self.class_method\nend";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "class_method");
        assert_eq!(symbol.kind, SymbolKind::METHOD);
        assert_eq!(symbol.visibility, Some(MethodVisibility::Public));
        assert_eq!(symbol.namespace_kind, Some(NamespaceKind::Singleton));
        assert!(symbol.children.is_empty());
    }

    #[test]
    fn test_constant_symbol_extraction() {
        let content = "MY_CONSTANT = 42";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "MY_CONSTANT");
        assert_eq!(symbol.kind, SymbolKind::CONSTANT);
        assert_eq!(symbol.visibility, Some(MethodVisibility::Public));
        assert_eq!(symbol.namespace_kind, None);
        assert!(symbol.children.is_empty());
    }

    #[test]
    fn test_multiple_symbols() {
        let content = r#"
class MyClass
  MY_CONSTANT = 42

  def instance_method
  end

  def self.class_method
  end
end

module MyModule
  def module_method
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        assert!(symbols.len() >= 5);

        // Check class
        let class_symbol = symbols.iter().find(|s| s.name == "MyClass").unwrap();
        assert_eq!(class_symbol.kind, SymbolKind::CLASS);

        // Check constant
        let constant_symbol = symbols.iter().find(|s| s.name == "MY_CONSTANT").unwrap();
        assert_eq!(constant_symbol.kind, SymbolKind::CONSTANT);

        // Check instance method
        let instance_method = symbols
            .iter()
            .find(|s| s.name == "instance_method")
            .unwrap();
        assert_eq!(instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );

        // Check class method
        let class_method = symbols.iter().find(|s| s.name == "class_method").unwrap();
        assert_eq!(class_method.kind, SymbolKind::METHOD);
        assert_eq!(class_method.namespace_kind, Some(NamespaceKind::Singleton));

        // Check module
        let module_symbol = symbols.iter().find(|s| s.name == "MyModule").unwrap();
        assert_eq!(module_symbol.kind, SymbolKind::MODULE);
    }

    #[test]
    fn test_visibility_modifiers() {
        let content = r#"
class MyClass
  def public_method
  end

  private

  def private_method
  end

  protected

  def protected_method
  end

  public

  def back_to_public
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Find methods by name and check their visibility
        if let Some(public_method) = symbols.iter().find(|s| s.name == "public_method") {
            assert_eq!(public_method.visibility, Some(MethodVisibility::Public));
        }

        if let Some(private_method) = symbols.iter().find(|s| s.name == "private_method") {
            assert_eq!(private_method.visibility, Some(MethodVisibility::Private));
        }

        if let Some(protected_method) = symbols.iter().find(|s| s.name == "protected_method") {
            assert_eq!(
                protected_method.visibility,
                Some(MethodVisibility::Protected)
            );
        }

        if let Some(back_to_public) = symbols.iter().find(|s| s.name == "back_to_public") {
            assert_eq!(back_to_public.visibility, Some(MethodVisibility::Public));
        }
    }

    #[test]
    fn test_nested_class_and_module() {
        let content = r#"
module OuterModule
  class InnerClass
    def inner_method
    end
  end

  module InnerModule
    INNER_CONSTANT = "test"
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Should extract all symbols at top level (nesting not implemented yet)
        assert!(symbols.len() >= 4);

        let outer_module = symbols.iter().find(|s| s.name == "OuterModule").unwrap();
        assert_eq!(outer_module.kind, SymbolKind::MODULE);

        let inner_class = symbols.iter().find(|s| s.name == "InnerClass").unwrap();
        assert_eq!(inner_class.kind, SymbolKind::CLASS);

        let inner_method = symbols.iter().find(|s| s.name == "inner_method").unwrap();
        assert_eq!(inner_method.kind, SymbolKind::METHOD);

        let inner_constant = symbols.iter().find(|s| s.name == "INNER_CONSTANT").unwrap();
        assert_eq!(inner_constant.kind, SymbolKind::CONSTANT);
    }

    #[test]
    fn test_hierarchy_building() {
        let content = r#"
module OuterModule
  class InnerClass
    def inner_method
    end
  end

  module InnerModule
    INNER_CONSTANT = "test"
  end
end
"#;
        let document = create_test_document(content);
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = DocumentSymbolsVisitor::new(&document);
        visitor.visit(&node);
        let hierarchical_symbols = visitor.build_hierarchy();

        // Should have one root symbol (OuterModule)
        assert_eq!(hierarchical_symbols.len(), 1);

        let outer_module = &hierarchical_symbols[0];
        assert_eq!(outer_module.name, "OuterModule");
        assert_eq!(outer_module.kind, SymbolKind::MODULE);

        // OuterModule should have 2 children: InnerClass and InnerModule
        assert_eq!(outer_module.children.len(), 2);

        let inner_class = outer_module
            .children
            .iter()
            .find(|s| s.name == "InnerClass")
            .unwrap();
        assert_eq!(inner_class.kind, SymbolKind::CLASS);

        // InnerClass should have 1 child: inner_method
        assert_eq!(inner_class.children.len(), 1);
        assert_eq!(inner_class.children[0].name, "inner_method");
        assert_eq!(inner_class.children[0].kind, SymbolKind::METHOD);

        let inner_module = outer_module
            .children
            .iter()
            .find(|s| s.name == "InnerModule")
            .unwrap();
        assert_eq!(inner_module.kind, SymbolKind::MODULE);

        // InnerModule should have 1 child: INNER_CONSTANT
        assert_eq!(inner_module.children.len(), 1);
        assert_eq!(inner_module.children[0].name, "INNER_CONSTANT");
        assert_eq!(inner_module.children[0].kind, SymbolKind::CONSTANT);
    }

    #[test]
    fn test_method_with_parameters() {
        let content = r#"
def method_with_params(param1, param2 = "default", *args, **kwargs, &block)
end
"#;
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "method_with_params");
        assert_eq!(symbol.kind, SymbolKind::METHOD);
        assert_eq!(symbol.namespace_kind, Some(NamespaceKind::Instance));
    }

    #[test]
    fn test_class_with_inheritance() {
        let content = "class Child < Parent\nend";
        let symbols = extract_symbols_from_content(content);

        assert_eq!(symbols.len(), 1);
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "Child");
        assert_eq!(symbol.kind, SymbolKind::CLASS);
    }

    #[test]
    fn test_singleton_class() {
        let content = r#"
class MyClass
  class << self
    def singleton_method
    end
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Should find the class
        let class_symbol = symbols.iter().find(|s| s.name == "MyClass").unwrap();
        assert_eq!(class_symbol.kind, SymbolKind::CLASS);

        // Should find the singleton method and it should be a class method
        let singleton_method = symbols
            .iter()
            .find(|s| s.name == "singleton_method")
            .unwrap();
        assert_eq!(singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
    }

    #[test]
    fn test_module_with_singleton_class_methods() {
        let content = r#"
module A
  class << self
    def hello
    end
  end

  private

  def helloa
  end

  def self.hellob
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Should find the module
        let module_symbol = symbols.iter().find(|s| s.name == "A").unwrap();
        assert_eq!(module_symbol.kind, SymbolKind::MODULE);

        // Should find hello method inside singleton class - should be class method
        let hello_method = symbols.iter().find(|s| s.name == "hello").unwrap();
        assert_eq!(hello_method.kind, SymbolKind::METHOD);
        assert_eq!(hello_method.namespace_kind, Some(NamespaceKind::Singleton));

        // Should find helloa method - should be instance method with private visibility
        let helloa_method = symbols.iter().find(|s| s.name == "helloa").unwrap();
        assert_eq!(helloa_method.kind, SymbolKind::METHOD);
        assert_eq!(helloa_method.namespace_kind, Some(NamespaceKind::Instance));
        assert_eq!(helloa_method.visibility, Some(MethodVisibility::Private));

        // Should find hellob method with self receiver - should be class method
        let hellob_method = symbols.iter().find(|s| s.name == "hellob").unwrap();
        assert_eq!(hellob_method.kind, SymbolKind::METHOD);
        assert_eq!(hellob_method.namespace_kind, Some(NamespaceKind::Singleton));
    }

    #[test]
    fn test_protected_and_private_singleton_methods() {
        let content = r#"
class TestClass
  class << self
    def public_singleton_method
    end

    protected

    def protected_singleton_method
    end

    private

    def private_singleton_method
    end

    public

    def another_public_singleton_method
    end
  end

  def instance_method
  end

  def self.class_method_with_self
  end

  protected

  def protected_instance_method
  end

  private

  def private_instance_method
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Should find the class
        let class_symbol = symbols.iter().find(|s| s.name == "TestClass").unwrap();
        assert_eq!(class_symbol.kind, SymbolKind::CLASS);

        // Should find public_singleton_method - should be class method with public visibility
        let public_singleton_method = symbols
            .iter()
            .find(|s| s.name == "public_singleton_method")
            .unwrap();
        assert_eq!(public_singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            public_singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            public_singleton_method.visibility,
            Some(MethodVisibility::Public)
        );

        // Should find protected_singleton_method - should be class method with protected visibility
        let protected_singleton_method = symbols
            .iter()
            .find(|s| s.name == "protected_singleton_method")
            .unwrap();
        assert_eq!(protected_singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            protected_singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            protected_singleton_method.visibility,
            Some(MethodVisibility::Protected)
        );

        // Should find private_singleton_method - should be class method with private visibility
        let private_singleton_method = symbols
            .iter()
            .find(|s| s.name == "private_singleton_method")
            .unwrap();
        assert_eq!(private_singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            private_singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            private_singleton_method.visibility,
            Some(MethodVisibility::Private)
        );

        // Should find another_public_singleton_method - should be class method with public visibility
        let another_public_singleton_method = symbols
            .iter()
            .find(|s| s.name == "another_public_singleton_method")
            .unwrap();
        assert_eq!(another_public_singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            another_public_singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            another_public_singleton_method.visibility,
            Some(MethodVisibility::Public)
        );

        // Should find instance_method - should be instance method with public visibility
        let instance_method = symbols
            .iter()
            .find(|s| s.name == "instance_method")
            .unwrap();
        assert_eq!(instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );
        assert_eq!(instance_method.visibility, Some(MethodVisibility::Public));

        // Should find class_method_with_self - should be class method with public visibility
        let class_method_with_self = symbols
            .iter()
            .find(|s| s.name == "class_method_with_self")
            .unwrap();
        assert_eq!(class_method_with_self.kind, SymbolKind::METHOD);
        assert_eq!(
            class_method_with_self.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            class_method_with_self.visibility,
            Some(MethodVisibility::Public)
        );

        // Should find protected_instance_method - should be instance method with protected visibility
        let protected_instance_method = symbols
            .iter()
            .find(|s| s.name == "protected_instance_method")
            .unwrap();
        assert_eq!(protected_instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            protected_instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );
        assert_eq!(
            protected_instance_method.visibility,
            Some(MethodVisibility::Protected)
        );

        // Should find private_instance_method - should be instance method with private visibility
        let private_instance_method = symbols
            .iter()
            .find(|s| s.name == "private_instance_method")
            .unwrap();
        assert_eq!(private_instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            private_instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );
        assert_eq!(
            private_instance_method.visibility,
            Some(MethodVisibility::Private)
        );
    }

    #[test]
    fn test_private_singleton_methods() {
        let content = r#"
class TestClass
  class << self
    def singleton_method
    end

    private

    def private_singleton_method
    end
  end

  def instance_method
  end

  def self.class_method_with_self
  end

  private

  def private_instance_method
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Should find the class
        let class_symbol = symbols.iter().find(|s| s.name == "TestClass").unwrap();
        assert_eq!(class_symbol.kind, SymbolKind::CLASS);

        // Should find singleton_method - should be class method with public visibility
        let singleton_method = symbols
            .iter()
            .find(|s| s.name == "singleton_method")
            .unwrap();
        assert_eq!(singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(singleton_method.visibility, Some(MethodVisibility::Public));

        // Should find private_singleton_method - should be class method with private visibility
        let private_singleton_method = symbols
            .iter()
            .find(|s| s.name == "private_singleton_method")
            .unwrap();
        assert_eq!(private_singleton_method.kind, SymbolKind::METHOD);
        assert_eq!(
            private_singleton_method.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            private_singleton_method.visibility,
            Some(MethodVisibility::Private)
        );

        // Should find instance_method - should be instance method with public visibility
        let instance_method = symbols
            .iter()
            .find(|s| s.name == "instance_method")
            .unwrap();
        assert_eq!(instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );
        assert_eq!(instance_method.visibility, Some(MethodVisibility::Public));

        // Should find class_method_with_self - should be class method with public visibility
        let class_method_with_self = symbols
            .iter()
            .find(|s| s.name == "class_method_with_self")
            .unwrap();
        assert_eq!(class_method_with_self.kind, SymbolKind::METHOD);
        assert_eq!(
            class_method_with_self.namespace_kind,
            Some(NamespaceKind::Singleton)
        );
        assert_eq!(
            class_method_with_self.visibility,
            Some(MethodVisibility::Public)
        );

        // Should find private_instance_method - should be instance method with private visibility
        let private_instance_method = symbols
            .iter()
            .find(|s| s.name == "private_instance_method")
            .unwrap();
        assert_eq!(private_instance_method.kind, SymbolKind::METHOD);
        assert_eq!(
            private_instance_method.namespace_kind,
            Some(NamespaceKind::Instance)
        );
        assert_eq!(
            private_instance_method.visibility,
            Some(MethodVisibility::Private)
        );
    }

    #[test]
    fn test_empty_file() {
        let content = "";
        let symbols = extract_symbols_from_content(content);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_multiple_top_level_classes() {
        let content = "class Aaa\nend\n\nclass Aab\nend";
        let document = create_test_document(content);
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = DocumentSymbolsVisitor::new(&document);
        visitor.visit(&node);

        // Test flat symbols
        let flat_symbols = visitor.symbols();
        assert_eq!(flat_symbols.len(), 2, "Should have 2 flat symbols");
        assert!(
            flat_symbols.iter().any(|s| s.name == "Aaa"),
            "Should find Aaa"
        );
        assert!(
            flat_symbols.iter().any(|s| s.name == "Aab"),
            "Should find Aab"
        );

        // Test hierarchy
        let hierarchical = visitor.build_hierarchy();
        assert_eq!(
            hierarchical.len(),
            2,
            "Should have 2 root symbols in hierarchy. Got: {:?}",
            hierarchical.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            hierarchical.iter().any(|s| s.name == "Aaa"),
            "Hierarchy should contain Aaa"
        );
        assert!(
            hierarchical.iter().any(|s| s.name == "Aab"),
            "Hierarchy should contain Aab"
        );
    }

    #[test]
    fn test_comments_and_whitespace() {
        let content = r#"
# This is a comment
class MyClass # inline comment
  # Another comment
  def my_method
    # Method comment
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        assert!(!symbols.is_empty());
        let class_symbol = symbols.iter().find(|s| s.name == "MyClass").unwrap();
        assert_eq!(class_symbol.kind, SymbolKind::CLASS);

        if let Some(method_symbol) = symbols.iter().find(|s| s.name == "my_method") {
            assert_eq!(method_symbol.kind, SymbolKind::METHOD);
        }
    }

    #[test]
    fn test_symbol_ranges() {
        let content = "class MyClass\n  def my_method\n  end\nend";
        let document = create_test_document(content);
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();

        let mut visitor = DocumentSymbolsVisitor::new(&document);
        visitor.visit(&node);
        let symbols = visitor.symbols();
        assert_eq!(symbols.len(), 2);

        // Check that ranges are valid
        for symbol in &symbols {
            assert!(symbol.range.start.line <= symbol.range.end.line);
            if symbol.range.start.line == symbol.range.end.line {
                assert!(symbol.range.start.character <= symbol.range.end.character);
            }

            // Selection range should be within the full range
            assert!(symbol.selection_range.start.line >= symbol.range.start.line);
            assert!(symbol.selection_range.end.line <= symbol.range.end.line);
        }
    }

    #[test]
    fn test_nested_visibility_scoping() {
        let content = r#"
module OuterModule
  def outer_method_public
  end

  private

  def outer_method_private
  end

  class InnerClass
    def inner_method_should_be_public
    end

    private

    def inner_method_private
    end
  end

  def another_outer_method_private
  end
end
"#;
        let symbols = extract_symbols_from_content(content);

        // Find the methods
        let outer_method_public = symbols
            .iter()
            .find(|s| s.name == "outer_method_public")
            .expect("Should find outer_method_public");

        let outer_method_private = symbols
            .iter()
            .find(|s| s.name == "outer_method_private")
            .expect("Should find outer_method_private");

        let inner_method_should_be_public = symbols
            .iter()
            .find(|s| s.name == "inner_method_should_be_public")
            .expect("Should find inner_method_should_be_public");

        let inner_method_private = symbols
            .iter()
            .find(|s| s.name == "inner_method_private")
            .expect("Should find inner_method_private");

        let another_outer_method_private = symbols
            .iter()
            .find(|s| s.name == "another_outer_method_private")
            .expect("Should find another_outer_method_private");

        // Check visibility
        assert_eq!(
            outer_method_public.visibility,
            Some(MethodVisibility::Public)
        );
        assert_eq!(
            outer_method_private.visibility,
            Some(MethodVisibility::Private)
        );
        assert_eq!(
            inner_method_should_be_public.visibility,
            Some(MethodVisibility::Public)
        ); // This should be public because it's in a new class scope
        assert_eq!(
            inner_method_private.visibility,
            Some(MethodVisibility::Private)
        );
        assert_eq!(
            another_outer_method_private.visibility,
            Some(MethodVisibility::Private)
        ); // This should remain private in the outer module
    }

    #[test]
    fn test_complex_nested_visibility_scoping() {
        let source = r#"
class OuterClass
  def public_method
  end

  private

  def private_method
  end

  module NestedModule
    def module_method_public
    end

    protected

    def module_method_protected
    end

    class DeeplyNestedClass
      def deeply_nested_public
      end

      private

      def deeply_nested_private
      end
    end

    def another_module_method_protected
    end
  end

  def outer_private_method
  end

  public

  def outer_public_again
  end
end
"#;
        let symbols = extract_symbols_from_content(source);

        // Helper function to find method by name
        let find_method = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("Should find {}", name))
        };

        // Check outer class methods
        assert_eq!(
            find_method("public_method").visibility,
            Some(MethodVisibility::Public)
        );
        assert_eq!(
            find_method("private_method").visibility,
            Some(MethodVisibility::Private)
        );
        assert_eq!(
            find_method("outer_private_method").visibility,
            Some(MethodVisibility::Private)
        );
        assert_eq!(
            find_method("outer_public_again").visibility,
            Some(MethodVisibility::Public)
        );

        // Check nested module methods (should start fresh with public)
        assert_eq!(
            find_method("module_method_public").visibility,
            Some(MethodVisibility::Public)
        );
        assert_eq!(
            find_method("module_method_protected").visibility,
            Some(MethodVisibility::Protected)
        );
        assert_eq!(
            find_method("another_module_method_protected").visibility,
            Some(MethodVisibility::Protected)
        );

        // Check deeply nested class methods (should start fresh with public)
        assert_eq!(
            find_method("deeply_nested_public").visibility,
            Some(MethodVisibility::Public)
        );
        assert_eq!(
            find_method("deeply_nested_private").visibility,
            Some(MethodVisibility::Private)
        );
    }

    #[test]
    fn test_complex_ruby_constructs() {
        let content = r#"
module Namespace
  class Base
    CONSTANT = "value"

    def initialize
    end

    private

    def private_helper
    end
  end

  class Derived < Base
    def self.factory_method
    end

    def override_method
    end
  end
end

def top_level_method
end

TOP_LEVEL_CONSTANT = 42
"#;
        let symbols = extract_symbols_from_content(content);

        // Should find all symbols
        assert!(symbols.len() >= 8);

        // Verify we have all expected symbol types
        let has_module = symbols.iter().any(|s| s.kind == SymbolKind::MODULE);
        let has_class = symbols.iter().any(|s| s.kind == SymbolKind::CLASS);
        let has_method = symbols.iter().any(|s| s.kind == SymbolKind::METHOD);
        let has_constant = symbols.iter().any(|s| s.kind == SymbolKind::CONSTANT);

        assert!(has_module);
        assert!(has_class);
        assert!(has_method);
        assert!(has_constant);
    }
}
