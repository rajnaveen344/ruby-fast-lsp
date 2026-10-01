//! Catalog-independent static scans of Ruby source for Java dependencies,
//! canonical proxy references, and catalog-sensitive JRuby semantics.

use super::declarations::evaluate_static_import_alias;
use super::syntax::{canonical_java_constant_path, dotted_call_name, is_java_class_name};
use ruby_prism::{
    visit_call_node, visit_constant_path_node, visit_constant_read_node, CallNode,
    ConstantPathNode, ConstantReadNode, Node, Visit,
};

#[cfg(test)]
std::thread_local! {
    pub(super) static SEMANTIC_PREFILTER_PARSE_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum StaticJavaDependency {
    Class(String),
    Package(String),
}

pub fn static_java_import_names(source: &str) -> Vec<String> {
    static_java_dependencies(source)
        .into_iter()
        .filter_map(|dependency| match dependency {
            StaticJavaDependency::Class(name) => Some(name),
            StaticJavaDependency::Package(_) => None,
        })
        .collect()
}

pub fn static_java_dependencies(source: &str) -> Vec<StaticJavaDependency> {
    record_semantic_prefilter_parse();
    let parse = ruby_prism::parse(source.as_bytes());
    static_java_dependencies_for_node(&parse.node())
}

fn static_java_dependencies_for_node(node: &Node<'_>) -> Vec<StaticJavaDependency> {
    let mut visitor = StaticImportVisitor {
        dependencies: Vec::new(),
    };
    visitor.visit(node);
    visitor.dependencies.sort();
    visitor.dependencies.dedup();
    visitor.dependencies
}

pub fn static_java_proxy_references(source: &str) -> Vec<String> {
    record_semantic_prefilter_parse();
    let parse = ruby_prism::parse(source.as_bytes());
    static_java_proxy_references_for_node(&parse.node())
}

fn static_java_proxy_references_for_node(node: &Node<'_>) -> Vec<String> {
    let mut visitor = StaticProxyVisitor {
        references: Vec::new(),
    };
    visitor.visit(node);
    visitor.references.sort();
    visitor.references.dedup();
    visitor.references
}

pub fn source_semantics_depend_on_jruby_catalog(source: &str) -> bool {
    record_semantic_prefilter_parse();
    let parse = ruby_prism::parse(source.as_bytes());
    let mut visitor = StaticNavigationVisitor::default();
    visitor.visit(&parse.node());
    visitor.catalog_sensitive
        || !visitor.dependencies.is_empty()
        || !visitor.proxy_references.is_empty()
}

fn record_semantic_prefilter_parse() {
    #[cfg(test)]
    SEMANTIC_PREFILTER_PARSE_COUNT.with(|count| count.set(count.get() + 1));
}

struct StaticImportVisitor {
    dependencies: Vec<StaticJavaDependency>,
}

struct StaticProxyVisitor {
    references: Vec<String>,
}

#[derive(Default)]
pub(super) struct StaticNavigationVisitor {
    pub(super) dependencies: Vec<StaticJavaDependency>,
    pub(super) proxy_references: Vec<String>,
    pub(super) constant_references: Vec<String>,
    catalog_sensitive: bool,
}

impl<'pr> Visit<'pr> for StaticNavigationVisitor {
    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        collect_static_dependencies_from_call(node, &mut self.dependencies);
        if matches!(
            node.name().as_slice(),
            b"java_package" | b"java_alias" | b"java_send" | b"java_method" | b"to_java"
        ) {
            self.catalog_sensitive = true;
        }
        if let Some(reference) = dotted_call_name(node).filter(|reference| reference.contains('.'))
        {
            self.proxy_references.push(reference);
        }
        visit_call_node(self, node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode<'pr>) {
        self.constant_references
            .push(String::from_utf8_lossy(node.name().as_slice()).to_string());
        visit_constant_read_node(self, node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode<'pr>) {
        if let Some(reference) = canonical_java_constant_path(node) {
            self.proxy_references.push(reference);
        }
        visit_constant_path_node(self, node);
    }
}

impl<'pr> Visit<'pr> for StaticProxyVisitor {
    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        if let Some(reference) = dotted_call_name(node).filter(|reference| reference.contains('.'))
        {
            self.references.push(reference);
        }
        visit_call_node(self, node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode<'pr>) {
        if let Some(reference) = canonical_java_constant_path(node) {
            self.references.push(reference);
        }
        visit_constant_path_node(self, node);
    }
}

impl<'pr> Visit<'pr> for StaticImportVisitor {
    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        collect_static_dependencies_from_call(node, &mut self.dependencies);
        visit_call_node(self, node);
    }
}

fn collect_static_dependencies_from_call(
    node: &CallNode<'_>,
    dependencies: &mut Vec<StaticJavaDependency>,
) {
    let has_supported_alias_block = node
        .block()
        .and_then(|block| block.as_block_node())
        .is_some_and(|block| {
            evaluate_static_import_alias(&block, "example.package", "Example").is_some()
        });
    if node.receiver().is_some() || (node.block().is_some() && !has_supported_alias_block) {
        return;
    }
    let Some(arguments) = node.arguments() else {
        return;
    };
    let mut names = Vec::new();
    for argument in arguments.arguments().iter() {
        collect_static_import_names(&argument, &mut names);
    }
    match node.name().as_slice() {
        b"java_import" => dependencies.extend(names.into_iter().map(StaticJavaDependency::Class)),
        b"include_package" => {
            dependencies.extend(names.into_iter().map(StaticJavaDependency::Package))
        }
        b"include" | b"java_implements" => {
            dependencies.extend(names.into_iter().map(StaticJavaDependency::Class))
        }
        b"import" => dependencies.extend(names.into_iter().map(|name| {
            if is_java_class_name(&name) {
                StaticJavaDependency::Class(name)
            } else {
                StaticJavaDependency::Package(name)
            }
        })),
        _ => {}
    }
}

fn collect_static_import_names(node: &Node<'_>, imports: &mut Vec<String>) {
    if let Some(array) = node.as_array_node() {
        for element in array.elements().iter() {
            collect_static_import_names(&element, imports);
        }
        return;
    }
    if let Some(string) = node.as_string_node() {
        imports.push(String::from_utf8_lossy(string.unescaped()).to_string());
        return;
    }
    if let Some(call) = node.as_call_node().and_then(|call| dotted_call_name(&call)) {
        if call.contains('.') {
            imports.push(call);
        }
    }
}
