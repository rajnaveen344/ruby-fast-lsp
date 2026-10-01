//! Fact-collector call host: name dispatch for Java DSL calls, static proxy
//! seeding, and the process-wide call-host cost probe.

use super::syntax::{canonical_java_constant_path, dotted_call_name, dotted_call_root};
use super::JrubyImportProvider;
use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyType, TypeProvenance};
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_prism::{CallNode, Node};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

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
pub(super) static CALL_HOST_JAVA_CTOR_INFERRED: AtomicU64 = AtomicU64::new(0);

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

impl JrubyImportProvider {
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
            let proxy = FullyQualifiedName::try_from(reference.as_str()).expect_invariant(
                "a catalog-owned Java proxy has an invalid Ruby constant path",
                "proxy_to_internal contains validated proxy identities",
                "preserve validation when constructing the catalog mapping",
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
                    RubyConstant::new(&part).expect_invariant(
                        "validated Java proxy part is not a Ruby constant",
                        "JavaClassName owns proxy validation",
                        "keep dotted proxy expression conversion single-sourced",
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
