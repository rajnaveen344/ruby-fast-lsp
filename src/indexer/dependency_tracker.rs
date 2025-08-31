use crate::indexer::indexer_gem::IndexerGem;
use log::debug;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;

/// Tracks dependencies between Ruby files based on require statements
#[derive(Debug)]
pub struct DependencyTracker {
    /// Maps file URIs to their direct dependencies
    dependencies: HashMap<Url, Vec<Url>>,
    /// Queue of files that need to be processed for dependencies
    processing_queue: VecDeque<Url>,
    /// Set of files that have already been processed
    processed_files: HashSet<Url>,
    /// Ruby lib directories for standard library resolution
    ruby_lib_dirs: Vec<PathBuf>,
    /// Project root directory
    project_root: PathBuf,
    /// Gem indexer for resolving gem dependencies
    gem_indexer: Option<IndexerGem>,
}

/// Represents a require statement found in Ruby code
#[derive(Debug, Clone, PartialEq)]
pub struct RequireStatement {
    /// The required path as specified in the code
    pub path: String,
    /// Whether this is a require_relative statement
    pub is_relative: bool,
    /// The file that contains this require statement
    pub source_file: Url,
}

impl DependencyTracker {
    /// Create a new dependency tracker
    pub fn new(project_root: PathBuf, ruby_lib_dirs: Vec<PathBuf>) -> Self {
        Self {
            dependencies: HashMap::new(),
            processing_queue: VecDeque::new(),
            processed_files: HashSet::new(),
            ruby_lib_dirs,
            project_root,
            gem_indexer: None,
        }
    }

    /// Set the gem indexer for enhanced gem resolution
    pub fn set_gem_indexer(&mut self, gem_indexer: IndexerGem) {
        self.gem_indexer = Some(gem_indexer);
    }

    /// Get a reference to the gem indexer
    pub fn gem_indexer(&self) -> Option<&IndexerGem> {
        self.gem_indexer.as_ref()
    }

    /// Add a require statement found during parsing
    pub fn add_require(&mut self, require_stmt: RequireStatement) {
        debug!("Adding require statement: {:?}", require_stmt);

        if let Some(resolved_path) = self.resolve_require_path(&require_stmt) {
            // Add dependency relationship
            let deps = self
                .dependencies
                .entry(require_stmt.source_file.clone())
                .or_insert_with(Vec::new);
            if !deps.contains(&resolved_path) {
                deps.push(resolved_path.clone());
                debug!(
                    "Added dependency: {:?} -> {:?}",
                    require_stmt.source_file, resolved_path
                );
            }

            // Add to processing queue if not already processed (for recursive dependency discovery)
            if !self.processed_files.contains(&resolved_path)
                && !self.processing_queue.contains(&resolved_path)
            {
                self.processing_queue.push_back(resolved_path.clone());
                debug!("Queued for recursive processing: {:?}", resolved_path);
            }
        } else {
            debug!("Could not resolve require path: {:?}", require_stmt.path);
        }
    }

    /// Process all dependencies recursively
    pub fn process_dependencies_recursively(&mut self) -> Vec<Url> {
        let mut all_processed = Vec::new();

        while let Some(file_uri) = self.processing_queue.pop_front() {
            if self.processed_files.contains(&file_uri) {
                continue;
            }

            self.processed_files.insert(file_uri.clone());
            all_processed.push(file_uri.clone());

            // Here we would parse the file and extract its require statements
            // This would be called from the indexing coordinator
            debug!("Processed file for dependencies: {:?}", file_uri);
        }

        all_processed
    }

    /// Get all transitive dependencies for a file
    pub fn get_transitive_dependencies(&self, file_uri: &Url) -> Vec<Url> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.collect_transitive_deps(file_uri, &mut visited, &mut result);
        result
    }

    /// Recursively collect transitive dependencies
    fn collect_transitive_deps(
        &self,
        file_uri: &Url,
        visited: &mut HashSet<Url>,
        result: &mut Vec<Url>,
    ) {
        if visited.contains(file_uri) {
            return; // Avoid cycles
        }

        visited.insert(file_uri.clone());

        if let Some(deps) = self.dependencies.get(file_uri) {
            for dep in deps {
                if !result.contains(dep) {
                    result.push(dep.clone());
                }
                self.collect_transitive_deps(dep, visited, result);
            }
        }
    }

    /// Add a file to the processing queue
    pub fn add_file_to_queue(&mut self, file_path: PathBuf) {
        if let Ok(uri) = Url::from_file_path(&file_path) {
            if !self.processed_files.contains(&uri) && !self.processing_queue.contains(&uri) {
                self.processing_queue.push_back(uri);
            }
        }
    }

    /// Get the next file to process from the queue
    pub fn get_next_file(&mut self) -> Option<PathBuf> {
        if let Some(uri) = self.processing_queue.pop_front() {
            self.processed_files.insert(uri.clone());
            uri.to_file_path().ok()
        } else {
            None
        }
    }

    /// Get the next file that needs to be processed for dependencies
    pub fn next_file_to_process(&mut self) -> Option<Url> {
        self.processing_queue.pop_front()
    }

    /// Mark a file as processed
    pub fn mark_processed(&mut self, file_uri: &Url) {
        self.processed_files.insert(file_uri.clone());
        debug!("Marked file as processed: {:?}", file_uri);
    }

    /// Check if there are more files to process
    pub fn has_pending_files(&self) -> bool {
        !self.processing_queue.is_empty()
    }

    /// Get all dependencies for a given file
    pub fn get_dependencies(&self, file_uri: &Url) -> Vec<Url> {
        self.dependencies.get(file_uri).cloned().unwrap_or_default()
    }

    /// Get all files that depend on a given file
    pub fn get_dependents(&self, file_uri: &Url) -> Vec<Url> {
        self.dependencies
            .iter()
            .filter_map(|(source, deps)| {
                if deps.contains(file_uri) {
                    Some(source.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Resolve a require path to an actual file URI
    fn resolve_require_path(&self, require_stmt: &RequireStatement) -> Option<Url> {
        if require_stmt.is_relative {
            self.resolve_relative_require(require_stmt)
        } else {
            self.resolve_absolute_require(require_stmt)
        }
    }

    /// Resolve a require_relative statement
    fn resolve_relative_require(&self, require_stmt: &RequireStatement) -> Option<Url> {
        let source_path = require_stmt.source_file.to_file_path().ok()?;
        let source_dir = source_path.parent()?;

        // Helper function to safely convert path to URL, canonicalizing only when necessary
        let safe_path_to_url = |path: PathBuf| -> Option<Url> {
            // Check if path contains problematic patterns that need canonicalization
            let path_str = path.to_string_lossy();
            let needs_canonicalization = path_str.contains("../")
                || path_str.contains("./")
                || path_str.matches("../").count() > 3; // Detect potential infinite loops

            if needs_canonicalization {
                if let Ok(canonical_path) = path.canonicalize() {
                    Url::from_file_path(canonical_path).ok()
                } else {
                    // Fallback to non-canonical if canonicalization fails
                    Url::from_file_path(path).ok()
                }
            } else {
                Url::from_file_path(path).ok()
            }
        };

        // Try with .rb extension first
        let mut target_path = source_dir.join(&require_stmt.path);
        if !target_path.extension().is_some() {
            target_path.set_extension("rb");
        }

        if target_path.exists() {
            return safe_path_to_url(target_path);
        }

        // Try without extension if it was already provided
        if require_stmt.path.ends_with(".rb") {
            let path_without_ext = require_stmt.path.strip_suffix(".rb").unwrap();
            let target_path = source_dir.join(path_without_ext);
            if target_path.exists() {
                return safe_path_to_url(target_path);
            }
        }

        debug!(
            "Could not resolve relative require: {:?} from {:?}",
            require_stmt.path, source_path
        );
        None
    }

    /// Resolve an absolute require statement
    fn resolve_absolute_require(&self, require_stmt: &RequireStatement) -> Option<Url> {
        let require_path = &require_stmt.path;

        // First, try to resolve in project directories
        if let Some(project_file) = self.resolve_in_project(require_path) {
            return Some(project_file);
        }

        // Then try standard library directories
        if let Some(stdlib_file) = self.resolve_in_stdlib(require_path) {
            return Some(stdlib_file);
        }

        debug!("Could not resolve absolute require: {:?}", require_path);
        None
    }

    /// Try to resolve a require in project directories
    fn resolve_in_project(&self, require_path: &str) -> Option<Url> {
        // Common project directories to search in order of priority
        let search_dirs = vec![
            self.project_root.join("lib"),
            self.project_root.join("app"),
            self.project_root.join("app").join("models"),
            self.project_root.join("app").join("controllers"),
            self.project_root.join("app").join("services"),
            self.project_root.join("app").join("helpers"),
            self.project_root.join("config"),
            self.project_root.join("spec"),
            self.project_root.join("test"),
            self.project_root.clone(),
        ];

        for search_dir in search_dirs {
            if let Some(resolved) = self.try_resolve_in_dir(&search_dir, require_path) {
                debug!(
                    "Resolved '{}' in project directory: {:?}",
                    require_path, search_dir
                );
                return Some(resolved);
            }
        }

        // Try resolving as a gem using the gem indexer
        if let Some(gem_resolved) = self.resolve_as_gem(require_path) {
            return Some(gem_resolved);
        }

        debug!(
            "Could not resolve '{}' in any project directory",
            require_path
        );
        None
    }

    /// Try to resolve a require in standard library directories
    fn resolve_in_stdlib(&self, require_path: &str) -> Option<Url> {
        // First try direct resolution in lib directories
        for lib_dir in &self.ruby_lib_dirs {
            if let Some(resolved) = self.try_resolve_in_dir(lib_dir, require_path) {
                debug!(
                    "Resolved '{}' in stdlib directory: {:?}",
                    require_path, lib_dir
                );
                return Some(resolved);
            }
        }

        // Try common stdlib subdirectories
        let stdlib_subdirs = [
            "ruby",
            "ruby/core",
            "ruby/stdlib",
            "gems",
            "site_ruby",
            "vendor_ruby",
        ];

        for lib_dir in &self.ruby_lib_dirs {
            for subdir in &stdlib_subdirs {
                let search_path = lib_dir.join(subdir);
                if search_path.exists() {
                    if let Some(resolved) = self.try_resolve_in_dir(&search_path, require_path) {
                        debug!(
                            "Resolved '{}' in stdlib subdirectory: {:?}",
                            require_path, search_path
                        );
                        return Some(resolved);
                    }
                }
            }
        }

        // Try version-specific directories (e.g., 3.0.0, 3.1.0)
        for lib_dir in &self.ruby_lib_dirs {
            if let Ok(entries) = std::fs::read_dir(lib_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                            // Check if it looks like a version directory (e.g., "3.0.0", "3.1")
                            if dir_name
                                .chars()
                                .next()
                                .map_or(false, |c| c.is_ascii_digit())
                            {
                                if let Some(resolved) = self.try_resolve_in_dir(&path, require_path)
                                {
                                    debug!(
                                        "Resolved '{}' in version-specific stdlib: {:?}",
                                        require_path, path
                                    );
                                    return Some(resolved);
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!(
            "Could not resolve '{}' in any stdlib directory",
            require_path
        );
        None
    }

    /// Try to resolve a require as a gem
    fn resolve_as_gem(&self, require_path: &str) -> Option<Url> {
        // Only resolve using the gem indexer - gems should only be searched in gem paths
        if let Some(gem_indexer) = &self.gem_indexer {
            if let Some(resolved) = self.resolve_with_gem_indexer(require_path, gem_indexer) {
                debug!("Resolved '{}' using gem indexer", require_path);
                return Some(resolved);
            }
        }

        debug!("Could not resolve '{}' as a gem", require_path);
        None
    }

    /// Resolve a require path using the gem indexer
    fn resolve_with_gem_indexer(
        &self,
        require_path: &str,
        gem_indexer: &IndexerGem,
    ) -> Option<Url> {
        // Try to find the gem that contains this require path
        for gem in gem_indexer.get_all_gems() {
            for lib_path in &gem.lib_paths {
                if let Some(resolved) = self.try_resolve_in_dir(lib_path, require_path) {
                    debug!(
                        "Resolved '{}' in gem '{}' at {:?}",
                        require_path, gem.name, lib_path
                    );
                    return Some(resolved);
                }
            }
        }

        // Also check gem paths directly
        for gem_path in gem_indexer.get_gem_lib_paths() {
            if let Some(resolved) = self.try_resolve_in_dir(&gem_path, require_path) {
                debug!("Resolved '{}' in gem path: {:?}", require_path, gem_path);
                return Some(resolved);
            }
        }

        None
    }

    /// Try to resolve a file in a specific directory
    fn try_resolve_in_dir(&self, dir: &Path, require_path: &str) -> Option<Url> {
        if !dir.exists() {
            return None;
        }

        // Helper function to safely convert path to URL, canonicalizing only when necessary
        let safe_path_to_url = |path: PathBuf| -> Option<Url> {
            // Check if path contains problematic patterns that need canonicalization
            let path_str = path.to_string_lossy();
            let needs_canonicalization = path_str.contains("../")
                || path_str.contains("./")
                || path_str.matches("../").count() > 3; // Detect potential infinite loops

            if needs_canonicalization {
                if let Ok(canonical_path) = path.canonicalize() {
                    Url::from_file_path(canonical_path).ok()
                } else {
                    // Fallback to non-canonical if canonicalization fails
                    Url::from_file_path(path).ok()
                }
            } else {
                Url::from_file_path(path).ok()
            }
        };

        // Strategy 1: Try exact path as provided
        let exact_path = dir.join(require_path);
        if exact_path.exists() && self.is_ruby_file(&exact_path) {
            return safe_path_to_url(exact_path);
        }

        // Strategy 2: Try with .rb extension if no extension provided
        if !require_path.contains('.') {
            let rb_path = dir.join(format!("{}.rb", require_path));
            if rb_path.exists() && self.is_ruby_file(&rb_path) {
                return safe_path_to_url(rb_path);
            }
        }

        // Strategy 3: Try as directory with index.rb
        let index_path = dir.join(require_path).join("index.rb");
        if index_path.exists() {
            return safe_path_to_url(index_path);
        }

        // Strategy 4: Try without extension if .rb was provided
        if require_path.ends_with(".rb") {
            let path_without_ext = require_path.strip_suffix(".rb").unwrap();
            let target_path = dir.join(path_without_ext);
            if target_path.exists() && self.is_ruby_file(&target_path) {
                return safe_path_to_url(target_path);
            }
        }

        // Strategy 5: Try with common Ruby file extensions
        for ext in ["rake", "ruby"] {
            let ext_path = dir.join(format!("{}.{}", require_path, ext));
            if ext_path.exists() && self.is_ruby_file(&ext_path) {
                return safe_path_to_url(ext_path);
            }
        }

        // Strategy 6: Handle nested paths (e.g., "foo/bar" -> "foo/bar.rb")
        if require_path.contains('/') {
            let nested_rb_path = dir.join(format!("{}.rb", require_path));
            if nested_rb_path.exists() && self.is_ruby_file(&nested_rb_path) {
                return safe_path_to_url(nested_rb_path);
            }
        }

        None
    }

    /// Check if a file is a Ruby file
    fn is_ruby_file(&self, path: &Path) -> bool {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            matches!(extension, "rb" | "ruby" | "rake")
        } else {
            // Check for files without extension that might be Ruby
            if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                matches!(filename, "Rakefile" | "Gemfile" | "Guardfile" | "Capfile")
            } else {
                false
            }
        }
    }

    /// Get statistics about the dependency tracking
    pub fn get_stats(&self) -> DependencyStats {
        let total_files = self.dependencies.len();
        let total_dependencies = self.dependencies.values().map(|deps| deps.len()).sum();
        let pending_files = self.processing_queue.len();
        let processed_files = self.processed_files.len();

        DependencyStats {
            total_files,
            total_dependencies,
            pending_files,
            processed_files,
        }
    }

    /// Add a stdlib dependency for tracking
    pub fn add_stdlib_dependency(&mut self, stdlib_name: String) {
        debug!("Adding stdlib dependency: {}", stdlib_name);
        // This could be used to track which stdlib modules are required
        // For now, we just log it
    }

    /// Add a gem dependency for tracking
    pub fn add_gem_dependency(&mut self, gem_name: String) {
        debug!("Adding gem dependency: {}", gem_name);
        // This could be used to track which gems are required
        // For now, we just log it
    }

    /// Clear all tracking data
    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.processing_queue.clear();
        self.processed_files.clear();
    }
}

/// Statistics about dependency tracking
#[derive(Debug, Clone)]
pub struct DependencyStats {
    pub total_files: usize,
    pub total_dependencies: usize,
    pub pending_files: usize,
    pub processed_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_tracker() -> (DependencyTracker, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();
        let ruby_lib_dirs = vec![temp_dir.path().join("lib")];
        let tracker = DependencyTracker::new(project_root, ruby_lib_dirs);
        (tracker, temp_dir)
    }

    #[test]
    fn test_relative_require_resolution() {
        let (tracker, temp_dir) = create_test_tracker();

        // Create test files
        let main_file = temp_dir.path().join("main.rb");
        let helper_file = temp_dir.path().join("helper.rb");

        fs::write(&main_file, "require_relative 'helper'").unwrap();
        fs::write(&helper_file, "# helper code").unwrap();

        let main_uri = Url::from_file_path(&main_file).unwrap();
        let require_stmt = RequireStatement {
            path: "helper".to_string(),
            is_relative: true,
            source_file: main_uri.clone(),
        };

        let resolved = tracker.resolve_require_path(&require_stmt);
        assert!(resolved.is_some());

        let expected_uri = Url::from_file_path(&helper_file).unwrap();
        assert_eq!(resolved.unwrap(), expected_uri);
    }

    #[test]
    fn test_dependency_queue() {
        let (mut tracker, _temp_dir) = create_test_tracker();

        assert!(!tracker.has_pending_files());

        let test_path = PathBuf::from("/test/file.rb");
        tracker.add_file_to_queue(test_path);

        assert!(tracker.has_pending_files());

        let next_file = tracker.get_next_file();
        assert!(next_file.is_some());
        assert_eq!(next_file.unwrap(), PathBuf::from("/test/file.rb"));

        assert!(!tracker.has_pending_files());
    }

    #[test]
    fn test_absolute_require_resolution() {
        let (tracker, temp_dir) = create_test_tracker();

        // Create lib directory and test file
        let lib_dir = temp_dir.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();
        let json_file = lib_dir.join("json.rb");
        fs::write(&json_file, "# JSON library").unwrap();

        let main_file = temp_dir.path().join("main.rb");
        let main_uri = Url::from_file_path(&main_file).unwrap();

        let require_stmt = RequireStatement {
            path: "json".to_string(),
            is_relative: false,
            source_file: main_uri,
        };

        let resolved = tracker.resolve_require_path(&require_stmt);
        assert!(resolved.is_some());
    }

    #[test]
    fn test_dependency_tracking() {
        let (mut tracker, temp_dir) = create_test_tracker();

        // Create test files
        let main_file = temp_dir.path().join("main.rb");
        let helper_file = temp_dir.path().join("helper.rb");

        fs::write(&main_file, "require_relative 'helper'").unwrap();
        fs::write(&helper_file, "# helper code").unwrap();

        let main_uri = Url::from_file_path(&main_file).unwrap();
        let helper_uri = Url::from_file_path(&helper_file).unwrap();

        let require_stmt = RequireStatement {
            path: "helper".to_string(),
            is_relative: true,
            source_file: main_uri.clone(),
        };

        tracker.add_require(require_stmt);

        let deps = tracker.get_dependencies(&main_uri);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], helper_uri);

        let dependents = tracker.get_dependents(&helper_uri);
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0], main_uri);
    }

    #[test]
    fn test_transitive_dependencies() {
        let (mut tracker, temp_dir) = create_test_tracker();

        // Create test files: main -> helper -> utils
        let main_file = temp_dir.path().join("main.rb");
        let helper_file = temp_dir.path().join("helper.rb");
        let utils_file = temp_dir.path().join("utils.rb");

        fs::write(&main_file, "require_relative 'helper'").unwrap();
        fs::write(&helper_file, "require_relative 'utils'").unwrap();
        fs::write(&utils_file, "# utils code").unwrap();

        let main_uri = Url::from_file_path(&main_file).unwrap();
        let helper_uri = Url::from_file_path(&helper_file).unwrap();
        let utils_uri = Url::from_file_path(&utils_file).unwrap();

        // Add main -> helper dependency
        tracker.add_require(RequireStatement {
            path: "helper".to_string(),
            is_relative: true,
            source_file: main_uri.clone(),
        });

        // Add helper -> utils dependency
        tracker.add_require(RequireStatement {
            path: "utils".to_string(),
            is_relative: true,
            source_file: helper_uri.clone(),
        });

        let transitive_deps = tracker.get_transitive_dependencies(&main_uri);
        assert_eq!(transitive_deps.len(), 2);
        assert!(transitive_deps.contains(&helper_uri));
        assert!(transitive_deps.contains(&utils_uri));
    }

    #[test]
    fn test_circular_dependency_handling() {
        let (mut tracker, temp_dir) = create_test_tracker();

        // Create circular dependency: a -> b -> a
        let file_a = temp_dir.path().join("a.rb");
        let file_b = temp_dir.path().join("b.rb");

        fs::write(&file_a, "require_relative 'b'").unwrap();
        fs::write(&file_b, "require_relative 'a'").unwrap();

        let uri_a = Url::from_file_path(&file_a).unwrap();
        let uri_b = Url::from_file_path(&file_b).unwrap();

        // Add a -> b dependency
        tracker.add_require(RequireStatement {
            path: "b".to_string(),
            is_relative: true,
            source_file: uri_a.clone(),
        });

        // Add b -> a dependency
        tracker.add_require(RequireStatement {
            path: "a".to_string(),
            is_relative: true,
            source_file: uri_b.clone(),
        });

        // Should handle circular dependencies without infinite loop
        let transitive_deps = tracker.get_transitive_dependencies(&uri_a);
        assert!(transitive_deps.len() >= 1); // Should contain at least uri_b
        assert!(transitive_deps.contains(&uri_b));
    }

    #[test]
    fn test_dependency_stats() {
        let (mut tracker, temp_dir) = create_test_tracker();

        let file1 = temp_dir.path().join("file1.rb");
        let file2 = temp_dir.path().join("file2.rb");
        fs::write(&file1, "require_relative 'file2'").unwrap();
        fs::write(&file2, "# code").unwrap();

        let uri1 = Url::from_file_path(&file1).unwrap();

        tracker.add_require(RequireStatement {
            path: "file2".to_string(),
            is_relative: true,
            source_file: uri1,
        });

        let stats = tracker.get_stats();
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.total_dependencies, 1);
        assert!(stats.pending_files > 0);
    }

    #[test]
    fn test_project_path_resolution() {
        let (tracker, temp_dir) = create_test_tracker();

        // Create app/models directory structure
        let models_dir = temp_dir.path().join("app").join("models");
        fs::create_dir_all(&models_dir).unwrap();
        let user_model = models_dir.join("user.rb");
        fs::write(&user_model, "class User; end").unwrap();

        let resolved = tracker.resolve_in_project("user");
        assert!(resolved.is_some());

        let expected_uri = Url::from_file_path(&user_model).unwrap();
        assert_eq!(resolved.unwrap(), expected_uri);
    }
}
