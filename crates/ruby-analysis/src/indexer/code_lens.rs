use crate::core::{FullyQualifiedName, RubyConstant};
use ruby_prism::{ModuleNode, Node, Visit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDefinitionForLens {
    pub fqn: FullyQualifiedName,
    pub start_offset: usize,
    pub end_offset: usize,
}

pub fn module_definitions_for_lens(content: &str) -> Vec<ModuleDefinitionForLens> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let root = parse_result.node();

    let mut collector = CodeLensCollector::new();
    collector.visit(&root);
    collector.modules
}

struct CodeLensCollector {
    modules: Vec<ModuleDefinitionForLens>,
    namespace_stack: Vec<String>,
}

impl CodeLensCollector {
    fn new() -> Self {
        Self {
            modules: Vec::new(),
            namespace_stack: Vec::new(),
        }
    }

    fn compute_fqn(&self, module_name: &str) -> Option<FullyQualifiedName> {
        let mut constants = Vec::new();

        for part in &self.namespace_stack {
            match RubyConstant::new(part) {
                Ok(constant) => constants.push(constant),
                Err(_) => return None,
            }
        }

        for part in module_name.split("::") {
            match RubyConstant::new(part) {
                Ok(constant) => constants.push(constant),
                Err(_) => return None,
            }
        }

        Some(FullyQualifiedName::from(constants))
    }

    fn extract_constant_name(&self, node: &Node) -> String {
        if let Some(constant_read) = node.as_constant_read_node() {
            String::from_utf8_lossy(constant_read.name().as_slice()).to_string()
        } else if node.as_constant_path_node().is_some() {
            let mut parts = Vec::new();
            self.collect_constant_path_parts(node, &mut parts);
            parts.join("::")
        } else {
            String::new()
        }
    }

    fn collect_constant_path_parts(&self, node: &Node, parts: &mut Vec<String>) {
        if let Some(constant_path) = node.as_constant_path_node() {
            if let Some(parent) = constant_path.parent() {
                self.collect_constant_path_parts(&parent, parts);
            }
            if let Some(name_bytes) = constant_path.name() {
                parts.push(String::from_utf8_lossy(name_bytes.as_slice()).to_string());
            }
        } else if let Some(constant_read) = node.as_constant_read_node() {
            parts.push(String::from_utf8_lossy(constant_read.name().as_slice()).to_string());
        }
    }
}

impl Visit<'_> for CodeLensCollector {
    fn visit_module_node(&mut self, node: &ModuleNode<'_>) {
        let constant_path = node.constant_path();
        let module_name = self.extract_constant_name(&constant_path);

        if !module_name.is_empty() {
            if let Some(fqn) = self.compute_fqn(&module_name) {
                self.modules.push(ModuleDefinitionForLens {
                    fqn,
                    start_offset: node.location().start_offset(),
                    end_offset: constant_path.location().end_offset(),
                });
            }

            let simple_name = module_name.split("::").last().unwrap_or(&module_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        if let Some(body) = node.body() {
            self.visit(&body);
        }

        if !module_name.is_empty() {
            self.namespace_stack.pop();
        }
    }

    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'_>) {
        let constant_path = node.constant_path();
        let class_name = self.extract_constant_name(&constant_path);

        if !class_name.is_empty() {
            let simple_name = class_name.split("::").last().unwrap_or(&class_name);
            self.namespace_stack.push(simple_name.to_string());
        }

        if let Some(body) = node.body() {
            self.visit(&body);
        }

        if !class_name.is_empty() {
            self.namespace_stack.pop();
        }
    }
}
