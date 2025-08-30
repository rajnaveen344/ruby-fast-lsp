use crate::types::ruby_variable::{RubyVariable, RubyVariableType};
use super::ruby_type::RubyType;
use tower_lsp::lsp_types::Location;

/// Represents a variable with its inferred type information
#[derive(Debug, Clone, PartialEq)]
pub struct TypedVariable {
    /// The variable information (name, type, etc.)
    pub variable: RubyVariable,
    /// The inferred type of the variable
    pub ruby_type: RubyType,
    /// Whether this type was explicitly declared or inferred
    pub is_explicit: bool,
    /// Source location where this type was determined
    pub source_location: Option<Location>,
}

/// Context for variable type tracking across scopes
#[derive(Debug, Clone, PartialEq)]
pub struct VariableTypeContext {
    /// Variables in the current scope with their types
    pub local_variables: Vec<TypedVariable>,
    /// Instance variables accessible in this context
    pub instance_variables: Vec<TypedVariable>,
    /// Class variables accessible in this context
    pub class_variables: Vec<TypedVariable>,
    /// Global variables accessible in this context
    pub global_variables: Vec<TypedVariable>,
}

impl TypedVariable {
    /// Create a new typed variable with inferred type
    pub fn new_inferred(
        variable: RubyVariable,
        ruby_type: RubyType,
        source_location: Option<Location>,
    ) -> Self {
        Self {
            variable,
            ruby_type,
            is_explicit: false,
            source_location,
        }
    }
    
    /// Create a new typed variable with explicit type declaration
    pub fn new_explicit(
        variable: RubyVariable,
        ruby_type: RubyType,
        source_location: Option<Location>,
    ) -> Self {
        Self {
            variable,
            ruby_type,
            is_explicit: true,
            source_location,
        }
    }
    
    /// Get the variable name
    pub fn name(&self) -> &str {
        self.variable.name()
    }
    
    /// Get the variable type (local, instance, class, global)
    pub fn variable_type(&self) -> &RubyVariableType {
        self.variable.variable_type()
    }
    
    /// Check if this variable can be assigned the given type
    pub fn can_assign_type(&self, _new_type: &RubyType) -> bool {
        // In Ruby, variables can be reassigned to different types
        // This method could be extended for stricter type checking if needed
        true
    }
    
    /// Update the type of this variable (for reassignment)
    pub fn update_type(
        &mut self,
        new_type: RubyType,
        source_location: Option<Location>,
    ) {
        self.ruby_type = new_type;
        self.source_location = source_location;
        // Reset explicit flag since this is a new inference
        self.is_explicit = false;
    }
    
    /// Merge type information from another typed variable (for union types)
    pub fn merge_with(&self, other: &TypedVariable) -> TypedVariable {
        if self.variable.name() != other.variable.name() {
            panic!("Cannot merge variables with different names");
        }
        
        let merged_type = if self.ruby_type == other.ruby_type {
            self.ruby_type.clone()
        } else {
            RubyType::union(vec![self.ruby_type.clone(), other.ruby_type.clone()])
        };
        
        let is_explicit = self.is_explicit && other.is_explicit;
        
        TypedVariable {
            variable: self.variable.clone(),
            ruby_type: merged_type,
            is_explicit,
            source_location: self.source_location.clone(),
        }
    }
}

impl VariableTypeContext {
    /// Create a new empty variable type context
    pub fn new() -> Self {
        Self {
            local_variables: Vec::new(),
            instance_variables: Vec::new(),
            class_variables: Vec::new(),
            global_variables: Vec::new(),
        }
    }
    
    /// Add or update a variable in the appropriate scope
    pub fn add_variable(&mut self, typed_var: TypedVariable) {
        let variables = match typed_var.variable_type() {
            RubyVariableType::Local(_) => &mut self.local_variables,
            RubyVariableType::Instance => &mut self.instance_variables,
            RubyVariableType::Class => &mut self.class_variables,
            RubyVariableType::Global => &mut self.global_variables,
        };
        
        // Check if variable already exists and update it
        if let Some(existing) = variables.iter_mut().find(|v| v.name() == typed_var.name()) {
            *existing = existing.merge_with(&typed_var);
        } else {
            variables.push(typed_var);
        }
    }
    
    /// Find a variable by name and type
    pub fn find_variable(&self, name: &str, var_type: &RubyVariableType) -> Option<&TypedVariable> {
        let variables = match var_type {
            RubyVariableType::Local(_) => &self.local_variables,
            RubyVariableType::Instance => &self.instance_variables,
            RubyVariableType::Class => &self.class_variables,
            RubyVariableType::Global => &self.global_variables,
        };
        
        variables.iter().find(|v| v.name() == name)
    }
    
    /// Find any variable by name (searches all scopes)
    pub fn find_any_variable(&self, name: &str) -> Option<&TypedVariable> {
        // Search in order of precedence: local -> instance -> class -> global
        self.local_variables.iter()
            .chain(self.instance_variables.iter())
            .chain(self.class_variables.iter())
            .chain(self.global_variables.iter())
            .find(|v| v.name() == name)
    }
    
    /// Get all variables in the context
    pub fn all_variables(&self) -> impl Iterator<Item = &TypedVariable> {
        self.local_variables.iter()
            .chain(self.instance_variables.iter())
            .chain(self.class_variables.iter())
            .chain(self.global_variables.iter())
    }
    
    /// Merge with another context (for scope inheritance)
    pub fn merge_with(&self, other: &VariableTypeContext) -> VariableTypeContext {
        let mut merged = self.clone();
        
        // Merge each variable type, with other taking precedence for conflicts
        for var in &other.local_variables {
            merged.add_variable(var.clone());
        }
        for var in &other.instance_variables {
            merged.add_variable(var.clone());
        }
        for var in &other.class_variables {
            merged.add_variable(var.clone());
        }
        for var in &other.global_variables {
            merged.add_variable(var.clone());
        }
        
        merged
    }
}

impl Default for VariableTypeContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ruby_variable::RubyVariable;
    
    #[test]
    fn test_typed_variable_creation() {
        let var = RubyVariable::new("x", RubyVariableType::Local(vec![])).unwrap();
        let typed_var = TypedVariable::new_inferred(
            var,
            RubyType::integer(),
            None,
        );
        
        assert_eq!(typed_var.name(), "x");
        assert_eq!(typed_var.ruby_type, RubyType::integer());
        assert!(!typed_var.is_explicit);
    }
    
    #[test]
    fn test_variable_type_context() {
        let mut context = VariableTypeContext::new();
        
        let local_var = TypedVariable::new_inferred(
            RubyVariable::new("x", RubyVariableType::Local(vec![])).unwrap(),
            RubyType::integer(),
            None,
        );
        
        let instance_var = TypedVariable::new_inferred(
            RubyVariable::new("@y", RubyVariableType::Instance).unwrap(),
            RubyType::string(),
            None,
        );
        
        context.add_variable(local_var);
        context.add_variable(instance_var);
        
        assert!(context.find_variable("x", &RubyVariableType::Local(vec![])).is_some());
        assert!(context.find_variable("@y", &RubyVariableType::Instance).is_some());
        assert!(context.find_variable("z", &RubyVariableType::Local(vec![])).is_none());
    }
    
    #[test]
    fn test_variable_merging() {
        let var1 = TypedVariable::new_inferred(
            RubyVariable::new("x", RubyVariableType::Local(vec![])).unwrap(),
            RubyType::integer(),
            None,
        );
        
        let var2 = TypedVariable::new_inferred(
            RubyVariable::new("x", RubyVariableType::Local(vec![])).unwrap(),
            RubyType::string(),
            None,
        );
        
        let merged = var1.merge_with(&var2);
        
        // Should create a union type
        match merged.ruby_type {
            RubyType::Union(types) => {
                assert!(types.contains(&RubyType::integer()));
                assert!(types.contains(&RubyType::string()));
            }
            _ => panic!("Expected union type"),
        }
    }
    
    #[test]
    fn test_context_merging() {
        let mut context1 = VariableTypeContext::new();
        let mut context2 = VariableTypeContext::new();
        
        context1.add_variable(TypedVariable::new_inferred(
            RubyVariable::new("x", RubyVariableType::Local(vec![])).unwrap(),
            RubyType::integer(),
            None,
        ));
        
        context2.add_variable(TypedVariable::new_inferred(
            RubyVariable::new("y", RubyVariableType::Local(vec![])).unwrap(),
            RubyType::string(),
            None,
        ));
        
        let merged = context1.merge_with(&context2);
        
        assert!(merged.find_variable("x", &RubyVariableType::Local(vec![])).is_some());
        assert!(merged.find_variable("y", &RubyVariableType::Local(vec![])).is_some());
    }
}