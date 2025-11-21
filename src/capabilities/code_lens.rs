use crate::indexer::entry::MixinType;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use log::debug;
use ruby_prism::{ModuleNode, Node, Visit};
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Handle CodeLens request for a document
pub async fn handle_code_lens(
    lang_server: &RubyLanguageServer,
    params: CodeLensParams,
) -> Option<Vec<CodeLens>> {
    let uri = &params.text_document.uri;

    // Check if CodeLens is enabled in configuration
    let config = lang_server.config.lock();
    if !config.code_lens_modules_enabled.unwrap_or(true) {
        return Some(Vec::new());
    }
    drop(config);

    // Get the document
    let document = match lang_server.get_doc(uri) {
        Some(doc) => doc,
        None => {
            debug!("Document not found for URI: {}", uri);
            return Some(Vec::new());
        }
    };

    // Parse the document to find module definitions
    let parse_result = ruby_prism::parse(document.content.as_bytes());
    let root_node = parse_result.node();

    // Visit the AST to find module definitions
    let mut visitor = ModuleCodeLensVisitor::new(lang_server, uri.clone());
    visitor.visit(&root_node);

    Some(visitor.code_lenses)
}

/// Visitor to find module definitions and generate CodeLens
struct ModuleCodeLensVisitor<'a> {
    lang_server: &'a RubyLanguageServer,
    uri: Url,
    code_lenses: Vec<CodeLens>,
    namespace_stack: Vec<String>,
}

impl<'a> ModuleCodeLensVisitor<'a> {
    fn new(lang_server: &'a RubyLanguageServer, uri: Url) -> Self {
        Self {
            lang_server,
            uri,
            code_lenses: Vec::new(),
            namespace_stack: vec!["Object".to_string()], // Start with Object as the top-level namespace
        }
    }

    /// Compute the fully qualified name for the current module
    fn compute_fqn(&self, module_name: &str) -> Option<FullyQualifiedName> {
        let mut constants = Vec::new();

        // Add namespace stack
        for part in &self.namespace_stack {
            match RubyConstant::new(part) {
                Ok(c) => constants.push(c),
                Err(_) => return None,
            }
        }

        // Handle namespaced module names like "A::B"
        let name_parts: Vec<&str> = module_name.split("::").collect();
        for part in name_parts {
            match RubyConstant::new(part) {
                Ok(c) => constants.push(c),
                Err(_) => return None,
            }
        }

        Some(FullyQualifiedName::from(constants))
    }

    /// Generate CodeLens for a module definition
    fn generate_code_lens_for_module(&mut self, node: &ModuleNode) {
        // Get the module name
        let _constant_path = node.constant_path();
        let module_name = self.extract_constant_name(&_constant_path);

        if module_name.is_empty() {
            return;
        }

        // Compute the fully qualified name
        let fqn = match self.compute_fqn(&module_name) {
            Some(f) => f,
            None => return,
        };

        debug!("Generating CodeLens for module: {:?}", fqn);

        // Query the index for mixin usages
        let index = self.lang_server.index.lock();
        let usages = index.get_mixin_usages(&fqn);
        drop(index);

        if usages.is_empty() {
            debug!("No usages found for module: {:?}", fqn);
            return;
        }

        // Count usages by type
        let mut counts: HashMap<MixinType, usize> = HashMap::new();
        for usage in &usages {
            *counts.entry(usage.mixin_type).or_insert(0) += 1;
        }

        // Build the label
        let label = format_code_lens_label(&counts);

        // Get the range for the module keyword
        let start_offset = node.location().start_offset();
        let end_offset = _constant_path.location().end_offset();

        let start_position = offset_to_position(&self.lang_server, &self.uri, start_offset);
        let end_position = offset_to_position(&self.lang_server, &self.uri, end_offset);

        let range = Range {
            start: start_position,
            end: end_position,
        };

        // Create the CodeLens with a command to show references
        // Collect the locations from usages
        let locations: Vec<Location> = usages.iter().map(|u| u.location.clone()).collect();

        let code_lens = CodeLens {
            range,
            command: Some(Command {
                title: label,
                command: "ruby-fast-lsp.showReferences".to_string(), // Use our custom wrapper command
                arguments: Some(vec![
                    serde_json::to_value(self.uri.as_str()).unwrap(), // Pass URI as string
                    serde_json::to_value(start_position).unwrap(),
                    serde_json::to_value(locations).unwrap(),
                ]),
            }),
            data: None,
        };

        self.code_lenses.push(code_lens);
    }

    /// Extract constant name from a node
    fn extract_constant_name(&self, node: &Node) -> String {
        if let Some(constant_read) = node.as_constant_read_node() {
            String::from_utf8_lossy(constant_read.name().as_slice()).to_string()
        } else if let Some(_constant_path) = node.as_constant_path_node() {
            // Handle namespaced constants like A::B
            let mut parts = Vec::new();
            self.collect_constant_path_parts(node, &mut parts);
            parts.join("::")
        } else {
            String::new()
        }
    }

    /// Recursively collect constant path parts
    fn collect_constant_path_parts(&self, node: &Node, parts: &mut Vec<String>) {
        if let Some(constant_path) = node.as_constant_path_node() {
            // Process parent first (left side)
            if let Some(parent) = constant_path.parent() {
                self.collect_constant_path_parts(&parent, parts);
            }

            // Then add the name (right side)
            if let Some(name_bytes) = constant_path.name() {
                let name = String::from_utf8_lossy(name_bytes.as_slice()).to_string();
                parts.push(name);
            }
        } else if let Some(constant_read) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice()).to_string();
            parts.push(name);
        }
    }
}

impl<'a> Visit<'_> for ModuleCodeLensVisitor<'a> {
    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        // Generate CodeLens for this module
        self.generate_code_lens_for_module(node);

        // Push the module name onto the namespace stack
        let _constant_path = node.constant_path();
        let module_name = self.extract_constant_name(&_constant_path);

        if !module_name.is_empty() {
            // For namespaced modules like "A::B", only push the last part
            let simple_name = module_name.split("::").last().unwrap_or(&module_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        // Visit children
        if let Some(body) = node.body() {
            self.visit(&body);
        }

        // Pop the module name from the namespace stack
        if !module_name.is_empty() {
            self.namespace_stack.pop();
        }
    }

    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'_>) {
        // Push class name onto namespace stack for nested modules
        let _constant_path = node.constant_path();
        let class_name = self.extract_constant_name(&_constant_path);

        if !class_name.is_empty() {
            let simple_name = class_name.split("::").last().unwrap_or(&class_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        // Visit children
        if let Some(body) = node.body() {
            self.visit(&body);
        }

        // Pop the class name
        if !class_name.is_empty() {
            self.namespace_stack.pop();
        }
    }
}

/// Format the CodeLens label based on usage counts
fn format_code_lens_label(counts: &HashMap<MixinType, usize>) -> String {
    let mut parts = Vec::new();

    if let Some(&count) = counts.get(&MixinType::Include) {
        if count > 0 {
            parts.push(format!("{} include", count));
        }
    }

    if let Some(&count) = counts.get(&MixinType::Prepend) {
        if count > 0 {
            parts.push(format!("{} prepend", count));
        }
    }

    if let Some(&count) = counts.get(&MixinType::Extend) {
        if count > 0 {
            parts.push(format!("{} extend", count));
        }
    }

    parts.join(" | ")
}

/// Convert byte offset to LSP Position
fn offset_to_position(lang_server: &RubyLanguageServer, uri: &Url, offset: usize) -> Position {
    let document = lang_server.get_doc(uri).unwrap();
    let content = &document.content;

    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for ch in content.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }

        current_offset += ch.len_utf8();
    }

    Position {
        line: line as u32,
        character: character as u32,
    }
}
