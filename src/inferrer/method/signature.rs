use crate::inferrer::RubyType;
use std::collections::HashMap;

/// Represents a method parameter with its type information
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: RubyType,
    /// Whether the parameter is required
    pub required: bool,
    /// Whether the parameter has a default value
    pub has_default: bool,
    /// Whether this is a keyword parameter
    pub keyword: bool,
    /// Whether this is a splat parameter (*args)
    pub splat: bool,
    /// Whether this is a double splat parameter (**kwargs)
    pub double_splat: bool,
    /// Whether this is a block parameter (&block)
    pub block: bool,
}

/// Represents a method signature with parameters and return type
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSignature {
    /// Method name
    pub name: String,
    /// Method parameters
    pub parameters: Vec<Parameter>,
    /// Return type
    pub return_type: RubyType,
    /// Whether this method can accept a block
    pub accepts_block: bool,
    /// Visibility (public, private, protected)
    pub visibility: MethodVisibility,
    /// Whether this is a class method
    pub class_method: bool,
    /// Confidence level of the signature inference
    pub confidence: f32,
}

/// Method visibility levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodVisibility {
    Public,
    Private,
    Protected,
}

/// Context for tracking method signatures across a codebase
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSignatureContext {
    /// Instance methods by class name
    pub instance_methods: HashMap<String, Vec<MethodSignature>>,
    /// Class methods by class name
    pub class_methods: HashMap<String, Vec<MethodSignature>>,
    /// Global methods (defined at top level)
    pub global_methods: Vec<MethodSignature>,
}

impl Parameter {
    /// Create a new required parameter
    pub fn new_required(name: String, param_type: RubyType) -> Self {
        Self {
            name,
            param_type,
            required: true,
            has_default: false,
            keyword: false,
            splat: false,
            double_splat: false,
            block: false,
        }
    }

    /// Create a new optional parameter with default value
    pub fn new_optional(name: String, param_type: RubyType) -> Self {
        Self {
            name,
            param_type,
            required: false,
            has_default: true,
            keyword: false,
            splat: false,
            double_splat: false,
            block: false,
        }
    }

    /// Create a new keyword parameter
    pub fn new_keyword(name: String, param_type: RubyType, required: bool) -> Self {
        Self {
            name,
            param_type,
            required,
            has_default: !required,
            keyword: true,
            splat: false,
            double_splat: false,
            block: false,
        }
    }

    /// Create a new splat parameter (*args)
    pub fn new_splat(name: String, element_type: RubyType) -> Self {
        Self {
            name,
            param_type: RubyType::array_of(element_type),
            required: false,
            has_default: false,
            keyword: false,
            splat: true,
            double_splat: false,
            block: false,
        }
    }

    /// Create a new double splat parameter (**kwargs)
    pub fn new_double_splat(name: String, value_type: RubyType) -> Self {
        Self {
            name,
            param_type: RubyType::hash_of(RubyType::symbol(), value_type),
            required: false,
            has_default: false,
            keyword: false,
            splat: false,
            double_splat: true,
            block: false,
        }
    }

    /// Create a new block parameter (&block)
    pub fn new_block(name: String) -> Self {
        Self {
            name,
            param_type: RubyType::Class(
                crate::types::fully_qualified_name::FullyQualifiedName::try_from("Proc").unwrap(),
            ),
            required: false,
            has_default: false,
            keyword: false,
            splat: false,
            double_splat: false,
            block: true,
        }
    }

    /// Check if this parameter can accept the given type
    pub fn can_accept_type(&self, arg_type: &RubyType) -> bool {
        self.param_type.is_compatible_with(arg_type)
    }
}

impl MethodSignature {
    /// Create a new method signature
    pub fn new(name: String, parameters: Vec<Parameter>, return_type: RubyType) -> Self {
        Self {
            name,
            parameters,
            return_type,
            accepts_block: false,
            visibility: MethodVisibility::Public,
            class_method: false,
            confidence: 1.0,
        }
    }

    /// Create a new method signature with confidence level
    pub fn new_inferred(
        name: String,
        parameters: Vec<Parameter>,
        return_type: RubyType,
        confidence: f32,
    ) -> Self {
        Self {
            name,
            parameters,
            return_type,
            accepts_block: false,
            visibility: MethodVisibility::Public,
            class_method: false,
            confidence,
        }
    }

    /// Set method visibility
    pub fn with_visibility(mut self, visibility: MethodVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Mark as class method
    pub fn as_class_method(mut self) -> Self {
        self.class_method = true;
        self
    }

    /// Mark as accepting block
    pub fn accepts_block(mut self) -> Self {
        self.accepts_block = true;
        self
    }

    /// Get required parameters
    pub fn required_parameters(&self) -> Vec<&Parameter> {
        self.parameters
            .iter()
            .filter(|p| p.required && !p.splat && !p.double_splat && !p.block)
            .collect()
    }

    /// Get optional parameters
    pub fn optional_parameters(&self) -> Vec<&Parameter> {
        self.parameters
            .iter()
            .filter(|p| !p.required && !p.splat && !p.double_splat && !p.block)
            .collect()
    }

    /// Get keyword parameters
    pub fn keyword_parameters(&self) -> Vec<&Parameter> {
        self.parameters.iter().filter(|p| p.keyword).collect()
    }

    /// Get splat parameter if any
    pub fn splat_parameter(&self) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.splat)
    }

    /// Get double splat parameter if any
    pub fn double_splat_parameter(&self) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.double_splat)
    }

    /// Get block parameter if any
    pub fn block_parameter(&self) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.block)
    }

    /// Check if this signature can be called with the given argument types
    pub fn can_call_with(&self, arg_types: &[RubyType]) -> bool {
        let required_params = self.required_parameters();
        let optional_params = self.optional_parameters();

        // Check minimum required arguments
        if arg_types.len() < required_params.len() {
            return false;
        }

        // Check maximum arguments (unless there's a splat parameter)
        if self.splat_parameter().is_none() {
            let max_args = required_params.len() + optional_params.len();
            if arg_types.len() > max_args {
                return false;
            }
        }

        // Check type compatibility for each argument
        for (i, arg_type) in arg_types.iter().enumerate() {
            if i < required_params.len() {
                if !required_params[i].can_accept_type(arg_type) {
                    return false;
                }
            } else if i < required_params.len() + optional_params.len() {
                let optional_index = i - required_params.len();
                if !optional_params[optional_index].can_accept_type(arg_type) {
                    return false;
                }
            } else if let Some(splat_param) = self.splat_parameter() {
                // For splat parameters, check if the argument is compatible with the element type
                if let RubyType::Array(element_types) = &splat_param.param_type {
                    if !element_types.is_empty() {
                        let element_type = &element_types[0];
                        if !element_type.is_compatible_with(arg_type) {
                            return false;
                        }
                    }
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Merge this signature with another (for method overloading)
    pub fn merge_with(&self, other: &MethodSignature) -> MethodSignature {
        if self.name != other.name {
            panic!("Cannot merge signatures with different method names");
        }

        // Create a union of return types
        let merged_return_type =
            RubyType::union(vec![self.return_type.clone(), other.return_type.clone()]);

        // For now, use the signature with higher confidence
        // TODO: Implement more sophisticated parameter merging
        if self.confidence >= other.confidence {
            MethodSignature {
                name: self.name.clone(),
                parameters: self.parameters.clone(),
                return_type: merged_return_type,
                accepts_block: self.accepts_block || other.accepts_block,
                visibility: self.visibility.clone(),
                class_method: self.class_method,
                confidence: (self.confidence + other.confidence) / 2.0,
            }
        } else {
            MethodSignature {
                name: other.name.clone(),
                parameters: other.parameters.clone(),
                return_type: merged_return_type,
                accepts_block: self.accepts_block || other.accepts_block,
                visibility: other.visibility.clone(),
                class_method: other.class_method,
                confidence: (self.confidence + other.confidence) / 2.0,
            }
        }
    }
}

impl MethodSignatureContext {
    /// Create a new method signature context
    pub fn new() -> Self {
        Self {
            instance_methods: HashMap::new(),
            class_methods: HashMap::new(),
            global_methods: Vec::new(),
        }
    }

    /// Add a method signature to the context
    pub fn add_method(&mut self, class_name: Option<String>, signature: MethodSignature) {
        match class_name {
            Some(class) => {
                if signature.class_method {
                    self.class_methods.entry(class).or_default().push(signature);
                } else {
                    self.instance_methods
                        .entry(class)
                        .or_default()
                        .push(signature);
                }
            }
            None => {
                self.global_methods.push(signature);
            }
        }
    }

    /// Find method signatures by name and class
    pub fn find_method(
        &self,
        class_name: Option<&str>,
        method_name: &str,
        is_class_method: bool,
    ) -> Vec<&MethodSignature> {
        match class_name {
            Some(class) => {
                let methods = if is_class_method {
                    self.class_methods.get(class)
                } else {
                    self.instance_methods.get(class)
                };

                methods
                    .map(|methods| {
                        methods
                            .iter()
                            .filter(|sig| sig.name == method_name)
                            .collect()
                    })
                    .unwrap_or_else(Vec::new)
            }
            None => self
                .global_methods
                .iter()
                .filter(|sig| sig.name == method_name)
                .collect(),
        }
    }

    /// Get all method signatures for a class
    pub fn get_class_methods(
        &self,
        class_name: &str,
    ) -> (Vec<&MethodSignature>, Vec<&MethodSignature>) {
        let instance_methods = self
            .instance_methods
            .get(class_name)
            .map(|methods| methods.iter().collect())
            .unwrap_or_default();

        let class_methods = self
            .class_methods
            .get(class_name)
            .map(|methods| methods.iter().collect())
            .unwrap_or_default();

        (instance_methods, class_methods)
    }

    /// Merge with another method signature context
    pub fn merge_with(&self, other: &MethodSignatureContext) -> MethodSignatureContext {
        let mut merged = self.clone();

        // Merge instance methods
        for (class_name, methods) in &other.instance_methods {
            for method in methods {
                merged.add_method(Some(class_name.clone()), method.clone());
            }
        }

        // Merge class methods
        for (class_name, methods) in &other.class_methods {
            for method in methods {
                merged.add_method(Some(class_name.clone()), method.clone());
            }
        }

        // Merge global methods
        for method in &other.global_methods {
            merged.add_method(None, method.clone());
        }

        merged
    }
}

impl Default for MethodSignatureContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_creation() {
        let param = Parameter::new_required("name".to_string(), RubyType::string());
        assert_eq!(param.name, "name");
        assert_eq!(param.param_type, RubyType::string());
        assert!(param.required);
        assert!(!param.has_default);

        let optional_param = Parameter::new_optional("age".to_string(), RubyType::integer());
        assert!(!optional_param.required);
        assert!(optional_param.has_default);

        let keyword_param =
            Parameter::new_keyword("verbose".to_string(), RubyType::boolean(), false);
        assert!(keyword_param.keyword);
        assert!(!keyword_param.required);

        let splat_param = Parameter::new_splat("args".to_string(), RubyType::Unknown);
        assert!(splat_param.splat);

        let block_param = Parameter::new_block("block".to_string());
        assert!(block_param.block);
    }

    #[test]
    fn test_method_signature_creation() {
        let params = vec![
            Parameter::new_required("name".to_string(), RubyType::string()),
            Parameter::new_optional("age".to_string(), RubyType::integer()),
        ];

        let signature = MethodSignature::new("greet".to_string(), params, RubyType::string());

        assert_eq!(signature.name, "greet");
        assert_eq!(signature.parameters.len(), 2);
        assert_eq!(signature.return_type, RubyType::string());
        assert_eq!(signature.required_parameters().len(), 1);
        assert_eq!(signature.optional_parameters().len(), 1);
    }

    #[test]
    fn test_method_call_compatibility() {
        let params = vec![
            Parameter::new_required("name".to_string(), RubyType::string()),
            Parameter::new_optional("age".to_string(), RubyType::integer()),
        ];

        let signature = MethodSignature::new("greet".to_string(), params, RubyType::string());

        // Valid calls
        assert!(signature.can_call_with(&[RubyType::string()]));
        assert!(signature.can_call_with(&[RubyType::string(), RubyType::integer()]));

        // Invalid calls
        assert!(!signature.can_call_with(&[])); // Missing required parameter
        assert!(!signature.can_call_with(&[
            RubyType::string(),
            RubyType::integer(),
            RubyType::string()
        ])); // Too many arguments
        assert!(!signature.can_call_with(&[RubyType::integer()])); // Wrong type for required parameter
    }

    #[test]
    fn test_method_signature_context() {
        let mut context = MethodSignatureContext::new();

        let signature = MethodSignature::new(
            "initialize".to_string(),
            vec![Parameter::new_required(
                "name".to_string(),
                RubyType::string(),
            )],
            RubyType::nil_class(),
        );

        context.add_method(Some("User".to_string()), signature);

        let found = context.find_method(Some("User"), "initialize", false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "initialize");

        let not_found = context.find_method(Some("User"), "nonexistent", false);
        assert_eq!(not_found.len(), 0);
    }

    #[test]
    fn test_signature_merging() {
        let sig1 = MethodSignature::new_inferred(
            "test".to_string(),
            vec![Parameter::new_required(
                "x".to_string(),
                RubyType::integer(),
            )],
            RubyType::string(),
            0.8,
        );

        let sig2 = MethodSignature::new_inferred(
            "test".to_string(),
            vec![Parameter::new_required(
                "x".to_string(),
                RubyType::integer(),
            )],
            RubyType::integer(),
            0.9,
        );

        let merged = sig1.merge_with(&sig2);

        // Should use the signature with higher confidence
        assert_eq!(merged.confidence, 0.85); // Average of 0.8 and 0.9

        // Return type should be a union
        match merged.return_type {
            RubyType::Union(types) => {
                assert!(types.contains(&RubyType::string()));
                assert!(types.contains(&RubyType::integer()));
            }
            _ => panic!("Expected union type"),
        }
    }
}
