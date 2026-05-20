use once_cell::unsync::OnceCell;
use ruby_analysis_core::{DiagnosticCandidate, ReferenceCandidate, TypeStore};
use ruby_analysis_engine::AnalysisEngine;
use ruby_prism::*;
use tower_lsp::lsp_types::Diagnostic;

use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::extensions::ExtensionRegistryHandle;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::yard::parser::{CommentLineInfo, YardParser};
use parking_lot::Mutex;
use ruby_analysis_inference::r#type::literal::LiteralAnalyzer;
use ruby_analysis_inference::RubyType;
use std::collections::HashMap;
use std::sync::Arc;

mod block_node;
mod call_node;
mod class_node;
mod class_variable_write_node;
mod constant_path_node;
mod constant_path_write_node;
mod constant_read_node;
mod constant_write_node;
mod def_node;
mod global_variable_write_node;
mod instance_variable_write_node;
mod local_variable_read_node;
mod local_variable_write_node;
mod method_return;
mod module_node;
mod parameters_node;
mod singleton_class_node;

pub struct FactCollector {
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub literal_analyzer: LiteralAnalyzer,
    pub diagnostics: Vec<Diagnostic>,
    pub type_store: TypeStore,
    pub extension_call_stack: Vec<ruby_fast_lsp_extension_api::ResolvedCall>,
    pub extension_index_patches: Vec<ruby_fast_lsp_extension_api::IndexPatch>,
    pub extension_registry: ExtensionRegistryHandle,
    pub analysis_engine: Arc<Mutex<AnalysisEngine>>,
    pub include_local_vars: bool,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub variable_types: OnceCell<HashMap<String, RubyType>>,
}

impl FactCollector {
    pub fn push_diagnostic(&mut self, diagnostic: tower_lsp::lsp_types::Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn analysis_only(
        document: RubyDocument,
        extension_registry: ExtensionRegistryHandle,
        analysis_engine: Arc<Mutex<AnalysisEngine>>,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();
        Self {
            document,
            scope_tracker,
            literal_analyzer: LiteralAnalyzer::new(),
            diagnostics: Vec::new(),
            type_store: TypeStore::new(),
            extension_call_stack: Vec::new(),
            extension_index_patches: Vec::new(),
            extension_registry,
            analysis_engine,
            include_local_vars: true,
            reference_candidates: Vec::new(),
            diagnostic_candidates: Vec::new(),
            variable_types: OnceCell::new(),
        }
    }

    pub fn infer_variable_type_cached(&self, var_name: &str) -> Option<RubyType> {
        let map = self
            .variable_types
            .get_or_init(|| build_variable_type_map(&self.document.content));
        map.get(var_name).cloned()
    }

    /// Infer type from a value node during indexing.
    ///
    /// This recursively walks the AST to infer types:
    /// - Literals → their type (String, Integer, etc.)
    /// - Constants → ClassReference
    /// - Local variables → look up their type
    /// - Method calls → recursively infer receiver type, then resolve method return type
    pub fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        // 1. Try literal analysis first (String, Integer, Array, Hash, Symbol, etc.)
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // 2. Constant read: `User` → ClassReference(User)
        if let Some(const_node) = value_node.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_node.name().as_slice()).to_string();
            if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
                return RubyType::ClassReference(fqn);
            }
        }

        // 3. Constant path: `MyApp::User` → ClassReference(MyApp::User)
        if let Some(path_node) = value_node.as_constant_path_node() {
            if let Some(path_str) = Self::flatten_constant_path(&path_node) {
                if let Ok(fqn) = FullyQualifiedName::try_from(path_str.as_str()) {
                    return RubyType::ClassReference(fqn);
                }
            }
        }

        // 4. Local variable read: look up the variable's type
        if let Some(lvar_read) = value_node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(lvar_read.name().as_slice()).to_string();
            if let Some(ty) = self.get_local_var_type(&var_name, &lvar_read.location()) {
                return ty;
            }
            return RubyType::Unknown;
        }

        // 5. Method call: recursively infer receiver type, then resolve method
        if let Some(call_node) = value_node.as_call_node() {
            let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

            // Determine receiver type
            let receiver_type = if let Some(receiver) = call_node.receiver() {
                // Has explicit receiver - recursively infer its type
                self.infer_type_from_value(&receiver)
            } else {
                // No receiver (implicit self) - use current class/module context
                let namespace = self.scope_tracker.get_ns_stack();
                if namespace.is_empty() {
                    return RubyType::Unknown;
                }
                let current_fqn = FullyQualifiedName::namespace(namespace.into());
                RubyType::Class(current_fqn)
            };

            if receiver_type == RubyType::Unknown {
                return RubyType::Unknown;
            }

            // Special case: `.new` on a ClassReference returns an instance
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &receiver_type {
                    return RubyType::Class(fqn.clone());
                }
            }

            if let Some(return_type) = self.resolve_method_return_type(&receiver_type, &method_name)
            {
                return return_type;
            }
        }

        RubyType::Unknown
    }

    /// Helper to get the type of a local variable by name at a given location.
    fn get_local_var_type(&self, var_name: &str, location: &Location) -> Option<RubyType> {
        let position = self.document.prism_location_to_lsp_range(location).start;

        // Use VariableScopes tree for type lookup
        let scope_id = self
            .document
            .variable_scopes()
            .find_scope_for_variable_at(var_name, position)
            .or_else(|| self.document.variable_scopes().scope_at_position(position))?;

        let ty = self
            .document
            .variable_scopes()
            .get_type_at_position(var_name, scope_id, position)?;

        if *ty != RubyType::Unknown {
            Some(ty.clone())
        } else {
            None
        }
    }

    /// Helper to flatten a ConstantPathNode into a string (e.g., "Module::Class")
    fn flatten_constant_path(node: &ConstantPathNode) -> Option<String> {
        let mut parts = Vec::new();
        use crate::analyzer_prism::utils;
        utils::collect_namespaces(node, &mut parts);

        if parts.is_empty() {
            None
        } else {
            // Join parts with "::"
            let path_str = parts
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("::");
            Some(path_str)
        }
    }

    /// Extract YARD documentation from comments preceding a method definition using Prism comments.
    pub fn extract_doc_comments(
        &self,
        method_start: usize,
    ) -> Option<crate::yard::types::YardMethodDoc> {
        // Find the first comment that starts AFTER or AT method_start.
        // We want the ones BEFORE it.
        let idx = self
            .document
            .get_comments()
            .partition_point(|c| c.0 < method_start);

        if idx == 0 {
            return None;
        }

        let mut comment_indices = Vec::new();
        let mut current_idx = idx - 1;

        // Check last comment is attached to method
        let (_, end) = self.document.get_comments()[current_idx];
        let range_between = &self.document.content[end..method_start];
        if !range_between.trim().is_empty() {
            return None;
        }
        comment_indices.push(current_idx);

        // Walk backwards to collect contiguous comment block
        while current_idx > 0 {
            let prev_idx = current_idx - 1;
            let (_, prev_end) = self.document.get_comments()[prev_idx];
            let (curr_start, _) = self.document.get_comments()[current_idx];

            let range_between = &self.document.content[prev_end..curr_start];
            if !range_between.trim().is_empty() {
                break;
            }
            comment_indices.push(prev_idx);
            current_idx = prev_idx;
        }

        comment_indices.reverse(); // Now in order top-down

        let mut line_infos = Vec::new();
        for &i in &comment_indices {
            let (start, end) = self.document.get_comments()[i];
            let raw_content = &self.document.content[start..end];
            let trimmed = raw_content.trim();
            // Prism comments include the #.
            let content = trimmed.trim_start_matches('#').trim_start();

            // Calculate precise location info for diagnostics
            // We need the position of the *content*, so find where it starts relative to the comment start
            let hash_offset = raw_content.find('#').unwrap_or(0);

            // Find content offset. If empty content, point to end of hash
            let content_offset_in_raw = if content.is_empty() {
                hash_offset + 1
            } else {
                raw_content.find(content).unwrap_or(hash_offset + 1)
            };

            let abs_content_start = start + content_offset_in_raw;
            let abs_pos = self.document.offset_to_position(abs_content_start);
            // YardParser uses line_length for diagnostic range end calculation in some cases
            // (end char is usually start char + content len, but passed as line_length in parser?)
            // Actually parser uses:
            // start: Position { line: line_info.line_number, character: line_info.content_start_char }
            // end: Position { line: line_info.line_number, character: line_info.line_length }
            // So line_length should be the COLUMN index of the end of the line (or length)
            let abs_end_pos = self.document.offset_to_position(end);

            line_infos.push(CommentLineInfo {
                content,
                line_number: abs_pos.line,
                content_start_char: abs_pos.character,
                line_length: abs_end_pos.character,
            });
        }

        let doc = YardParser::parse_lines(&line_infos, true);

        if doc.has_type_info() || doc.description.is_some() {
            Some(doc)
        } else {
            None
        }
    }
}

fn build_variable_type_map(content: &str) -> HashMap<String, RubyType> {
    use crate::types::ruby_namespace::RubyConstant;

    let mut map: HashMap<String, RubyType> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(eq_idx) = trimmed.find('=') else {
            continue;
        };
        let lhs = trimmed[..eq_idx].trim();
        let mut chars = lhs.chars();
        match chars.next() {
            Some(c) if c.is_lowercase() || c == '_' => {}
            Some(_) | None => continue,
        }
        if !lhs.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rhs_full = trimmed[eq_idx + 1..].trim();
        let Some(new_pos) = rhs_full.find(".new") else {
            continue;
        };
        let class_part = rhs_full[..new_pos].trim();
        if !class_part.chars().next().is_some_and(|c| c.is_uppercase()) {
            continue;
        }
        let parts: Vec<_> = class_part
            .split("::")
            .filter_map(|s| RubyConstant::new(s.trim()).ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        map.entry(lhs.to_string())
            .or_insert_with(|| RubyType::Class(FullyQualifiedName::Constant(parts)));
    }
    map
}

impl Visit<'_> for FactCollector {
    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        self.process_singleton_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
        self.process_constant_write_node_exit(node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode) {
        self.process_constant_path_write_node_entry(node);
        visit_constant_path_write_node(self, node);
        self.process_constant_path_write_node_exit(node);
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write_node_entry(node);
        visit_local_variable_write_node(self, node);
        self.process_local_variable_write_node_exit(node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_target_node_entry(node);
        visit_local_variable_target_node(self, node);
        self.process_local_variable_target_node_exit(node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_or_write_node_entry(node);
        visit_local_variable_or_write_node(self, node);
        self.process_local_variable_or_write_node_exit(node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        self.process_local_variable_and_write_node_entry(node);
        visit_local_variable_and_write_node(self, node);
        self.process_local_variable_and_write_node_exit(node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        self.process_local_variable_operator_write_node_entry(node);
        visit_local_variable_operator_write_node(self, node);
        self.process_local_variable_operator_write_node_exit(node);
    }

    fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'_>) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write_node_entry(node);
        visit_class_variable_write_node(self, node);
        self.process_class_variable_write_node_exit(node);
    }

    fn visit_class_variable_target_node(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_target_node_entry(node);
        visit_class_variable_target_node(self, node);
        self.process_class_variable_target_node_exit(node);
    }

    fn visit_class_variable_or_write_node(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_or_write_node_entry(node);
        visit_class_variable_or_write_node(self, node);
        self.process_class_variable_or_write_node_exit(node);
    }

    fn visit_class_variable_and_write_node(&mut self, node: &ClassVariableAndWriteNode) {
        self.process_class_variable_and_write_node_entry(node);
        visit_class_variable_and_write_node(self, node);
        self.process_class_variable_and_write_node_exit(node);
    }

    fn visit_class_variable_operator_write_node(&mut self, node: &ClassVariableOperatorWriteNode) {
        self.process_class_variable_operator_write_node_entry(node);
        visit_class_variable_operator_write_node(self, node);
        self.process_class_variable_operator_write_node_exit(node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write_node_entry(node);
        visit_instance_variable_write_node(self, node);
        self.process_instance_variable_write_node_exit(node);
    }

    fn visit_instance_variable_target_node(&mut self, node: &InstanceVariableTargetNode) {
        self.process_instance_variable_target_node_entry(node);
        visit_instance_variable_target_node(self, node);
        self.process_instance_variable_target_node_exit(node);
    }

    fn visit_instance_variable_or_write_node(&mut self, node: &InstanceVariableOrWriteNode) {
        self.process_instance_variable_or_write_node_entry(node);
        visit_instance_variable_or_write_node(self, node);
        self.process_instance_variable_or_write_node_exit(node);
    }

    fn visit_instance_variable_and_write_node(&mut self, node: &InstanceVariableAndWriteNode) {
        self.process_instance_variable_and_write_node_entry(node);
        visit_instance_variable_and_write_node(self, node);
        self.process_instance_variable_and_write_node_exit(node);
    }

    fn visit_instance_variable_operator_write_node(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_operator_write_node_entry(node);
        visit_instance_variable_operator_write_node(self, node);
        self.process_instance_variable_operator_write_node_exit(node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write_node_entry(node);
        visit_global_variable_write_node(self, node);
        self.process_global_variable_write_node_exit(node);
    }

    fn visit_global_variable_target_node(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_target_node_entry(node);
        visit_global_variable_target_node(self, node);
        self.process_global_variable_target_node_exit(node);
    }

    fn visit_global_variable_or_write_node(&mut self, node: &GlobalVariableOrWriteNode) {
        self.process_global_variable_or_write_node_entry(node);
        visit_global_variable_or_write_node(self, node);
        self.process_global_variable_or_write_node_exit(node);
    }

    fn visit_global_variable_and_write_node(&mut self, node: &GlobalVariableAndWriteNode) {
        self.process_global_variable_and_write_node_entry(node);
        visit_global_variable_and_write_node(self, node);
        self.process_global_variable_and_write_node_exit(node);
    }

    fn visit_global_variable_operator_write_node(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_operator_write_node_entry(node);
        visit_global_variable_operator_write_node(self, node);
        self.process_global_variable_operator_write_node_exit(node);
    }
}
