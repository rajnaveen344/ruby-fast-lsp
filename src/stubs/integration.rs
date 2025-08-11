use std::sync::Arc;

use anyhow::Result;
use log::{debug, info};
use tower_lsp::lsp_types::Location;

use super::loader::StubLoader;
use super::version::MinorVersion;

/// Integration layer between the stub system and LSP handlers
/// 
/// This provides a high-level interface for LSP handlers to access
/// Ruby core class stub files for "Go to Definition" functionality.
#[derive(Debug)]
pub struct StubIntegration {
    loader: Arc<StubLoader>,
    current_version: Option<MinorVersion>,
}

impl StubIntegration {
    /// Create a new stub integration with the default stub path
    pub fn new() -> Self {
        Self {
            loader: Arc::new(StubLoader::default()),
            current_version: None,
        }
    }

    /// Create a new stub integration with a custom stub path
    pub fn with_path<P: AsRef<std::path::Path>>(path: P) -> Self {
        Self {
            loader: Arc::new(StubLoader::new(path)),
            current_version: None,
        }
    }

    /// Initialize the stub system with a Ruby version
    /// 
    /// This should be called when the LSP detects the Ruby version
    /// being used in the workspace.
    pub fn initialize(&mut self, version: MinorVersion) -> Result<()> {
        info!("Initializing stub system for Ruby {}", version);
        
        let loaded_version = self.loader.load_version(version)?;
        self.current_version = Some(loaded_version);
        
        if loaded_version != version {
            info!(
                "Using Ruby {} stubs for requested version {}",
                loaded_version, version
            );
        }
        
        Ok(())
    }

    /// Switch to a different Ruby version
    pub fn switch_version(&mut self, version: MinorVersion) -> Result<()> {
        let loaded_version = self.loader.switch_version(version)?;
        self.current_version = Some(loaded_version);
        Ok(())
    }

    /// Get the file location for a Ruby core class
    /// 
    /// This returns the location of the stub file for the specified class,
    /// which can be used for "Go to Definition" functionality.
    pub fn get_class_definition(&self, class_name: &str) -> Option<Location> {
        let location = self.loader.get_class_location(class_name)?;
        debug!("Found stub file for class {}: {}", class_name, location.uri);
        Some(location)
    }

    /// Get all available Ruby core class names
    pub fn get_available_classes(&self) -> Vec<String> {
        self.loader.get_class_names()
    }

    /// Check if a class is a Ruby core class
    pub fn is_core_class(&self, class_name: &str) -> bool {
        self.loader.has_class(class_name)
    }

    /// Get the currently loaded Ruby version
    pub fn get_current_version(&self) -> Option<MinorVersion> {
        self.current_version
    }

    /// Check if the stub system is initialized
    pub fn is_initialized(&self) -> bool {
        self.current_version.is_some()
    }

    /// Check if stubs are available for a specific version
    pub fn is_version_available(&self, version: MinorVersion) -> bool {
        self.loader.is_version_available(version)
    }

    /// Get the closest available version for a requested version
    pub fn find_closest_version(&self, version: MinorVersion) -> Option<MinorVersion> {
        version.find_closest_supported()
    }

    /// Get statistics about the loaded stubs
    pub fn get_stats(&self) -> StubStats {
        let class_count = self.loader.get_class_names().len();
        StubStats {
            loaded_version: self.current_version,
            class_count,
            is_initialized: self.is_initialized(),
        }
    }
}

impl Default for StubIntegration {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the loaded stubs
#[derive(Debug, Clone)]
pub struct StubStats {
    pub loaded_version: Option<MinorVersion>,
    pub class_count: usize,
    pub is_initialized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_integration() -> (StubIntegration, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create version directory structure
        let version_dir = base_path.join("rubystubs27");
        fs::create_dir_all(&version_dir).unwrap();

        // Create sample stub files
        fs::write(version_dir.join("object.rb"), "# Object class stub\nclass Object\nend").unwrap();
        fs::write(version_dir.join("string.rb"), "# String class stub\nclass String\nend").unwrap();
        fs::write(version_dir.join("array.rb"), "# Array class stub\nclass Array\nend").unwrap();

        let integration = StubIntegration::with_path(base_path);
        (integration, temp_dir)
    }

    #[test]
    fn test_integration_initialization() {
        let (mut integration, _temp_dir) = create_test_integration();
        
        assert!(!integration.is_initialized());
        
        let version = MinorVersion::new(2, 7);
        integration.initialize(version).unwrap();
        
        assert!(integration.is_initialized());
        assert_eq!(integration.get_current_version(), Some(version));
    }

    #[test]
    fn test_class_definition_lookup() {
        let (mut integration, _temp_dir) = create_test_integration();
        
        let version = MinorVersion::new(2, 7);
        integration.initialize(version).unwrap();
        
        let object_location = integration.get_class_definition("Object");
        assert!(object_location.is_some());
        
        let string_location = integration.get_class_definition("String");
        assert!(string_location.is_some());
        
        let nonexistent_location = integration.get_class_definition("NonExistent");
        assert!(nonexistent_location.is_none());
    }

    #[test]
    fn test_core_class_detection() {
        let (mut integration, _temp_dir) = create_test_integration();
        
        let version = MinorVersion::new(2, 7);
        integration.initialize(version).unwrap();
        
        assert!(integration.is_core_class("Object"));
        assert!(integration.is_core_class("String"));
        assert!(integration.is_core_class("Array"));
        assert!(!integration.is_core_class("MyCustomClass"));
    }

    #[test]
    fn test_available_classes() {
        let (mut integration, _temp_dir) = create_test_integration();
        
        let version = MinorVersion::new(2, 7);
        integration.initialize(version).unwrap();
        
        let classes = integration.get_available_classes();
        assert!(classes.contains(&"Object".to_string()));
        assert!(classes.contains(&"String".to_string()));
        assert!(classes.contains(&"Array".to_string()));
        assert_eq!(classes.len(), 3);
    }

    #[test]
    fn test_stats() {
        let (mut integration, _temp_dir) = create_test_integration();
        
        let version = MinorVersion::new(2, 7);
        integration.initialize(version).unwrap();
        
        let stats = integration.get_stats();
        assert_eq!(stats.loaded_version, Some(version));
        assert_eq!(stats.class_count, 3);
        assert!(stats.is_initialized);
    }
}