//! RBS file loader for loading type definitions.
//!
//! This module provides functionality to load RBS files from:
//! - Embedded Ruby core and stdlib type definitions (compiled into the binary)
//! - Project sig/ directories
//! - Custom RBS files

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::embedded;
use crate::types::*;
use crate::Parser;

/// RBS file loader
pub struct Loader {
    /// Parsed declarations indexed by fully qualified name
    declarations: HashMap<String, Declaration>,

    /// Class methods indexed by "ClassName#method_name" or "ClassName.method_name"
    methods: HashMap<String, MethodDecl>,

    /// Loaded file paths
    loaded_files: Vec<PathBuf>,
}

impl Loader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            declarations: HashMap::new(),
            methods: HashMap::new(),
            loaded_files: Vec::new(),
        }
    }

    /// Create a new loader with embedded Ruby core types pre-loaded
    ///
    /// This loads type definitions for Ruby core classes like String, Integer,
    /// Array, Hash, etc. These are embedded in the binary at compile time
    /// and don't require any external files.
    pub fn with_core_types() -> Result<Self, LoadError> {
        let mut loader = Self::new();
        loader.load_embedded_core()?;
        Ok(loader)
    }

    /// Create a new loader with both core and stdlib types pre-loaded
    ///
    /// This loads type definitions for Ruby core classes and standard library
    /// modules. These are embedded in the binary at compile time.
    pub fn with_stdlib_types() -> Result<Self, LoadError> {
        let mut loader = Self::new();
        loader.load_embedded_core()?;
        loader.load_embedded_stdlib()?;
        Ok(loader)
    }

    /// Load embedded Ruby core type definitions
    ///
    /// Loads type definitions for classes like String, Integer, Array, Hash,
    /// Object, etc. from the embedded RBS content.
    pub fn load_embedded_core(&mut self) -> Result<usize, LoadError> {
        let mut count = 0;
        for (name, content) in embedded::core_rbs_files() {
            self.load_string(content, Some(PathBuf::from(name)))?;
            count += 1;
        }
        Ok(count)
    }

    /// Load embedded Ruby stdlib type definitions
    ///
    /// Loads type definitions for standard library modules like JSON, YAML,
    /// FileUtils, etc. from the embedded RBS content.
    pub fn load_embedded_stdlib(&mut self) -> Result<usize, LoadError> {
        let mut count = 0;
        for (name, content) in embedded::stdlib_rbs_files() {
            self.load_string(content, Some(PathBuf::from(name)))?;
            count += 1;
        }
        Ok(count)
    }

    /// Check if embedded core types are available
    pub fn has_embedded_core() -> bool {
        embedded::core_file_count() > 0
    }

    /// Check if embedded stdlib types are available
    pub fn has_embedded_stdlib() -> bool {
        embedded::stdlib_file_count() > 0
    }

    // Legacy methods for backward compatibility with file-based loading

    /// Load bundled Ruby core type definitions from disk (legacy)
    #[deprecated(note = "Use load_embedded_core() instead")]
    pub fn load_bundled_core(&mut self) -> Result<usize, LoadError> {
        self.load_embedded_core()
    }

    /// Load bundled Ruby stdlib type definitions from disk (legacy)
    #[deprecated(note = "Use load_embedded_stdlib() instead")]
    pub fn load_bundled_stdlib(&mut self) -> Result<usize, LoadError> {
        self.load_embedded_stdlib()
    }

    /// Check if bundled core types are available (legacy)
    #[deprecated(note = "Use has_embedded_core() instead")]
    pub fn has_bundled_core() -> bool {
        Self::has_embedded_core()
    }

    /// Check if bundled stdlib types are available (legacy)
    #[deprecated(note = "Use has_embedded_stdlib() instead")]
    pub fn has_bundled_stdlib() -> bool {
        Self::has_embedded_stdlib()
    }

    /// Load RBS files from a directory recursively
    pub fn load_directory(&mut self, path: &Path) -> Result<usize, LoadError> {
        if !path.exists() {
            return Err(LoadError::NotFound(path.to_path_buf()));
        }

        let mut count = 0;

        if path.is_file() {
            if path.extension().map_or(false, |ext| ext == "rbs") {
                self.load_file(path)?;
                count += 1;
            }
        } else if path.is_dir() {
            for entry in std::fs::read_dir(path).map_err(|e| LoadError::Io(e.to_string()))? {
                let entry = entry.map_err(|e| LoadError::Io(e.to_string()))?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    count += self.load_directory(&entry_path)?;
                } else if entry_path.extension().map_or(false, |ext| ext == "rbs") {
                    self.load_file(&entry_path)?;
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Load a single RBS file
    pub fn load_file(&mut self, path: &Path) -> Result<(), LoadError> {
        let content = std::fs::read_to_string(path).map_err(|e| LoadError::Io(e.to_string()))?;

        self.load_string(&content, Some(path.to_path_buf()))?;
        self.loaded_files.push(path.to_path_buf());

        Ok(())
    }

    /// Load RBS from a string
    pub fn load_string(&mut self, content: &str, path: Option<PathBuf>) -> Result<(), LoadError> {
        let mut parser = Parser::new();
        let declarations = parser.parse(content).map_err(|e| LoadError::Parse {
            message: e.message,
            path,
        })?;

        for decl in declarations {
            self.index_declaration(decl);
        }

        Ok(())
    }

    /// Index a declaration for lookup
    fn index_declaration(&mut self, decl: Declaration) {
        match &decl {
            Declaration::Class(class) => {
                let name = &class.name;
                self.declarations.insert(name.clone(), decl.clone());

                // Index methods
                for method in &class.methods {
                    let key = if method.kind == MethodKind::Singleton {
                        format!("{}.{}", name, method.name)
                    } else {
                        format!("{}#{}", name, method.name)
                    };
                    self.methods.insert(key, method.clone());
                }
            }
            Declaration::Module(module) => {
                let name = &module.name;
                self.declarations.insert(name.clone(), decl.clone());

                // Index methods
                for method in &module.methods {
                    let key = if method.kind == MethodKind::Singleton {
                        format!("{}.{}", name, method.name)
                    } else {
                        format!("{}#{}", name, method.name)
                    };
                    self.methods.insert(key, method.clone());
                }
            }
            Declaration::Interface(interface) => {
                self.declarations
                    .insert(interface.name.clone(), decl.clone());
            }
            Declaration::TypeAlias(alias) => {
                self.declarations.insert(alias.name.clone(), decl.clone());
            }
            Declaration::Constant(constant) => {
                self.declarations
                    .insert(constant.name.clone(), decl.clone());
            }
            Declaration::Global(global) => {
                self.declarations.insert(global.name.clone(), decl.clone());
            }
        }
    }

    /// Look up a class by name
    pub fn get_class(&self, name: &str) -> Option<&ClassDecl> {
        match self.declarations.get(name)? {
            Declaration::Class(class) => Some(class),
            _ => None,
        }
    }

    /// Look up a module by name
    pub fn get_module(&self, name: &str) -> Option<&ModuleDecl> {
        match self.declarations.get(name)? {
            Declaration::Module(module) => Some(module),
            _ => None,
        }
    }

    /// Look up an interface by name
    pub fn get_interface(&self, name: &str) -> Option<&InterfaceDecl> {
        match self.declarations.get(name)? {
            Declaration::Interface(interface) => Some(interface),
            _ => None,
        }
    }

    /// Look up a type alias by name
    pub fn get_type_alias(&self, name: &str) -> Option<&TypeAliasDecl> {
        match self.declarations.get(name)? {
            Declaration::TypeAlias(alias) => Some(alias),
            _ => None,
        }
    }

    /// Look up an instance method by class name and method name
    pub fn get_instance_method(&self, class_name: &str, method_name: &str) -> Option<&MethodDecl> {
        let key = format!("{}#{}", class_name, method_name);
        self.methods.get(&key)
    }

    /// Look up a singleton (class) method by class name and method name
    pub fn get_singleton_method(&self, class_name: &str, method_name: &str) -> Option<&MethodDecl> {
        let key = format!("{}.{}", class_name, method_name);
        self.methods.get(&key)
    }

    /// Get the return type of a method
    pub fn get_method_return_type(
        &self,
        class_name: &str,
        method_name: &str,
        is_singleton: bool,
    ) -> Option<&RbsType> {
        let method = if is_singleton {
            self.get_singleton_method(class_name, method_name)?
        } else {
            self.get_instance_method(class_name, method_name)?
        };

        method.return_type()
    }

    /// Get all loaded declarations
    pub fn declarations(&self) -> impl Iterator<Item = &Declaration> {
        self.declarations.values()
    }

    /// Get all loaded class names
    pub fn class_names(&self) -> impl Iterator<Item = &str> {
        self.declarations.iter().filter_map(|(name, decl)| {
            if matches!(decl, Declaration::Class(_)) {
                Some(name.as_str())
            } else {
                None
            }
        })
    }

    /// Get all loaded module names
    pub fn module_names(&self) -> impl Iterator<Item = &str> {
        self.declarations.iter().filter_map(|(name, decl)| {
            if matches!(decl, Declaration::Module(_)) {
                Some(name.as_str())
            } else {
                None
            }
        })
    }

    /// Get the number of loaded files
    pub fn loaded_file_count(&self) -> usize {
        self.loaded_files.len()
    }

    /// Get the number of indexed declarations
    pub fn declaration_count(&self) -> usize {
        self.declarations.len()
    }

    /// Get the number of indexed methods
    pub fn method_count(&self) -> usize {
        self.methods.len()
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during loading
#[derive(Debug, Clone)]
pub enum LoadError {
    NotFound(PathBuf),
    Io(String),
    Parse {
        message: String,
        path: Option<PathBuf>,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound(path) => write!(f, "Path not found: {}", path.display()),
            LoadError::Io(msg) => write!(f, "IO error: {}", msg),
            LoadError::Parse { message, path } => {
                if let Some(p) = path {
                    write!(f, "Parse error in {}: {}", p.display(), message)
                } else {
                    write!(f, "Parse error: {}", message)
                }
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_string() {
        let mut loader = Loader::new();
        let source = r#"
class String
  def length: () -> Integer
  def upcase: () -> String
  def self.new: (?String str) -> String
end
"#;
        let result = loader.load_string(source, None);
        assert!(result.is_ok());

        // Check class was loaded
        let class = loader.get_class("String");
        assert!(class.is_some());
        assert_eq!(class.unwrap().methods.len(), 3);

        // Check instance method lookup
        let length = loader.get_instance_method("String", "length");
        assert!(length.is_some());

        // Check singleton method lookup
        let new = loader.get_singleton_method("String", "new");
        assert!(new.is_some());

        // Check return type lookup
        let return_type = loader.get_method_return_type("String", "length", false);
        assert!(return_type.is_some());
    }

    #[test]
    fn test_load_module() {
        let mut loader = Loader::new();
        let source = r#"
module Enumerable[Elem]
  def map: [U] () { (Elem) -> U } -> Array[U]
  def each: () { (Elem) -> void } -> self
end
"#;
        let result = loader.load_string(source, None);
        assert!(result.is_ok());

        let module = loader.get_module("Enumerable");
        assert!(module.is_some());
    }

    #[test]
    fn test_method_counts() {
        let mut loader = Loader::new();
        let source = r#"
class Foo
  def bar: () -> String
  def baz: () -> Integer
end

class Qux
  def quux: () -> void
end
"#;
        loader.load_string(source, None).unwrap();

        assert_eq!(loader.declaration_count(), 2);
        assert_eq!(loader.method_count(), 3);
    }

    #[test]
    fn test_embedded_core_exists() {
        #[allow(deprecated)]
        let has_core = Loader::has_bundled_core();
        assert!(has_core, "Embedded core RBS types should be available");
    }

    #[test]
    fn test_embedded_stdlib_exists() {
        // Stdlib might or might not be embedded depending on build
        #[allow(deprecated)]
        let has_stdlib = Loader::has_bundled_stdlib();
        println!("Embedded stdlib available: {}", has_stdlib);
    }

    #[test]
    fn test_load_embedded_core() {
        let loader = Loader::with_core_types();
        assert!(
            loader.is_ok(),
            "Failed to load embedded core types: {:?}",
            loader.err()
        );

        let loader = loader.unwrap();
        println!("Loaded {} core declarations", loader.declaration_count());
        println!("Loaded {} core methods", loader.method_count());

        // Verify essential core classes are loaded
        assert!(
            loader.get_class("String").is_some(),
            "String class should be loaded"
        );
        assert!(
            loader.get_class("Integer").is_some(),
            "Integer class should be loaded"
        );
        assert!(
            loader.get_class("Array").is_some(),
            "Array class should be loaded"
        );
        assert!(
            loader.get_class("Hash").is_some(),
            "Hash class should be loaded"
        );
        assert!(
            loader.get_class("Object").is_some(),
            "Object class should be loaded"
        );

        // Verify some common methods
        assert!(
            loader.get_instance_method("String", "length").is_some(),
            "String#length should be loaded"
        );
        let upcase_method = loader.get_instance_method("String", "upcase");
        assert!(upcase_method.is_some(), "String#upcase should be loaded");
        let upcase_method = upcase_method.unwrap();
        println!(
            "String#upcase has {} overloads",
            upcase_method.overloads.len()
        );
        for (i, overload) in upcase_method.overloads.iter().enumerate() {
            println!("  Overload {}: {:?}", i, overload.return_type);
        }
        let return_type = upcase_method.return_type();
        println!("String#upcase return_type(): {:?}", return_type);
        // upcase should return String, not self? or Optional(SelfType)
        assert!(
            matches!(return_type, Some(RbsType::Class(name)) if name == "String"),
            "String#upcase should return String, got {:?}",
            return_type
        );
        assert!(
            loader.get_instance_method("Array", "first").is_some(),
            "Array#first should be loaded"
        );
        assert!(
            loader.get_instance_method("Array", "push").is_some(),
            "Array#push should be loaded"
        );
    }

    #[test]
    fn test_string_method_return_types() {
        let loader = Loader::with_core_types().expect("Failed to load embedded core types");

        // String#length should return Integer
        let length_type = loader.get_method_return_type("String", "length", false);
        assert!(
            length_type.is_some(),
            "String#length return type should be available"
        );
        if let Some(RbsType::Class(name)) = length_type {
            assert_eq!(name, "Integer", "String#length should return Integer");
        }

        // String#upcase should return String
        let upcase_type = loader.get_method_return_type("String", "upcase", false);
        assert!(
            upcase_type.is_some(),
            "String#upcase return type should be available"
        );
        if let Some(RbsType::Class(name)) = upcase_type {
            assert_eq!(name, "String", "String#upcase should return String");
        }
    }

    #[test]
    fn test_embedded_works_without_files() {
        // This test verifies that embedded types work even if there are no
        // external RBS files. This is critical for distribution.
        use crate::embedded;

        let core_count = embedded::core_file_count();
        assert!(core_count > 0, "Should have embedded core files");
        println!("Embedded {} core RBS files in binary", core_count);

        // Load and verify
        let loader = Loader::with_core_types().unwrap();
        assert!(loader.declaration_count() > 0, "Should have declarations");
        assert!(loader.method_count() > 0, "Should have methods");
    }
}
