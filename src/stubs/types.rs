use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::version::MinorVersion;

/// Method visibility in Ruby stubs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

/// Method kind in Ruby stubs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MethodKind {
    /// Instance method (called on instances)
    Instance,
    /// Class method (called on the class itself)
    Class,
}

/// A stub for a Ruby core class containing all its methods and constants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassStub {
    /// The fully qualified name of the class (e.g., "Object", "String", "Array")
    pub name: String,
    
    /// The superclass name if any (e.g., "Object" for most classes)
    pub superclass: Option<String>,
    
    /// Instance methods defined on this class
    pub instance_methods: Vec<MethodStub>,
    
    /// Class methods defined on this class
    pub class_methods: Vec<MethodStub>,
    
    /// Constants defined on this class
    pub constants: Vec<ConstantStub>,
    
    /// Modules included in this class
    pub includes: Vec<String>,
    
    /// Documentation for the class
    pub documentation: Option<String>,
    
    /// Source information (ruby-doc.org, RDoc, etc.)
    pub source: StubSource,
    
    /// Ruby version this stub was generated for
    pub version: MinorVersion,
}

/// A stub for a Ruby method with signature and documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodStub {
    /// Method name
    pub name: String,
    
    /// Method parameters with their names and types if available
    pub parameters: Vec<ParameterStub>,
    
    /// Return type if known
    pub return_type: Option<String>,
    
    /// Method visibility (public, private, protected)
    pub visibility: MethodVisibility,
    
    /// Method kind (instance, class)
    pub kind: MethodKind,
    
    /// Method documentation
    pub documentation: Option<String>,
    
    /// RDoc link if available
    pub rdoc_link: Option<String>,
    
    /// Method signature as it appears in documentation
    pub signature: Option<String>,
}

/// A stub for a method parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterStub {
    /// Parameter name
    pub name: String,
    
    /// Parameter type (required, optional, keyword, block, etc.)
    pub param_type: ParameterType,
    
    /// Default value if any
    pub default_value: Option<String>,
    
    /// Parameter documentation
    pub documentation: Option<String>,
}

/// Types of method parameters in Ruby
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    /// Required positional parameter
    Required,
    /// Optional positional parameter with default value
    Optional,
    /// Rest parameter (*args)
    Rest,
    /// Required keyword parameter
    KeywordRequired,
    /// Optional keyword parameter with default value
    KeywordOptional,
    /// Keyword rest parameter (**kwargs)
    KeywordRest,
    /// Block parameter (&block)
    Block,
}

/// A stub for a Ruby constant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantStub {
    /// Constant name
    pub name: String,
    
    /// Constant value if known
    pub value: Option<String>,
    
    /// Constant type if known
    pub const_type: Option<String>,
    
    /// Constant documentation
    pub documentation: Option<String>,
    
    /// RDoc link if available
    pub rdoc_link: Option<String>,
}

/// Information about where a stub was generated from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubSource {
    /// Primary source (ruby-doc.org, rdoc, source_code)
    pub primary: String,
    
    /// URL or path to the source
    pub url: Option<String>,
    
    /// Generation timestamp
    pub generated_at: String,
    
    /// Additional attribution information
    pub attribution: Option<String>,
}

/// Container for all stubs for a specific Ruby version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionStubs {
    /// Ruby version these stubs are for
    pub version: MinorVersion,
    
    /// Core classes indexed by name
    pub core_classes: HashMap<String, ClassStub>,
    
    /// Metadata about the stub generation
    pub metadata: StubMetadata,
}

/// Metadata about stub generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubMetadata {
    /// When these stubs were generated
    pub generated_at: String,
    
    /// Version of the stub generator
    pub generator_version: String,
    
    /// Total number of classes
    pub class_count: usize,
    
    /// Total number of methods
    pub method_count: usize,
    
    /// Total number of constants
    pub constant_count: usize,
    
    /// Sources used for generation
    pub sources: Vec<String>,
    
    /// Any warnings or issues during generation
    pub warnings: Vec<String>,
}

impl ClassStub {
    /// Create a new class stub
    pub fn new(name: String, version: MinorVersion) -> Self {
        Self {
            name,
            superclass: None,
            instance_methods: Vec::new(),
            class_methods: Vec::new(),
            constants: Vec::new(),
            includes: Vec::new(),
            documentation: None,
            source: StubSource {
                primary: "unknown".to_string(),
                url: None,
                generated_at: chrono::Utc::now().to_rfc3339(),
                attribution: None,
            },
            version,
        }
    }

    /// Add an instance method to this class
    pub fn add_instance_method(&mut self, method: MethodStub) {
        self.instance_methods.push(method);
    }

    /// Add a class method to this class
    pub fn add_class_method(&mut self, method: MethodStub) {
        self.class_methods.push(method);
    }

    /// Add a constant to this class
    pub fn add_constant(&mut self, constant: ConstantStub) {
        self.constants.push(constant);
    }

    /// Get all methods (instance and class) for completion
    pub fn all_methods(&self) -> impl Iterator<Item = &MethodStub> {
        self.instance_methods.iter().chain(self.class_methods.iter())
    }

    /// Find a method by name and kind
    pub fn find_method(&self, name: &str, kind: MethodKind) -> Option<&MethodStub> {
        match kind {
            MethodKind::Instance => self.instance_methods.iter().find(|m| m.name == name),
            MethodKind::Class => self.class_methods.iter().find(|m| m.name == name),
        }
    }

    /// Find a method by name (searches both instance and class methods)
    pub fn find_method_by_name(&self, name: &str) -> Option<&MethodStub> {
        self.instance_methods.iter()
            .chain(self.class_methods.iter())
            .find(|m| m.name == name)
    }

    /// Find a constant by name
    pub fn find_constant(&self, name: &str) -> Option<&ConstantStub> {
        self.constants.iter().find(|c| c.name == name)
    }
}

impl MethodStub {
    /// Create a new method stub
    pub fn new(name: String, kind: MethodKind) -> Self {
        Self {
            name,
            parameters: Vec::new(),
            return_type: None,
            visibility: MethodVisibility::Public,
            kind,
            documentation: None,
            rdoc_link: None,
            signature: None,
        }
    }

    /// Add a parameter to this method
    pub fn add_parameter(&mut self, parameter: ParameterStub) {
        self.parameters.push(parameter);
    }

    /// Generate a method signature string for display
    pub fn generate_signature(&self) -> String {
        if let Some(ref sig) = self.signature {
            return sig.clone();
        }

        let params: Vec<String> = self.parameters.iter().map(|p| {
            match p.param_type {
                ParameterType::Required => p.name.clone(),
                ParameterType::Optional => {
                    if let Some(ref default) = p.default_value {
                        format!("{} = {}", p.name, default)
                    } else {
                        format!("{}?", p.name)
                    }
                }
                ParameterType::Rest => format!("*{}", p.name),
                ParameterType::KeywordRequired => format!("{}: ", p.name),
                ParameterType::KeywordOptional => {
                    if let Some(ref default) = p.default_value {
                        format!("{}: {}", p.name, default)
                    } else {
                        format!("{}:", p.name)
                    }
                }
                ParameterType::KeywordRest => format!("**{}", p.name),
                ParameterType::Block => format!("&{}", p.name),
            }
        }).collect();

        format!("{}({})", self.name, params.join(", "))
    }
}

impl ParameterStub {
    /// Create a new parameter stub
    pub fn new(name: String, param_type: ParameterType) -> Self {
        Self {
            name,
            param_type,
            default_value: None,
            documentation: None,
        }
    }

    /// Create a required parameter
    pub fn required(name: String) -> Self {
        Self::new(name, ParameterType::Required)
    }

    /// Create an optional parameter with default value
    pub fn optional(name: String, default_value: Option<String>) -> Self {
        Self {
            name,
            param_type: ParameterType::Optional,
            default_value,
            documentation: None,
        }
    }

    /// Create a rest parameter (*args)
    pub fn rest(name: String) -> Self {
        Self::new(name, ParameterType::Rest)
    }

    /// Create a block parameter (&block)
    pub fn block(name: String) -> Self {
        Self::new(name, ParameterType::Block)
    }
}

impl ConstantStub {
    /// Create a new constant stub
    pub fn new(name: String) -> Self {
        Self {
            name,
            value: None,
            const_type: None,
            documentation: None,
            rdoc_link: None,
        }
    }
}

impl VersionStubs {
    /// Create a new version stubs container
    pub fn new(version: MinorVersion) -> Self {
        Self {
            version,
            core_classes: HashMap::new(),
            metadata: StubMetadata {
                generated_at: chrono::Utc::now().to_rfc3339(),
                generator_version: env!("CARGO_PKG_VERSION").to_string(),
                class_count: 0,
                method_count: 0,
                constant_count: 0,
                sources: Vec::new(),
                warnings: Vec::new(),
            },
        }
    }

    /// Add a class stub
    pub fn add_class(&mut self, class_stub: ClassStub) {
        self.core_classes.insert(class_stub.name.clone(), class_stub);
        self.update_metadata();
    }

    /// Get a class stub by name
    pub fn get_class(&self, name: &str) -> Option<&ClassStub> {
        self.core_classes.get(name)
    }

    /// Update metadata counts
    fn update_metadata(&mut self) {
        self.metadata.class_count = self.core_classes.len();
        self.metadata.method_count = self.core_classes.values()
            .map(|c| c.instance_methods.len() + c.class_methods.len())
            .sum();
        self.metadata.constant_count = self.core_classes.values()
            .map(|c| c.constants.len())
            .sum();
    }

    /// Get all class names
    pub fn class_names(&self) -> impl Iterator<Item = &String> {
        self.core_classes.keys()
    }

    /// Find all methods matching a name across all classes
    pub fn find_methods_by_name(&self, name: &str) -> Vec<(&ClassStub, &MethodStub)> {
        self.core_classes.values()
            .flat_map(|class| {
                class.all_methods()
                    .filter(|method| method.name == name)
                    .map(move |method| (class, method))
            })
            .collect()
    }
}