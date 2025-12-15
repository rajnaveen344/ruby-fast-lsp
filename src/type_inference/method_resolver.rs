//! Method Resolution for Type Inference
//!
//! Resolves method calls to their return types by:
//! 1. Determining the receiver type
//! 2. Looking up the method in the index
//! 3. Falling back to RBS type definitions for built-in classes
//! 4. Returning the method's return type

use parking_lot::Mutex;
use ruby_prism::*;
use std::sync::Arc;

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::indexer::index::RubyIndex;
use crate::type_inference::literal_analyzer::LiteralAnalyzer;
use crate::type_inference::rbs_index::get_rbs_method_return_type_as_ruby_type;
use crate::type_inference::ruby_type::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

/// Resolves method calls to their return types
pub struct MethodResolver {
    index: Arc<Mutex<RubyIndex>>,
    literal_analyzer: LiteralAnalyzer,
    /// Current namespace context (for resolving 'self')
    current_namespace: Vec<RubyConstant>,
}

impl MethodResolver {
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            current_namespace: Vec::new(),
        }
    }

    /// Create a MethodResolver with namespace context for resolving 'self'
    pub fn with_namespace(index: Arc<Mutex<RubyIndex>>, namespace: Vec<RubyConstant>) -> Self {
        log::debug!(
            "MethodResolver::with_namespace called with: {:?}",
            namespace
        );
        Self {
            index,
            literal_analyzer: LiteralAnalyzer::new(),
            current_namespace: namespace,
        }
    }

    /// Static method to resolve return type given an index, receiver type, and method name.
    /// Useful when you don't have a MethodResolver instance.
    pub fn resolve_method_return_type(
        index: &RubyIndex,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        // Handle class reference calling .new
        if method_name == "new" {
            if let RubyType::ClassReference(fqn) = receiver_type {
                // .new returns an instance of the class
                return Some(RubyType::Class(fqn.clone()));
            }
        }

        // Get the class name for RBS lookup
        let class_name = Self::get_class_name_for_rbs_static(receiver_type);

        // Determine if this is a singleton (class) method call
        let is_singleton = matches!(
            receiver_type,
            RubyType::ClassReference(_) | RubyType::ModuleReference(_)
        );

        // Get the class/module FQN from the receiver type
        let owner_fqn = match receiver_type {
            RubyType::Class(fqn) => Some(fqn.clone()),
            RubyType::ClassReference(fqn) => Some(fqn.clone()),
            RubyType::Module(fqn) => Some(fqn.clone()),
            RubyType::ModuleReference(fqn) => Some(fqn.clone()),
            _ => None,
        };

        // Try to look up in Ruby index first (for user-defined methods)
        if let Some(owner_fqn) = owner_fqn {
            let method_kind = if is_singleton {
                MethodKind::Class
            } else {
                MethodKind::Instance
            };

            if let Ok(ruby_method) = RubyMethod::new(method_name, method_kind) {
                if let Some(entries) = index.get_methods_by_name(&ruby_method) {
                    for entry in entries {
                        if let EntryKind::Method(data) = &entry.kind {
                            let owner = &data.owner;
                            let return_type = &data.return_type;
                            if *owner == owner_fqn {
                                if let Some(rt) = return_type {
                                    return Some(rt.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fall back to RBS type definitions for built-in methods
        get_rbs_method_return_type_as_ruby_type(class_name.as_deref()?, method_name, is_singleton)
    }

    /// Static helper to get class name for RBS lookup
    fn get_class_name_for_rbs_static(ruby_type: &RubyType) -> Option<String> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                if let FullyQualifiedName::Constant(parts) = fqn {
                    parts.last().map(|c| c.to_string())
                } else {
                    None
                }
            }
            RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => {
                if let FullyQualifiedName::Constant(parts) = fqn {
                    parts.last().map(|c| c.to_string())
                } else {
                    None
                }
            }
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            RubyType::Union(_) => None,
            RubyType::Unknown => None,
        }
    }

    /// Resolve the return type of a method call
    pub fn resolve_call_type(&self, call_node: &CallNode) -> Option<RubyType> {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

        // Get receiver type
        let receiver_type = self.resolve_receiver_type(call_node.receiver())?;
        log::debug!(
            "resolve_call_type: method={}, receiver_type={:?}",
            method_name,
            receiver_type
        );

        // Look up method and get its return type
        let result = self.lookup_method_return_type(&receiver_type, &method_name);
        log::debug!("resolve_call_type: result={:?}", result);
        result
    }

    /// Resolve the type of a receiver expression
    fn resolve_receiver_type(&self, receiver: Option<Node>) -> Option<RubyType> {
        let receiver = receiver?;

        // Try literal analysis first
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(&receiver) {
            return Some(literal_type);
        }

        // Handle constant read (e.g., User.new)
        if let Some(const_read) = receiver.as_constant_read_node() {
            let const_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            // This is a class/module reference, not an instance
            return Some(RubyType::ClassReference(FullyQualifiedName::Constant(
                vec![RubyConstant::new(&const_name).ok()?],
            )));
        }

        // Handle constant path (e.g., Foo::Bar.new)
        if let Some(const_path) = receiver.as_constant_path_node() {
            let fqn = self.resolve_constant_path(&const_path)?;
            return Some(RubyType::ClassReference(fqn));
        }

        // Handle self - resolve to current class if we have namespace context
        let is_self = receiver.as_self_node().is_some();
        if is_self {
            log::debug!(
                "Resolving self receiver with namespace context: {:?}",
                self.current_namespace
            );
            if !self.current_namespace.is_empty() {
                // Self is an instance of the current class/module
                let fqn = FullyQualifiedName::Constant(self.current_namespace.clone());
                log::debug!("Self resolved to: {:?}", fqn);
                return Some(RubyType::Class(fqn));
            }
            // No namespace context, fall back to Unknown
            log::debug!("Self has no namespace context, returning Unknown");
            return Some(RubyType::Unknown);
        }

        // Handle local variable read
        if let Some(local_var) = receiver.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
            return self.lookup_local_variable_type(&var_name);
        }

        // Handle instance variable read
        if let Some(ivar) = receiver.as_instance_variable_read_node() {
            let var_name = String::from_utf8_lossy(ivar.name().as_slice()).to_string();
            return self.lookup_instance_variable_type(&var_name);
        }

        // Handle chained method calls (e.g., user.profile.name)
        if let Some(call) = receiver.as_call_node() {
            return self.resolve_call_type(&call);
        }

        // Handle parenthesized expressions
        if let Some(parens) = receiver.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.resolve_receiver_type(Some(body));
            }
        }

        None
    }

    /// Resolve a constant path to an FQN
    fn resolve_constant_path(&self, const_path: &ConstantPathNode) -> Option<FullyQualifiedName> {
        let mut parts = Vec::new();

        // Get the child constant name
        if let Some(name_node) = const_path.name() {
            let name = String::from_utf8_lossy(name_node.as_slice()).to_string();
            parts.push(RubyConstant::new(&name).ok()?);
        }

        // Get parent parts
        if let Some(parent) = const_path.parent() {
            if let Some(parent_path) = parent.as_constant_path_node() {
                if let Some(parent_fqn) = self.resolve_constant_path(&parent_path) {
                    if let FullyQualifiedName::Constant(parent_parts) = parent_fqn {
                        let mut full_parts = parent_parts;
                        full_parts.extend(parts);
                        return Some(FullyQualifiedName::Constant(full_parts));
                    }
                }
            } else if let Some(const_read) = parent.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                let mut full_parts = vec![RubyConstant::new(&parent_name).ok()?];
                full_parts.extend(parts);
                return Some(FullyQualifiedName::Constant(full_parts));
            }
        } else {
            // No parent means this is a top-level constant
            return Some(FullyQualifiedName::Constant(parts));
        }

        None
    }

    /// Look up a method's return type given receiver type and method name
    fn lookup_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        // Handle class reference calling .new
        if method_name == "new" {
            if let RubyType::ClassReference(fqn) = receiver_type {
                // .new returns an instance of the class
                return Some(RubyType::Class(fqn.clone()));
            }
        }

        // Get the class name for RBS lookup
        let class_name = self.get_class_name_for_rbs(receiver_type);

        // Determine if this is a singleton (class) method call
        let is_singleton = matches!(
            receiver_type,
            RubyType::ClassReference(_) | RubyType::ModuleReference(_)
        );

        // Get the class/module FQN from the receiver type
        let owner_fqn = match receiver_type {
            RubyType::Class(fqn) => Some(fqn.clone()),
            RubyType::ClassReference(fqn) => Some(fqn.clone()),
            RubyType::Module(fqn) => Some(fqn.clone()),
            RubyType::ModuleReference(fqn) => Some(fqn.clone()),
            // For built-in types without FQN, we'll use RBS
            _ => None,
        };

        // Try to look up in Ruby index first (for user-defined methods)
        if let Some(owner_fqn) = owner_fqn {
            // Determine method kind based on receiver
            let method_kind = if is_singleton {
                MethodKind::Class
            } else {
                MethodKind::Instance
            };

            // Look up the method in the index
            let index = self.index.lock();

            // Get the ancestor chain for this class/module
            let ancestors = crate::indexer::ancestor_chain::get_ancestor_chain(
                &index,
                &owner_fqn,
                is_singleton,
            );

            // Search through the ancestor chain for the method
            if let Ok(ruby_method) = RubyMethod::new(method_name, method_kind) {
                if let Some(entries) = index.get_methods_by_name(&ruby_method) {
                    // Find method that belongs to any class in the ancestor chain
                    for entry in entries {
                        if let EntryKind::Method(data) = &entry.kind {
                            let owner = &data.owner;
                            let return_type = &data.return_type;
                            // Check if owner is in ancestor chain
                            if *owner == owner_fqn || ancestors.contains(owner) {
                                if let Some(rt) = return_type {
                                    return Some(rt.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Try instance method if class method not found (and vice versa)
            let alt_kind = match method_kind {
                MethodKind::Class => MethodKind::Instance,
                MethodKind::Instance => MethodKind::Class,
                MethodKind::Unknown => {
                    return self.lookup_rbs_method(class_name.as_deref(), method_name, is_singleton)
                }
            };

            if let Ok(ruby_method) = RubyMethod::new(method_name, alt_kind) {
                if let Some(entries) = index.get_methods_by_name(&ruby_method) {
                    for entry in entries {
                        if let EntryKind::Method(data) = &entry.kind {
                            let owner = &data.owner;
                            let return_type = &data.return_type;
                            // Check if owner is in ancestor chain
                            if *owner == owner_fqn || ancestors.contains(owner) {
                                if let Some(rt) = return_type {
                                    return Some(rt.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fall back to RBS type definitions for built-in methods
        self.lookup_rbs_method(class_name.as_deref(), method_name, is_singleton)
    }

    /// Get the class name for RBS lookup from a RubyType
    fn get_class_name_for_rbs(&self, ruby_type: &RubyType) -> Option<String> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                // Extract the class name from FQN
                if let FullyQualifiedName::Constant(parts) = fqn {
                    parts.last().map(|c| c.to_string())
                } else {
                    None
                }
            }
            RubyType::Module(fqn) | RubyType::ModuleReference(fqn) => {
                if let FullyQualifiedName::Constant(parts) = fqn {
                    parts.last().map(|c| c.to_string())
                } else {
                    None
                }
            }
            // Built-in types map to their class names
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            RubyType::Union(_) => None, // Can't lookup methods on union types directly
            RubyType::Unknown => None,
        }
    }

    /// Look up a method's return type from RBS definitions
    fn lookup_rbs_method(
        &self,
        class_name: Option<&str>,
        method_name: &str,
        is_singleton: bool,
    ) -> Option<RubyType> {
        let class_name = class_name?;
        log::debug!(
            "Looking up RBS method: {}{}{}",
            class_name,
            if is_singleton { "." } else { "#" },
            method_name
        );
        get_rbs_method_return_type_as_ruby_type(class_name, method_name, is_singleton)
    }

    /// Look up a local variable's type from the index
    fn lookup_local_variable_type(&self, var_name: &str) -> Option<RubyType> {
        log::debug!("Looking up local variable type for: {}", var_name);
        let index = self.index.lock();

        // Search through all definitions for local variables with matching name
        for (fqn, entries) in index.definitions() {
            if let FullyQualifiedName::LocalVariable(name, _) = fqn {
                if name == var_name {
                    log::debug!("Found local variable {} in index", var_name);
                    for entry in entries {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            let r#type = &data.r#type;
                            log::debug!("Variable {} has type: {:?}", var_name, r#type);
                            if *r#type != RubyType::Unknown {
                                return Some(r#type.clone());
                            }
                        }
                    }
                }
            }
        }

        log::debug!("Local variable {} not found in index", var_name);
        None
    }

    /// Look up an instance variable's type from the index
    fn lookup_instance_variable_type(&self, var_name: &str) -> Option<RubyType> {
        let index = self.index.lock();

        // Search through all definitions for instance variables with matching name
        for (fqn, entries) in index.definitions() {
            if let FullyQualifiedName::InstanceVariable(name) = fqn {
                if name == var_name {
                    for entry in entries {
                        if let EntryKind::InstanceVariable(data) = &entry.kind {
                            let r#type = &data.r#type;
                            if *r#type != RubyType::Unknown {
                                return Some(r#type.clone());
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::entry::entry_builder::EntryBuilder;
    use crate::indexer::entry::{MethodKind, MethodOrigin, MethodVisibility};
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn create_test_index() -> Arc<Mutex<RubyIndex>> {
        Arc::new(Mutex::new(RubyIndex::new()))
    }

    fn create_test_location() -> Location {
        Location {
            uri: Url::parse("file:///test.rb").unwrap(),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 10),
            },
        }
    }

    #[test]
    fn test_method_resolver_creation() {
        let index = create_test_index();
        let _resolver = MethodResolver::new(index);
        // Just verify it can be created
        assert!(true);
    }

    #[test]
    fn test_class_new_returns_instance() {
        let index = create_test_index();
        let resolver = MethodResolver::new(index);

        // Test that calling .new on a class reference returns an instance
        let class_ref =
            RubyType::ClassReference(FullyQualifiedName::Constant(vec![RubyConstant::new(
                "User",
            )
            .unwrap()]));

        let result = resolver.lookup_method_return_type(&class_ref, "new");

        assert!(result.is_some());
        if let Some(RubyType::Class(fqn)) = result {
            assert_eq!(
                fqn,
                FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()])
            );
        } else {
            panic!("Expected Class type");
        }
    }

    #[test]
    fn test_lookup_method_with_return_type() {
        let index = create_test_index();

        // Add a User class with a name method that returns String
        let user_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()]);

        let method_entry = EntryBuilder::new()
            .fqn(FullyQualifiedName::method(
                vec![RubyConstant::new("User").unwrap()],
                RubyMethod::new("name", MethodKind::Instance).unwrap(),
            ))
            .location(create_test_location())
            .kind(EntryKind::new_method(
                RubyMethod::new("name", MethodKind::Instance).unwrap(),
                vec![],
                user_fqn.clone(),
                MethodVisibility::Public,
                MethodOrigin::Direct,
                None,
                None,
                None,
                Some(RubyType::string()),
                vec![],
            ))
            .build()
            .unwrap();

        {
            let mut idx = index.lock();
            idx.add_entry(method_entry);
        }

        let resolver = MethodResolver::new(index);

        // Test looking up the name method on a User instance
        let user_instance = RubyType::Class(user_fqn);
        let result = resolver.lookup_method_return_type(&user_instance, "name");

        assert!(result.is_some(), "Should find name method");
        assert_eq!(result.unwrap(), RubyType::string());
    }

    #[test]
    fn test_lookup_class_method() {
        let index = create_test_index();

        // Add a User class with a find class method that returns User
        let user_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()]);

        let method_entry = EntryBuilder::new()
            .fqn(FullyQualifiedName::method(
                vec![RubyConstant::new("User").unwrap()],
                RubyMethod::new("find", MethodKind::Class).unwrap(),
            ))
            .location(create_test_location())
            .kind(EntryKind::new_method(
                RubyMethod::new("find", MethodKind::Class).unwrap(),
                vec![],
                user_fqn.clone(),
                MethodVisibility::Public,
                MethodOrigin::Direct,
                None,
                None,
                None,
                Some(RubyType::Class(user_fqn.clone())),
                vec![],
            ))
            .build()
            .unwrap();

        {
            let mut idx = index.lock();
            idx.add_entry(method_entry);
        }

        let resolver = MethodResolver::new(index);

        // Test looking up the find class method on User class reference
        let user_class_ref = RubyType::ClassReference(user_fqn.clone());
        let result = resolver.lookup_method_return_type(&user_class_ref, "find");

        assert!(result.is_some(), "Should find find class method");
        if let Some(RubyType::Class(fqn)) = result {
            assert_eq!(fqn, user_fqn);
        } else {
            panic!("Expected Class type for find result");
        }
    }

    #[test]
    fn test_lookup_nonexistent_method() {
        let index = create_test_index();
        let resolver = MethodResolver::new(index);

        let user_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()]);
        let user_instance = RubyType::Class(user_fqn);

        let result = resolver.lookup_method_return_type(&user_instance, "nonexistent");
        assert!(result.is_none(), "Should not find nonexistent method");
    }

    #[test]
    fn test_lookup_method_without_return_type() {
        let index = create_test_index();

        // Add a method without a return type
        let user_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()]);

        let method_entry = EntryBuilder::new()
            .fqn(FullyQualifiedName::method(
                vec![RubyConstant::new("User").unwrap()],
                RubyMethod::new("unknown_return", MethodKind::Instance).unwrap(),
            ))
            .location(create_test_location())
            .kind(EntryKind::new_method(
                RubyMethod::new("unknown_return", MethodKind::Instance).unwrap(),
                vec![],
                user_fqn.clone(),
                MethodVisibility::Public,
                MethodOrigin::Direct,
                None,
                None,
                None,
                None, // No return type
                vec![],
            ))
            .build()
            .unwrap();

        {
            let mut idx = index.lock();
            idx.add_entry(method_entry);
        }

        let resolver = MethodResolver::new(index);

        let user_instance = RubyType::Class(user_fqn);
        let result = resolver.lookup_method_return_type(&user_instance, "unknown_return");

        assert!(
            result.is_none(),
            "Should return None for method without return type"
        );
    }

    #[test]
    fn test_nested_class_new() {
        let index = create_test_index();
        let resolver = MethodResolver::new(index);

        // Test that Foo::Bar.new returns Foo::Bar instance
        let nested_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("Bar").unwrap(),
        ]);

        let class_ref = RubyType::ClassReference(nested_fqn.clone());
        let result = resolver.lookup_method_return_type(&class_ref, "new");

        assert!(result.is_some());
        if let Some(RubyType::Class(fqn)) = result {
            assert_eq!(fqn, nested_fqn);
        } else {
            panic!("Expected Class type for nested class");
        }
    }

    #[test]
    fn test_lookup_local_variable_type() {
        let index = create_test_index();

        // Add a local variable with a known type
        use crate::types::scope::LVScopeStack;

        let scope_stack = LVScopeStack::default();
        let var_fqn =
            FullyQualifiedName::local_variable("user".to_string(), scope_stack.clone()).unwrap();

        let var_entry = EntryBuilder::new()
            .fqn(var_fqn)
            .location(create_test_location())
            .kind(EntryKind::new_local_variable(
                "user".to_string(),
                scope_stack,
                RubyType::Class(FullyQualifiedName::Constant(vec![RubyConstant::new(
                    "User",
                )
                .unwrap()])),
            ))
            .build()
            .unwrap();

        {
            let mut idx = index.lock();
            idx.add_entry(var_entry);
        }

        let resolver = MethodResolver::new(index);

        let result = resolver.lookup_local_variable_type("user");
        assert!(result.is_some(), "Should find local variable 'user'");

        if let Some(RubyType::Class(fqn)) = result {
            assert_eq!(
                fqn,
                FullyQualifiedName::Constant(vec![RubyConstant::new("User").unwrap()])
            );
        } else {
            panic!("Expected Class type for user variable");
        }
    }

    #[test]
    fn test_lookup_instance_variable_type() {
        let index = create_test_index();

        // Add an instance variable with a known type
        let ivar_fqn = FullyQualifiedName::instance_variable("@name".to_string()).unwrap();

        let ivar_entry = EntryBuilder::new()
            .fqn(ivar_fqn)
            .location(create_test_location())
            .kind(EntryKind::new_instance_variable(
                "@name".to_string(),
                RubyType::string(),
            ))
            .build()
            .unwrap();

        {
            let mut idx = index.lock();
            idx.add_entry(ivar_entry);
        }

        let resolver = MethodResolver::new(index);

        let result = resolver.lookup_instance_variable_type("@name");
        assert!(result.is_some(), "Should find instance variable '@name'");
        assert_eq!(result.unwrap(), RubyType::string());
    }
}
