use parking_lot::Mutex;
use ruby_prism::Node;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

use crate::inferrer::cfg::builder::CfgBuilder;
use crate::inferrer::cfg::dataflow::{DataflowAnalyzer, DataflowResults, TypeState};
use crate::inferrer::cfg::graph::ControlFlowGraph;
use crate::inferrer::r#type::ruby::RubyType;

/// Special key for top-level CFG (not inside any method)
const TOP_LEVEL_CFG_KEY: usize = usize::MAX;

/// The main type narrowing engine.
///
/// This struct is the entry point for CFG-based type inference.
/// It manages the lifecycle of CFG states for open files and provides
/// methods to query narrowed types at specific positions.
pub struct TypeNarrowingEngine {
    file_states: Mutex<HashMap<Url, FileCfgState>>,
}

impl Default for TypeNarrowingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Public API
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
            state.content = content.to_string();
            state.method_cfgs.clear();
            state.analyzed = false;
        } else {
            states.insert(uri.clone(), FileCfgState::new(content.to_string()));
        }
    }

    /// Get the narrowed type of a variable at a specific position.
    ///
    /// This uses lazy analysis - it only analyzes the method containing the offset
    /// if it hasn't been analyzed yet.
    ///
    /// The variable name is inferred from the offset.
    pub fn get_narrowed_type(
        &self,
        uri: &Url,
        offset: usize,
        content: Option<&str>,
    ) -> Option<RubyType> {
        // Ensure file is registered if content is provided
        if let Some(c) = content {
            let mut states = self.file_states.lock();
            if !states.contains_key(uri) {
                states.insert(uri.clone(), FileCfgState::new(c.to_string()));
            }
        }

        // No cached CFG found - do lazy analysis for just the method at this offset
        self.lazy_analyze_at_offset(uri, offset);

        // Infer variable name from source
        let states = self.file_states.lock();
        let state = states.get(uri)?;
        let source = state.content.clone();
        drop(states);

        let var_name = self.infer_variable_at_offset(&source, offset)?;

        // Get the result from the newly built CFG
        let states = self.file_states.lock();
        let state = states.get(uri)?;

        for (key, method_state) in &state.method_cfgs {
            if *key == TOP_LEVEL_CFG_KEY {
                continue;
            }
            if method_state.contains_offset(offset) {
                return method_state.get_type_at_position(&var_name, offset);
            }
        }

        if let Some(top_level) = state.method_cfgs.get(&TOP_LEVEL_CFG_KEY) {
            if top_level.contains_offset(offset) {
                return top_level.get_type_at_position(&var_name, offset);
            }
        }

        None
    }
}

/// Private Helpers
impl TypeNarrowingEngine {
    /// Lazily analyze only the method containing the given offset
    fn lazy_analyze_at_offset(&self, uri: &Url, offset: usize) {
        let mut states = self.file_states.lock();
        let state = match states.get_mut(uri) {
            Some(s) => s,
            None => return,
        };

        // Check if we already have a CFG for this offset
        for (key, method_state) in &state.method_cfgs {
            if *key == TOP_LEVEL_CFG_KEY {
                continue;
            }
            if method_state.contains_offset(offset) {
                return; // Already analyzed
            }
        }

        let source = state.content.clone();
        drop(states);

        // Parse and find the method containing this offset
        let result = ruby_prism::parse(source.as_bytes());
        let root = result.node();
        let program = match root.as_program_node() {
            Some(p) => p,
            None => return,
        };

        // Find and build CFG for just the method at this offset
        if let Some(method_cfg) = self.find_and_build_method_at_offset(&program, &source, offset) {
            let mut states = self.file_states.lock();
            if let Some(state) = states.get_mut(uri) {
                state
                    .method_cfgs
                    .insert(method_cfg.start_offset, method_cfg);
            }
        } else {
            // Not in a method - build top-level CFG if not already done
            let mut states = self.file_states.lock();
            if let Some(state) = states.get_mut(uri) {
                if !state.method_cfgs.contains_key(&TOP_LEVEL_CFG_KEY) {
                    drop(states);
                    if let Some(top_level) = self.build_top_level_cfg(&root, &source) {
                        let mut states = self.file_states.lock();
                        if let Some(state) = states.get_mut(uri) {
                            state.method_cfgs.insert(TOP_LEVEL_CFG_KEY, top_level);
                        }
                    }
                }
            }
        }
    }

    /// Find and build CFG for the method containing the given offset
    fn find_and_build_method_at_offset(
        &self,
        program: &ruby_prism::ProgramNode,
        source: &str,
        offset: usize,
    ) -> Option<MethodCfgState> {
        let statements = program.statements();
        self.find_method_at_offset_recursive(&statements, source, offset)
    }

    /// Recursively search for a method containing the offset
    fn find_method_at_offset_recursive(
        &self,
        body: &ruby_prism::StatementsNode,
        source: &str,
        offset: usize,
    ) -> Option<MethodCfgState> {
        for stmt in body.body().iter() {
            let loc = stmt.location();
            if offset < loc.start_offset() || offset > loc.end_offset() {
                continue; // Skip nodes that don't contain the offset
            }

            if let Some(def_node) = stmt.as_def_node() {
                return self.build_method_cfg(&def_node, source);
            } else if let Some(class_node) = stmt.as_class_node() {
                if let Some(body) = class_node.body() {
                    if let Some(statements) = body.as_statements_node() {
                        if let Some(result) =
                            self.find_method_at_offset_recursive(&statements, source, offset)
                        {
                            return Some(result);
                        }
                    }
                }
            } else if let Some(module_node) = stmt.as_module_node() {
                if let Some(body) = module_node.body() {
                    if let Some(statements) = body.as_statements_node() {
                        if let Some(result) =
                            self.find_method_at_offset_recursive(&statements, source, offset)
                        {
                            return Some(result);
                        }
                    }
                }
            } else if let Some(singleton_class) = stmt.as_singleton_class_node() {
                if let Some(body) = singleton_class.body() {
                    if let Some(statements) = body.as_statements_node() {
                        if let Some(result) =
                            self.find_method_at_offset_recursive(&statements, source, offset)
                        {
                            return Some(result);
                        }
                    }
                }
            }
        }
        None
    }

    /// Build CFG for a single method
    fn build_method_cfg(
        &self,
        def_node: &ruby_prism::DefNode,
        source: &str,
    ) -> Option<MethodCfgState> {
        // Build CFG
        let builder = CfgBuilder::new(source.as_bytes());
        let cfg = builder.build_from_method(def_node);

        // Get parameter types (for now, start with unknown)
        let params = self.extract_parameters(def_node);

        // Run dataflow analysis
        let mut analyzer = DataflowAnalyzer::new(&cfg);
        analyzer.analyze(TypeState::from_parameters(&params));
        let dataflow = analyzer.into_results();

        Some(MethodCfgState {
            cfg,
            dataflow,
            start_offset: def_node.location().start_offset(),
            end_offset: def_node.location().end_offset(),
            method_name: String::from_utf8_lossy(def_node.name().as_slice()).to_string(),
        })
    }

    /// Build CFG for top-level statements
    fn build_top_level_cfg(&self, root: &Node, source: &str) -> Option<MethodCfgState> {
        let program = root.as_program_node()?;
        let statements = program.statements();

        // Build CFG from top-level statements
        let builder = CfgBuilder::new(source.as_bytes());
        let cfg = builder.build_from_statements(&statements);

        // Run dataflow analysis
        let mut analyzer = DataflowAnalyzer::new(&cfg);
        analyzer.analyze(TypeState::new());
        let dataflow = analyzer.into_results();

        Some(MethodCfgState {
            cfg,
            dataflow,
            start_offset: 0,
            end_offset: source.len(),
            method_name: "<top-level>".to_string(),
        })
    }

    /// Try to infer the variable name at the given offset
    fn infer_variable_at_offset(&self, source: &str, offset: usize) -> Option<String> {
        let result = ruby_prism::parse(source.as_bytes());
        let root = result.node();
        self.find_variable_at_offset_recursive(&root, offset)
    }

    /// Recursively search for a variable at the offset
    fn find_variable_at_offset_recursive(&self, node: &Node, offset: usize) -> Option<String> {
        let loc = node.location();
        if offset < loc.start_offset() || offset >= loc.end_offset() {
            return None;
        }

        // Check if it's a variable read
        if let Some(read) = node.as_local_variable_read_node() {
            return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
        }
        if let Some(read) = node.as_instance_variable_read_node() {
            return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
        }
        if let Some(read) = node.as_class_variable_read_node() {
            return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
        }
        if let Some(read) = node.as_global_variable_read_node() {
            return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
        }

        // Check if it's a variable write (and we are on the name)
        if let Some(write) = node.as_local_variable_write_node() {
            let name_loc = write.name_loc();
            if offset >= name_loc.start_offset() && offset < name_loc.end_offset() {
                return Some(String::from_utf8_lossy(write.name().as_slice()).to_string());
            }
            if let Some(res) = self.find_variable_at_offset_recursive(&write.value(), offset) {
                return Some(res);
            }
        }
        if let Some(write) = node.as_local_variable_or_write_node() {
            let name_loc = write.name_loc();
            if offset >= name_loc.start_offset() && offset < name_loc.end_offset() {
                return Some(String::from_utf8_lossy(write.name().as_slice()).to_string());
            }
            if let Some(res) = self.find_variable_at_offset_recursive(&write.value(), offset) {
                return Some(res);
            }
        }
        if let Some(write) = node.as_local_variable_and_write_node() {
            let name_loc = write.name_loc();
            if offset >= name_loc.start_offset() && offset < name_loc.end_offset() {
                return Some(String::from_utf8_lossy(write.name().as_slice()).to_string());
            }
            if let Some(res) = self.find_variable_at_offset_recursive(&write.value(), offset) {
                return Some(res);
            }
        }
        // Handle instance variable writes
        if let Some(write) = node.as_instance_variable_write_node() {
            let name_loc = write.name_loc();
            if offset >= name_loc.start_offset() && offset < name_loc.end_offset() {
                return Some(String::from_utf8_lossy(write.name().as_slice()).to_string());
            }
            if let Some(res) = self.find_variable_at_offset_recursive(&write.value(), offset) {
                return Some(res);
            }
        }

        // Recurse into children
        if let Some(stmts) = node.as_statements_node() {
            for stmt in stmts.body().iter() {
                if let Some(res) = self.find_variable_at_offset_recursive(&stmt, offset) {
                    return Some(res);
                }
            }
        }

        if let Some(program) = node.as_program_node() {
            return self.find_variable_at_offset_recursive(&program.statements().as_node(), offset);
        }

        if let Some(def) = node.as_def_node() {
            if let Some(body) = def.body() {
                if let Some(res) = self.find_variable_at_offset_recursive(&body, offset) {
                    return Some(res);
                }
            }
        }

        if let Some(class_node) = node.as_class_node() {
            if let Some(body) = class_node.body() {
                if let Some(res) = self.find_variable_at_offset_recursive(&body, offset) {
                    return Some(res);
                }
            }
        }

        if let Some(module_node) = node.as_module_node() {
            if let Some(body) = module_node.body() {
                if let Some(res) = self.find_variable_at_offset_recursive(&body, offset) {
                    return Some(res);
                }
            }
        }

        if let Some(if_node) = node.as_if_node() {
            if let Some(stmts) = if_node.statements() {
                if let Some(res) = self.find_variable_at_offset_recursive(&stmts.as_node(), offset)
                {
                    return Some(res);
                }
            }
            if let Some(subsequent) = if_node.subsequent() {
                if let Some(res) = self.find_variable_at_offset_recursive(&subsequent, offset) {
                    return Some(res);
                }
            }
        }

        if let Some(else_node) = node.as_else_node() {
            if let Some(stmts) = else_node.statements() {
                if let Some(res) = self.find_variable_at_offset_recursive(&stmts.as_node(), offset)
                {
                    return Some(res);
                }
            }
        }

        if let Some(call) = node.as_call_node() {
            if let Some(receiver) = call.receiver() {
                if let Some(res) = self.find_variable_at_offset_recursive(&receiver, offset) {
                    return Some(res);
                }
            }
            if let Some(args) = call.arguments() {
                for arg in args.arguments().iter() {
                    if let Some(res) = self.find_variable_at_offset_recursive(&arg, offset) {
                        return Some(res);
                    }
                }
            }
        }

        None
    }

    /// Extract parameter names from a def node
    fn extract_parameters(&self, def_node: &ruby_prism::DefNode) -> Vec<(String, RubyType)> {
        let mut params = Vec::new();

        if let Some(parameters) = def_node.parameters() {
            // Required parameters
            for param in parameters.requireds().iter() {
                if let Some(req) = param.as_required_parameter_node() {
                    let name = String::from_utf8_lossy(req.name().as_slice()).to_string();
                    params.push((name, RubyType::Unknown));
                }
            }

            // Optional parameters
            for param in parameters.optionals().iter() {
                if let Some(opt) = param.as_optional_parameter_node() {
                    let name = String::from_utf8_lossy(opt.name().as_slice()).to_string();
                    params.push((name, RubyType::Unknown));
                }
            }

            // Rest parameter
            if let Some(rest) = parameters.rest() {
                if let Some(rest_param) = rest.as_rest_parameter_node() {
                    if let Some(name) = rest_param.name() {
                        let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                        params.push((name_str, RubyType::Array(vec![RubyType::Unknown])));
                    }
                }
            }

            // Keyword parameters
            for param in parameters.keywords().iter() {
                if let Some(kw) = param.as_required_keyword_parameter_node() {
                    let name = String::from_utf8_lossy(kw.name().as_slice()).to_string();
                    params.push((name, RubyType::Unknown));
                } else if let Some(kw) = param.as_optional_keyword_parameter_node() {
                    let name = String::from_utf8_lossy(kw.name().as_slice()).to_string();
                    params.push((name, RubyType::Unknown));
                }
            }

            // Block parameter
            if let Some(block) = parameters.block() {
                if let Some(name) = block.name() {
                    let name_str = String::from_utf8_lossy(name.as_slice()).to_string();
                    params.push((name_str, RubyType::Unknown));
                }
            }
        }

        params
    }
}

/// Test Helpers (used by integration tests in mod.rs)
#[cfg(test)]
impl TypeNarrowingEngine {
    /// Analyze a file and build CFGs for all methods (Eager analysis for testing)
    pub fn analyze_file(&self, uri: &Url) {
        let mut states = self.file_states.lock();
        let state = match states.get_mut(uri) {
            Some(s) => s,
            None => return,
        };

        if state.analyzed {
            return;
        }

        let source = state.content.clone();
        drop(states);

        // Parse the file
        let result = ruby_prism::parse(source.as_bytes());
        let root = result.node();
        let program = match root.as_program_node() {
            Some(p) => p,
            None => return,
        };

        // Build top-level CFG
        if let Some(top_level) = self.build_top_level_cfg(&root, &source) {
            let mut states = self.file_states.lock();
            if let Some(state) = states.get_mut(uri) {
                state.method_cfgs.insert(TOP_LEVEL_CFG_KEY, top_level);
            }
        }

        // Collect all method CFGs
        let statements = program.statements();
        self.collect_method_cfgs(uri, &statements, &source);

        // Mark as analyzed
        let mut states = self.file_states.lock();
        if let Some(state) = states.get_mut(uri) {
            state.analyzed = true;
        }
    }

    /// Recursively collect method CFGs from AST
    fn collect_method_cfgs(&self, uri: &Url, body: &ruby_prism::StatementsNode, source: &str) {
        for stmt in body.body().iter() {
            if let Some(def_node) = stmt.as_def_node() {
                if let Some(method_cfg) = self.build_method_cfg(&def_node, source) {
                    let mut states = self.file_states.lock();
                    if let Some(state) = states.get_mut(uri) {
                        state
                            .method_cfgs
                            .insert(method_cfg.start_offset, method_cfg);
                    }
                }
            } else if let Some(class_node) = stmt.as_class_node() {
                if let Some(body) = class_node.body() {
                    if let Some(statements) = body.as_statements_node() {
                        self.collect_method_cfgs(uri, &statements, source);
                    }
                }
            } else if let Some(module_node) = stmt.as_module_node() {
                if let Some(body) = module_node.body() {
                    if let Some(statements) = body.as_statements_node() {
                        self.collect_method_cfgs(uri, &statements, source);
                    }
                }
            } else if let Some(singleton_class) = stmt.as_singleton_class_node() {
                if let Some(body) = singleton_class.body() {
                    if let Some(statements) = body.as_statements_node() {
                        self.collect_method_cfgs(uri, &statements, source);
                    }
                }
            }
        }
    }

    /// Get method CFG states for testing
    /// Returns Vec of (method_name, start_offset, end_offset)
    pub fn get_method_cfgs(&self, uri: &Url) -> Vec<(String, usize, usize)> {
        let states = self.file_states.lock();
        let state = match states.get(uri) {
            Some(s) => s,
            None => return Vec::new(),
        };

        state
            .method_cfgs
            .iter()
            .filter(|(key, _)| **key != TOP_LEVEL_CFG_KEY)
            .map(|(_, method_state)| {
                (
                    method_state.method_name.clone(),
                    method_state.start_offset,
                    method_state.end_offset,
                )
            })
            .collect()
    }

    /// Check if a file has been analyzed
    pub fn has_analysis(&self, uri: &Url) -> bool {
        let states = self.file_states.lock();
        states.get(uri).is_some_and(|s| s.analyzed)
    }

    /// Get stats for testing (file_count, method_count)
    /// method_count excludes top-level CFGs
    pub fn get_stats(&self) -> (usize, usize) {
        let states = self.file_states.lock();
        let file_count = states.len();
        let method_count: usize = states
            .values()
            .map(|s| {
                s.method_cfgs
                    .keys()
                    .filter(|k| **k != TOP_LEVEL_CFG_KEY)
                    .count()
            })
            .sum();
        (file_count, method_count)
    }
}

/// CFG state for a single file
#[derive(Debug)]
pub struct FileCfgState {
    /// Source content for offset calculations
    pub content: String,
    /// CFG and dataflow results for each method (keyed by start offset)
    pub method_cfgs: HashMap<usize, MethodCfgState>,
    /// Whether the file has been analyzed
    pub analyzed: bool,
}

impl FileCfgState {
    pub fn new(content: String) -> Self {
        Self {
            content,
            method_cfgs: HashMap::new(),
            analyzed: false,
        }
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
    /// Uses pre-computed snapshots with binary search for O(log n) lookup
    pub fn get_type_at_position(&self, var_name: &str, offset: usize) -> Option<RubyType> {
        // Find the block containing this offset
        for (block_id, block) in &self.cfg.blocks {
            if offset >= block.location.start_offset && offset <= block.location.end_offset {
                // Use the pre-computed snapshots with binary search
                return self
                    .dataflow
                    .get_type_at_offset(*block_id, var_name, offset);
            }
        }
        None
    }

    /// Check if an offset is within this method
    pub fn contains_offset(&self, offset: usize) -> bool {
        offset >= self.start_offset && offset <= self.end_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_narrowing_if_nil_check() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"
def process(value)
  if value.nil?
    puts "nil"
  else
    puts value.upcase
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // The method should have a CFG
        let states = engine.file_states.lock();
        let state = states.get(&uri).unwrap();
        assert!(!state.method_cfgs.is_empty(), "Should have method CFGs");
    }

    #[test]
    fn test_type_narrowing_simple_assignment() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"
def test
  a = "hello"
  puts a
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Check that we can get the type of 'a' after assignment
        // The offset should point to the variable 'a' in 'puts a'
        let offset = source.find("puts a").unwrap() + 5; // point to 'a'
        let narrowed_type = engine.get_narrowed_type(&uri, offset, None);

        assert!(narrowed_type.is_some(), "Should have type for 'a'");
        if let Some(ty) = narrowed_type {
            assert_eq!(ty, RubyType::string(), "Type should be String");
        }
    }

    #[test]
    fn test_top_level_string_variable() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"name = "hello"
puts name"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Check that we can get the type of 'name' at top level
        let offset = source.find("puts name").unwrap() + 5; // point to 'name'
        let narrowed_type = engine.get_narrowed_type(&uri, offset, None);

        assert!(narrowed_type.is_some(), "Should have type for 'name'");
        if let Some(ty) = narrowed_type {
            assert_eq!(ty, RubyType::string(), "Type should be String");
        }
    }

    #[test]
    fn test_top_level_string_with_method_call() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"name = "hello"
upper = name."#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Check that we can get the type of 'name' at top level
        let offset = source.find("upper = name").unwrap() + 8; // point to 'name'
        let narrowed_type = engine.get_narrowed_type(&uri, offset, None);

        assert!(narrowed_type.is_some(), "Should have type for 'name'");
        if let Some(ty) = narrowed_type {
            assert_eq!(ty, RubyType::string(), "Type should be String");
        }
    }

    #[test]
    fn test_variable_to_variable_assignment() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"a = 'str'
b = a
puts b."#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // Check that 'a' has type String
        let offset = source.find("b = a").unwrap() + 4; // point to 'a'
        let a_type = engine.get_narrowed_type(&uri, offset, None);
        assert!(a_type.is_some(), "Should have type for 'a'");
        assert_eq!(a_type.unwrap(), RubyType::string(), "a should be String");

        // Check that 'b' has type String (propagated from 'a')
        let offset = source.find("puts b").unwrap() + 5; // point to 'b'
        let b_type = engine.get_narrowed_type(&uri, offset, None);
        assert!(b_type.is_some(), "Should have type for 'b'");
        assert_eq!(b_type.unwrap(), RubyType::string(), "b should be String");
    }

    #[test]
    fn test_or_and_assignment_types() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"a = 'str'
b = 1
c = a || b
d = a && b"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // c = a || b: a is truthy (String), so c should be String
        let c_offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, c_offset, None);
        assert!(c_type.is_some(), "Should have type for 'c'");
        assert_eq!(
            c_type.unwrap(),
            RubyType::string(),
            "c should be String (a is truthy)"
        );

        // d = a && b: a is truthy, so d should be Integer (b's type)
        let d_offset = source.find("d = a").unwrap(); // point to 'd'
        let d_type = engine.get_narrowed_type(&uri, d_offset, None);
        assert!(d_type.is_some(), "Should have type for 'd'");
        assert_eq!(
            d_type.unwrap(),
            RubyType::integer(),
            "d should be Integer (a is truthy, so && returns b)"
        );
    }

    #[test]
    fn test_and_with_nilable_left() {
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = r#"a = nil
b = 1
c = a && b"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // c = a && b: a is nil (falsy), so c should be NilClass
        let offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, offset, None);
        assert!(c_type.is_some(), "Should have type for 'c'");
        assert_eq!(
            c_type.unwrap(),
            RubyType::nil_class(),
            "c should be NilClass (a is falsy)"
        );
    }

    #[test]
    fn test_or_with_truthy_left() {
        // a || b where a is truthy -> result is a's type
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "a = 'str'\nb = 1\nc = a || b";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            c_type,
            Some(RubyType::string()),
            "c should be String (a is truthy, || returns a)"
        );
    }

    #[test]
    fn test_or_with_falsy_left() {
        // a || b where a is falsy -> result is b's type
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "a = nil\nb = 'str'\nc = a || b";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            c_type,
            Some(RubyType::string()),
            "c should be String (a is falsy, || returns b)"
        );
    }

    #[test]
    fn test_and_with_truthy_left() {
        // a && b where a is truthy -> result is b's type
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "a = 'str'\nb = 1\nc = a && b";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            c_type,
            Some(RubyType::integer()),
            "c should be Integer (a is truthy, && returns b)"
        );
    }

    #[test]
    fn test_and_with_falsy_left() {
        // a && b where a is falsy -> result is a's type
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "a = false\nb = 'str'\nc = a && b";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("c = a").unwrap(); // point to 'c'
        let c_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            c_type,
            Some(RubyType::false_class()),
            "c should be FalseClass (a is falsy, && returns a)"
        );
    }

    #[test]
    fn test_or_assign_operator() {
        // x ||= 'str' where x is nil -> x becomes String
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = nil\nx ||= 'str'";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x ||=").unwrap(); // point to 'x' in x ||=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            x_type,
            Some(RubyType::string()),
            "x should become String after ||= (x was nil)"
        );
    }

    #[test]
    fn test_and_assign_operator() {
        // x &&= 'str' where x is nil -> x stays nil
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = nil\nx &&= 'str'";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x &&=").unwrap(); // point to 'x' in x &&=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            x_type,
            Some(RubyType::nil_class()),
            "x should stay NilClass after &&= (x was falsy)"
        );
    }

    #[test]
    fn test_chained_or() {
        // a || b || c
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "a = nil\nb = false\nc = 'str'\nd = a || b || c";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("d = a").unwrap(); // point to 'd'
        let d_type = engine.get_narrowed_type(&uri, offset, None);

        // a is nil (falsy), b is false (falsy), so result is c (String)
        assert_eq!(
            d_type,
            Some(RubyType::string()),
            "d should be String (a and b are falsy)"
        );
    }

    #[test]
    fn test_or_assign_with_truthy_existing_value() {
        // x = 'str'; x ||= 1 -> x stays String (truthy, so ||= doesn't assign)
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = 'str'\nx ||= 1";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x ||=").unwrap(); // point to 'x' in x ||=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            x_type,
            Some(RubyType::string()),
            "x should stay String (x was truthy, so ||= doesn't assign)"
        );
    }

    #[test]
    fn test_and_assign_with_falsy_existing_value() {
        // x = nil; x &&= 'str' -> x stays nil (falsy, so &&= doesn't assign)
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = nil\nx &&= 'str'";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x &&=").unwrap(); // point to 'x' in x &&=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            x_type,
            Some(RubyType::nil_class()),
            "x should stay NilClass (x was falsy, so &&= doesn't assign)"
        );
    }

    #[test]
    fn test_or_assign_with_false() {
        // x = false; x ||= 'str' -> x becomes String
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = false\nx ||= 'str'";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x ||=").unwrap(); // point to 'x' in x ||=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        assert_eq!(
            x_type,
            Some(RubyType::string()),
            "x should become String (x was false, so ||= assigns the right side)"
        );
    }

    #[test]
    fn test_and_assign_with_truthy_existing_value() {
        // x = 1; x &&= 'str' -> x becomes String (1 is truthy)
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "x = 1\nx &&= 'str'";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = source.find("x &&=").unwrap(); // point to 'x' in x &&=
        let x_type = engine.get_narrowed_type(&uri, offset, None);

        // x &&= 'str' where x was 1 (truthy) -> x becomes String
        assert_eq!(
            x_type,
            Some(RubyType::string()),
            "x should become String (x was truthy, so &&= assigns the right side)"
        );
    }

    #[test]
    fn test_method_call_assignment_returns_none() {
        // CFG doesn't know method return types, should return None
        // so that inlay hints can fall back to index
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let source = "name = user.name";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let offset = 0; // point to 'name' at start
        let name_type = engine.get_narrowed_type(&uri, offset, None);

        // CFG doesn't know the return type of user.name
        // Should return None so inlay hints can use index fallback
        assert_eq!(
            name_type, None,
            "CFG should return None for method call assignments (index has the type)"
        );
    }

    #[test]
    fn test_multiple_assignments_same_variable() {
        // Test that CFG returns correct type at each position
        // e = nil (line 1) -> NilClass
        // e ||= 1 (line 2) -> Integer
        let engine = TypeNarrowingEngine::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        // "e = nil\n" is 8 bytes (0-7), "e ||= 1" starts at 8
        let source = "e = nil\ne ||= 1";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        // At offset 0 (start of first 'e'), type should be NilClass
        let e_type_line1 = engine.get_narrowed_type(&uri, 0, None);

        // At offset 8 (start of second 'e'), type should be Integer
        let e_type_line2 = engine.get_narrowed_type(&uri, 8, None);

        // At offset 8 (second 'e'), type should be Integer (after ||=)
        let e_type_end = engine.get_narrowed_type(&uri, 8, None);

        assert_eq!(
            e_type_line1,
            Some(RubyType::nil_class()),
            "e should be NilClass at line 1"
        );

        assert_eq!(
            e_type_line2,
            Some(RubyType::integer()),
            "e should be Integer at line 2 (after ||=)"
        );

        assert_eq!(
            e_type_end,
            Some(RubyType::integer()),
            "e should be Integer at end of file"
        );
    }
}
