use super::{
    decompiler::{JavaDecompiler, JavaDecompilerError},
    java_catalog::{JavaClassDeclaration, ProjectJavaCatalog},
    source_navigation::{JavaSourceResolutionError, JavaSourceResolver, ResolvedJavaSource},
};
use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, MethodParamFact, MethodParamKind,
    MethodReferenceAccess, MethodReferenceCandidate, MethodReferenceDiagnostics, NamespaceKind,
    ReferenceCandidate, RubyConstant, RubyMethod, RubyType, SourceFileId, SymbolFact, SymbolKind,
    TextRange, TypeFact, TypeProvenance, TypeSubject,
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(test)]
std::thread_local! {
    static SEMANTIC_PREFILTER_PARSE_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

/// Process-wide probe for JRuby call-host cost during fact collection.
///
/// Every Prism `CallNode` on a file with an installed provider enters
/// [`JrubyImportProvider::process_call_node`]. Handler fields count how many
/// calls reached that named Java/JRuby form after the name dispatch.
static CALL_HOST_ENTRIES: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_SEED_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_IMPORT_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_IMPORT_DISPATCH_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_INCLUDE_PACKAGE_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_INTERFACE_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_PACKAGE_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_ALIAS_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_DISPATCH_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_TO_JAVA_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_CTOR_NS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_SEED_DOTTED: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_SEED_CATALOG_HITS: AtomicU64 = AtomicU64::new(0);
static CALL_HOST_JAVA_CTOR_INFERRED: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JrubyCallHostProbeSnapshot {
    pub entries: u64,
    pub seed_ns: u64,
    pub import_ns: u64,
    pub import_dispatch_ns: u64,
    pub include_package_ns: u64,
    pub java_interface_ns: u64,
    pub java_package_ns: u64,
    pub java_alias_ns: u64,
    pub java_dispatch_ns: u64,
    pub to_java_ns: u64,
    pub java_ctor_ns: u64,
    pub seed_dotted_candidates: u64,
    pub seed_catalog_hits: u64,
    pub java_ctor_inferred: u64,
}

impl JrubyCallHostProbeSnapshot {
    pub fn total_handler_ns(self) -> u64 {
        self.seed_ns
            + self.import_ns
            + self.import_dispatch_ns
            + self.include_package_ns
            + self.java_interface_ns
            + self.java_package_ns
            + self.java_alias_ns
            + self.java_dispatch_ns
            + self.to_java_ns
            + self.java_ctor_ns
    }
}

pub fn jruby_call_host_probe_snapshot() -> JrubyCallHostProbeSnapshot {
    JrubyCallHostProbeSnapshot {
        entries: CALL_HOST_ENTRIES.load(Ordering::Relaxed),
        seed_ns: CALL_HOST_SEED_NS.load(Ordering::Relaxed),
        import_ns: CALL_HOST_IMPORT_NS.load(Ordering::Relaxed),
        import_dispatch_ns: CALL_HOST_IMPORT_DISPATCH_NS.load(Ordering::Relaxed),
        include_package_ns: CALL_HOST_INCLUDE_PACKAGE_NS.load(Ordering::Relaxed),
        java_interface_ns: CALL_HOST_JAVA_INTERFACE_NS.load(Ordering::Relaxed),
        java_package_ns: CALL_HOST_JAVA_PACKAGE_NS.load(Ordering::Relaxed),
        java_alias_ns: CALL_HOST_JAVA_ALIAS_NS.load(Ordering::Relaxed),
        java_dispatch_ns: CALL_HOST_JAVA_DISPATCH_NS.load(Ordering::Relaxed),
        to_java_ns: CALL_HOST_TO_JAVA_NS.load(Ordering::Relaxed),
        java_ctor_ns: CALL_HOST_JAVA_CTOR_NS.load(Ordering::Relaxed),
        seed_dotted_candidates: CALL_HOST_SEED_DOTTED.load(Ordering::Relaxed),
        seed_catalog_hits: CALL_HOST_SEED_CATALOG_HITS.load(Ordering::Relaxed),
        java_ctor_inferred: CALL_HOST_JAVA_CTOR_INFERRED.load(Ordering::Relaxed),
    }
}

pub fn reset_jruby_call_host_probe() {
    for counter in [
        &CALL_HOST_ENTRIES,
        &CALL_HOST_SEED_NS,
        &CALL_HOST_IMPORT_NS,
        &CALL_HOST_IMPORT_DISPATCH_NS,
        &CALL_HOST_INCLUDE_PACKAGE_NS,
        &CALL_HOST_JAVA_INTERFACE_NS,
        &CALL_HOST_JAVA_PACKAGE_NS,
        &CALL_HOST_JAVA_ALIAS_NS,
        &CALL_HOST_JAVA_DISPATCH_NS,
        &CALL_HOST_TO_JAVA_NS,
        &CALL_HOST_JAVA_CTOR_NS,
        &CALL_HOST_SEED_DOTTED,
        &CALL_HOST_SEED_CATALOG_HITS,
        &CALL_HOST_JAVA_CTOR_INFERRED,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
}

fn record_call_host_hit(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn log_jruby_call_host_probe(label: &str, project: &Path) {
    let snap = jruby_call_host_probe_snapshot();
    log::info!(
        "[PERF][jruby call host] label={} project={} entries={} total_handler={} \
         seed={} import={} import_dispatch={} include_package={} \
         java_interface={} java_package={} java_alias={} \
         java_dispatch={} to_java={} java_ctor={} \
         seed_dotted={} seed_catalog_hits={} java_ctor_inferred={}",
        label,
        project.display(),
        snap.entries,
        snap.total_handler_ns(),
        snap.seed_ns,
        snap.import_ns,
        snap.import_dispatch_ns,
        snap.include_package_ns,
        snap.java_interface_ns,
        snap.java_package_ns,
        snap.java_alias_ns,
        snap.java_dispatch_ns,
        snap.to_java_ns,
        snap.java_ctor_ns,
        snap.seed_dotted_candidates,
        snap.seed_catalog_hits,
        snap.java_ctor_inferred,
    );
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

    pub fn source_may_reference_static_java(&self, source: &str) -> bool {
        self.source_hint_may_reference_static_java(&StaticJavaSourceHint::from_source(source))
    }

    pub fn source_hint_may_reference_static_java(&self, hint: &StaticJavaSourceHint) -> bool {
        hint.definite_catalog_semantics
            || hint.dotted_roots.iter().any(|root| {
                root == "Java" || self.static_top_level_packages.contains(root.as_str())
            })
    }

    fn call_may_need_jruby_host(&self, node: &CallNode<'_>) -> bool {
        match node.name().as_slice() {
            b"java_import" | b"import" | b"include_package" | b"include" | b"java_implements"
            | b"java_package" | b"java_alias" | b"java_send" | b"java_method" | b"to_java"
            | b"new" => true,
            _ => self.call_may_be_static_java_proxy(node),
        }
    }

    fn should_seed_static_proxy(&self, node: &CallNode<'_>) -> bool {
        if self.call_may_be_static_java_proxy(node) {
            return true;
        }
        if node.name().as_slice() != b"new" {
            return false;
        }
        node.receiver().is_some_and(|receiver| {
            receiver
                .as_call_node()
                .is_some_and(|call| self.call_may_be_static_java_proxy(&call))
                || receiver
                    .as_constant_read_node()
                    .is_some_and(|constant| constant.name().as_slice() == b"Java")
                || receiver.as_constant_path_node().is_some()
        })
    }

    fn call_may_be_static_java_proxy(&self, call: &CallNode<'_>) -> bool {
        let Some(root) = dotted_call_root(call) else {
            return false;
        };
        let Ok(root) = std::str::from_utf8(root) else {
            return false;
        };
        root == "Java" || self.static_top_level_packages.contains(root)
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
        if let Some(path) = node.as_constant_path_node() {
            let Some(reference) = canonical_java_constant_path(&path) else {
                return;
            };
            if !self.proxy_to_internal.contains_key(&reference) {
                return;
            }
            // The selected project's class catalog proves this proxy exists.
            // Ordinary constant inference must not guess Java classes from
            // syntax before the provider installs its runtime evidence.
            let proxy = FullyQualifiedName::try_from(reference.as_str()).expect(
                "INVARIANT VIOLATED: a catalog-owned Java proxy has an invalid Ruby constant path. This is a bug because proxy_to_internal contains validated proxy identities. Fix: preserve validation when constructing the catalog mapping.",
            );
            visitor.direct_push_expression_type(
                node,
                RubyType::ClassReference(proxy),
                TypeProvenance::Runtime,
            );
            return;
        }
        let Some(call) = node.as_call_node() else {
            return;
        };
        if let Some(receiver) = call.receiver() {
            self.seed_static_proxy_expression(visitor, &receiver);
        }
        let Some(dotted_name) = dotted_call_name(&call) else {
            return;
        };
        CALL_HOST_SEED_DOTTED.fetch_add(1, Ordering::Relaxed);
        let Ok(java_name) = JavaClassName::parse(&dotted_name) else {
            return;
        };
        if !self.catalog.classes.contains_key(java_name.internal_name()) {
            return;
        }
        CALL_HOST_SEED_CATALOG_HITS.fetch_add(1, Ordering::Relaxed);
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
        let source = FullyQualifiedName::namespace(visitor.scope_tracker().get_ns_stack());
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
            visitor.add_graph_edge_fact(GraphEdgeFact::new(
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

        let current_namespace =
            FullyQualifiedName::namespace(visitor.scope_tracker().get_ns_stack());
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
        visitor.add_reference_candidate(ReferenceCandidate::method(
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
                caller: visitor.scope_tracker().current_method_fqn().cloned(),
                call_expression_range: None,
                preferred_definition_range: self
                    .preferred_method_definition_range(&selected.owner, &selected.method),
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: method_range,
                    receiver_label: Some(proxy.to_string()),
                    receiver_expression_range: None,
                    receiver_type: None,
                    diagnose_unresolved: false,
                    allow_unindexed_owner: false,
                    signature: None,
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
        CALL_HOST_JAVA_CTOR_INFERRED.fetch_add(1, Ordering::Relaxed);
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
            RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => return None,
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
            | RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
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
        visitor.add_reference_candidate(ReferenceCandidate::method(
            old_name_range,
            MethodReferenceCandidate {
                owner: proxy.namespace_parts().to_vec(),
                owner_kind: NamespaceKind::Instance,
                method: old_method,
                is_super: false,
                access: MethodReferenceAccess::Normal,
                caller: visitor.scope_tracker().current_method_fqn().cloned(),
                call_expression_range: None,
                preferred_definition_range: None,
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: old_name_range,
                    receiver_label: None,
                    receiver_expression_range: None,
                    receiver_type: None,
                    diagnose_unresolved: false,
                    allow_unindexed_owner: false,
                    signature: None,
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
        visitor.add_type_fact(fact.clone());
        visitor.add_direct_type_fact(fact);
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

        let mut alias_parts = visitor.scope_tracker().get_ns_stack();
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
            visitor.add_symbol_fact(
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
        visitor.add_reference_candidate(ReferenceCandidate::resolved(
            import.name_range,
            proxy_fqn.clone(),
            visitor.scope_tracker().current_method_fqn().cloned(),
        ));
        let type_fact = TypeFact::new(
            TypeSubject::Constant(alias_fqn),
            RubyType::ClassReference(proxy_fqn),
            declaration_range,
            TypeProvenance::Runtime,
        );
        visitor.add_type_fact(type_fact.clone());
        visitor.add_direct_type_fact(type_fact);
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
    fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode<'_>) -> bool {
        if !self.call_may_need_jruby_host(node) {
            return false;
        }
        CALL_HOST_ENTRIES.fetch_add(1, Ordering::Relaxed);

        if self.should_seed_static_proxy(node) {
            record_call_host_hit(&CALL_HOST_SEED_NS);
            self.seed_static_proxy_expression(visitor, &node.as_node());
        }

        match node.name().as_slice() {
            b"java_import" => {
                record_call_host_hit(&CALL_HOST_IMPORT_NS);
                self.process_import_call(visitor, node);
            }
            b"import" => {
                record_call_host_hit(&CALL_HOST_IMPORT_DISPATCH_NS);
                self.process_import_dispatch(visitor, node);
            }
            b"include_package" => {
                record_call_host_hit(&CALL_HOST_INCLUDE_PACKAGE_NS);
                self.process_include_package_call(visitor, node);
            }
            b"include" | b"java_implements" => {
                record_call_host_hit(&CALL_HOST_JAVA_INTERFACE_NS);
                self.process_java_interface_call(visitor, node);
            }
            b"java_package" => {
                record_call_host_hit(&CALL_HOST_JAVA_PACKAGE_NS);
                self.process_java_package_call(visitor, node);
            }
            b"java_alias" => {
                record_call_host_hit(&CALL_HOST_JAVA_ALIAS_NS);
                self.process_java_alias_call(visitor, node);
            }
            b"java_send" | b"java_method" => {
                record_call_host_hit(&CALL_HOST_JAVA_DISPATCH_NS);
                self.process_java_dispatch_call(visitor, node);
            }
            b"to_java" => {
                record_call_host_hit(&CALL_HOST_TO_JAVA_NS);
                self.process_to_java_call(visitor, node);
            }
            b"new" => {
                record_call_host_hit(&CALL_HOST_JAVA_CTOR_NS);
                self.process_java_constructor_call(visitor, node);
            }
            _ => {}
        }

        false
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

fn dotted_call_root<'a>(call: &CallNode<'a>) -> Option<&'a [u8]> {
    if call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    match call.receiver() {
        None => Some(call.name().as_slice()),
        Some(receiver) => {
            if receiver
                .as_constant_read_node()
                .is_some_and(|constant| constant.name().as_slice() == b"Java")
            {
                return Some(b"Java");
            }
            dotted_call_root(&receiver.as_call_node()?)
        }
    }
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
        visitor.scope_tracker().get_ns_stack(),
    ));
    let direct = visitor
        .direct_facts()
        .types
        .iter()
        .rev()
        .find(|fact| fact.subject == subject && fact.provenance == TypeProvenance::Runtime)
        .map(|fact| fact.ruby_type.clone());
    let local = direct.or_else(|| {
        visitor
            .type_facts_for(&subject)
            .into_iter()
            .rev()
            .find(|fact| fact.provenance == TypeProvenance::Runtime)
            .map(|fact| fact.ruby_type)
    });
    let ruby_type = local.or_else(|| {
        visitor
            .analysis_engine()
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
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
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
mod tests;
