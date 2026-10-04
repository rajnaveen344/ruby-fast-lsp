//! Required standard library and gem dependency scanning for project sources.

use super::IndexerProject;
use anyhow::Result;
use log::info;
use parking_lot::Mutex;
use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

impl IndexerProject {
    /// Quick scan for dependencies without full indexing.
    /// This reads project files and extracts require/gem statements to determine
    /// which gems and stdlib modules are needed.
    pub fn scan_for_dependencies(&self) -> Result<()> {
        info!("Scanning project files for dependencies...");
        self.clear_dependencies();

        let ruby_files = self.collect_project_files()?;
        let required_stdlib = &self.required_stdlib;
        let required_gems = &self.required_gems;

        ruby_files.par_iter().for_each(|file_path| {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                Self::extract_and_track_dependencies(&content, required_stdlib, required_gems);
            }
        });

        info!(
            "Dependency scan complete: {} stdlib modules, {} gems required",
            required_stdlib.lock().len(),
            required_gems.lock().len()
        );
        Ok(())
    }

    /// Extract dependencies from content and update trackers (Static helper for parallelism)
    pub(super) fn extract_and_track_dependencies(
        content: &str,
        required_stdlib: &Arc<Mutex<HashSet<String>>>,
        required_gems: &Arc<Mutex<HashSet<String>>>,
    ) {
        let mut stdlib_deps = Vec::new();
        let mut gem_deps = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Require
            if let Some(required) = Self::parse_require_statement(trimmed)
                .filter(|_| !trimmed.starts_with("require_relative "))
            {
                if Self::is_load_path_feature(&required) {
                    stdlib_deps.push(required);
                }
            }

            // Gem
            if let Some(gem_name) = Self::parse_gem_statement(trimmed) {
                gem_deps.push(gem_name);
            }
        }

        if !stdlib_deps.is_empty() {
            required_stdlib.lock().extend(stdlib_deps);
        }
        if !gem_deps.is_empty() {
            required_gems.lock().extend(gem_deps);
        }
    }

    /// Parse a require statement and extract the module name
    fn parse_require_statement(line: &str) -> Option<String> {
        // Handle various require patterns:
        // require 'module'
        // require "module"
        // require_relative 'module'

        if line.starts_with("require ") || line.starts_with("require_relative ") {
            // Find the quoted string
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let module_name = &line[start + 1..start + 1 + end];
                    return Some(module_name.to_string());
                }
            }
        }

        None
    }

    /// Whether a `require` feature could name a file on the selected
    /// runtime's load path. The stdlib indexer keeps only features the exact
    /// runtime ships, so every bare feature is a candidate; relative, absolute,
    /// and traversing names never are, so a lookup cannot leave a load-path root.
    fn is_load_path_feature(module_name: &str) -> bool {
        let path = std::path::Path::new(module_name);
        !module_name.is_empty()
            && path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
    }

    /// Parse a gem statement from Gemfile
    fn parse_gem_statement(line: &str) -> Option<String> {
        if line.starts_with("gem ") {
            // Find the quoted gem name
            if let Some(start) = line.find('"').or_else(|| line.find('\'')) {
                let quote_char = line.chars().nth(start).unwrap();
                if let Some(end) = line[start + 1..].find(quote_char) {
                    let gem_name = &line[start + 1..start + 1 + end];
                    return Some(gem_name.to_string());
                }
            }
        }

        None
    }

    /// Clear previously tracked dependencies
    pub(super) fn clear_dependencies(&self) {
        self.required_stdlib.lock().clear();
        self.required_gems.lock().clear();
    }

    /// Get the list of required stdlib modules
    pub fn get_required_stdlib(&self) -> Vec<String> {
        self.required_stdlib.lock().iter().cloned().collect()
    }

    /// Get the list of required gems
    pub fn get_required_gems(&self) -> Vec<String> {
        self.required_gems.lock().iter().cloned().collect()
    }

    /// Check if a specific gem is required
    pub fn requires_gem(&self, gem_name: &str) -> bool {
        self.required_gems.lock().contains(gem_name)
    }
}
