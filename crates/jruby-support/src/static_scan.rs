//! Catalog-independent static scans of Ruby source for Java dependencies,
//! canonical proxy references, and catalog-sensitive JRuby semantics.

use crate::syntax::{
    canonical_java_constant_path, dotted_call_name, evaluate_static_import_alias,
    is_java_class_name, package_module_call_reference,
};
use ruby_prism::{
    visit_call_node, visit_constant_path_node, visit_constant_read_node, CallNode,
    ConstantPathNode, ConstantReadNode, Node, Visit,
};

#[cfg(test)]
std::thread_local! {
    static SEMANTIC_PREFILTER_PARSE_COUNT: std::cell::Cell<usize> = const {
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
struct StaticNavigationVisitor {
    dependencies: Vec<StaticJavaDependency>,
    proxy_references: Vec<String>,
    constant_references: Vec<String>,
    catalog_sensitive: bool,
}

/// Static Java evidence of one syntax tree, each list sorted and deduplicated:
/// declared dependencies, dotted or canonical proxy references, and every bare
/// constant read (a candidate `include_package` constant).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticNavigationScan {
    pub dependencies: Vec<StaticJavaDependency>,
    pub proxy_references: Vec<String>,
    pub constant_references: Vec<String>,
}

pub fn static_navigation_scan(node: &Node<'_>) -> StaticNavigationScan {
    let mut visitor = StaticNavigationVisitor::default();
    visitor.visit(node);
    visitor.dependencies.sort();
    visitor.dependencies.dedup();
    visitor.proxy_references.sort();
    visitor.proxy_references.dedup();
    visitor.constant_references.sort();
    visitor.constant_references.dedup();
    StaticNavigationScan {
        dependencies: visitor.dependencies,
        proxy_references: visitor.proxy_references,
        constant_references: visitor.constant_references,
    }
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
        if let Some(reference) = static_call_proxy_reference(node) {
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
        if let Some(reference) = static_call_proxy_reference(node) {
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

/// A dotted `java.util.List` or package-module `Java::JavaUtil.List` call.
fn static_call_proxy_reference(node: &CallNode<'_>) -> Option<String> {
    dotted_call_name(node)
        .filter(|reference| reference.contains('.'))
        .or_else(|| package_module_call_reference(node))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_import_scan_uses_the_same_static_forms_and_ignores_dynamic_aliases() {
        assert_eq!(
            static_java_import_names(
                "java_import 'java.util.Map$Entry'\n\
                     import ['java.lang.String', dynamic_name]\n\
                     java_import(java.lang.Thread) { |_package, name| \"J#{name}\" }\n"
            ),
            vec![
                "java.lang.String".to_string(),
                "java.lang.Thread".to_string(),
                "java.util.Map$Entry".to_string(),
            ]
        );
        assert_eq!(
            static_java_dependencies(
                "include_package 'java.util'\nimport 'java.lang'\nimport 'java.time.Instant'\n"
            ),
            vec![
                StaticJavaDependency::Class("java.time.Instant".to_string()),
                StaticJavaDependency::Package("java.lang".to_string()),
                StaticJavaDependency::Package("java.util".to_string()),
            ]
        );
    }

    #[test]
    fn preflight_proxy_scan_finds_dotted_and_canonical_java_proxy_forms() {
        let references = static_java_proxy_references(
            "DOTTED = java.lang.String.new\n\
                 CANONICAL = Java::JavaUtil::Map::Entry\n\
                 PACKAGE_CALL = Java::JavaUtil.ArrayList\n\
                 LOWERCASE = Java::JavaUtil.helper\n\
                 WITH_ARGUMENT = Java::JavaUtil.Vector(1)\n",
        );
        assert!(references.contains(&"java.lang.String".to_string()));
        assert!(references.contains(&"Java::JavaUtil::Map::Entry".to_string()));
        assert!(references.contains(&"Java::JavaUtil::ArrayList".to_string()));
        assert!(
            !references
                .iter()
                .any(|reference| reference.ends_with("::helper") || reference.ends_with("::Vector")),
            "only argument-free class-name calls on a package module are proxy lookups: \
             {references:?}"
        );
    }

    #[test]
    fn gem_semantic_prefilter_parses_each_source_once() {
        SEMANTIC_PREFILTER_PARSE_COUNT.with(|count| count.set(0));

        assert!(!source_semantics_depend_on_jruby_catalog(
            "class PlainRuby\n  def value\n    42\n  end\nend\n"
        ));

        let parse_count = SEMANTIC_PREFILTER_PARSE_COUNT.with(|count| count.get());
        assert_eq!(
            parse_count, 1,
            "the gem cache-key prefilter must derive all JRuby semantic evidence from one Prism parse"
        );
    }
}
