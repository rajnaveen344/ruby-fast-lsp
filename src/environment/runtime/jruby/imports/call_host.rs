//! Fact-collector call host: name dispatch for Java DSL calls, static proxy
//! seeding, and the process-wide call-host cost probe.

use super::JrubyImportProvider;
use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyType, TypeProvenance};
use ruby_analysis::indexer::fact_collector::{FactCollector, FactCollectorExtensionHost};
use ruby_analysis::stats::{StatsRegistry, StatsSnapshot};
use ruby_fast_lsp_jruby_support::syntax::{
    canonical_java_constant_path, dotted_call_name, dotted_call_root,
};
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_prism::{CallNode, Node};
use std::path::Path;
use std::sync::LazyLock;

ruby_analysis::stat_set! {
    /// Process-wide probe for JRuby call-host cost during fact collection.
    ///
    /// Every Prism `CallNode` on a file with an installed provider enters
    /// [`JrubyImportProvider::process_call_node`] and counts as an entry.
    /// Handler statistics count how many calls reached that named Java/JRuby
    /// form after the name dispatch.
    pub enum CallHostStat {
        Entries = "entries",
        Seed = "seed",
        Import = "import",
        ImportDispatch = "import_dispatch",
        IncludePackage = "include_package",
        JavaInterface = "java_interface",
        JavaPackage = "java_package",
        JavaAlias = "java_alias",
        JavaDispatch = "java_dispatch",
        ToJava = "to_java",
        JavaCtor = "java_ctor",
        SeedDotted = "seed_dotted",
        SeedCatalogHits = "seed_catalog_hits",
        JavaCtorInferred = "java_ctor_inferred",
    }
}

const CALL_HOST_HANDLERS: [CallHostStat; 10] = [
    CallHostStat::Seed,
    CallHostStat::Import,
    CallHostStat::ImportDispatch,
    CallHostStat::IncludePackage,
    CallHostStat::JavaInterface,
    CallHostStat::JavaPackage,
    CallHostStat::JavaAlias,
    CallHostStat::JavaDispatch,
    CallHostStat::ToJava,
    CallHostStat::JavaCtor,
];

pub(super) static CALL_HOST_STATS: LazyLock<StatsRegistry<CallHostStat>> =
    LazyLock::new(StatsRegistry::default);

pub fn jruby_call_host_probe_snapshot() -> StatsSnapshot<CallHostStat> {
    CALL_HOST_STATS.snapshot()
}

/// Calls that reached any named handler after the name dispatch.
pub fn jruby_call_host_handler_hits(snapshot: &StatsSnapshot<CallHostStat>) -> u64 {
    CALL_HOST_HANDLERS.iter().fold(0u64, |total, &stat| {
        total.saturating_add(snapshot.get(stat))
    })
}

pub fn reset_jruby_call_host_probe() {
    CALL_HOST_STATS.reset();
}

pub(crate) fn log_jruby_call_host_probe(label: &str, project: &Path) {
    let snapshot = jruby_call_host_probe_snapshot();
    let details = snapshot
        .iter()
        .filter(|(stat, _, _)| *stat != CallHostStat::Entries)
        .map(|(_, name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ");
    log::info!(
        "[PERF][jruby call host] label={} project={} entries={} total_handler={} {}",
        label,
        project.display(),
        snapshot.get(CallHostStat::Entries),
        jruby_call_host_handler_hits(&snapshot),
        details,
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
        CALL_HOST_STATS.increment(CallHostStat::SeedDotted);
        let Ok(java_name) = JavaClassName::parse(&dotted_name) else {
            return;
        };
        if !self.catalog.classes.contains_key(java_name.internal_name()) {
            return;
        }
        CALL_HOST_STATS.increment(CallHostStat::SeedCatalogHits);
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
        CALL_HOST_STATS.increment(CallHostStat::Entries);

        if self.should_seed_static_proxy(node) {
            CALL_HOST_STATS.increment(CallHostStat::Seed);
            self.seed_static_proxy_expression(visitor, &node.as_node());
        }

        match node.name().as_slice() {
            b"java_import" => {
                CALL_HOST_STATS.increment(CallHostStat::Import);
                self.process_import_call(visitor, node);
            }
            b"import" => {
                CALL_HOST_STATS.increment(CallHostStat::ImportDispatch);
                self.process_import_dispatch(visitor, node);
            }
            b"include_package" => {
                CALL_HOST_STATS.increment(CallHostStat::IncludePackage);
                self.process_include_package_call(visitor, node);
            }
            b"include" | b"java_implements" => {
                CALL_HOST_STATS.increment(CallHostStat::JavaInterface);
                self.process_java_interface_call(visitor, node);
            }
            b"java_package" => {
                CALL_HOST_STATS.increment(CallHostStat::JavaPackage);
                self.process_java_package_call(visitor, node);
            }
            b"java_alias" => {
                CALL_HOST_STATS.increment(CallHostStat::JavaAlias);
                self.process_java_alias_call(visitor, node);
            }
            b"java_send" | b"java_method" => {
                CALL_HOST_STATS.increment(CallHostStat::JavaDispatch);
                self.process_java_dispatch_call(visitor, node);
            }
            b"to_java" => {
                CALL_HOST_STATS.increment(CallHostStat::ToJava);
                self.process_to_java_call(visitor, node);
            }
            b"new" => {
                CALL_HOST_STATS.increment(CallHostStat::JavaCtor);
                self.process_java_constructor_call(visitor, node);
            }
            _ => {}
        }

        false
    }
}
