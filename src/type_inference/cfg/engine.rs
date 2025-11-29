//! Type Narrowing Engine - manages CFGs for open files and provides type queries.
//!
//! This module provides the main integration point between the CFG-based type
//! narrowing system and the LSP server.

use std::collections::HashMap;
use std::time::Instant;

use parking_lot::Mutex;
use tower_lsp::lsp_types::Url;

use crate::type_inference::ruby_type::RubyType;

use super::builder::CfgBuilder;
use super::dataflow::{DataflowAnalyzer, DataflowResults, TypeState};
use super::graph::ControlFlowGraph;

/// Special key for top-level code CFG (not inside any method)
const TOP_LEVEL_CFG_KEY: usize = usize::MAX;

/// State for a single open file's CFG analysis
#[derive(Debug)]
pub struct FileCfgState {
    /// Source content
    pub content: String,
    /// CFGs for each method in the file (keyed by method start offset)
    /// Also includes top-level code under TOP_LEVEL_CFG_KEY
    pub method_cfgs: HashMap<usize, MethodCfgState>,
    /// Last analysis timestamp
    pub last_analyzed: Instant,
    /// Whether the file needs re-analysis
    pub dirty: bool,
}

impl FileCfgState {
    pub fn new(content: String) -> Self {
        Self {
            content,
            method_cfgs: HashMap::new(),
            last_analyzed: Instant::now(),
            dirty: true,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn update_content(&mut self, content: String) {
        self.content = content;
        self.dirty = true;
    }
}

/// CFG state for a single method
#[derive(Debug)]
pub struct MethodCfgState {
    /// The control flow graph
    pub cfg: ControlFlowGraph,
    /// Dataflow analysis results
    pub dataflow: DataflowResults,
    /// Byte range of the method in source
    pub start_offset: usize,
    pub end_offset: usize,
    /// Method name for debugging
    pub method_name: String,
}

impl MethodCfgState {
    /// Get the narrowed type of a variable at a specific position
    pub fn get_type_at_position(&self, var_name: &str, offset: usize) -> Option<RubyType> {
        // Find the block containing this offset
        for (block_id, block) in &self.cfg.blocks {
            if offset >= block.location.start_offset && offset <= block.location.end_offset {
                // Check exit state first (includes assignments made in this block)
                // then fall back to entry state
                if let Some(state) = self.dataflow.get_exit_state(*block_id) {
                    if let Some(ty) = state.get_type(var_name) {
                        return Some(ty.clone());
                    }
                }
                if let Some(state) = self.dataflow.get_entry_state(*block_id) {
                    if let Some(ty) = state.get_type(var_name) {
                        return Some(ty.clone());
                    }
                }
            }
        }
        None
    }

    /// Check if an offset is within this method
    pub fn contains_offset(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset <= self.end_offset
    }
}

/// The main type narrowing engine
pub struct TypeNarrowingEngine {
    /// CFG states for open files
    file_states: Mutex<HashMap<Url, FileCfgState>>,
}

impl Default for TypeNarrowingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeNarrowingEngine {
    pub fn new() -> Self {
        Self {
            file_states: Mutex::new(HashMap::new()),
        }
    }

    /// Called when a file is opened
    pub fn on_file_open(&self, uri: &Url, content: &str) {
        let mut states = self.file_states.lock();
        let state = FileCfgState::new(content.to_string());
        states.insert(uri.clone(), state);
    }

    /// Called when a file is closed
    pub fn on_file_close(&self, uri: &Url) {
        let mut states = self.file_states.lock();
        states.remove(uri);
        log::debug!("Type narrowing: dropped CFG cache for {}", uri);
    }

    /// Called when a file is changed
    pub fn on_file_change(&self, uri: &Url, content: &str) {
        let mut states = self.file_states.lock();
        if let Some(state) = states.get_mut(uri) {
            state.update_content(content.to_string());
        } else {
            // File wasn't tracked, add it now
            states.insert(uri.clone(), FileCfgState::new(content.to_string()));
        }
    }

    /// Analyze a file and build CFGs for all methods and top-level code
    pub fn analyze_file(&self, uri: &Url) {
        let mut states = self.file_states.lock();
        let Some(state) = states.get_mut(uri) else {
            return;
        };

        if !state.dirty {
            return;
        }

        let content = state.content.clone();
        drop(states); // Release lock during analysis

        // Parse the file
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let Some(program) = node.as_program_node() else {
            return;
        };

        let mut method_cfgs = HashMap::new();

        // Build CFG for top-level code (statements not inside methods)
        let statements_node = program.statements();
        let top_level_cfg = self.build_top_level_cfg(&statements_node, content.as_bytes());
        if let Some(cfg_state) = top_level_cfg {
            method_cfgs.insert(TOP_LEVEL_CFG_KEY, cfg_state);
        }

        // Find all method definitions and build CFGs
        for stmt in statements_node.body().iter() {
            self.collect_method_cfgs(&stmt, content.as_bytes(), &mut method_cfgs);
        }

        // Update state
        let mut states = self.file_states.lock();
        if let Some(state) = states.get_mut(uri) {
            state.method_cfgs = method_cfgs;
            state.last_analyzed = Instant::now();
            state.dirty = false;
        }
    }

    /// Build a CFG for top-level code (outside any method)
    fn build_top_level_cfg(
        &self,
        statements: &ruby_prism::StatementsNode,
        source: &[u8],
    ) -> Option<MethodCfgState> {
        // Check if there's any top-level code (not just class/module/method definitions)
        let has_top_level_code = statements.body().iter().any(|stmt| {
            stmt.as_def_node().is_none()
                && stmt.as_class_node().is_none()
                && stmt.as_module_node().is_none()
                && stmt.as_singleton_class_node().is_none()
        });

        if !has_top_level_code {
            return None;
        }

        // Build CFG from the statements body as a "block"
        let builder = CfgBuilder::new(source);
        let cfg = builder.build_from_statements(statements);

        // Run dataflow analysis (no parameters for top-level)
        let initial_state = TypeState::new();
        let mut analyzer = DataflowAnalyzer::new(&cfg);
        analyzer.analyze(initial_state);
        let dataflow = analyzer.into_results();

        let start_offset = statements.location().start_offset();
        let end_offset = statements.location().end_offset();

        Some(MethodCfgState {
            cfg,
            dataflow,
            start_offset,
            end_offset,
            method_name: "<top-level>".to_string(),
        })
    }

    /// Recursively collect method CFGs from AST
    fn collect_method_cfgs(
        &self,
        node: &ruby_prism::Node,
        source: &[u8],
        cfgs: &mut HashMap<usize, MethodCfgState>,
    ) {
        if let Some(def_node) = node.as_def_node() {
            let method_name = String::from_utf8_lossy(def_node.name().as_slice()).to_string();
            let start_offset = def_node.location().start_offset();
            let end_offset = def_node.location().end_offset();

            // Build CFG
            let builder = CfgBuilder::new(source);
            let cfg = builder.build_from_method(&def_node);

            // Run dataflow analysis
            let initial_state = TypeState::from_parameters(&cfg.parameters);
            let mut analyzer = DataflowAnalyzer::new(&cfg);
            analyzer.analyze(initial_state);
            let dataflow = analyzer.into_results();

            let method_state = MethodCfgState {
                cfg,
                dataflow,
                start_offset,
                end_offset,
                method_name,
            };

            cfgs.insert(start_offset, method_state);
        }

        // Recurse into class/module definitions
        if let Some(class_node) = node.as_class_node() {
            if let Some(body) = class_node.body() {
                self.collect_method_cfgs_from_body(&body, source, cfgs);
            }
        } else if let Some(module_node) = node.as_module_node() {
            if let Some(body) = module_node.body() {
                self.collect_method_cfgs_from_body(&body, source, cfgs);
            }
        } else if let Some(singleton_class) = node.as_singleton_class_node() {
            if let Some(body) = singleton_class.body() {
                self.collect_method_cfgs_from_body(&body, source, cfgs);
            }
        }
    }

    fn collect_method_cfgs_from_body(
        &self,
        body: &ruby_prism::Node,
        source: &[u8],
        cfgs: &mut HashMap<usize, MethodCfgState>,
    ) {
        if let Some(stmts) = body.as_statements_node() {
            for stmt in stmts.body().iter() {
                self.collect_method_cfgs(&stmt, source, cfgs);
            }
        } else {
            self.collect_method_cfgs(body, source, cfgs);
        }
    }

    /// Get the narrowed type of a variable at a specific position
    pub fn get_narrowed_type(&self, uri: &Url, var_name: &str, offset: usize) -> Option<RubyType> {
        // Ensure file is analyzed
        self.analyze_file(uri);

        let states = self.file_states.lock();
        let state = states.get(uri)?;

        // First, try to find a method containing this offset (more specific than top-level)
        for (key, method_state) in &state.method_cfgs {
            // Skip top-level CFG in first pass - we want to check methods first
            if *key == TOP_LEVEL_CFG_KEY {
                continue;
            }
            if method_state.contains_offset(offset) {
                return method_state.get_type_at_position(var_name, offset);
            }
        }

        // If not in any method, check top-level CFG
        if let Some(top_level) = state.method_cfgs.get(&TOP_LEVEL_CFG_KEY) {
            if top_level.contains_offset(offset) {
                return top_level.get_type_at_position(var_name, offset);
            }
        }

        None
    }

    /// Get the narrowed type at a specific line/column
    pub fn get_narrowed_type_at_line_col(
        &self,
        uri: &Url,
        var_name: &str,
        line: u32,
        col: u32,
    ) -> Option<RubyType> {
        // First, convert line/col to offset
        let states = self.file_states.lock();
        let state = states.get(uri)?;

        let offset = self.line_col_to_offset(&state.content, line, col)?;
        drop(states);

        self.get_narrowed_type(uri, var_name, offset)
    }

    /// Convert line/column to byte offset
    fn line_col_to_offset(&self, content: &str, line: u32, col: u32) -> Option<usize> {
        let mut current_line = 1u32;
        let mut current_col = 0u32;

        for (i, ch) in content.char_indices() {
            if current_line == line && current_col == col {
                return Some(i);
            }

            if ch == '\n' {
                if current_line == line {
                    // We're past the target column on this line
                    return Some(i);
                }
                current_line += 1;
                current_col = 0;
            } else {
                current_col += 1;
            }
        }

        // If we're at the end of the file
        if current_line == line {
            return Some(content.len());
        }

        None
    }

    /// Get all method CFGs for a file (for debugging/testing)
    pub fn get_method_cfgs(&self, uri: &Url) -> Vec<(String, usize, usize)> {
        self.analyze_file(uri);

        let states = self.file_states.lock();
        let Some(state) = states.get(uri) else {
            return Vec::new();
        };

        state
            .method_cfgs
            .values()
            .map(|m| (m.method_name.clone(), m.start_offset, m.end_offset))
            .collect()
    }

    /// Check if a file has CFG analysis available
    pub fn has_analysis(&self, uri: &Url) -> bool {
        let states = self.file_states.lock();
        states.get(uri).map(|s| !s.dirty).unwrap_or(false)
    }

    /// Get statistics for debugging
    pub fn get_stats(&self) -> (usize, usize) {
        let states = self.file_states.lock();
        let file_count = states.len();
        let method_count: usize = states.values().map(|s| s.method_cfgs.len()).sum();
        (file_count, method_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_file_lifecycle() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"
def foo(x)
  if x.nil?
    "nil"
  else
    x.upcase
  end
end
"#;

        // Open file
        engine.on_file_open(&uri, source);

        // Analyze
        engine.analyze_file(&uri);

        // Check methods were found
        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].0, "foo");

        // Close file
        engine.on_file_close(&uri);

        // Methods should be gone
        let methods = engine.get_method_cfgs(&uri);
        assert!(methods.is_empty());
    }

    #[test]
    fn test_engine_file_change() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source1 = "def foo; end";
        let source2 = "def foo; end\ndef bar; end";

        engine.on_file_open(&uri, source1);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);

        // Change file
        engine.on_file_change(&uri, source2);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_line_col_to_offset() {
        let engine = TypeNarrowingEngine::new();
        let content = "def foo\n  x = 1\nend";

        // Line 1, col 0 -> offset 0
        assert_eq!(engine.line_col_to_offset(content, 1, 0), Some(0));

        // Line 2, col 0 -> offset 8 (after "def foo\n")
        assert_eq!(engine.line_col_to_offset(content, 2, 0), Some(8));

        // Line 2, col 2 -> offset 10
        assert_eq!(engine.line_col_to_offset(content, 2, 2), Some(10));
    }

    #[test]
    fn test_top_level_code_type_inference() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"a = [1, 2, 3]
a.each"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Check that top-level CFG was created
        {
            let states = engine.file_states.lock();
            let state = states.get(&uri).unwrap();
            assert!(
                state.method_cfgs.contains_key(&TOP_LEVEL_CFG_KEY),
                "Should have top-level CFG"
            );
        }

        // Get type of 'a' at line 2 (the a.each line)
        // Line 2 starts at offset 14 (after "a = [1, 2, 3]\n")
        let offset = 14; // Start of line 2
        let narrowed_type = engine.get_narrowed_type(&uri, "a", offset);
        assert!(
            narrowed_type.is_some(),
            "Should have narrowed type for 'a' at top level"
        );

        // Verify it's an Array type
        let ty = narrowed_type.unwrap();
        match ty {
            RubyType::Array(_) => {}
            _ => panic!("Expected Array type, got {:?}", ty),
        }
    }

    #[test]
    fn test_top_level_string_variable() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"name = "hello"
puts name."#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Get type of 'name' at line 2 (the puts name. line)
        let offset = 20; // somewhere on line 2
        let narrowed_type = engine.get_narrowed_type(&uri, "name", offset);

        assert!(
            narrowed_type.is_some(),
            "Should have narrowed type for 'name' at top level"
        );

        // Verify it's a String type
        let ty = narrowed_type.unwrap();
        assert_eq!(ty, RubyType::string(), "Expected String type, got {:?}", ty);
    }

    #[test]
    fn test_top_level_string_with_method_call() {
        // Test the exact scenario: name = "hello" followed by name.
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"name = "hello"
name.upcase"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Get type of 'name' at the method call position
        let offset = 20; // somewhere on line 2 after "name."
        let narrowed_type = engine.get_narrowed_type(&uri, "name", offset);

        assert!(
            narrowed_type.is_some(),
            "Should have narrowed type for 'name'"
        );

        let ty = narrowed_type.unwrap();

        // Check it's String
        match &ty {
            RubyType::Class(fqn) => {
                let name = fqn.to_string();
                assert!(
                    name.contains("String"),
                    "Expected String class, got: {}",
                    name
                );
            }
            _ => panic!("Expected Class type, got {:?}", ty),
        }
    }
}
