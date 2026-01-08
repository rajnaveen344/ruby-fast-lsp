use ruby_prism::*;

use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::FileId;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::ruby_document::RubyDocument;
use crate::yard::parser::{CommentLineInfo, YardParser};

mod block_node;
mod call_node;
mod class_node;
mod class_variable_write_node;
mod constant_path_write_node;
mod constant_write_node;
mod def_node;
mod global_variable_write_node;
mod instance_variable_write_node;
mod local_variable_write_node;
mod module_node;
mod parameters_node;
mod singleton_class_node;

use crate::indexer::entry::Entry;

pub struct IndexVisitor {
    pub index: Index<Unlocked>,
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub literal_analyzer: LiteralAnalyzer,
    pub diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>,
}

impl IndexVisitor {
    pub fn new(index: Index<Unlocked>, document: RubyDocument) -> Self {
        let scope_tracker = ScopeTracker::new(&document);
        Self {
            index,
            document,
            scope_tracker,
            literal_analyzer: LiteralAnalyzer::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn push_diagnostic(&mut self, diagnostic: tower_lsp::lsp_types::Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Add an entry to the index immediately
    /// Converts placeholder FileId to actual FileId using document URI
    pub fn add_entry(&mut self, mut entry: Entry) {
        let mut index = self.index.lock();
        // Convert placeholder FileId::default() to actual FileId
        if entry.location.file_id == FileId::default() {
            entry.location.file_id = index.get_or_insert_file(&self.document.uri);
        }
        index.add_entry(entry);
    }

    /// Infer type from a value node during indexing.
    /// This is the shared type inference logic used by all variable write nodes.
    pub fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        // ... (this method body is unchanged, providing context)
        // Try literal analysis first
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // LIGHTWEIGHT OPTIMIZATION: Handle `Constant.new` and `Module::Class.new`
        if let Some(call_node) = value_node.as_call_node() {
            if call_node.name().as_slice() == b"new" {
                if let Some(receiver) = call_node.receiver() {
                    let fqn_string = if let Some(const_node) = receiver.as_constant_read_node() {
                        // Simple constant: `User.new`
                        Some(String::from_utf8_lossy(const_node.name().as_slice()).to_string())
                    } else if let Some(path_node) = receiver.as_constant_path_node() {
                        // Namespaced constant: `MyApp::User.new`
                        // We need to flatten the path node (e.g. parent -> child)
                        // This is a simplified flattening for typical cases
                        Self::flatten_constant_path(&path_node)
                    } else {
                        None
                    };

                    if let Some(name) = fqn_string {
                        use crate::types::fully_qualified_name::FullyQualifiedName;
                        if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
                            return RubyType::Class(fqn);
                        }
                    }
                }
            }
        }

        // Handle local variable reads: look up the type from already-indexed entries
        if let Some(lvar_read) = value_node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(lvar_read.name().as_slice()).to_string();
            let lvar_line = self
                .document
                .prism_location_to_lsp_range(&lvar_read.location())
                .start
                .line;

            // Search all scopes for this variable
            for (_scope_id, entries) in self.document.get_all_lvars() {
                // Find entries for this variable that are before the read position
                // and return the type from the most recent one
                let matching_entry = entries
                    .iter()
                    .filter(|entry| {
                        if let EntryKind::LocalVariable(data) = &entry.kind {
                            data.name == var_name && entry.location.range.start.line < lvar_line
                        } else {
                            false
                        }
                    })
                    .last();

                if let Some(entry) = matching_entry {
                    if let EntryKind::LocalVariable(data) = &entry.kind {
                        // Get the most recent assignment type
                        if let Some(assignment) = data.assignments.last() {
                            if assignment.r#type != RubyType::Unknown {
                                return assignment.r#type.clone();
                            }
                        }
                    }
                }
            }
        }

        RubyType::Unknown
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

impl Visit<'_> for IndexVisitor {
    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
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
