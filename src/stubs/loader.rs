use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use log::{debug, info, warn};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::version::MinorVersion;

/// Manages Ruby core class stub files for "Go to Definition" functionality
#[derive(Debug)]
pub struct StubLoader {
    /// Base directory containing stub files (e.g., "vsix/stubs/")
    base_path: PathBuf,
    
    /// Currently loaded Ruby version
    current_version: Arc<RwLock<Option<MinorVersion>>>,
    
    /// Cache of class name to file path mappings
    class_files: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl StubLoader {
    /// Create a new stub loader with the given base path
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            current_version: Arc::new(RwLock::new(None)),
            class_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load stubs for a specific Ruby version
    /// Returns the closest available version if exact match not found
    pub fn load_version(&self, requested_version: MinorVersion) -> Result<MinorVersion> {
        let target_version = requested_version
            .find_closest_supported()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No supported Ruby version found for {}. Minimum supported version is 1.8",
                    requested_version
                )
            })?;

        if target_version != requested_version {
            info!(
                "Requested Ruby version {} not directly supported, using closest version {}",
                requested_version, target_version
            );
        }

        let version_path = self.base_path.join(target_version.to_directory_name());
        
        if !version_path.exists() {
            return Err(anyhow::anyhow!(
                "Stub directory not found: {}. Please ensure stubs are properly packaged.",
                version_path.display()
            ));
        }

        debug!("Loading stubs from: {}", version_path.display());

        // Load all .rb files in the version directory
        let mut class_files = HashMap::new();
        self.load_stub_files(&version_path, &mut class_files)?;

        // Store the loaded version and class files
        {
            let mut current = self.current_version.write().unwrap();
            *current = Some(target_version);
        }
        {
            let mut files = self.class_files.write().unwrap();
            *files = class_files;
        }

        info!("Successfully loaded {} stub files for Ruby {}", 
              self.class_files.read().unwrap().len(), target_version);
        Ok(target_version)
    }

    /// Load all .rb stub files from a directory
    fn load_stub_files(&self, dir_path: &Path, class_files: &mut HashMap<String, PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir_path)
            .with_context(|| format!("Failed to read stub directory: {}", dir_path.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("rb") {
                 if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                     // Convert file name to class name (e.g., "string.rb" -> "String")
                     let class_name = self.file_name_to_class_name(file_stem);
                     debug!("Found stub file for class: {}", class_name);
                     class_files.insert(class_name, path);
                 }
             }
        }
        Ok(())
    }

    /// Convert a file name to a class name
    /// Examples: "string" -> "String", "io_error" -> "IOError", "open_ssl" -> "OpenSSL"
    fn file_name_to_class_name(&self, file_name: &str) -> String {
        match file_name {
            "io_error" => "IOError".to_string(),
            "open_ssl" => "OpenSSL".to_string(),
            "big_math" => "BigMath".to_string(),
            "ruby_vm" => "RubyVM".to_string(),
            "tk_util" => "TkUtil".to_string(),
            _ => {
                // Convert snake_case to PascalCase
                file_name
                    .split('_')
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        }
                    })
                    .collect()
            }
        }
    }

    /// Switch to a different Ruby version
    pub fn switch_version(&self, new_version: MinorVersion) -> Result<MinorVersion> {
        debug!("Switching to Ruby version {}", new_version);
        self.load_version(new_version)
    }

    /// Get the file location for a Ruby core class
    pub fn get_class_location(&self, class_name: &str) -> Option<Location> {
        let files = self.class_files.read().unwrap();
        let file_path = files.get(class_name)?;
        
        // Convert file path to URI
        let uri = Url::from_file_path(file_path).ok()?;
        
        // Return location pointing to the beginning of the file
        Some(Location {
            uri,
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 0 },
            },
        })
    }

    /// Get all available class names for the current version
    pub fn get_class_names(&self) -> Vec<String> {
        let files = self.class_files.read().unwrap();
        files.keys().cloned().collect()
    }

    /// Get the currently loaded Ruby version
    pub fn get_loaded_version(&self) -> Option<MinorVersion> {
        let current = self.current_version.read().unwrap();
        *current
    }

    /// Check if a version is currently loaded
    pub fn is_version_loaded(&self, version: MinorVersion) -> bool {
        self.get_loaded_version() == Some(version)
    }

    /// Check if stubs are available for a version (directory exists)
    pub fn is_version_available(&self, version: MinorVersion) -> bool {
        let version_path = self.base_path.join(version.to_directory_name());
        version_path.exists()
    }

    /// Check if a class has a stub file available
    pub fn has_class(&self, class_name: &str) -> bool {
        let files = self.class_files.read().unwrap();
        files.contains_key(class_name)
    }

    /// Find bundled stubs relative to the executable location
    /// This supports both packaged (../stubs) and development (vsix/stubs) scenarios
    fn find_bundled_stubs() -> Option<PathBuf> {
        // Try to get the executable path
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // In VSIX package: executable is in bin/<platform>/, stubs are in stubs/
                // So we need to go up two levels: bin/<platform>/ -> bin/ -> root/ -> stubs/
                let vsix_stubs = exe_dir.parent()?.parent()?.join("stubs");
                if vsix_stubs.exists() {
                    debug!("Found VSIX bundled stubs at: {}", vsix_stubs.display());
                    return Some(vsix_stubs);
                }
                
                // Alternative: stubs might be directly adjacent to the executable
                let adjacent_stubs = exe_dir.join("stubs");
                if adjacent_stubs.exists() {
                    debug!("Found adjacent stubs at: {}", adjacent_stubs.display());
                    return Some(adjacent_stubs);
                }
            }
        }
        
        // Try development scenario: vsix/stubs in current working directory
        let dev_stubs = PathBuf::from("vsix/stubs");
        if dev_stubs.exists() {
            debug!("Found development stubs at: {}", dev_stubs.display());
            return Some(dev_stubs);
        }
        
        None
    }
}

impl Default for StubLoader {
    fn default() -> Self {
        // Try to find stubs relative to the executable location first
        if let Some(stub_path) = Self::find_bundled_stubs() {
            debug!("Using bundled stubs at: {}", stub_path.display());
            Self::new(stub_path)
        } else {
            // Fallback to development path
            warn!("Bundled stubs not found, falling back to development path: vsix/stubs");
            Self::new("vsix/stubs")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_stub_structure() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create version directory
        let version_dir = base_path.join("rubystubs27");
        fs::create_dir_all(&version_dir).unwrap();

        // Create sample stub files
        fs::write(version_dir.join("object.rb"), "# Object class stub").unwrap();
        fs::write(version_dir.join("string.rb"), "# String class stub").unwrap();
        fs::write(version_dir.join("array.rb"), "# Array class stub").unwrap();

        temp_dir
    }

    #[test]
    fn test_stub_loader_creation() {
        let temp_dir = create_test_stub_structure();
        let loader = StubLoader::new(temp_dir.path());
        
        assert_eq!(loader.base_path, temp_dir.path());
        assert!(loader.get_loaded_version().is_none());
    }

    #[test]
    fn test_load_version() {
        let temp_dir = create_test_stub_structure();
        let loader = StubLoader::new(temp_dir.path());
        
        let version = MinorVersion::new(2, 7);
        let loaded_version = loader.load_version(version).unwrap();
        
        assert_eq!(loaded_version, version);
        assert_eq!(loader.get_loaded_version(), Some(version));
    }

    #[test]
    fn test_get_class_location() {
        let temp_dir = create_test_stub_structure();
        let loader = StubLoader::new(temp_dir.path());
        
        let version = MinorVersion::new(2, 7);
        loader.load_version(version).unwrap();
        
        let object_location = loader.get_class_location("Object");
        assert!(object_location.is_some());
        
        let string_location = loader.get_class_location("String");
        assert!(string_location.is_some());
        
        let nonexistent_location = loader.get_class_location("NonExistent");
        assert!(nonexistent_location.is_none());
    }

    #[test]
    fn test_file_name_to_class_name() {
        let loader = StubLoader::default();
        
        assert_eq!(loader.file_name_to_class_name("object"), "Object");
        assert_eq!(loader.file_name_to_class_name("string"), "String");
        assert_eq!(loader.file_name_to_class_name("io_error"), "IOError");
        assert_eq!(loader.file_name_to_class_name("open_ssl"), "OpenSSL");
        assert_eq!(loader.file_name_to_class_name("big_math"), "BigMath");
    }

    #[test]
    fn test_has_class() {
        let temp_dir = create_test_stub_structure();
        let loader = StubLoader::new(temp_dir.path());
        
        let version = MinorVersion::new(2, 7);
        loader.load_version(version).unwrap();
        
        assert!(loader.has_class("Object"));
        assert!(loader.has_class("String"));
        assert!(loader.has_class("Array"));
        assert!(!loader.has_class("NonExistent"));
    }
}