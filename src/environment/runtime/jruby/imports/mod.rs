//! JRuby Java import provider: the catalog-backed fact-collector extension
//! that resolves Java DSL calls and canonical proxies for one project.

mod call_host;
mod declarations;
mod java_methods;
mod java_types;
mod navigation;
mod static_scan;
mod syntax;

pub(crate) use call_host::log_jruby_call_host_probe;
pub use call_host::{
    jruby_call_host_handler_hits, jruby_call_host_probe_snapshot, reset_jruby_call_host_probe,
    CallHostStat,
};
pub(crate) use java_types::ruby_type_for_jvm;
pub use static_scan::{
    source_semantics_depend_on_jruby_catalog, static_java_dependencies, static_java_import_names,
    static_java_proxy_references, StaticJavaDependency,
};

use super::{
    decompiler::{JavaDecompiler, JavaDecompilerError},
    java_catalog::{JavaClassDeclaration, ProjectJavaCatalog},
    source_navigation::{JavaSourceResolutionError, JavaSourceResolver},
};
use parking_lot::RwLock;
use ruby_analysis::core::TextRange;
use ruby_fast_lsp_jruby_support::JavaClassName;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syntax::java_package_prefix;

const MAX_INCLUDED_PACKAGE_CLASSES: usize = 4_096;

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
}

#[cfg(test)]
mod tests;
