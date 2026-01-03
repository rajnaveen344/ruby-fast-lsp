//! Return Type Inference using CFG-based dataflow analysis.
//!
//! This module infers return types from method bodies by:
//! 1. Building a Control Flow Graph (CFG) from the method AST
//! 2. Running dataflow analysis to propagate type narrowing
//! 3. Collecting return types from all exit paths with proper narrowed types
//!
//! ## Architecture
//!
//! The inferrer uses a stack-based approach for recursive inference:
//! - Entry point: `infer_method_return_type(index, method_fqn)`
//! - Looks up method definitions by FQN
//! - Loads file content, parses AST, finds DefNode
//! - Infers return type, caching results in the index
//! - For method calls, recursively infers callee return types
//! - Stack tracks in-progress inferences to detect cycles

use crate::indexer::entry::{entry_kind::EntryKind, MethodKind};
use crate::indexer::index::{FileId, RubyIndex};
use crate::inferrer::cfg::{CfgBuilder, DataflowAnalyzer, StatementKind, TypeState};
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use ruby_prism::*;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

// ============================================================================
// Public API - Stack-based inference
// ============================================================================

/// Map of file URIs to their content bytes for virtual/dirty files.
pub type FileContentMap<'a> = HashMap<&'a Url, &'a [u8]>;

/// Infer return type for a method by its FQN.
/// This is the primary entry point for type inference.
///
/// Uses stack-based cycle detection and caches results in the index.
/// Provide `None` for `stack` when calling from outside (it will be initialized internally).
/// Provide `file_contents` to override content for files (e.g. dirty buffers in IDE or virtual test files).
pub fn infer_method_return_type(
    index: &mut RubyIndex,
    method_fqn: &FullyQualifiedName,
    stack: Option<&mut Vec<FullyQualifiedName>>,
    file_contents: Option<&FileContentMap>,
) -> Option<RubyType> {
    let mut local_stack = Vec::new();
    let stack = stack.unwrap_or(&mut local_stack);

    if stack.contains(method_fqn) {
        return None;
    }

    if let Some(cached) = check_cache(index, method_fqn, file_contents) {
        return Some(cached);
    }

    stack.push(method_fqn.clone());

    let mut return_values = Vec::new();
    let locations = get_method_locations(index, method_fqn);

    for (file_id, line) in locations {
        if let Some(content) = load_content_for_inference(index, file_id, file_contents) {
            let parse_result = ruby_prism::parse(&content);
            let node = parse_result.node();

            if let Some(def_node) = find_def_node_at_line(&node, line, &content) {
                let owner_fqn = FullyQualifiedName::Constant(method_fqn.namespace_parts());
                let mut ctx = InferenceContext {
                    index,
                    stack,
                    source: &content,
                    owner_fqn,
                    file_contents,
                };

                return_values.extend(infer_from_def_node(&mut ctx, &def_node));
            }
        }
    }

    stack.pop();

    if return_values.is_empty() {
        return None;
    }

    let return_types: Vec<RubyType> = return_values.into_iter().map(|(ty, _, _)| ty).collect();
    let union_type = RubyType::union(return_types);

    update_index_cache(index, method_fqn, &union_type);

    Some(union_type)
}

/// Helper to infer return values (type + location) for a specific DefNode without full FQN context.
/// Useful for validation during indexing or testing.
pub fn infer_return_values_for_node(
    index: &mut RubyIndex,
    source: &[u8],
    def_node: &DefNode,
) -> Vec<(RubyType, usize, usize)> {
    let mut stack = Vec::new();
    // We don't know the owner FQN here, so we use empty.
    let owner_fqn = FullyQualifiedName::Constant(vec![]);
    let mut ctx = InferenceContext {
        index,
        stack: &mut stack,
        source,
        owner_fqn,
        file_contents: None,
    };
    infer_from_def_node(&mut ctx, def_node)
}

/// Helper to infer return type for a specific DefNode without full FQN context.
/// Useful for validation during indexing or testing.
pub fn infer_return_type_for_node(
    index: &mut RubyIndex,
    source: &[u8],
    def_node: &DefNode,
) -> Option<RubyType> {
    let values = infer_return_values_for_node(index, source, def_node);
    if values.is_empty() {
        None
    } else {
        let types: Vec<RubyType> = values.into_iter().map(|(ty, _, _)| ty).collect();
        Some(RubyType::union(types))
    }
}

/// Helper to infer return type for a method call (receiver.method).
/// Checks RBS first, then source code inference.
pub fn infer_method_call(
    index: &mut RubyIndex,
    receiver_type: &RubyType,
    method_name: &str,
    file_contents: Option<&FileContentMap>,
) -> Option<RubyType> {
    // 1. Try MethodResolver (which checks RBS and Index signatures)
    if let Some(ty) = crate::inferrer::method::MethodResolver::resolve_method_return_type(
        index,
        receiver_type,
        method_name,
    ) {
        return Some(ty);
    }

    // 2. Fallback to source inference
    let method_fqn = match receiver_type {
        RubyType::ClassReference(fqn) | RubyType::Class(fqn) => FullyQualifiedName::method(
            fqn.namespace_parts(),
            RubyMethod::new(method_name, MethodKind::Instance).ok()?,
        ),
        // TODO: Handle other types
        _ => return None,
    };

    infer_method_return_type(index, &method_fqn, None, file_contents)
}

fn check_cache(
    index: &RubyIndex,
    method_fqn: &FullyQualifiedName,
    file_contents: Option<&FileContentMap>,
) -> Option<RubyType> {
    // Skip cache if the method is defined in any of the provided files (dirty buffers)
    if let Some(contents) = file_contents {
        if let Some(entries) = index.get(method_fqn) {
            let is_in_provided = entries.iter().any(|e| {
                index
                    .get_file_url(e.location.file_id)
                    .map_or(false, |url| contents.contains_key(url))
            });
            if is_in_provided {
                return None;
            }
        }
    }

    index.get(method_fqn)?.iter().find_map(|entry| {
        if let EntryKind::Method(data) = &entry.kind {
            if let Some(rt) = &data.return_type {
                if *rt != RubyType::Unknown {
                    return Some(rt.clone());
                }
            }
        }
        None
    })
}

fn get_method_locations(index: &RubyIndex, method_fqn: &FullyQualifiedName) -> Vec<(FileId, u32)> {
    index
        .get(method_fqn)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| {
                    if let EntryKind::Method(data) = &e.kind {
                        let line = data
                            .return_type_position
                            .map(|p| p.line)
                            .unwrap_or(e.location.range.start.line);
                        Some((e.location.file_id, line))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn load_content_for_inference<'a>(
    index: &RubyIndex,
    file_id: FileId,
    file_contents: Option<&'a FileContentMap<'a>>,
) -> Option<std::borrow::Cow<'a, [u8]>> {
    let file_url = index.get_file_url(file_id)?;

    // Check if content is provided in the map (virtual/dirty files)
    if let Some(contents) = file_contents {
        if let Some(&content) = contents.get(file_url) {
            return Some(std::borrow::Cow::Borrowed(content));
        }
    }

    // Fall back to loading from disk
    load_file_content(file_url).map(std::borrow::Cow::Owned)
}

fn update_index_cache(
    index: &mut RubyIndex,
    method_fqn: &FullyQualifiedName,
    return_type: &RubyType,
) {
    if let Some(entry_ids) = index.get_entry_ids_for_fqn(method_fqn) {
        let ids = entry_ids.clone();
        for id in ids {
            index.update_method_return_type(id, return_type.clone());
        }
    }
}

/// Load file content from URL
fn load_file_content(url: &tower_lsp::lsp_types::Url) -> Option<Vec<u8>> {
    let path = url.to_file_path().ok()?;
    std::fs::read(&path).ok()
}

// ============================================================================
// Inference Context
// ============================================================================

/// Context passed through inference, containing index and stack.
struct InferenceContext<'a> {
    index: &'a mut RubyIndex,
    stack: &'a mut Vec<FullyQualifiedName>,
    source: &'a [u8],
    owner_fqn: FullyQualifiedName,
    file_contents: Option<&'a FileContentMap<'a>>,
}

impl<'a> InferenceContext<'a> {
    fn infer_expression(&mut self, node: &Node, state: Option<&TypeState>) -> Option<RubyType> {
        let literal_analyzer = LiteralAnalyzer::new();

        // Literal analysis
        if let Some(ty) = literal_analyzer.analyze_literal(node) {
            return Some(ty);
        }

        // Variable reads
        if let Some(local_var) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(local_var.name().as_slice()).to_string();
            if let Some(type_state) = state {
                if let Some(ty) = type_state.get_type(&var_name) {
                    return Some(ty.clone());
                }
            }
        }

        // Method calls - recursive inference using stack!
        if let Some(call) = node.as_call_node() {
            return self.infer_call(&call, state);
        }

        // Control flow
        if let Some(if_node) = node.as_if_node() {
            return self.infer_if_return_type(&if_node, state);
        }

        if let Some(case_node) = node.as_case_node() {
            return self.infer_case_return_type(&case_node, state);
        }

        // Handle parenthesized expressions
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.infer_expression(&body, state);
            }
            return Some(RubyType::nil_class());
        }

        // Interpolated string always returns String
        if node.as_interpolated_string_node().is_some() {
            return Some(RubyType::string());
        }

        if let Some(stmts) = node.as_statements_node() {
            let stmt_list: Vec<_> = stmts.body().iter().collect();
            if let Some(last) = stmt_list.last() {
                return self.infer_expression(last, state);
            }
        }

        // Handle else node (from if/unless)
        if let Some(else_node) = node.as_else_node() {
            if let Some(statements) = else_node.statements() {
                return self.infer_expression(&statements.as_node(), state);
            }
            return Some(RubyType::nil_class());
        }

        None
    }

    fn infer_call(&mut self, call: &CallNode, state: Option<&TypeState>) -> Option<RubyType> {
        let method_name = String::from_utf8_lossy(call.name().as_slice()).to_string();
        let ruby_method = RubyMethod::new(&method_name, MethodKind::Instance).ok()?;

        // 1. Determine receiver type
        let receiver_type = if let Some(receiver) = call.receiver() {
            self.infer_expression(&receiver, state)
        } else {
            // Implicit self
            Some(RubyType::ClassReference(self.owner_fqn.clone()))
        };

        let recv_type = receiver_type?;

        // 2. Get the receiver's FQN for MRO lookup
        let receiver_fqn = match &recv_type {
            RubyType::ClassReference(fqn) | RubyType::Class(fqn) => fqn.clone(),
            // TODO: Handle other types
            _ => return Some(RubyType::Unknown),
        };

        // 3. Search for method in the receiver's ancestor chain (MRO)
        // This follows Ruby's method resolution order: self -> prepends -> includes -> superclass
        let ancestor_chain = self.index.get_ancestor_chain(&receiver_fqn, false);

        for ancestor in &ancestor_chain {
            let method_fqn =
                FullyQualifiedName::method(ancestor.namespace_parts(), ruby_method.clone());

            if let Some(ty) = infer_method_return_type(
                self.index,
                &method_fqn,
                Some(self.stack),
                self.file_contents,
            ) {
                return Some(ty);
            }
        }

        // 4. Fallback for cross-module calls: search classes that include this module
        // When ModuleA#foo calls bar, and bar is defined in ModuleB, we need to find
        // classes that include both ModuleA and ModuleB to resolve the call.
        if let Some(ty) = self.infer_via_including_classes(&receiver_fqn, &ruby_method) {
            return Some(ty);
        }

        // 5. Method not found - return Unknown
        Some(RubyType::Unknown)
    }

    /// Search for a method by looking at classes that include the current module.
    /// This handles cross-module calls where two modules are mixed into the same class.
    fn infer_via_including_classes(
        &mut self,
        module_fqn: &FullyQualifiedName,
        ruby_method: &RubyMethod,
    ) -> Option<RubyType> {
        // Get all classes/modules that include this module
        let includers = self.index.get_including_classes(module_fqn);

        for includer_fqn in includers {
            // Get the includer's full MRO
            let mro = self.index.get_ancestor_chain(&includer_fqn, false);

            // Search for the method in the includer's MRO
            for ancestor in &mro {
                let method_fqn =
                    FullyQualifiedName::method(ancestor.namespace_parts(), ruby_method.clone());

                if let Some(ty) = infer_method_return_type(
                    self.index,
                    &method_fqn,
                    Some(self.stack),
                    self.file_contents,
                ) {
                    return Some(ty);
                }
            }
        }

        None
    }

    /// Infer return type from an if/elsif/else chain
    fn infer_if_return_type(
        &mut self,
        if_node: &IfNode,
        state: Option<&TypeState>,
    ) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Then branch
        if let Some(statements) = if_node.statements() {
            if let Some(then_type) = self.infer_expression(&statements.as_node(), state) {
                branch_types.push(then_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        // Else branch
        if let Some(subsequent) = if_node.subsequent() {
            if let Some(else_type) = self.infer_expression(&subsequent, state) {
                branch_types.push(else_type);
            }
        } else {
            branch_types.push(RubyType::nil_class());
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Infer return type from a case/when statement with proper type narrowing
    fn infer_case_return_type(
        &mut self,
        case_node: &CaseNode,
        _parent_state: Option<&TypeState>,
    ) -> Option<RubyType> {
        let mut branch_types = Vec::new();

        // Get the case predicate variable name
        let case_var = case_node
            .predicate()
            .and_then(|p| self.extract_variable_name(&p));

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                // Extract the pattern type from the when clause
                let pattern_type = self.extract_when_pattern_type(&when_node);

                // Create a narrowed state for this branch
                let narrowed_state = if let (Some(ref var_name), Some(ref narrowed_type)) =
                    (&case_var, &pattern_type)
                {
                    let mut state = TypeState::new();
                    state.set_type(var_name.clone(), narrowed_type.clone());
                    Some(state)
                } else {
                    None
                };

                if let Some(statements) = when_node.statements() {
                    if let Some(when_type) =
                        self.infer_expression(&statements.as_node(), narrowed_state.as_ref())
                    {
                        branch_types.push(when_type);
                    }
                } else {
                    branch_types.push(RubyType::nil_class());
                }
            }
        }

        // Else clause
        if let Some(else_clause) = case_node.else_clause() {
            if let Some(else_type) = self.infer_expression(&else_clause.as_node(), None) {
                branch_types.push(else_type);
            }
        } else {
            // No else means case can return nil
            branch_types.push(RubyType::nil_class());
        }

        if branch_types.is_empty() {
            None
        } else {
            Some(RubyType::union(branch_types))
        }
    }

    /// Extract variable name from a node
    fn extract_variable_name(&self, node: &Node) -> Option<String> {
        if let Some(local_var) = node.as_local_variable_read_node() {
            return Some(String::from_utf8_lossy(local_var.name().as_slice()).to_string());
        }
        None
    }

    /// Extract pattern type from a when clause
    fn extract_when_pattern_type(&self, when_node: &WhenNode) -> Option<RubyType> {
        let first_condition = when_node.conditions().iter().next()?;

        // Handle constant like String, Integer
        if let Some(const_read) = first_condition.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            return Some(self.constant_to_ruby_type(&name));
        }

        // Handle nil literal
        if first_condition.as_nil_node().is_some() {
            return Some(RubyType::nil_class());
        }

        // Handle true literal
        if first_condition.as_true_node().is_some() {
            return Some(RubyType::true_class());
        }

        // Handle false literal
        if first_condition.as_false_node().is_some() {
            return Some(RubyType::false_class());
        }

        None
    }

    /// Convert constant name to RubyType
    fn constant_to_ruby_type(&self, name: &str) -> RubyType {
        match name {
            "String" => RubyType::string(),
            "Integer" => RubyType::integer(),
            "Float" => RubyType::float(),
            "Symbol" => RubyType::symbol(),
            "TrueClass" => RubyType::true_class(),
            "FalseClass" => RubyType::false_class(),
            "NilClass" => RubyType::nil_class(),
            "Array" => RubyType::Array(vec![RubyType::Unknown]),
            "Hash" => RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown]),
            _ => RubyType::class(name),
        }
    }
}

/// Infer return type from a DefNode using the new stack-based approach.
/// This is a simplified version that handles basic cases.
fn infer_from_def_node(
    ctx: &mut InferenceContext,
    def_node: &DefNode,
) -> Vec<(RubyType, usize, usize)> {
    let literal_analyzer = LiteralAnalyzer::new();

    // Try fast path first (simple literals, no CFG needed)
    if let Some(ty) = try_simple_inference_static(&literal_analyzer, def_node) {
        // Find location for simple inference (whole method)
        let loc = def_node.location();
        return vec![(ty, loc.start_offset(), loc.end_offset())];
    }

    // Fall back to CFG-based inference
    let builder = CfgBuilder::new(ctx.source);
    let cfg = builder.build_from_method(def_node);

    let mut analyzer = DataflowAnalyzer::new(&cfg);
    let initial_state = TypeState::from_parameters(&cfg.parameters);
    analyzer.analyze(initial_state);
    let _results = analyzer.into_results();

    // Collect return types from exit blocks
    let mut return_values = Vec::new();

    for exit_id in &cfg.exits {
        if let Some(block) = cfg.blocks.get(exit_id) {
            for stmt in &block.statements {
                if let StatementKind::Return { value_type } = &stmt.kind {
                    let ty = value_type.clone();

                    // If CFG didn't infer type (e.g. method call), try to re-infer using context
                    if ty.is_none() || ty == Some(RubyType::Unknown) {
                        // Ideally we'd find the return node and re-infer.
                        // For now we just use what we have.
                    }

                    if let Some(t) = ty {
                        return_values.push((t, stmt.start_offset, stmt.end_offset));
                    }
                }
            }
        }
    }

    // Handle implicit return from method body
    if let Some(body) = def_node.body() {
        let ends_with_explicit_return = if let Some(stmts) = body.as_statements_node() {
            stmts
                .body()
                .iter()
                .last()
                .map(|s| s.as_return_node().is_some())
                .unwrap_or(false)
        } else {
            body.as_return_node().is_some()
        };

        if !ends_with_explicit_return {
            // Use context to infer implicit return!
            // We need to find the last expression node.
            let last_node = if let Some(stmts) = body.as_statements_node() {
                stmts.body().iter().last()
            } else {
                Some(body) // Direct expression
            };

            if let Some(node) = last_node {
                // Get exit state from CFG if possible
                if let Some(ty) = ctx.infer_expression(&node, None) {
                    let start = node.location().start_offset();
                    let end = node.location().end_offset();
                    return_values.push((ty, start, end));
                }
            }
        }
    } else {
        // Empty method returns nil
        let start = def_node.name_loc().start_offset();
        let end = def_node.name_loc().end_offset();
        return_values.push((RubyType::nil_class(), start, end));
    }

    return_values
}

/// Fast path inference for simple methods (static version).
fn try_simple_inference_static(analyzer: &LiteralAnalyzer, method: &DefNode) -> Option<RubyType> {
    let body = method.body()?;

    if let Some(stmts) = body.as_statements_node() {
        let stmt_list: Vec<_> = stmts.body().iter().collect();

        if stmt_list.is_empty() {
            return Some(RubyType::nil_class());
        }

        let last = stmt_list.last()?;

        if let Some(ret) = last.as_return_node() {
            if let Some(args) = ret.arguments() {
                let args_list: Vec<_> = args.arguments().iter().collect();
                if args_list.len() == 1 {
                    return analyzer.analyze_literal(&args_list[0]);
                }
            }
            return Some(RubyType::nil_class());
        }

        return analyzer.analyze_literal(last);
    }

    analyzer.analyze_literal(&body)
}

/// Find a DefNode at the given line in the AST.
fn find_def_node_at_line<'a>(
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
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
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
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(sclass) = node.as_singleton_class_node() {
        if let Some(body) = sclass.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    None
}

// ============================================================================
// Legacy API - ReturnTypeInferrer struct (for backward compatibility)
// ============================================================================

// [Legacy code removed]

#[cfg(test)]
mod tests {
    use super::*;

    fn infer_return_type(source: &str) -> Option<RubyType> {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();
        let mut index = crate::indexer::index::RubyIndex::new();

        if let Some(program) = ast.as_program_node() {
            let statements = program.statements();
            for stmt in statements.body().iter() {
                if let Some(def_node) = stmt.as_def_node() {
                    return infer_return_type_for_node(&mut index, source.as_bytes(), &def_node);
                }
            }
        }
        None
    }
    // =========================================================================
    // Simple literal returns
    // =========================================================================

    #[test]
    fn test_simple_string_return() {
        let source = r#"
def greet
  "hello"
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::string()));
    }

    #[test]
    fn test_simple_integer_return() {
        let source = r#"
def answer
  42
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::integer()));
    }

    #[test]
    fn test_simple_float_return() {
        let source = r#"
def pi
  3.14
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::float()));
    }

    #[test]
    fn test_simple_symbol_return() {
        let source = r#"
def status
  :ok
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::symbol()));
    }

    #[test]
    fn test_simple_true_return() {
        let source = r#"
def active?
  true
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::true_class()));
    }

    #[test]
    fn test_simple_false_return() {
        let source = r#"
def disabled?
  false
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::false_class()));
    }

    #[test]
    fn test_simple_nil_return() {
        let source = r#"
def nothing
  nil
end
"#;
        let result = infer_return_type(source);
        assert_eq!(result, Some(RubyType::nil_class()));
    }

    // =========================================================================
    // Case/when with type narrowing
    // =========================================================================

    #[test]
    fn test_case_when_with_method_calls() {
        let source = r#"
def process(value)
  case value
  when String
    value.upcase
  when Integer
    value + 1
  when nil
    "nil"
  end
end
"#;
        let result = infer_return_type(source);

        assert!(result.is_some(), "Expected a return type");
        let result = result.unwrap();

        if let RubyType::Union(types) = &result {
            assert!(
                types.contains(&RubyType::string()),
                "Expected String in union, got: {:?}",
                types
            );
            assert!(
                types.contains(&RubyType::integer()),
                "Expected Integer in union, got: {:?}",
                types
            );
            assert!(
                types.contains(&RubyType::nil_class()),
                "Expected NilClass in union, got: {:?}",
                types
            );
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }

    #[test]
    fn test_string_method_return_type() {
        let source = r#"
def get_upper
  "hello".upcase
end
"#;
        let result = infer_return_type(source);
        assert_eq!(
            result,
            Some(RubyType::string()),
            "String#upcase should return String"
        );
    }

    #[test]
    fn test_integer_addition_return_type() {
        let source = r#"
def add_one
  1 + 1
end
"#;
        let result = infer_return_type(source);
        assert_eq!(
            result,
            Some(RubyType::integer()),
            "Integer#+ should return Integer"
        );
    }

    // =========================================================================
    // If/else branches
    // =========================================================================

    #[test]
    fn test_if_else_different_types() {
        let source = r#"
def maybe_number(flag)
  if flag
    42
  else
    "not a number"
  end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some());
        if let Some(RubyType::Union(types)) = &result {
            assert!(types.contains(&RubyType::integer()));
            assert!(types.contains(&RubyType::string()));
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }

    #[test]
    fn test_if_without_else() {
        let source = r#"
def maybe_string(flag)
  if flag
    "hello"
  end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some());
        if let Some(RubyType::Union(types)) = result {
            assert!(types.contains(&RubyType::string()));
            assert!(types.contains(&RubyType::nil_class()));
        } else {
            panic!("Expected Union type");
        }
    }

    // =========================================================================
    // Empty method
    // =========================================================================

    #[test]
    fn test_empty_method() {
        let source = r#"
def empty
end
"#;
        let result = infer_return_type(source);
        // Empty methods return nil in Ruby - this is deterministic behavior
        assert_eq!(result, Some(RubyType::nil_class()));
    }

    // =========================================================================
    // Mixed explicit return and implicit return
    // =========================================================================

    #[test]
    fn test_explicit_and_implicit_return() {
        // This method has:
        // 1. An explicit `return 1.0` in the if branch
        // 2. An implicit return from the case statement
        let source = r#"
def process(value)
    if rand > 0.5
        return 1.0
    end

    case value
    when String
        value.upcase
    when Integer
        value + 1
    when nil
        "nil"
    end
end
"#;
        let result = infer_return_type(source);
        assert!(result.is_some(), "Expected a return type");
        let result = result.unwrap();

        if let RubyType::Union(types) = &result {
            // Should contain Float (from explicit return 1.0)
            assert!(
                types.contains(&RubyType::float()),
                "Expected Float in union, got: {:?}",
                types
            );
            // Should contain String (from upcase and "nil")
            assert!(
                types.contains(&RubyType::string()),
                "Expected String in union, got: {:?}",
                types
            );
            // Should contain Integer (from value + 1)
            assert!(
                types.contains(&RubyType::integer()),
                "Expected Integer in union, got: {:?}",
                types
            );
            // Should contain NilClass (from no else in case)
            assert!(
                types.contains(&RubyType::nil_class()),
                "Expected NilClass in union, got: {:?}",
                types
            );
        } else {
            panic!("Expected Union type, got: {:?}", result);
        }
    }

    // =========================================================================
    // New stack-based API tests
    // =========================================================================

    #[test]
    fn test_new_api_infer_from_def_node_simple() {
        // Test the new infer_from_def_node function directly
        let source = r#"
def greet
  "hello"
end
"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();

        if let Some(program) = ast.as_program_node() {
            for stmt in program.statements().body().iter() {
                if let Some(def_node) = stmt.as_def_node() {
                    let mut index = crate::indexer::index::RubyIndex::new();
                    let result =
                        infer_return_type_for_node(&mut index, source.as_bytes(), &def_node);
                    assert_eq!(result, Some(RubyType::string()));
                    return;
                }
            }
        }
        panic!("No def node found");
    }

    #[test]
    fn test_new_api_find_def_node_at_line() {
        let source = r#"class Foo
  def bar
    42
  end
end
"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let ast = parse_result.node();

        // Line 1 (0-indexed) should have the def
        let def_node = find_def_node_at_line(&ast, 1, source.as_bytes());
        assert!(def_node.is_some(), "Should find def node at line 1");

        let def = def_node.unwrap();
        let name = std::str::from_utf8(def.name().as_slice()).unwrap();
        assert_eq!(name, "bar");
    }
}
