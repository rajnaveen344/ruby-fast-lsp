use super::{
    decompiler::{JavaDecompiler, JavaDecompilerError},
    java_catalog::{JavaClassDeclaration, ProjectJavaCatalog},
    source_navigation::{JavaSourceResolutionError, JavaSourceResolver, ResolvedJavaSource},
};
use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, MethodCallSignatureCandidate,
    MethodParamFact, MethodParamKind, MethodReferenceAccess, MethodReferenceCandidate,
    MethodReferenceDiagnostics, NamespaceKind, ReferenceCandidate, RubyConstant, RubyMethod,
    RubyType, SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance,
    TypeSubject,
};
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::{
    parse_method_descriptor, ClassKind, JavaSourceClassLocation, JvmType, MemberInfo,
    MethodDescriptor, Visibility,
};
use ruby_prism::{
    visit_call_node, visit_constant_path_node, visit_constant_read_node, CallNode,
    ConstantPathNode, ConstantReadNode, Node, Visit,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(test)]
std::thread_local! {
    static SEMANTIC_PREFILTER_PARSE_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

const MAX_INCLUDED_PACKAGE_CLASSES: usize = 4_096;
const MAX_STATIC_IMPORT_ALIAS_BYTES: usize = 256;
const MAX_JAVA_HIERARCHY_TYPES: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaImplementationResolutionError {
    Source(JavaSourceResolutionError),
    Decompiler(JavaDecompilerError),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticJavaNavigationPlan {
    pub signature_class_names: Vec<String>,
    pub implementation_class_names: Vec<String>,
}

/// Compact, catalog-independent evidence retained by the first project pass.
///
/// The exact JRuby catalog may still be under construction while ordinary Ruby
/// facts are collected. Keeping only definite Java DSL/canonical-proxy markers
/// and dotted receiver roots lets the owning project later replay the bounded
/// subset whose semantics depend on that catalog without retaining source
/// buffers or reading every project file a second time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticJavaSourceHint {
    definite_catalog_semantics: bool,
    dotted_roots: Vec<String>,
}

impl StaticJavaSourceHint {
    pub fn from_source(source: &str) -> Self {
        let mut definite_catalog_semantics = [
            "java_import",
            "include_package",
            "java_implements",
            "java_package",
            "java_alias",
            "java_send",
            "java_method",
            "to_java",
        ]
        .iter()
        .any(|marker| source.contains(marker));

        let mut dotted_roots = Vec::new();
        let mut characters = source.char_indices().peekable();
        while let Some((start, character)) = characters.next() {
            if !(character.is_alphabetic() || character == '_' || character == '$') {
                continue;
            }
            let mut end = start + character.len_utf8();
            while let Some(&(offset, next)) = characters.peek() {
                if !(next.is_alphanumeric() || next == '_' || next == '$') {
                    break;
                }
                characters.next();
                end = offset + next.len_utf8();
            }
            let identifier = &source[start..end];
            let suffix = source[end..].trim_start_matches(char::is_whitespace);
            if identifier == "Java" && suffix.starts_with("::") {
                definite_catalog_semantics = true;
            }
            if suffix.starts_with('.') {
                dotted_roots.push(source[start..end].to_string());
            }
        }
        dotted_roots.sort();
        dotted_roots.dedup();
        Self {
            definite_catalog_semantics,
            dotted_roots,
        }
    }
}

#[derive(Debug)]
pub struct JrubyImportProvider {
    catalog: Arc<ProjectJavaCatalog>,
    proxy_to_internal: BTreeMap<String, Vec<String>>,
    static_top_level_packages: BTreeSet<String>,
    source_resolver: Option<Arc<JavaSourceResolver>>,
    decompiler: Option<Arc<JavaDecompiler>>,
    signature_cache_root: Option<PathBuf>,
    method_navigation_ranges: RwLock<BTreeMap<(String, String, String), TextRange>>,
    registered_navigation_classes: RwLock<BTreeSet<String>>,
}

impl JrubyImportProvider {
    pub fn new(catalog: Arc<ProjectJavaCatalog>) -> Self {
        let mut proxy_to_internal = BTreeMap::<String, Vec<String>>::new();
        let mut static_top_level_packages = BTreeSet::new();
        for internal_name in catalog.classes.keys() {
            if let Some(package) = internal_name.split('/').next() {
                static_top_level_packages.insert(package.to_string());
            }
            // JVM classfiles may legitimately contain anonymous and compiler-generated
            // names such as `Outer$1`. They are classpath truth, but JRuby cannot expose
            // them as ordinary Ruby proxy constants. Keep them in metadata for exact
            // descriptor relationships and omit only the invalid Ruby proxy projection.
            let Ok(java_name) = JavaClassName::parse(internal_name) else {
                continue;
            };
            proxy_to_internal
                .entry(java_name.ruby_fqn())
                .or_default()
                .push(internal_name.clone());
        }
        Self {
            catalog,
            proxy_to_internal,
            static_top_level_packages,
            source_resolver: None,
            decompiler: None,
            signature_cache_root: None,
            method_navigation_ranges: RwLock::new(BTreeMap::new()),
            registered_navigation_classes: RwLock::new(BTreeSet::new()),
        }
    }

    pub fn with_source_resolver(mut self, resolver: Arc<JavaSourceResolver>) -> Self {
        self.source_resolver = Some(resolver);
        self
    }

    pub fn with_decompiler(mut self, decompiler: Arc<JavaDecompiler>) -> Self {
        self.decompiler = Some(decompiler);
        self
    }

    pub fn with_signature_cache_root(mut self, cache_root: PathBuf) -> Self {
        self.signature_cache_root = Some(cache_root);
        self
    }

    pub fn signature_cache_root(&self) -> Option<&Path> {
        self.signature_cache_root.as_deref()
    }

    pub fn classpath_fingerprint(&self) -> &str {
        &self.catalog.classpath_fingerprint_sha256
    }

    pub fn class_declaration(&self, internal_name: &str) -> Option<&JavaClassDeclaration> {
        self.catalog.classes.get(internal_name)
    }

    pub(crate) fn register_method_navigation_ranges(
        &self,
        internal_name: &str,
        location: &JavaSourceClassLocation,
        file_id: SourceFileId,
    ) {
        assert_eq!(
            internal_name, location.internal_name,
            "INVARIANT VIOLATED: JRuby navigation registration received mismatched class identities. \
             This is a bug because verified Java source locations belong to exactly one catalog class. \
             Fix: register each location with the internal class name used to resolve it."
        );
        self.registered_navigation_classes
            .write()
            .insert(internal_name.to_string());
        let mut ranges = self.method_navigation_ranges.write();
        for method in &location.methods {
            let key = (
                internal_name.to_string(),
                method.name.clone(),
                method.descriptor.clone(),
            );
            let range = TextRange::new(
                file_id,
                method.declaration_range.start,
                method.declaration_range.end,
            );
            if let Some(previous) = ranges.insert(key.clone(), range) {
                assert_eq!(
                    previous, range,
                    "INVARIANT VIOLATED: one JVM method identity mapped to two implementation ranges. \
                     This is a bug because source/decompiler verification must select one exact member. \
                     Fix: reject ambiguous Java source before navigation registration."
                );
            }
        }
    }

    fn preferred_method_definition_range(
        &self,
        internal_name: &str,
        method: &MemberInfo,
    ) -> Option<TextRange> {
        self.method_navigation_ranges
            .read()
            .get(&(
                internal_name.to_string(),
                method.name.clone(),
                method.descriptor.clone(),
            ))
            .copied()
    }

    pub(crate) fn has_registered_navigation_class(&self, internal_name: &str) -> bool {
        self.registered_navigation_classes
            .read()
            .contains(internal_name)
    }

    pub fn resolved_source(
        &self,
        internal_name: &str,
    ) -> Result<Option<ResolvedJavaSource>, JavaSourceResolutionError> {
        let Some(resolver) = &self.source_resolver else {
            return Ok(None);
        };
        let Some(declaration) = self.catalog.classes.get(internal_name) else {
            return Ok(None);
        };
        resolver.resolve(declaration)
    }

    pub fn resolved_implementation(
        &self,
        internal_name: &str,
    ) -> Result<Option<ResolvedJavaSource>, JavaImplementationResolutionError> {
        Ok(self
            .resolved_navigation_implementations(internal_name)?
            .into_iter()
            .next())
    }

    pub fn resolved_navigation_implementations(
        &self,
        internal_name: &str,
    ) -> Result<Vec<ResolvedJavaSource>, JavaImplementationResolutionError> {
        let Some(declaration) = self.catalog.classes.get(internal_name) else {
            return Ok(Vec::new());
        };
        let exact_source = self
            .resolved_source(internal_name)
            .map_err(JavaImplementationResolutionError::Source)?;
        let Some(decompiler) = &self.decompiler else {
            return Ok(exact_source.into_iter().collect());
        };

        if let Some(exact_source) = exact_source {
            if !has_missing_concrete_navigation_methods(&declaration.class, &exact_source.location)
            {
                return Ok(vec![exact_source]);
            }
            let Some(mut decompiled) = decompiler
                .decompile(declaration)
                .map_err(JavaImplementationResolutionError::Decompiler)?
            else {
                return Ok(vec![exact_source]);
            };
            let Some(mut supplemental) =
                supplemental_implementation_location(&exact_source.location, decompiled.location)
            else {
                return Ok(vec![exact_source]);
            };
            supplemental.methods.retain(|location| {
                declaration.class.methods.iter().any(|method| {
                    method.name == location.name
                        && method.descriptor == location.descriptor
                        && concrete_navigation_method(method)
                })
            });
            if supplemental.methods.is_empty() {
                return Ok(vec![exact_source]);
            }
            supplemental.fields.clear();
            decompiled.location = supplemental;
            return Ok(vec![exact_source, decompiled]);
        }

        Ok(decompiler
            .decompile(declaration)
            .map_err(JavaImplementationResolutionError::Decompiler)?
            .into_iter()
            .collect())
    }

    pub fn generated_signature(
        &self,
        import_name: &str,
    ) -> Result<Option<(String, String)>, ruby_fast_lsp_jruby_support::SignatureError> {
        let Ok(java_name) = JavaClassName::parse(import_name) else {
            return Ok(None);
        };
        let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
            return Ok(None);
        };
        let source = ruby_fast_lsp_jruby_support::generate_ruby_signature(&declaration.class)?;
        Ok(Some((java_name.internal_name().to_string(), source)))
    }

    pub fn class_names_in_package(&self, package: &str) -> Result<Vec<String>, String> {
        let Some(prefix) = java_package_prefix(package) else {
            return Err(format!("`{package}` is not a valid Java package name"));
        };
        let mut names = self
            .catalog
            .classes
            .keys()
            .filter_map(|internal| {
                let class = internal.strip_prefix(&prefix)?;
                if class.contains('/') || class.contains('$') {
                    return None;
                }
                Some(internal.clone())
            })
            .collect::<Vec<_>>();
        names.sort();
        if names.len() > MAX_INCLUDED_PACKAGE_CLASSES {
            return Err(format!(
                "Java package `{package}` contains {} direct classes, exceeding the bounded limit of {MAX_INCLUDED_PACKAGE_CLASSES}",
                names.len()
            ));
        }
        Ok(names)
    }

    pub fn static_navigation_class_names(&self, source: &str) -> Result<Vec<String>, String> {
        Ok(self.static_navigation_plan(source)?.signature_class_names)
    }

    pub fn source_may_reference_static_java(&self, source: &str) -> bool {
        self.source_hint_may_reference_static_java(&StaticJavaSourceHint::from_source(source))
    }

    pub fn source_hint_may_reference_static_java(&self, hint: &StaticJavaSourceHint) -> bool {
        hint.definite_catalog_semantics
            || hint.dotted_roots.iter().any(|root| {
                root == "Java" || self.static_top_level_packages.contains(root.as_str())
            })
    }

    pub fn static_navigation_plan(&self, source: &str) -> Result<StaticJavaNavigationPlan, String> {
        let parse = ruby_prism::parse(source.as_bytes());
        self.static_navigation_plan_for_node(&parse.node())
    }

    pub fn static_navigation_plan_for_node(
        &self,
        node: &Node<'_>,
    ) -> Result<StaticJavaNavigationPlan, String> {
        let mut signature_class_names = BTreeSet::new();
        let mut implementation_class_names = BTreeSet::new();
        let mut visitor = StaticNavigationVisitor::default();
        visitor.visit(node);
        visitor.dependencies.sort();
        visitor.dependencies.dedup();
        visitor.proxy_references.sort();
        visitor.proxy_references.dedup();
        visitor.constant_references.sort();
        visitor.constant_references.dedup();
        for dependency in visitor.dependencies {
            match dependency {
                StaticJavaDependency::Class(name) => {
                    if let Some(class_name) = self.class_name_for_static_proxy_reference(&name)? {
                        signature_class_names.insert(class_name.clone());
                        implementation_class_names.insert(class_name);
                    }
                }
                StaticJavaDependency::Package(package) => {
                    signature_class_names.extend(self.class_names_in_package(&package)?);
                }
            }
        }
        for reference in visitor.proxy_references {
            if let Some(class_name) = self.class_name_for_static_proxy_reference(&reference)? {
                signature_class_names.insert(class_name.clone());
                implementation_class_names.insert(class_name);
            }
        }
        let mut package_classes_by_constant = BTreeMap::<String, Vec<String>>::new();
        for internal_name in &signature_class_names {
            let Ok(name) = JavaClassName::parse(internal_name) else {
                continue;
            };
            package_classes_by_constant
                .entry(name.imported_constant().to_string())
                .or_default()
                .push(internal_name.clone());
        }
        for constant in visitor.constant_references {
            let Some(candidates) = package_classes_by_constant.get(&constant) else {
                continue;
            };
            if candidates.len() == 1 {
                implementation_class_names.insert(candidates[0].clone());
            }
        }
        Ok(StaticJavaNavigationPlan {
            signature_class_names: signature_class_names.into_iter().collect(),
            implementation_class_names: implementation_class_names.into_iter().collect(),
        })
    }

    pub fn class_name_for_static_proxy_reference(
        &self,
        reference: &str,
    ) -> Result<Option<String>, String> {
        if reference.contains("::") {
            let Some(internal_names) = self.proxy_to_internal.get(reference) else {
                return Ok(None);
            };
            if internal_names.len() != 1 {
                return Err(format!(
                    "JRuby proxy `{reference}` maps to multiple classpath identities: {}",
                    internal_names.join(", ")
                ));
            }
            return Ok(Some(internal_names[0].clone()));
        }
        let Ok(java_name) = JavaClassName::parse(reference) else {
            return Ok(None);
        };
        Ok(self
            .catalog
            .classes
            .contains_key(java_name.internal_name())
            .then(|| java_name.internal_name().to_string()))
    }

    fn seed_static_proxy_expression(&self, visitor: &mut FactCollector, node: &Node<'_>) {
        let Some(call) = node.as_call_node() else {
            return;
        };
        if let Some(receiver) = call.receiver() {
            self.seed_static_proxy_expression(visitor, &receiver);
        }
        let Some(dotted_name) = dotted_call_name(&call) else {
            return;
        };
        let Ok(java_name) = JavaClassName::parse(&dotted_name) else {
            return;
        };
        if !self.catalog.classes.contains_key(java_name.internal_name()) {
            return;
        }
        let proxy = FullyQualifiedName::constant(
            java_name
                .ruby_namespace_parts()
                .into_iter()
                .map(|part| {
                    RubyConstant::new(&part).expect(
                        "INVARIANT VIOLATED: validated Java proxy part is not a Ruby constant. \
                         This is a bug because JavaClassName owns proxy validation. \
                         Fix: keep dotted proxy expression conversion single-sourced.",
                    )
                })
                .collect::<Vec<_>>(),
        );
        visitor.direct_push_expression_type(
            node,
            RubyType::ClassReference(proxy),
            TypeProvenance::Runtime,
        );
    }

    fn process_import_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_import" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let alias_block = node.block().and_then(|block| block.as_block_node());
        if node.block().is_some() && alias_block.is_none() {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-import-alias",
                "Dynamic java_import alias blocks are not resolved statically yet.".to_string(),
            );
            return;
        }
        let mut imports = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut imports);
        }
        for import in imports {
            let alias = if let Some(block) = &alias_block {
                let Ok(java_name) = JavaClassName::parse(&import.name) else {
                    self.add_import(visitor, import, None, true);
                    continue;
                };
                let Some(alias) = evaluate_static_import_alias(
                    block,
                    &java_name.package().join("."),
                    java_name.imported_constant(),
                ) else {
                    visitor.push_warning_diagnostic(
                        visitor.text_range_from_offsets(
                            node.location().start_offset(),
                            node.location().end_offset(),
                        ),
                        "unsupported-jruby-import-alias",
                        "The java_import alias block is not a bounded literal interpolation of its package and class-name parameters.".to_string(),
                    );
                    return;
                };
                Some(alias)
            } else {
                None
            };
            self.add_import(visitor, import, alias, true);
        }
    }

    fn process_import_dispatch(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"import" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let alias_block = node.block().and_then(|block| block.as_block_node());
        if node.block().is_some() && alias_block.is_none() {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-import-alias",
                "Dynamic import alias blocks are not resolved statically yet.".to_string(),
            );
            return;
        }
        let mut imports = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut imports);
        }
        for import in imports {
            if is_java_class_name(&import.name) {
                let alias = if let Some(block) = &alias_block {
                    let Ok(java_name) = JavaClassName::parse(&import.name) else {
                        self.add_import(visitor, import, None, true);
                        continue;
                    };
                    let Some(alias) = evaluate_static_import_alias(
                        block,
                        &java_name.package().join("."),
                        java_name.imported_constant(),
                    ) else {
                        visitor.push_warning_diagnostic(
                            visitor.text_range_from_offsets(
                                node.location().start_offset(),
                                node.location().end_offset(),
                            ),
                            "unsupported-jruby-import-alias",
                            "The import alias block is not a bounded literal interpolation of its package and class-name parameters.".to_string(),
                        );
                        return;
                    };
                    Some(alias)
                } else {
                    None
                };
                self.add_import(visitor, import, alias, true);
            } else {
                self.add_package(visitor, import);
            }
        }
    }

    fn process_include_package_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"include_package" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut packages = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut packages);
        }
        for package in packages {
            self.add_package(visitor, package);
        }
    }

    fn process_java_interface_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some()
            || !matches!(node.name().as_slice(), b"include" | b"java_implements")
        {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut interfaces = Vec::new();
        for argument in arguments.arguments().iter() {
            collect_static_imports(visitor, &argument, &mut interfaces);
        }
        let source = FullyQualifiedName::namespace(visitor.scope_tracker.get_ns_stack());
        for interface in interfaces {
            if !is_java_class_name(&interface.name) {
                continue;
            }
            let Ok(java_name) = JavaClassName::parse(&interface.name) else {
                continue;
            };
            let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
                visitor.push_error_diagnostic(
                    interface.range,
                    "unresolved-java-interface",
                    format!(
                        "Java interface `{}` is not present on this project's isolated classpath.",
                        interface.name
                    ),
                );
                continue;
            };
            if declaration.class.kind() != ClassKind::Interface {
                visitor.push_error_diagnostic(
                    interface.range,
                    "invalid-java-interface",
                    format!("Java type `{}` is not an interface.", interface.name),
                );
                continue;
            }
            let target = FullyQualifiedName::namespace(
                java_name
                    .ruby_namespace_parts()
                    .into_iter()
                    .map(|part| {
                        RubyConstant::new(&part).expect(
                            "INVARIANT VIOLATED: validated Java interface proxy part is not a Ruby constant. \
                             This is a bug because JavaClassName owns proxy validation. \
                             Fix: keep Java interface proxy conversion single-sourced.",
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            visitor.direct_facts.graph_edges.push(GraphEdgeFact::new(
                source.clone(),
                target,
                GraphEdgeKind::Include,
                interface.range,
            ));
        }
    }

    fn process_java_package_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_package" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            visitor.push_warning_diagnostic(
                visitor
                    .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
                "unsupported-jruby-java-package",
                "java_package is a jrubyc declaration and requires exactly one static Java package name.".to_string(),
            );
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        let package = if arguments.len() == 1 {
            static_symbol_or_string(&arguments[0]).or_else(|| {
                arguments[0]
                    .as_call_node()
                    .and_then(|call| dotted_call_name(&call))
            })
        } else {
            None
        };
        if package.as_deref().and_then(java_package_prefix).is_none() {
            visitor.push_warning_diagnostic(
                visitor
                    .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
                "unsupported-jruby-java-package",
                "java_package is a jrubyc declaration and requires exactly one static Java package name.".to_string(),
            );
        }
    }

    fn process_java_alias_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_alias" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut arguments = arguments.arguments().iter();
        let Some(new_name_node) = arguments.next() else {
            return;
        };
        let Some(old_name_node) = arguments.next() else {
            return;
        };
        let Some(new_name) = static_symbol_or_string(&new_name_node) else {
            return;
        };
        let Some(old_name) = static_symbol_or_string(&old_name_node) else {
            return;
        };
        let Some(new_method) = RubyMethod::new(&new_name).ok() else {
            return;
        };
        let Some(old_method) = RubyMethod::new(&old_name).ok() else {
            return;
        };
        let signature = if let Some(signature_node) = arguments.next() {
            if arguments.next().is_some() {
                return;
            }
            let Some(signature) = self.static_java_signature(visitor, &signature_node) else {
                visitor.push_warning_diagnostic(
                    visitor.text_range_from_offsets(
                        signature_node.location().start_offset(),
                        signature_node.location().end_offset(),
                    ),
                    "unsupported-jruby-java-alias",
                    "java_alias parameter types must be a static array of Java primitive or fully qualified class names.".to_string(),
                );
                return;
            };
            Some(signature)
        } else {
            None
        };

        let current_namespace = FullyQualifiedName::namespace(visitor.scope_tracker.get_ns_stack());
        let Some(proxy) = current_runtime_proxy(visitor).or_else(|| {
            self.proxy_to_internal
                .contains_key(&current_namespace.to_string())
                .then_some(current_namespace)
        }) else {
            return;
        };
        let proxy_name = proxy.to_string();
        let Some(internal_names) = self.proxy_to_internal.get(&proxy_name) else {
            return;
        };
        if internal_names.len() != 1 {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "ambiguous-java-proxy",
                format!(
                    "Java proxy `{proxy_name}` maps to multiple classpath identities: {}.",
                    internal_names.join(", ")
                ),
            );
            return;
        }
        let declaration = self.catalog.classes.get(&internal_names[0]).expect(
            "INVARIANT VIOLATED: Java proxy reverse index points at a missing catalog class. \
             This is a bug because both structures are built atomically from the same catalog. \
             Fix: keep JrubyImportProvider::new reverse-index construction synchronized.",
        );
        let matching = declaration
            .class
            .methods
            .iter()
            .filter(|method| {
                method.name == old_name
                    && !method.is_static()
                    && signature.as_ref().is_none_or(|expected| {
                        parse_method_descriptor(&method.descriptor)
                            .is_ok_and(|descriptor| descriptor.parameters == *expected)
                    })
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    old_name_node.location().start_offset(),
                    old_name_node.location().end_offset(),
                ),
                "unresolved-java-method-alias",
                format!(
                    "Java method `{old_name}` with the selected parameter signature is not present on `{proxy_name}`."
                ),
            );
            return;
        }

        let range = visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset());
        let name_range = visitor.text_range_from_offsets(
            new_name_node.location().start_offset(),
            new_name_node.location().end_offset(),
        );
        let old_name_range = visitor.text_range_from_offsets(
            old_name_node.location().start_offset(),
            old_name_node.location().end_offset(),
        );
        for method in matching {
            self.push_java_alias_method(
                visitor,
                &proxy,
                new_method,
                old_method,
                method,
                range,
                name_range,
                old_name_range,
            );
        }
    }

    fn process_java_dispatch_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if !matches!(node.name().as_slice(), b"java_send" | b"java_method") {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let dispatch_name = if node.name().as_slice() == b"java_send" {
            "java_send"
        } else {
            "java_method"
        };
        let call_range = visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset());
        let Some((proxy, receiver_kind)) = self.runtime_proxy_for_expression(visitor, &receiver)
        else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            visitor.push_warning_diagnostic(
                call_range,
                "unsupported-jruby-java-dispatch",
                format!(
                    "{dispatch_name} requires a static Java method name and an optional static parameter-type array."
                ),
            );
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        let Some(method_name_node) = arguments.first() else {
            visitor.push_warning_diagnostic(
                call_range,
                "unsupported-jruby-java-dispatch",
                format!(
                    "{dispatch_name} requires a static Java method name and an optional static parameter-type array."
                ),
            );
            return;
        };
        let Some(method_name) = static_symbol_or_string(method_name_node) else {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "unsupported-jruby-java-dispatch",
                format!("{dispatch_name} method names must be static symbols or strings."),
            );
            return;
        };
        let Ok(ruby_method) = RubyMethod::new(&method_name) else {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "invalid-java-method-name",
                format!("`{method_name}` cannot be represented as a Ruby method name."),
            );
            return;
        };

        let (signature, actual_argument_count) = match arguments.get(1) {
            Some(signature_node) => {
                let Some(signature) = self.static_java_signature(visitor, signature_node) else {
                    visitor.push_warning_diagnostic(
                        visitor.text_range_from_offsets(
                            signature_node.location().start_offset(),
                            signature_node.location().end_offset(),
                        ),
                        "unsupported-jruby-java-dispatch",
                        format!(
                            "{dispatch_name} parameter types must be a static array of Java primitive, imported, canonical, or fully qualified class names."
                        ),
                    );
                    return;
                };
                (signature, arguments.len().saturating_sub(2))
            }
            None => (Vec::new(), 0),
        };
        if dispatch_name == "java_method" && arguments.len() > 2 {
            visitor.push_error_diagnostic(
                call_range,
                "invalid-java-method-handle",
                "java_method accepts only a method name and parameter-type array.".to_string(),
            );
            return;
        }
        if dispatch_name == "java_send" && actual_argument_count != signature.len() {
            visitor.push_error_diagnostic(
                call_range,
                "invalid-java-method-arguments",
                format!(
                    "java_send selected {} Java parameter(s) but received {actual_argument_count} argument(s).",
                    signature.len()
                ),
            );
            return;
        }

        let candidates = match self.java_method_candidates(&proxy, &method_name, &signature) {
            Ok(candidates) => candidates,
            Err(message) => {
                visitor.push_error_diagnostic(call_range, "invalid-java-hierarchy", message);
                return;
            }
        };
        let candidates = candidates
            .into_iter()
            .filter(|candidate| match (dispatch_name, receiver_kind) {
                ("java_send", NamespaceKind::Instance) => !candidate.method.is_static(),
                ("java_send", NamespaceKind::Singleton) => candidate.method.is_static(),
                ("java_method", NamespaceKind::Instance) => !candidate.method.is_static(),
                ("java_method", NamespaceKind::Singleton) => true,
                (_, NamespaceKind::Instance | NamespaceKind::Singleton) => false,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "unresolved-java-method",
                format!(
                    "Java method `{method_name}` with parameter signature `{}` is not present on `{proxy}` for this receiver.",
                    display_java_signature(&signature)
                ),
            );
            return;
        }
        if candidates.len() != 1 {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "ambiguous-java-method",
                format!(
                    "Java method `{method_name}` with parameter signature `{}` resolves to {} declarations on `{proxy}`.",
                    display_java_signature(&signature),
                    candidates.len()
                ),
            );
            return;
        }
        let selected = &candidates[0];
        let owner = JavaClassName::parse(&selected.owner).expect(
            "INVARIANT VIOLATED: selected Java method owner is not a valid internal class name. \
             This is a bug because Java catalog construction validates every class identity. \
             Fix: retain the canonical catalog key as the selected method owner.",
        );
        let owner_parts = owner
            .ruby_namespace_parts()
            .into_iter()
            .map(|part| {
                RubyConstant::new(&part).expect(
                    "INVARIANT VIOLATED: validated Java method owner is not Ruby-constant-safe. \
                     This is a bug because JavaClassName owns proxy validation. \
                     Fix: keep Java method reference owner conversion single-sourced.",
                )
            })
            .collect::<Vec<_>>();
        let method_range = visitor.text_range_from_offsets(
            method_name_node.location().start_offset(),
            method_name_node.location().end_offset(),
        );
        visitor
            .reference_candidates
            .push(ReferenceCandidate::method(
                method_range,
                MethodReferenceCandidate {
                    owner: owner_parts,
                    owner_kind: if selected.method.is_static() {
                        NamespaceKind::Singleton
                    } else {
                        NamespaceKind::Instance
                    },
                    method: ruby_method,
                    is_super: false,
                    access: MethodReferenceAccess::VisibilityBypass,
                    caller: visitor.scope_tracker.current_method_fqn().cloned(),
                    preferred_definition_range: self
                        .preferred_method_definition_range(&selected.owner, &selected.method),
                    diagnostics: MethodReferenceDiagnostics {
                        diagnostic_range: method_range,
                        receiver_label: Some(proxy.to_string()),
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        signature: MethodCallSignatureCandidate::default(),
                    },
                },
            ));

        let return_type = if dispatch_name == "java_send" {
            ruby_type_for_jvm(&selected.descriptor.returns)
        } else if receiver_kind == NamespaceKind::Singleton && !selected.method.is_static() {
            RubyType::Class(FullyQualifiedName::try_from("UnboundMethod").expect(
                "INVARIANT VIOLATED: built-in UnboundMethod FQN is invalid. \
                     This is a bug because it is a static Ruby core constant. \
                     Fix: keep built-in runtime type names valid Ruby constants.",
            ))
        } else {
            RubyType::Class(FullyQualifiedName::try_from("Method").expect(
                "INVARIANT VIOLATED: built-in Method FQN is invalid. \
                 This is a bug because it is a static Ruby core constant. \
                 Fix: keep built-in runtime type names valid Ruby constants.",
            ))
        };
        visitor.direct_push_expression_type(&node.as_node(), return_type, TypeProvenance::Runtime);
    }

    fn process_to_java_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.name().as_slice() != b"to_java" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            if let RubyType::Array(_) = visitor.infer_type_from_value(&receiver) {
                visitor.direct_push_expression_type(
                    &node.as_node(),
                    RubyType::array_of(RubyType::Class(
                        FullyQualifiedName::try_from("Java::JavaLang::Object").expect(
                            "INVARIANT VIOLATED: Java Object proxy FQN is invalid. \
                             This is a bug because it is a canonical JRuby proxy name. \
                             Fix: keep built-in Java proxy identities valid Ruby constants.",
                        ),
                    )),
                    TypeProvenance::Runtime,
                );
            }
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        if arguments.len() != 1 {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "unsupported-jruby-to-java",
                "to_java accepts zero or one static Java target type.".to_string(),
            );
            return;
        }
        let Some(target) = self.static_to_java_type(visitor, &arguments[0]) else {
            return;
        };
        let receiver_type = visitor.infer_type_from_value(&receiver);
        let result = if matches!(receiver_type, RubyType::Array(_)) {
            RubyType::array_of(ruby_type_for_jvm(&target))
        } else {
            ruby_type_for_to_java_scalar(&target)
        };
        visitor.direct_push_expression_type(&node.as_node(), result, TypeProvenance::Runtime);
    }

    fn process_java_constructor_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.name().as_slice() != b"new" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some((proxy, NamespaceKind::Singleton)) =
            self.runtime_proxy_for_expression(visitor, &receiver)
        else {
            return;
        };
        visitor.direct_push_expression_type(
            &node.as_node(),
            RubyType::Class(proxy),
            TypeProvenance::Runtime,
        );
    }

    fn runtime_proxy_for_expression(
        &self,
        visitor: &FactCollector,
        node: &Node<'_>,
    ) -> Option<(FullyQualifiedName, NamespaceKind)> {
        let (proxy, kind) = match visitor.infer_type_from_value(node) {
            RubyType::Class(proxy) | RubyType::Module(proxy) => (proxy, NamespaceKind::Instance),
            RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => {
                (proxy, NamespaceKind::Singleton)
            }
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None
            }
        };
        self.proxy_to_internal
            .contains_key(&proxy.to_string())
            .then_some((proxy, kind))
    }

    fn static_java_signature(
        &self,
        visitor: &FactCollector,
        node: &Node<'_>,
    ) -> Option<Vec<JvmType>> {
        let array = node.as_array_node()?;
        array
            .elements()
            .iter()
            .map(|element| self.static_java_type(visitor, &element))
            .collect()
    }

    fn static_java_type(&self, visitor: &FactCollector, node: &Node<'_>) -> Option<JvmType> {
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"[]"
                && call.arguments().is_none()
                && call.block().is_none()
            {
                return call
                    .receiver()
                    .and_then(|receiver| self.static_java_type(visitor, &receiver))
                    .map(|element| JvmType::Array(Box::new(element)));
            }
            if call.arguments().is_none()
                && call.block().is_none()
                && call.receiver().as_ref().is_some_and(|receiver| {
                    receiver
                        .as_constant_read_node()
                        .is_some_and(|constant| constant.name().as_slice() == b"Java")
                })
            {
                return primitive_java_type(call.name().as_slice());
            }
            if let Some(class_name) =
                dotted_call_name(&call).and_then(|name| self.canonical_catalog_class(&name))
            {
                return Some(JvmType::Object(class_name));
            }
        }
        if let Some(reference) = static_constant_reference(node)
            .and_then(|reference| self.canonical_catalog_class(&reference))
        {
            return Some(JvmType::Object(reference));
        }
        match visitor.infer_type_from_value(node) {
            RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => self
                .canonical_catalog_class(&proxy.to_string())
                .map(JvmType::Object),
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Union(_)
            | RubyType::Unknown => None,
        }
    }

    fn static_to_java_type(&self, visitor: &FactCollector, node: &Node<'_>) -> Option<JvmType> {
        if let Some(name) = static_symbol_or_string(node) {
            return to_java_symbol_type(&name);
        }
        self.static_java_type(visitor, node)
    }

    fn canonical_catalog_class(&self, name: &str) -> Option<String> {
        self.class_name_for_static_proxy_reference(name)
            .ok()
            .flatten()
    }

    fn java_method_candidates(
        &self,
        proxy: &FullyQualifiedName,
        method_name: &str,
        signature: &[JvmType],
    ) -> Result<Vec<SelectedJavaMethod>, String> {
        let Some(roots) = self.proxy_to_internal.get(&proxy.to_string()) else {
            return Ok(Vec::new());
        };
        if roots.len() != 1 {
            return Err(format!(
                "Java proxy `{proxy}` maps to multiple classpath identities: {}.",
                roots.join(", ")
            ));
        }
        let mut queue = VecDeque::from([(roots[0].clone(), 0usize)]);
        let mut visited = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut selected = Vec::new();
        while let Some((owner, depth)) = queue.pop_front() {
            if !visited.insert(owner.clone()) {
                continue;
            }
            if visited.len() > MAX_JAVA_HIERARCHY_TYPES {
                return Err(format!(
                    "Java hierarchy for `{proxy}` exceeds the bounded limit of {MAX_JAVA_HIERARCHY_TYPES} types."
                ));
            }
            let Some(declaration) = self.catalog.classes.get(&owner) else {
                continue;
            };
            for method in &declaration.class.methods {
                if method.name != method_name || method.visibility() != Visibility::Public {
                    continue;
                }
                let Ok(descriptor) = parse_method_descriptor(&method.descriptor) else {
                    continue;
                };
                if descriptor.parameters != signature {
                    continue;
                }
                let identity = (
                    method.name.clone(),
                    method.descriptor.clone(),
                    method.is_static(),
                );
                if identities.insert(identity) {
                    selected.push(SelectedJavaMethod {
                        owner: owner.clone(),
                        method: method.clone(),
                        descriptor,
                        depth,
                    });
                }
            }
            if let Some(super_name) = &declaration.class.super_name {
                queue.push_back((super_name.clone(), depth + 1));
            }
            for interface in &declaration.class.interfaces {
                queue.push_back((interface.clone(), depth + 1));
            }
        }
        selected.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.owner.cmp(&right.owner))
                .then_with(|| left.method.descriptor.cmp(&right.method.descriptor))
        });
        Ok(selected)
    }

    fn push_java_alias_method(
        &self,
        visitor: &mut FactCollector,
        proxy: &FullyQualifiedName,
        new_method: RubyMethod,
        old_method: RubyMethod,
        method: &MemberInfo,
        range: TextRange,
        name_range: TextRange,
        old_name_range: TextRange,
    ) {
        let descriptor = parse_method_descriptor(&method.descriptor).expect(
            "INVARIANT VIOLATED: catalog method descriptor failed after alias selection. \
             This is a bug because selection parsed the same descriptor successfully. \
             Fix: keep Java alias descriptor validation single-sourced.",
        );
        let params = descriptor
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter_type)| {
                let name = method
                    .parameters
                    .get(index)
                    .map(|parameter| {
                        ruby_fast_lsp_jruby_support::ruby_parameter_name(&parameter.name, index)
                    })
                    .unwrap_or_else(|| format!("arg{index}"));
                let kind = if method.is_varargs() && index + 1 == descriptor.parameters.len() {
                    MethodParamKind::Rest
                } else {
                    MethodParamKind::Required
                };
                MethodParamFact::new(name, kind).with_signature_metadata(
                    Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                        parameter_type,
                    )),
                    None,
                )
            })
            .collect();
        visitor.direct_push_method_fact_with_signature_and_name_range(
            proxy.namespace_parts().to_vec(),
            NamespaceKind::Instance,
            new_method,
            range,
            name_range,
            params,
            Some(format!(
                "JRuby alias of Java method `{}` with descriptor `{}`.",
                method.name, method.descriptor
            )),
            Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                &descriptor.returns,
            )),
        );
        visitor
            .reference_candidates
            .push(ReferenceCandidate::method(
                old_name_range,
                MethodReferenceCandidate {
                    owner: proxy.namespace_parts().to_vec(),
                    owner_kind: NamespaceKind::Instance,
                    method: old_method,
                    is_super: false,
                    access: MethodReferenceAccess::Normal,
                    caller: visitor.scope_tracker.current_method_fqn().cloned(),
                    preferred_definition_range: None,
                    diagnostics: MethodReferenceDiagnostics {
                        diagnostic_range: old_name_range,
                        receiver_label: None,
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        signature: MethodCallSignatureCandidate::default(),
                    },
                },
            ));
        let alias_fqn = FullyQualifiedName::method(proxy.namespace_parts().to_vec(), new_method);
        let return_type = ruby_type_for_jvm(&descriptor.returns);
        let fact = TypeFact::new(
            TypeSubject::MethodReturn(alias_fqn),
            return_type,
            range,
            TypeProvenance::Runtime,
        );
        visitor.type_store.add(fact.clone());
        visitor.direct_facts.types.push(fact);
    }

    fn add_package(&self, visitor: &mut FactCollector, package: StaticJavaImport) {
        let names = match self.class_names_in_package(&package.name) {
            Ok(names) => names,
            Err(message) => {
                visitor.push_error_diagnostic(package.range, "invalid-java-package", message);
                return;
            }
        };
        if names.is_empty() {
            visitor.push_error_diagnostic(
                package.range,
                "unresolved-java-package",
                format!(
                    "Java package `{}` has no direct classes on this project's isolated classpath.",
                    package.name
                ),
            );
            return;
        }
        for name in names {
            let alias = name
                .rsplit('/')
                .next()
                .expect("INVARIANT VIOLATED: validated internal Java class has no class component")
                .to_string();
            self.add_import(
                visitor,
                StaticJavaImport {
                    name,
                    range: package.range,
                    name_range: package.name_range,
                },
                Some(alias),
                false,
            );
        }
    }

    fn add_import(
        &self,
        visitor: &mut FactCollector,
        import: StaticJavaImport,
        alias: Option<String>,
        emit_symbol: bool,
    ) {
        let Ok(java_name) = JavaClassName::parse(&import.name) else {
            visitor.push_error_diagnostic(
                import.range,
                "invalid-java-import",
                format!(
                    "`{}` is not a valid fully qualified Java class name.",
                    import.name
                ),
            );
            return;
        };
        let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
            visitor.push_error_diagnostic(
                import.range,
                "unresolved-java-import",
                format!(
                    "Java class `{}` is not present on this project's isolated classpath.",
                    import.name
                ),
            );
            return;
        };
        assert_eq!(
            declaration.class.name,
            java_name.internal_name(),
            "INVARIANT VIOLATED: Java catalog key and declaration name disagree. \
             This is a bug because archive ingestion validates class identity before catalog insertion. \
             Fix: preserve the parsed internal name as the catalog key."
        );

        let mut alias_parts = visitor.scope_tracker.get_ns_stack();
        let alias_name = alias
            .as_deref()
            .unwrap_or_else(|| java_name.imported_constant());
        let Ok(alias) = RubyConstant::new(alias_name) else {
            visitor.push_error_diagnostic(
                import.name_range,
                "invalid-java-import-alias",
                format!(
                    "Java class `{}` cannot be imported as Ruby constant `{}`.",
                    import.name, alias_name
                ),
            );
            return;
        };
        alias_parts.push(alias);
        let alias_fqn = FullyQualifiedName::constant(alias_parts);
        let declaration_range = import.range;
        if emit_symbol {
            visitor.direct_facts.symbols.push(
                SymbolFact::new(alias_fqn.clone(), SymbolKind::Constant, declaration_range)
                    .with_name_range(import.name_range),
            );
        }

        let proxy_parts: Vec<RubyConstant> = java_name
            .ruby_namespace_parts()
            .into_iter()
            .map(|part| {
                RubyConstant::new(&part).expect(
                    "INVARIANT VIOLATED: JRuby proxy name component is not a Ruby constant. \
                     This is a bug because JavaClassName owns proxy constant validation. \
                     Fix: keep proxy name generation Ruby-constant-safe.",
                )
            })
            .collect();
        let proxy_fqn = FullyQualifiedName::constant(proxy_parts);
        visitor
            .reference_candidates
            .push(ReferenceCandidate::resolved(
                import.name_range,
                proxy_fqn.clone(),
                visitor.scope_tracker.current_method_fqn().cloned(),
            ));
        let type_fact = TypeFact::new(
            TypeSubject::Constant(alias_fqn),
            RubyType::ClassReference(proxy_fqn),
            declaration_range,
            TypeProvenance::Runtime,
        );
        visitor.type_store.add(type_fact.clone());
        visitor.direct_facts.types.push(type_fact);
    }
}

fn has_missing_concrete_navigation_methods(
    class: &ruby_fast_lsp_jvm_metadata::ClassFile,
    exact: &JavaSourceClassLocation,
) -> bool {
    class.methods.iter().any(|method| {
        concrete_navigation_method(method)
            && !exact.methods.iter().any(|location| {
                location.name == method.name && location.descriptor == method.descriptor
            })
    })
}

fn concrete_navigation_method(method: &MemberInfo) -> bool {
    !method.is_abstract()
        && !method.is_native()
        && method.name != "<clinit>"
        && (method.name == "<init>" || RubyMethod::new(&method.name).is_ok())
        && matches!(
            method.visibility(),
            Visibility::Public | Visibility::Protected
        )
}

fn supplemental_implementation_location(
    exact: &JavaSourceClassLocation,
    mut decompiled: JavaSourceClassLocation,
) -> Option<JavaSourceClassLocation> {
    assert_eq!(
        exact.internal_name, decompiled.internal_name,
        "INVARIANT VIOLATED: exact and decompiled Java locations identify different classes. \
         This is a bug because per-member precedence can compare only one winning class identity. \
         Fix: decompile the same catalog declaration selected by exact-source resolution."
    );
    decompiled.methods.retain(|candidate| {
        !exact.methods.iter().any(|preferred| {
            preferred.name == candidate.name && preferred.descriptor == candidate.descriptor
        })
    });
    decompiled.fields.retain(|candidate| {
        !exact.fields.iter().any(|preferred| {
            preferred.name == candidate.name && preferred.descriptor == candidate.descriptor
        })
    });
    (!decompiled.methods.is_empty() || !decompiled.fields.is_empty()).then_some(decompiled)
}

impl FactCollectorExtensionHost for JrubyImportProvider {
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        self.seed_static_proxy_expression(visitor, &node.as_node());
        self.process_import_call(visitor, node);
        self.process_import_dispatch(visitor, node);
        self.process_include_package_call(visitor, node);
        self.process_java_interface_call(visitor, node);
        self.process_java_package_call(visitor, node);
        self.process_java_alias_call(visitor, node);
        self.process_java_dispatch_call(visitor, node);
        self.process_to_java_call(visitor, node);
        self.process_java_constructor_call(visitor, node);
    }
}

#[derive(Debug, Clone)]
struct SelectedJavaMethod {
    owner: String,
    method: MemberInfo,
    descriptor: MethodDescriptor,
    depth: usize,
}

#[derive(Debug)]
struct StaticJavaImport {
    name: String,
    range: TextRange,
    name_range: TextRange,
}

fn collect_static_imports(
    visitor: &FactCollector,
    node: &Node<'_>,
    imports: &mut Vec<StaticJavaImport>,
) {
    if let Some(array) = node.as_array_node() {
        for element in array.elements().iter() {
            collect_static_imports(visitor, &element, imports);
        }
        return;
    }
    if let Some(string) = node.as_string_node() {
        let name = String::from_utf8_lossy(string.unescaped()).to_string();
        let content = string.content_loc();
        let name_start = content
            .end_offset()
            .saturating_sub(imported_name_length(&name));
        imports.push(StaticJavaImport {
            name,
            range: visitor.text_range_from_offsets(
                node.location().start_offset(),
                node.location().end_offset(),
            ),
            name_range: visitor.text_range_from_offsets(name_start, content.end_offset()),
        });
        return;
    }
    let Some(call) = node.as_call_node() else {
        return;
    };
    let Some(name) = dotted_call_name(&call) else {
        return;
    };
    if !name.contains('.') {
        return;
    }
    let Some(message) = call.message_loc() else {
        return;
    };
    imports.push(StaticJavaImport {
        name,
        range: visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset()),
        name_range: visitor.text_range_from_offsets(message.start_offset(), message.end_offset()),
    });
}

fn dotted_call_name(call: &CallNode<'_>) -> Option<String> {
    if call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    let name = std::str::from_utf8(call.name().as_slice()).ok()?;
    if let Some(receiver) = call.receiver() {
        if receiver
            .as_constant_read_node()
            .is_some_and(|constant| constant.name().as_slice() == b"Java")
        {
            return Some(name.to_string());
        }
        let receiver = receiver.as_call_node()?;
        let prefix = dotted_call_name(&receiver)?;
        return Some(format!("{prefix}.{name}"));
    }
    Some(name.to_string())
}

fn imported_name_length(name: &str) -> usize {
    name.rsplit(['.', '$'])
        .next()
        .map(str::len)
        .unwrap_or(name.len())
}

fn is_java_class_name(name: &str) -> bool {
    name.rsplit(['.', '/'])
        .next()
        .and_then(|class| class.split('$').next())
        .and_then(|class| class.chars().next())
        .is_some_and(char::is_uppercase)
}

fn java_package_prefix(package: &str) -> Option<String> {
    let mut components = package.split('.');
    let first = components.next()?;
    if !valid_java_identifier(first) {
        return None;
    }
    let mut normalized = first.to_string();
    for component in components {
        if !valid_java_identifier(component) {
            return None;
        }
        normalized.push('/');
        normalized.push_str(component);
    }
    normalized.push('/');
    Some(normalized)
}

fn static_symbol_or_string(node: &Node<'_>) -> Option<String> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    node.as_string_node()
        .map(|string| String::from_utf8_lossy(string.unescaped()).to_string())
}

fn primitive_java_type(name: &[u8]) -> Option<JvmType> {
    match name {
        b"byte" => Some(JvmType::Byte),
        b"char" => Some(JvmType::Char),
        b"double" => Some(JvmType::Double),
        b"float" => Some(JvmType::Float),
        b"int" => Some(JvmType::Int),
        b"long" => Some(JvmType::Long),
        b"short" => Some(JvmType::Short),
        b"boolean" => Some(JvmType::Boolean),
        _ => None,
    }
}

fn to_java_symbol_type(name: &str) -> Option<JvmType> {
    match name {
        "byte" => Some(JvmType::Byte),
        "char" => Some(JvmType::Char),
        "double" => Some(JvmType::Double),
        "float" => Some(JvmType::Float),
        "int" | "integer" => Some(JvmType::Int),
        "long" => Some(JvmType::Long),
        "short" => Some(JvmType::Short),
        "boolean" => Some(JvmType::Boolean),
        "string" => Some(JvmType::Object("java/lang/String".to_string())),
        "object" => Some(JvmType::Object("java/lang/Object".to_string())),
        _ => None,
    }
}

fn static_constant_reference(node: &Node<'_>) -> Option<String> {
    if let Some(read) = node.as_constant_read_node() {
        return Some(String::from_utf8_lossy(read.name().as_slice()).to_string());
    }
    let path = node.as_constant_path_node()?;
    let mut parts = Vec::new();
    collect_ruby_constant_path(&path, &mut parts)?;
    Some(parts.join("::"))
}

fn display_java_signature(signature: &[JvmType]) -> String {
    signature
        .iter()
        .map(display_java_type)
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_java_type(ty: &JvmType) -> String {
    match ty {
        JvmType::Byte => "byte".to_string(),
        JvmType::Char => "char".to_string(),
        JvmType::Double => "double".to_string(),
        JvmType::Float => "float".to_string(),
        JvmType::Int => "int".to_string(),
        JvmType::Long => "long".to_string(),
        JvmType::Short => "short".to_string(),
        JvmType::Boolean => "boolean".to_string(),
        JvmType::Void => "void".to_string(),
        JvmType::Object(name) => name.replace('/', "."),
        JvmType::Array(element) => format!("{}[]", display_java_type(element)),
    }
}

fn ruby_type_for_to_java_scalar(ty: &JvmType) -> RubyType {
    let proxy = match ty {
        JvmType::Byte => "Java::JavaLang::Byte",
        JvmType::Char => "Java::JavaLang::Character",
        JvmType::Double => "Java::JavaLang::Double",
        JvmType::Float => "Java::JavaLang::Float",
        JvmType::Int => "Java::JavaLang::Integer",
        JvmType::Long => "Java::JavaLang::Long",
        JvmType::Short => "Java::JavaLang::Short",
        JvmType::Boolean => "Java::JavaLang::Boolean",
        JvmType::Void => return RubyType::nil_class(),
        JvmType::Object(name) => {
            return JavaClassName::parse(name)
                .map(|name| {
                    RubyType::Class(
                        FullyQualifiedName::try_from(name.ruby_fqn().as_str()).expect(
                            "INVARIANT VIOLATED: validated Java class produced an invalid JRuby proxy FQN. \
                             This is a bug because JavaClassName owns proxy validation. \
                             Fix: keep Java-to-Ruby proxy conversion single-sourced.",
                        ),
                    )
                })
                .unwrap_or(RubyType::Unknown);
        }
        JvmType::Array(element) => return RubyType::array_of(ruby_type_for_jvm(element)),
    };
    RubyType::Class(FullyQualifiedName::try_from(proxy).expect(
        "INVARIANT VIOLATED: Java primitive wrapper proxy FQN is invalid. \
         This is a bug because wrapper mappings are static canonical JRuby names. \
         Fix: keep primitive wrapper proxy names valid Ruby constants.",
    ))
}

fn current_runtime_proxy(visitor: &FactCollector) -> Option<FullyQualifiedName> {
    let subject = TypeSubject::Constant(FullyQualifiedName::constant(
        visitor.scope_tracker.get_ns_stack(),
    ));
    let direct = visitor
        .direct_facts
        .types
        .iter()
        .rev()
        .find(|fact| fact.subject == subject && fact.provenance == TypeProvenance::Runtime)
        .map(|fact| fact.ruby_type.clone());
    let local = direct.or_else(|| {
        visitor
            .type_store
            .facts_for(&subject)
            .into_iter()
            .rev()
            .find(|fact| fact.provenance == TypeProvenance::Runtime)
            .map(|fact| fact.ruby_type)
    });
    let ruby_type = local.or_else(|| {
        visitor
            .analysis_engine
            .read()
            .type_facts_for(&subject)
            .into_iter()
            .rev()
            .find(|fact| fact.provenance == TypeProvenance::Runtime)
            .map(|fact| fact.ruby_type)
    })?;
    match ruby_type {
        RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => Some(proxy),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}

pub(crate) fn ruby_type_for_jvm(ty: &JvmType) -> RubyType {
    match ty {
        JvmType::Byte | JvmType::Char | JvmType::Int | JvmType::Long | JvmType::Short => {
            RubyType::integer()
        }
        JvmType::Double | JvmType::Float => RubyType::float(),
        JvmType::Boolean => RubyType::boolean(),
        JvmType::Void => RubyType::nil_class(),
        JvmType::Object(name) => JavaClassName::parse(name)
            .map(|name| {
                RubyType::Class(FullyQualifiedName::constant(
                    name.ruby_namespace_parts()
                        .into_iter()
                        .map(|part| {
                            RubyConstant::new(&part).expect(
                                "INVARIANT VIOLATED: validated Java proxy part is not a Ruby constant. \
                                 This is a bug because JavaClassName owns proxy validation. \
                                 Fix: keep Java-to-Ruby proxy conversion single-sourced.",
                            )
                        })
                        .collect::<Vec<_>>(),
                ))
            })
            .unwrap_or(RubyType::Unknown),
        JvmType::Array(element) => RubyType::array_of(ruby_type_for_jvm(element)),
    }
}

fn evaluate_static_import_alias(
    block: &ruby_prism::BlockNode<'_>,
    package: &str,
    class_name: &str,
) -> Option<String> {
    let parameters = block
        .parameters()?
        .as_block_parameters_node()?
        .parameters()?;
    if parameters.requireds().iter().count() != 2
        || parameters.optionals().iter().next().is_some()
        || parameters.rest().is_some()
        || parameters.posts().iter().next().is_some()
        || parameters.keywords().iter().next().is_some()
        || parameters.keyword_rest().is_some()
        || parameters.block().is_some()
    {
        return None;
    }
    let names = parameters
        .requireds()
        .iter()
        .map(|parameter| {
            parameter
                .as_required_parameter_node()
                .map(|parameter| String::from_utf8_lossy(parameter.name().as_slice()).to_string())
        })
        .collect::<Option<Vec<_>>>()?;
    let expression = single_expression(block.body()?)?;
    let alias = evaluate_static_alias_expression(
        expression,
        (&names[0], package),
        (&names[1], class_name),
    )?;
    (alias.len() <= MAX_STATIC_IMPORT_ALIAS_BYTES).then_some(alias)
}

fn single_expression(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(statements) = node.as_statements_node() {
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    if let Some(embedded) = node.as_embedded_statements_node() {
        let statements = embedded.statements()?;
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    Some(node)
}

fn evaluate_static_alias_expression(
    node: Node<'_>,
    first: (&str, &str),
    second: (&str, &str),
) -> Option<String> {
    if let Some(string) = node.as_string_node() {
        return Some(String::from_utf8_lossy(string.unescaped()).to_string());
    }
    if let Some(local) = node.as_local_variable_read_node() {
        let name = String::from_utf8_lossy(local.name().as_slice());
        return match name.as_ref() {
            name if name == first.0 => Some(first.1.to_string()),
            name if name == second.0 => Some(second.1.to_string()),
            _ => None,
        };
    }
    let interpolated = node.as_interpolated_string_node()?;
    let mut output = String::new();
    for part in interpolated.parts().iter() {
        let value = if let Some(string) = part.as_string_node() {
            String::from_utf8_lossy(string.unescaped()).to_string()
        } else {
            evaluate_static_alias_expression(single_expression(part)?, first, second)?
        };
        if output.len().saturating_add(value.len()) > MAX_STATIC_IMPORT_ALIAS_BYTES {
            return None;
        }
        output.push_str(&value);
    }
    Some(output)
}

fn valid_java_identifier(component: &str) -> bool {
    let mut chars = component.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && chars
            .all(|character| character.is_alphanumeric() || character == '_' || character == '$')
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

fn canonical_java_constant_path(node: &ConstantPathNode<'_>) -> Option<String> {
    let mut parts = Vec::new();
    collect_ruby_constant_path(node, &mut parts)?;
    (parts.first().is_some_and(|part| part == "Java") && parts.len() >= 3).then(|| parts.join("::"))
}

fn collect_ruby_constant_path(node: &ConstantPathNode<'_>, parts: &mut Vec<String>) -> Option<()> {
    if let Some(parent) = node.parent() {
        if let Some(path) = parent.as_constant_path_node() {
            collect_ruby_constant_path(&path, parts)?;
        } else if let Some(read) = parent.as_constant_read_node() {
            parts.push(String::from_utf8_lossy(read.name().as_slice()).to_string());
        } else {
            return None;
        }
    }
    let name = node.name()?;
    parts.push(String::from_utf8_lossy(name.as_slice()).to_string());
    Some(())
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

#[cfg(test)]
mod tests {
    use super::super::java_catalog::JavaClassDeclaration;
    use super::*;
    use parking_lot::RwLock;
    use ruby_analysis::core::{ReferenceCandidateKind, SourceKind, TypeProvenance};
    use ruby_analysis::engine::{AnalysisEngine, SourceFileInput};
    use ruby_analysis::indexer::RubyDocument;
    use ruby_fast_lsp_jvm_metadata::{
        ClassFile, JavaSourceMemberLocation, MemberInfo, MethodParameter, SourceByteRange,
    };
    use ruby_prism::Visit;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Url;

    fn catalog(class_names: &[&str]) -> Arc<ProjectJavaCatalog> {
        let classes = class_names
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    JavaClassDeclaration {
                        class: Arc::new(ClassFile {
                            minor_version: 0,
                            major_version: 61,
                            access_flags: 0x0021,
                            name: (*name).to_string(),
                            super_name: Some("java/lang/Object".to_string()),
                            interfaces: Vec::new(),
                            fields: Vec::new(),
                            methods: Vec::new(),
                            source_file: None,
                            signature: None,
                            annotations: Vec::new(),
                            inner_classes: Vec::new(),
                            record_components: Vec::new(),
                            module_name: None,
                        }),
                        artifact_path: PathBuf::from("/fixture/runtime.jar"),
                        artifact_fingerprint_sha256: "fixture".to_string(),
                        entry_name: format!("{name}.class"),
                        release: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        Arc::new(ProjectJavaCatalog {
            classpath_fingerprint_sha256: "fixture-classpath".to_string(),
            classes,
            duplicates: Vec::new(),
        })
    }

    fn collect(source: &str, class_names: &[&str]) -> FactCollector {
        collect_with_catalog(source, catalog(class_names))
    }

    #[test]
    fn provider_ignores_anonymous_jvm_classes_without_losing_named_nested_proxies() {
        let provider = JrubyImportProvider::new(catalog(&[
            "com/apple/eawt/FullScreenHandler$1",
            "java/util/Map$Entry",
        ]));

        assert_eq!(
            provider
                .class_name_for_static_proxy_reference("Java::JavaUtil::Map::Entry")
                .unwrap(),
            Some("java/util/Map$Entry".to_string())
        );
        assert_eq!(
            provider
                .class_name_for_static_proxy_reference("Java::ComAppleEawt::FullScreenHandler::1")
                .unwrap(),
            None
        );
    }

    fn collect_with_catalog(source: &str, catalog: Arc<ProjectJavaCatalog>) -> FactCollector {
        collect_with_provider(source, Arc::new(JrubyImportProvider::new(catalog)))
    }

    fn collect_with_provider(source: &str, provider: Arc<JrubyImportProvider>) -> FactCollector {
        let path = PathBuf::from("/workspace/admin/imports.rb");
        let uri = Url::from_file_path(&path).expect("fixture path must be a file URI");
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path,
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(document, provider, engine);
        let parse = ruby_prism::parse(source.as_bytes());
        collector.visit(&parse.node());
        collector
    }

    #[test]
    fn imports_dotted_java_class_into_current_lexical_namespace() {
        let collector = collect(
            "module Admin\n  java_import java.lang.String\nend\n",
            &["java/lang/String"],
        );
        let alias =
            FullyQualifiedName::try_from("Admin::String").expect("fixture alias FQN must be valid");
        assert!(collector
            .direct_facts
            .symbols
            .iter()
            .any(|fact| fact.fqn == alias && fact.kind == SymbolKind::Constant));
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(alias.clone())
                && fact.ruby_type
                    == RubyType::ClassReference(
                        FullyQualifiedName::try_from("Java::JavaLang::String")
                            .expect("fixture proxy FQN must be valid"),
                    )
                && fact.provenance == TypeProvenance::Runtime
        }));
        assert!(collector.reference_candidates.iter().any(|candidate| {
            matches!(
                &candidate.kind,
                ReferenceCandidateKind::Resolved { target, .. }
                    if *target == FullyQualifiedName::try_from("Java::JavaLang::String").unwrap()
            )
        }));
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn infers_instances_constructed_from_a_canonical_dotted_java_proxy() {
        let collector = collect("INSTANCE = java.lang.String.new\n", &["java/lang/String"]);
        let instance =
            FullyQualifiedName::try_from("INSTANCE").expect("fixture constant must be valid");
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(instance.clone())
                && fact.ruby_type
                    == RubyType::Class(
                        FullyQualifiedName::try_from("Java::JavaLang::String").unwrap(),
                    )
        }));
    }

    #[test]
    fn generated_signature_source_preserves_its_java_proxy_class_declaration() {
        let collector = collect(
            "module Java\n\
             \x20 module JavaLang\n\
             \x20   class String < Java::JavaLang::Object\n\
             \x20     def self.new; end\n\
             \x20   end\n\
             \x20 end\n\
             end\n",
            &["java/lang/String"],
        );
        let proxy = FullyQualifiedName::namespace(
            ["Java", "JavaLang", "String"]
                .into_iter()
                .map(|part| RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        assert!(collector
            .direct_facts
            .symbols
            .iter()
            .any(|fact| fact.fqn == proxy && fact.kind == SymbolKind::Class));
    }

    #[test]
    fn java_alias_projects_the_selected_java_overload_onto_the_proxy_owner() {
        let mut catalog = Arc::try_unwrap(catalog(&["java/util/ArrayList", "java/lang/Object"]))
            .expect("fixture catalog must have one owner");
        let declaration = catalog
            .classes
            .get_mut("java/util/ArrayList")
            .expect("fixture class must exist");
        Arc::make_mut(&mut declaration.class)
            .methods
            .push(MemberInfo {
                access_flags: 0x0001,
                name: "add".to_string(),
                descriptor: "(ILjava/lang/Object;)Z".to_string(),
                signature: None,
                exceptions: Vec::new(),
                parameters: vec![
                    MethodParameter {
                        name: "index".to_string(),
                        access_flags: 0,
                    },
                    MethodParameter {
                        name: "value".to_string(),
                        access_flags: 0,
                    },
                ],
                annotations: Vec::new(),
                first_line: None,
            });
        let collector = collect_with_catalog(
            "java_import java.util.ArrayList\n\
             class ArrayList\n\
               java_alias :simple_add, :add, [Java::int, java.lang.Object]\n\
             end\n",
            Arc::new(catalog),
        );
        let alias = FullyQualifiedName::method(
            ["Java", "JavaUtil", "ArrayList"]
                .into_iter()
                .map(|part| RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
            ruby_analysis::core::RubyMethod::new("simple_add").unwrap(),
        );
        let method = collector
            .direct_facts
            .methods
            .iter()
            .find(|fact| fact.fqn == alias)
            .expect("java_alias must define the alias on the Java proxy");
        assert_eq!(method.params, vec!["index", "value"]);
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.subject == TypeSubject::MethodReturn(alias.clone())
                && fact.ruby_type == RubyType::boolean()
                && fact.provenance == TypeProvenance::Runtime
        }));
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn java_send_selects_an_exact_overload_projects_its_return_and_references_its_name() {
        let mut catalog = Arc::try_unwrap(catalog(&[
            "java/util/ArrayList",
            "java/lang/Object",
            "java/lang/String",
        ]))
        .expect("fixture catalog must have one owner");
        let declaration = catalog
            .classes
            .get_mut("java/util/ArrayList")
            .expect("fixture class must exist");
        let list = Arc::make_mut(&mut declaration.class);
        list.methods.extend([
            MemberInfo {
                access_flags: 0x0001,
                name: "get".to_string(),
                descriptor: "(I)Ljava/lang/Object;".to_string(),
                signature: None,
                exceptions: Vec::new(),
                parameters: vec![MethodParameter {
                    name: "index".to_string(),
                    access_flags: 0,
                }],
                annotations: Vec::new(),
                first_line: None,
            },
            MemberInfo {
                access_flags: 0x0001,
                name: "get".to_string(),
                descriptor: "(Ljava/lang/String;)Ljava/lang/String;".to_string(),
                signature: None,
                exceptions: Vec::new(),
                parameters: vec![MethodParameter {
                    name: "key".to_string(),
                    access_flags: 0,
                }],
                annotations: Vec::new(),
                first_line: None,
            },
        ]);
        let provider = Arc::new(JrubyImportProvider::new(Arc::new(catalog)));
        let preferred_range = TextRange::new(SourceFileId(99), 10, 40);
        provider.register_method_navigation_ranges(
            "java/util/ArrayList",
            &JavaSourceClassLocation {
                internal_name: "java/util/ArrayList".to_string(),
                declaration_range: SourceByteRange::new(0, 100),
                name_range: SourceByteRange::new(0, 9),
                methods: vec![
                    JavaSourceMemberLocation {
                        name: "get".to_string(),
                        descriptor: "(I)Ljava/lang/Object;".to_string(),
                        declaration_range: SourceByteRange::new(10, 40),
                        name_range: SourceByteRange::new(20, 23),
                    },
                    JavaSourceMemberLocation {
                        name: "get".to_string(),
                        descriptor: "(Ljava/lang/String;)Ljava/lang/String;".to_string(),
                        declaration_range: SourceByteRange::new(50, 90),
                        name_range: SourceByteRange::new(60, 63),
                    },
                ],
                fields: Vec::new(),
            },
            SourceFileId(99),
        );
        let collector = collect_with_provider(
            "java_import java.util.ArrayList\n\
             LIST = ArrayList.new\n\
             RESULT = LIST.java_send(:get, [Java::int], 0)\n",
            provider,
        );
        let result =
            FullyQualifiedName::try_from("RESULT").expect("fixture result constant must be valid");
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(result.clone())
                && fact.ruby_type
                    == RubyType::Class(
                        FullyQualifiedName::try_from("Java::JavaLang::Object").unwrap(),
                    )
                && fact.provenance == TypeProvenance::Runtime
        }));
        let (selected, selected_range) = collector
            .reference_candidates
            .iter()
            .find_map(|candidate| match &candidate.kind {
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    preferred_definition_range,
                    ..
                } if method.as_str() == "get" => {
                    Some(((owner, owner_kind), *preferred_definition_range))
                }
                ReferenceCandidateKind::Constant { .. }
                | ReferenceCandidateKind::Method { .. }
                | ReferenceCandidateKind::Resolved { .. } => None,
            })
            .expect("java_send method-name symbol must reference the selected Java method");
        assert_eq!(
            selected.0.as_slice(),
            FullyQualifiedName::try_from("Java::JavaUtil::ArrayList")
                .unwrap()
                .namespace_parts()
                .as_slice()
        );
        assert_eq!(*selected.1, NamespaceKind::Instance);
        assert_eq!(
            selected_range,
            Some(preferred_range),
            "java_send must retain the exact source/decompiled range for the selected JVM descriptor"
        );
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn decompiled_supplement_retains_only_members_missing_from_exact_source() {
        let exact = JavaSourceClassLocation {
            internal_name: "fixtures/RichFixture".to_string(),
            declaration_range: SourceByteRange::new(0, 100),
            name_range: SourceByteRange::new(6, 17),
            methods: vec![JavaSourceMemberLocation {
                name: "combine".to_string(),
                descriptor: "(Ljava/lang/String;[I)Ljava/util/List;".to_string(),
                declaration_range: SourceByteRange::new(10, 40),
                name_range: SourceByteRange::new(20, 27),
            }],
            fields: vec![JavaSourceMemberLocation {
                name: "COUNT".to_string(),
                descriptor: "I".to_string(),
                declaration_range: SourceByteRange::new(41, 50),
                name_range: SourceByteRange::new(42, 47),
            }],
        };
        let mut decompiled = exact.clone();
        decompiled.methods.push(JavaSourceMemberLocation {
            name: "syntheticBridge".to_string(),
            descriptor: "()V".to_string(),
            declaration_range: SourceByteRange::new(60, 80),
            name_range: SourceByteRange::new(61, 76),
        });
        decompiled.fields.push(JavaSourceMemberLocation {
            name: "GENERATED".to_string(),
            descriptor: "Ljava/lang/String;".to_string(),
            declaration_range: SourceByteRange::new(81, 95),
            name_range: SourceByteRange::new(82, 91),
        });

        let supplemental = supplemental_implementation_location(&exact, decompiled)
            .expect("missing bytecode members must produce a decompiled supplement");
        assert_eq!(
            supplemental
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
            vec!["syntheticBridge"]
        );
        assert_eq!(
            supplemental
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["GENERATED"]
        );
    }

    #[test]
    fn java_method_distinguishes_bound_static_and_unbound_instance_handles() {
        let mut catalog = Arc::try_unwrap(catalog(&["java/lang/String"]))
            .expect("fixture catalog must have one owner");
        let declaration = catalog
            .classes
            .get_mut("java/lang/String")
            .expect("fixture class must exist");
        let string = Arc::make_mut(&mut declaration.class);
        string.methods.extend([
            MemberInfo {
                access_flags: 0x0009,
                name: "valueOf".to_string(),
                descriptor: "(I)Ljava/lang/String;".to_string(),
                signature: None,
                exceptions: Vec::new(),
                parameters: vec![MethodParameter {
                    name: "value".to_string(),
                    access_flags: 0,
                }],
                annotations: Vec::new(),
                first_line: None,
            },
            MemberInfo {
                access_flags: 0x0001,
                name: "substring".to_string(),
                descriptor: "(I)Ljava/lang/String;".to_string(),
                signature: None,
                exceptions: Vec::new(),
                parameters: vec![MethodParameter {
                    name: "start".to_string(),
                    access_flags: 0,
                }],
                annotations: Vec::new(),
                first_line: None,
            },
        ]);
        let collector = collect_with_catalog(
            "java_import java.lang.String\n\
             STATIC_HANDLE = String.java_method(:valueOf, [Java::int])\n\
             UNBOUND_HANDLE = String.java_method(:substring, [Java::int])\n\
             INSTANCE = String.new\n\
             BOUND_HANDLE = INSTANCE.java_method(:substring, [Java::int])\n",
            Arc::new(catalog),
        );
        for (constant, expected) in [
            ("STATIC_HANDLE", "Method"),
            ("UNBOUND_HANDLE", "UnboundMethod"),
            ("BOUND_HANDLE", "Method"),
        ] {
            let constant = FullyQualifiedName::try_from(constant).unwrap();
            let expected = RubyType::Class(FullyQualifiedName::try_from(expected).unwrap());
            assert!(
                collector.direct_facts.types.iter().any(|fact| {
                    fact.subject == TypeSubject::Constant(constant.clone())
                        && fact.ruby_type == expected
                        && fact.provenance == TypeProvenance::Runtime
                }),
                "{constant} must have type {expected}"
            );
        }
        assert_eq!(
            collector
                .reference_candidates
                .iter()
                .filter(|candidate| matches!(
                    &candidate.kind,
                    ReferenceCandidateKind::Method { method, .. }
                        if matches!(method.as_str(), "valueOf" | "substring")
                ))
                .count(),
            3
        );
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn to_java_projects_explicit_object_primitive_and_array_targets() {
        let collector = collect(
            "OBJECT = 'value'.to_java(java.lang.CharSequence)\n\
             INTEGER = 1.to_java(Java::int)\n\
             INTS = [1, 2].to_java(Java::int)\n",
            &["java/lang/CharSequence"],
        );
        for (constant, expected) in [
            (
                "OBJECT",
                RubyType::Class(
                    FullyQualifiedName::try_from("Java::JavaLang::CharSequence").unwrap(),
                ),
            ),
            (
                "INTEGER",
                RubyType::Class(FullyQualifiedName::try_from("Java::JavaLang::Integer").unwrap()),
            ),
            ("INTS", RubyType::array_of(RubyType::integer())),
        ] {
            let constant = FullyQualifiedName::try_from(constant).unwrap();
            assert!(
                collector.direct_facts.types.iter().any(|fact| {
                    fact.subject == TypeSubject::Constant(constant.clone())
                        && fact.ruby_type == expected
                        && fact.provenance == TypeProvenance::Runtime
                }),
                "{constant} must have type {expected}"
            );
        }
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn java_interfaces_connect_to_ruby_classes_through_include_and_java_implements() {
        let mut catalog = Arc::try_unwrap(catalog(&["java/lang/Runnable"]))
            .expect("fixture catalog must have one owner");
        let declaration = catalog
            .classes
            .get_mut("java/lang/Runnable")
            .expect("fixture interface must exist");
        Arc::make_mut(&mut declaration.class).access_flags = 0x0601;
        let collector = collect_with_catalog(
            "class Worker\n\
             \x20 java_implements java.lang.Runnable\n\
             end\n\
             class IncludedWorker\n\
             \x20 include java.lang.Runnable\n\
             end\n",
            Arc::new(catalog),
        );
        let target = FullyQualifiedName::namespace(
            ["Java", "JavaLang", "Runnable"]
                .into_iter()
                .map(|part| RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        for source in ["Worker", "IncludedWorker"] {
            let source = FullyQualifiedName::namespace(vec![RubyConstant::new(source).unwrap()]);
            assert!(collector.direct_facts.graph_edges.iter().any(|edge| {
                edge.source == source
                    && edge.target == target
                    && edge.kind == ruby_analysis::core::GraphEdgeKind::Include
            }));
        }
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn ordinary_ruby_include_argument_calls_are_not_java_interfaces() {
        let collector = collect("[301, 302].should include last_response.status\n", &[]);

        assert!(
            collector
                .analysis_diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "unresolved-java-interface"),
            "ordinary Ruby include arguments must not be interpreted as JRuby interface names"
        );
    }

    #[test]
    fn java_package_accepts_only_the_static_jrubyc_declaration_form() {
        let collector = collect(
            "java_package 'com.example.generated'\n\
             dynamic_package = 'com.example.dynamic'\n\
             java_package dynamic_package\n",
            &[],
        );
        assert_eq!(collector.analysis_diagnostics.len(), 1);
        assert_eq!(
            collector.analysis_diagnostics[0].code,
            "unsupported-jruby-java-package"
        );
    }

    #[test]
    fn supports_string_array_and_nested_java_class_imports() {
        let collector = collect(
            "java_import ['java.lang.String', 'java.util.Map$Entry']\n",
            &["java/lang/String", "java/util/Map$Entry"],
        );
        assert!(collector
            .direct_facts
            .symbols
            .iter()
            .any(|fact| fact.fqn == FullyQualifiedName::try_from("String").unwrap()));
        assert!(collector
            .direct_facts
            .symbols
            .iter()
            .any(|fact| fact.fqn == FullyQualifiedName::try_from("Entry").unwrap()));
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.ruby_type
                == RubyType::ClassReference(
                    FullyQualifiedName::try_from("Java::JavaUtil::Map::Entry").unwrap(),
                )
        }));
    }

    #[test]
    fn evaluates_bounded_java_import_alias_blocks_without_executing_ruby() {
        let collector = collect(
            "module Types\n\
               java_import('java.lang.String') { |package, name| \"J#{name}\" }\n\
             end\n",
            &["java/lang/String"],
        );
        let alias =
            FullyQualifiedName::try_from("Types::JString").expect("fixture alias must be valid");
        assert!(collector
            .direct_facts
            .symbols
            .iter()
            .any(|fact| fact.fqn == alias && fact.kind == SymbolKind::Constant));
        assert!(collector.direct_facts.types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(alias.clone())
                && fact.ruby_type
                    == RubyType::ClassReference(
                        FullyQualifiedName::try_from("Java::JavaLang::String").unwrap(),
                    )
        }));
        assert!(collector.analysis_diagnostics.is_empty());
    }

    #[test]
    fn reports_missing_project_class_and_dynamic_alias_block() {
        let collector = collect(
            "java_import java.lang.Missing\n\
             java_import(java.lang.String) { |_package, name| name.upcase }\n",
            &["java/lang/String"],
        );
        let codes = collector
            .analysis_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec!["unresolved-java-import", "unsupported-jruby-import-alias"]
        );
        assert!(collector.direct_facts.symbols.is_empty());
    }

    #[test]
    fn include_package_and_import_package_add_bounded_lazy_constant_types() {
        let collector = collect(
            "module Util\n  include_package 'java.util'\n  import 'java.lang'\nend\n",
            &[
                "java/util/List",
                "java/util/Map",
                "java/util/Map$Entry",
                "java/lang/String",
            ],
        );
        for (alias, proxy) in [
            ("Util::List", "Java::JavaUtil::List"),
            ("Util::Map", "Java::JavaUtil::Map"),
            ("Util::String", "Java::JavaLang::String"),
        ] {
            assert!(collector.direct_facts.types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(FullyQualifiedName::try_from(alias).unwrap())
                    && fact.ruby_type
                        == RubyType::ClassReference(FullyQualifiedName::try_from(proxy).unwrap())
            }));
        }
        assert!(
            collector
                .direct_facts
                .symbols
                .iter()
                .all(|fact| fact.kind != SymbolKind::Constant),
            "include_package constants are runtime const_missing results, not source declarations"
        );
        assert!(collector.analysis_diagnostics.is_empty());
    }

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
             CANONICAL = Java::JavaUtil::Map::Entry\n",
        );
        assert!(references.contains(&"java.lang.String".to_string()));
        assert!(references.contains(&"Java::JavaUtil::Map::Entry".to_string()));
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

    #[test]
    fn package_preflight_materializes_signatures_but_only_referenced_implementations() {
        let provider = JrubyImportProvider::new(catalog(&[
            "java/util/ArrayList",
            "java/util/HashMap",
            "java/time/Instant",
        ]));
        let plan = provider
            .static_navigation_plan(
                "include_package 'java.util'\n\
                 java_import 'java.time.Instant'\n\
                 LIST = ArrayList.new\n",
            )
            .expect("static navigation plan must resolve checked catalog names");

        assert_eq!(
            plan.signature_class_names,
            vec![
                "java/time/Instant".to_string(),
                "java/util/ArrayList".to_string(),
                "java/util/HashMap".to_string(),
            ],
            "package imports need signatures for every bounded direct class"
        );
        assert_eq!(
            plan.implementation_class_names,
            vec![
                "java/time/Instant".to_string(),
                "java/util/ArrayList".to_string(),
            ],
            "exact source/decompilation work is needed only for explicit imports and referenced \
             package proxies"
        );
    }

    #[test]
    fn static_navigation_prefilter_covers_every_supported_java_entry_form() {
        let provider = JrubyImportProvider::new(catalog(&["java/lang/String", "com/example/Demo"]));
        for source in [
            "java_import 'java.lang.String'\n",
            "include_package 'java.lang'\nString.new\n",
            "import 'java.lang'\nString.new\n",
            "include java.lang.String\n",
            "java_implements java.lang.String\n",
            "Java::JavaLang::String.new\n",
            "Java :: JavaLang :: String.new\n",
            "com.example.Demo.new\n",
            "com . example . Demo.new\n",
        ] {
            assert!(
                provider.source_may_reference_static_java(source),
                "the exact-catalog prefilter must retain supported Java source: {source:?}"
            );
        }
        assert!(
            !provider.source_may_reference_static_java("module Admin\n  user.profile.name\nend\n"),
            "ordinary Ruby call chains must not pay a redundant JRuby AST traversal"
        );
    }
}
