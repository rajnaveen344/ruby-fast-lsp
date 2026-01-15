//! Unified type query API for Ruby code.
//!
//! This module provides a single entry point for all type queries, abstracting away
//! the complexity of checking caches, triggering inference, and storing results.
//!
//! Handlers (hover, inlay hints, completion) should use this API instead of
//! directly interacting with the inferrer or index.

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::{EntryId, RubyIndex};
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use ruby_prism::{Node, Visit};
use tower_lsp::lsp_types::{Position, Range, Url};

/// A type hint to display (for inlay hints, hover, etc.)
#[derive(Debug, Clone)]
pub struct TypeHint {
    /// Where to show the hint (end of the identifier)
    pub position: Position,
    /// The inferred or declared type
    pub ruby_type: RubyType,
    /// What kind of construct this type is for
    pub kind: TypeHintKind,
    /// Optional tooltip text (e.g., YARD description)
    pub tooltip: Option<String>,
}

/// The kind of construct a type hint is for
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeHintKind {
    /// Method return type: `def foo -> String`
    MethodReturn,
    /// Local variable: `x = "hello"` → `x: String`
    LocalVariable,
    /// Instance variable: `@name = "bob"` → `@name: String`
    InstanceVariable,
    /// Class variable: `@@count = 0` → `@@count: Integer`
    ClassVariable,
    /// Global variable: `$debug = true` → `$debug: TrueClass`
    GlobalVariable,
    /// Method parameter from YARD: `def foo(x)` → `x: Integer`
    MethodParameter,
}

/// Unified type query interface.
///
/// Provides methods to query types for various constructs, automatically
/// handling inference and caching.
pub struct TypeQuery<'a> {
    index: Index<Unlocked>,
    uri: &'a Url,
    content: &'a [u8],
}

impl<'a> TypeQuery<'a> {
    /// Create a new TypeQuery for a specific file.
    pub fn new(index: Index<Unlocked>, uri: &'a Url, content: &'a [u8]) -> Self {
        Self {
            index,
            uri,
            content,
        }
    }

    /// Get all type hints in a range (for inlay hints).
    ///
    /// Returns types for:
    /// - Method return types
    /// - Local variables
    /// - Instance/class/global variables
    /// - Method parameters (from YARD)
    pub fn get_types_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        // Collect method hints (with inference)
        hints.extend(self.get_method_hints_in_range(range));

        // Collect variable hints from index
        hints.extend(self.get_variable_hints_in_range(range));

        // Collect local variable hints (from document lvars - handled separately)
        // Note: Local variables are stored in the Document, not the Index,
        // so they need to be passed in or queried separately

        hints
    }

    /// Get all inlay hints for a document in the given range.
    ///
    /// This is the unified entry point for inlay hints, combining:
    /// - Local variable type hints (from document.lvars)
    /// - Instance/class/global variable hints (from index)
    /// - Method parameter hints (from YARD)
    /// - Method return type hints (inferred or from YARD)
    pub fn get_inlay_hints_in_range(
        &mut self,
        document: &RubyDocument,
        range: &Range,
    ) -> Vec<TypeHint> {
        // First, trigger method return type inference for visible methods
        self.infer_visible_method_types(range);

        let mut hints = Vec::new();

        // 1. Local variable hints from document.lvars
        hints.extend(self.get_local_var_hints(document, range));

        // 2. Instance/class/global variable hints from index
        hints.extend(self.get_variable_hints_in_range(range));

        // 3. Method return type hints
        hints.extend(self.get_method_return_hints_in_range(range));

        // 4. Method parameter hints from YARD
        hints.extend(self.get_param_hints_in_range(range));

        hints
    }

    /// Infer return types for methods in the visible range and update the index.
    fn infer_visible_method_types(&mut self, range: &Range) {
        // Collect methods needing inference
        let methods_needing_inference: Vec<(u32, EntryId)> = {
            let index = self.index.lock();
            index
                .get_entry_ids_for_uri(self.uri)
                .iter()
                .filter_map(|&entry_id| {
                    let entry = index.get_entry(entry_id)?;
                    if let EntryKind::Method(data) = &entry.kind {
                        let method_line = entry.location.range.start.line;
                        if method_line >= range.start.line && method_line <= range.end.line {
                            if data.return_type.is_none() {
                                if let Some(pos) = data.return_type_position {
                                    return Some((pos.line, entry_id));
                                }
                            }
                        }
                    }
                    None
                })
                .collect()
        };

        if methods_needing_inference.is_empty() {
            return;
        }

        // Parse file once and infer visible methods
        let parse_result = ruby_prism::parse(self.content);
        let node = parse_result.node();

        let mut file_contents = std::collections::HashMap::new();
        file_contents.insert(self.uri, self.content);

        // Infer and collect results
        let inferred_types: Vec<(EntryId, RubyType)> = methods_needing_inference
            .iter()
            .filter_map(|(line, entry_id)| {
                let def_node = find_def_node_recursive(&node, *line, self.content)?;

                let mut index = self.index.lock();
                let owner_fqn = index.get_entry(*entry_id).and_then(|e| {
                    if let EntryKind::Method(m) = &e.kind {
                        Some(m.owner.clone())
                    } else {
                        None
                    }
                });

                let inferred_ty = crate::inferrer::return_type::infer_return_type_for_node(
                    &mut index,
                    self.content,
                    &def_node,
                    owner_fqn,
                    Some(&file_contents),
                )?;

                Some((*entry_id, inferred_ty))
            })
            .collect();

        // Update the index with results
        if !inferred_types.is_empty() {
            let mut index = self.index.lock();
            for (entry_id, inferred_ty) in inferred_types {
                index.update_method_return_type(entry_id, inferred_ty);
            }
        }
    }

    /// Get local variable type hints from document.lvars
    fn get_local_var_hints(
        &self,
        document: &RubyDocument,
        range: &Range,
    ) -> Vec<TypeHint> {
        let mut hints = Vec::new();
        let content_str = std::str::from_utf8(self.content).unwrap_or("");

        for (_scope_id, entries) in document.get_all_lvars() {
            for entry in entries {
                // Skip entries outside the requested range
                if !Self::is_in_range(&entry.location.range.start, range)
                    && !Self::is_in_range(&entry.location.range.end, range)
                {
                    continue;
                }

                if let EntryKind::LocalVariable(data) = &entry.kind {
                    // Get type from assignment tracking
                    let from_lvar = data.assignments.last().map(|a| a.r#type.clone());

                    // Resolve final type (type narrowing removed)
                    let final_type = self.resolve_local_var_type_internal(
                        content_str,
                        &data.name,
                        from_lvar.as_ref(),
                        None,
                    );

                    let ruby_type = final_type.unwrap_or(RubyType::Unknown);

                    hints.push(TypeHint {
                        position: entry.location.range.end,
                        ruby_type,
                        kind: TypeHintKind::LocalVariable,
                        tooltip: None,
                    });
                }
            }
        }

        hints
    }

    /// Resolve local variable type using fallback chain
    fn resolve_local_var_type_internal(
        &self,
        content: &str,
        name: &str,
        known_type: Option<&RubyType>,
        type_narrowing: Option<RubyType>,
    ) -> Option<RubyType> {
        // 1. Try type narrowing
        if let Some(ty) = type_narrowing {
            if ty != RubyType::Unknown {
                return Some(ty);
            }
        }

        // 2. Try known type from assignment tracking
        if let Some(ty) = known_type {
            if *ty != RubyType::Unknown {
                return Some(ty.clone());
            }
        }

        // 3. Try fallback inference
        let index = self.index.lock();
        infer_type_from_assignment(content, name, &index)
    }

    /// Get method return type hints in a range (after inference has been done)
    fn get_method_return_hints_in_range(&self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        let index = self.index.lock();
        for entry_id in index.get_entry_ids_for_uri(self.uri) {
            let Some(entry) = index.get_entry(entry_id) else {
                continue;
            };

            if !Self::is_in_range(&entry.location.range.start, range) {
                continue;
            }

            if let EntryKind::Method(data) = &entry.kind {
                // Priority: Inferred type > YARD type
                let ruby_type = if let Some(ty) = &data.return_type {
                    ty.clone()
                } else if let Some(yard_doc) = &data.yard_doc {
                    if let Some(type_str) = yard_doc.format_return_type() {
                        RubyType::class(&type_str)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                if let Some(pos) = data.return_type_position {
                    let tooltip = data
                        .yard_doc
                        .as_ref()
                        .and_then(|d| d.get_return_description().cloned());

                    hints.push(TypeHint {
                        position: pos,
                        ruby_type,
                        kind: TypeHintKind::MethodReturn,
                        tooltip,
                    });
                }
            }
        }

        hints
    }

    /// Get method parameter hints from YARD in a range
    fn get_param_hints_in_range(&self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        let index = self.index.lock();
        for entry_id in index.get_entry_ids_for_uri(self.uri) {
            let Some(entry) = index.get_entry(entry_id) else {
                continue;
            };

            if !Self::is_in_range(&entry.location.range.start, range) {
                continue;
            }

            if let EntryKind::Method(data) = &entry.kind {
                if let Some(yard_doc) = &data.yard_doc {
                    for param in &data.params {
                        if let Some(type_str) = yard_doc.get_param_type_str(&param.name) {
                            let tooltip = yard_doc
                                .params
                                .iter()
                                .find(|p| p.name == param.name)
                                .and_then(|p| p.description.clone());

                            hints.push(TypeHint {
                                position: param.end_position,
                                ruby_type: RubyType::class(&type_str),
                                kind: TypeHintKind::MethodParameter,
                                tooltip,
                            });
                        }
                    }
                }
            }
        }

        hints
    }

    /// Get type at a specific position (for hover).
    ///
    /// Returns the type of whatever construct is at the given position.
    pub fn get_type_at(&mut self, position: Position) -> Option<RubyType> {
        // First check if there's a method at this position
        if let Some(ty) = self.get_method_type_at(position) {
            return Some(ty);
        }

        // Check for variables at this position
        if let Some(ty) = self.get_variable_type_at(position) {
            return Some(ty);
        }

        None
    }

    /// Get method return type hints in a range.
    fn get_method_hints_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        // Collect methods needing inference
        let methods_to_process: Vec<(
            EntryId,
            Position,
            Option<RubyType>,
            Option<String>,
            Option<String>,
        )> = {
            let index = self.index.lock();
            index
                .get_entry_ids_for_uri(self.uri)
                .iter()
                .filter_map(|&entry_id| {
                    let entry = index.get_entry(entry_id)?;
                    if !Self::is_in_range(&entry.location.range.start, range) {
                        return None;
                    }

                    if let EntryKind::Method(data) = &entry.kind {
                        let return_type_pos = data.return_type_position?;
                        let yard_return =
                            data.yard_doc.as_ref().and_then(|d| d.format_return_type());
                        let yard_desc = data
                            .yard_doc
                            .as_ref()
                            .and_then(|d| d.get_return_description().cloned());
                        Some((
                            entry_id,
                            return_type_pos,
                            data.return_type.clone(),
                            yard_return,
                            yard_desc,
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Process each method
        for (entry_id, pos, existing_type, yard_return, yard_desc) in methods_to_process {
            let ruby_type = if let Some(ty) = existing_type {
                // Already have a type (from RBS, YARD, or previous inference)
                ty
            } else {
                // Need to infer - try to get the type
                if let Some(ty) = self.infer_method_return_type(entry_id, pos.line) {
                    // Cache the result
                    let mut index = self.index.lock();
                    index.update_method_return_type(entry_id, ty.clone());
                    ty
                } else if yard_return.is_some() {
                    // Fall back to YARD string - but we can't parse it as RubyType yet
                    // Skip for now
                    continue;
                } else {
                    continue;
                }
            };

            hints.push(TypeHint {
                position: pos,
                ruby_type,
                kind: TypeHintKind::MethodReturn,
                tooltip: yard_desc,
            });
        }

        hints
    }

    /// Get variable type hints in a range.
    fn get_variable_hints_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        let index = self.index.lock();
        let entries = index.file_entries(self.uri);

        for entry in entries {
            if !Self::is_in_range(&entry.location.range.start, range) {
                continue;
            }

            let (ruby_type, kind) = match &entry.kind {
                EntryKind::InstanceVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::InstanceVariable)
                }
                EntryKind::ClassVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::ClassVariable)
                }
                EntryKind::GlobalVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::GlobalVariable)
                }
                _ => continue,
            };

            hints.push(TypeHint {
                position: entry.location.range.end,
                ruby_type,
                kind,
                tooltip: None,
            });
        }

        hints
    }

    /// Get method type at a specific position.
    fn get_method_type_at(&mut self, position: Position) -> Option<RubyType> {
        let (entry_id, existing_type, line) = {
            let index = self.index.lock();
            let entry_id = index
                .get_entry_ids_for_uri(self.uri)
                .iter()
                .find(|&&eid| {
                    if let Some(entry) = index.get_entry(eid) {
                        Self::position_in_entry_range(position, &entry.location.range)
                    } else {
                        false
                    }
                })
                .copied()?;

            let entry = index.get_entry(entry_id)?;
            if let EntryKind::Method(data) = &entry.kind {
                (
                    entry_id,
                    data.return_type.clone(),
                    entry.location.range.start.line,
                )
            } else {
                return None;
            }
        };

        if let Some(ty) = existing_type {
            return Some(ty);
        }

        // Need to infer
        if let Some(ty) = self.infer_method_return_type(entry_id, line) {
            let mut index = self.index.lock();
            index.update_method_return_type(entry_id, ty.clone());
            Some(ty)
        } else {
            None
        }
    }

    /// Get variable type at a specific position.
    fn get_variable_type_at(&self, position: Position) -> Option<RubyType> {
        let index = self.index.lock();
        let entries = index.file_entries(self.uri);

        for entry in entries {
            if !Self::position_in_entry_range(position, &entry.location.range) {
                continue;
            }

            let ruby_type = match &entry.kind {
                EntryKind::InstanceVariable(data) => Some(data.r#type.clone()),
                EntryKind::ClassVariable(data) => Some(data.r#type.clone()),
                EntryKind::GlobalVariable(data) => Some(data.r#type.clone()),
                _ => None,
            };

            if let Some(ty) = ruby_type {
                if ty != RubyType::Unknown {
                    return Some(ty);
                }
            }
        }

        None
    }

    /// Infer return type for a method at the given line.
    /// Parses the file and finds the DefNode, then infers its return type.
    fn infer_method_return_type(&self, _entry_id: EntryId, line: u32) -> Option<RubyType> {
        // Parse the file and find the DefNode at this line
        let parse_result = ruby_prism::parse(self.content);
        let node = parse_result.node();
        let def_node = find_def_node_recursive(&node, line, self.content)?;

        // Create inferrer and infer the return type
        let mut index = self.index.lock();
        crate::inferrer::return_type::infer_return_type_for_node(
            &mut index,
            self.content,
            &def_node,
            None,
            None,
        )
    }

    /// Get type for a local variable by name at a position.
    /// Checks method parameters first, then falls back to assignment inference.
    pub fn get_local_variable_type(&self, name: &str, position: Position) -> Option<RubyType> {
        // 1. Check if this is a method parameter
        if let Some(param_type) = self.get_method_parameter_type(name, position) {
            return Some(param_type);
        }

        // 2. Try inferring from assignment pattern (e.g., var = Class.new)
        let content_str = std::str::from_utf8(self.content).ok()?;
        let index = self.index.lock();
        infer_type_from_assignment(content_str, name, &index)
    }

    /// Get type for a method parameter if the variable is a parameter of the enclosing method.
    fn get_method_parameter_type(&self, param_name: &str, position: Position) -> Option<RubyType> {
        let index = self.index.lock();

        // Find the method that contains this position using entry.location (full method body)
        for entry in index.file_entries(self.uri) {
            if let EntryKind::Method(data) = &entry.kind {
                // Check if position is within the method's location range (def to end)
                let range = &entry.location.range;
                let is_in_method =
                    position.line >= range.start.line && position.line <= range.end.line;

                if is_in_method {
                    // Check if this method has a parameter with the given name
                    let has_param = data.params.iter().any(|p| p.name == param_name);

                    if has_param {
                        // Check param_types first (from YARD conversion)
                        for (name, param_type) in &data.param_types {
                            if name == param_name && *param_type != RubyType::Unknown {
                                return Some(param_type.clone());
                            }
                        }
                        // Also check YARD docs for parameter types
                        if let Some(yard_doc) = &data.yard_doc {
                            if let Some(type_str) = yard_doc.get_param_type_str(param_name) {
                                return Some(RubyType::class(&type_str));
                            }
                        }
                        // Parameter exists but has no type info - return Unknown
                        return Some(RubyType::Unknown);
                    }
                }
            }
        }
        None
    }

    /// Get return type for a method call given the receiver type and method name.
    pub fn get_method_call_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, receiver_type, method_name)
    }

    /// Get return type for a method definition at a position (with on-demand inference).
    pub fn get_method_definition_type(&mut self, position: Position) -> Option<RubyType> {
        self.get_method_type_at(position)
    }

    /// Check if a position is within a range.
    #[inline]
    pub fn is_in_range(pos: &Position, range: &Range) -> bool {
        (pos.line > range.start.line
            || (pos.line == range.start.line && pos.character >= range.start.character))
            && (pos.line < range.end.line
                || (pos.line == range.end.line && pos.character <= range.end.character))
    }

    /// Check if a position is within an entry's range.
    #[inline]
    fn position_in_entry_range(pos: Position, range: &Range) -> bool {
        pos.line >= range.start.line && pos.line <= range.end.line
    }
}

/// Find a DefNode at the given line in the AST.
fn find_def_node_recursive<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &[u8],
) -> Option<ruby_prism::DefNode<'a>> {
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        let line = content[..offset].iter().filter(|&&b| b == b'\n').count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(sclass) = node.as_singleton_class_node() {
        if let Some(body) = sclass.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    None
}

/// Infer type from assignment patterns using robust AST analysis.
/// Replaces the brittle string-parsing approach with proper parsing and type resolution.
pub fn infer_type_from_assignment(
    content: &str,
    var_name: &str,
    index: &RubyIndex,
) -> Option<RubyType> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let root = parse_result.node();

    struct AssignmentFinder<'a> {
        var_name: &'a str,
        best_type: Option<RubyType>,
        index: &'a RubyIndex,
    }

    impl<'a> Visit<'a> for AssignmentFinder<'a> {
        fn visit_local_variable_write_node(
            &mut self,
            node: &ruby_prism::LocalVariableWriteNode<'a>,
        ) {
            let name = String::from_utf8_lossy(node.name().as_slice());
            if name == self.var_name {
                let val = node.value();
                self.best_type = infer_value_type(&val, self.index);
            }
            ruby_prism::visit_local_variable_write_node(self, node);
        }
    }

    let mut finder = AssignmentFinder {
        var_name,
        best_type: None,
        index,
    };
    finder.visit(&root);

    finder.best_type
}

fn infer_value_type<'a>(node: &Node<'a>, index: &RubyIndex) -> Option<RubyType> {
    let literal_analyzer = LiteralAnalyzer::new();

    // 1. Literals
    if let Some(ty) = literal_analyzer.analyze_literal(node) {
        return Some(ty);
    }

    // 2. Constant Read
    if let Some(const_node) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(const_node.name().as_slice()).to_string();
        if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
            return Some(RubyType::ClassReference(fqn));
        }
    }

    // 3. Constant Path
    if let Some(path_node) = node.as_constant_path_node() {
        if let Some(full_name) = flatten_constant_path(&path_node) {
            if let Ok(fqn) = FullyQualifiedName::try_from(full_name.as_str()) {
                return Some(RubyType::ClassReference(fqn));
            }
        }
    }

    // 4. Call Node (Recursive)
    if let Some(call_node) = node.as_call_node() {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

        let receiver_type = if let Some(receiver) = call_node.receiver() {
            infer_value_type(&receiver, index)
        } else {
            // Implicit self - assume Unknown for context-free fallback
            return None;
        };

        if let Some(recv_type) = receiver_type {
            return MethodResolver::resolve_method_return_type(index, &recv_type, &method_name);
        }
    }

    None
}

fn flatten_constant_path<'a>(node: &ruby_prism::ConstantPathNode<'a>) -> Option<String> {
    let parent_str = if let Some(parent) = node.parent() {
        if let Some(p) = parent.as_constant_path_node() {
            flatten_constant_path(&p)?
        } else if let Some(p) = parent.as_constant_read_node() {
            String::from_utf8_lossy(p.name().as_slice()).to_string()
        } else {
            return None;
        }
    } else {
        return None;
    };

    let name = node
        .name()
        .map(|n| String::from_utf8_lossy(n.as_slice()).to_string())?;
    Some(format!("{}::{}", parent_str, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_hint_kind_equality() {
        assert_eq!(TypeHintKind::MethodReturn, TypeHintKind::MethodReturn);
        assert_ne!(TypeHintKind::MethodReturn, TypeHintKind::LocalVariable);
    }

    #[test]
    fn test_is_in_range() {
        let range = Range {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 0,
            },
        };

        // Inside range
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 7,
                character: 5
            },
            &range
        ));

        // At start
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 5,
                character: 0
            },
            &range
        ));

        // At end
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 10,
                character: 0
            },
            &range
        ));

        // Before range
        assert!(!TypeQuery::is_in_range(
            &Position {
                line: 4,
                character: 0
            },
            &range
        ));

        // After range
        assert!(!TypeQuery::is_in_range(
            &Position {
                line: 11,
                character: 0
            },
            &range
        ));
    }
}
