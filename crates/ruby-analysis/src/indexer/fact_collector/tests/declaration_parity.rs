//! Declaration parity between the seed walk and the fact collector.
//!
//! Project files take declarations from the seed, while dependency sources
//! take them from the collector, so both walks must declare the same symbols,
//! methods, visibility overrides, and namespace graph for one source.

use crate::core::{FileAnalysis, FullyQualifiedName, RubyConstant, SourceKind};
use crate::engine::{Project, SourceFileInput};
use crate::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use crate::indexer::{AnalysisIndexer, RubyDocument};
use parking_lot::RwLock;
use ruby_prism::Visit;
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

/// Both walks' declarations for `source`, seed first. The collector sees only
/// the core namespaces, as it does for a dependency file.
fn walk_declarations(source: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let path = PathBuf::from("/workspace/lib/parity.rb");
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: path.clone(),
        content: source.to_string(),
        kind: SourceKind::Gem,
    });
    let known = core_namespaces();
    let seed = AnalysisIndexer::with_known_namespaces(file_id, known.clone()).index_source(source);

    let engine = Arc::new(RwLock::new(engine));
    let uri = Url::from_file_path(&path).expect("parity path is absolute");
    let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
    let mut collector =
        FactCollector::analysis_only(document, Arc::new(NullFactCollectorExtensionHost), engine)
            .without_body_inference()
            .with_shared_direct_known_namespaces(Arc::new(known));
    collector.visit(&ruby_prism::parse(source.as_bytes()).node());
    let collected = collector.finish().analysis;
    (declarations(&seed), declarations(&collected))
}

/// Each declaration only one walk produced, prefixed with that walk.
fn declaration_differences(source: &str) -> Vec<String> {
    let (seed, collected) = walk_declarations(source);
    let mut differences = Vec::new();
    differences.extend(
        seed.difference(&collected)
            .map(|declaration| format!("seed only: {declaration}")),
    );
    differences.extend(
        collected
            .difference(&seed)
            .map(|declaration| format!("collector only: {declaration}")),
    );
    differences
}

/// Assert both walks agree on `source` and both declare every `expected`
/// prefix, so a shared mistake cannot pass as parity.
fn assert_walks_declare(source: &str, expected: &[&str]) {
    assert_eq!(declaration_differences(source), Vec::<String>::new());
    let (seed, _) = walk_declarations(source);
    for prefix in expected {
        assert!(
            seed.iter()
                .any(|declaration| declaration.starts_with(prefix)),
            "missing `{prefix}` in {seed:#?}"
        );
    }
}

/// The core namespaces every project knows before its own files are indexed.
fn core_namespaces() -> HashSet<FullyQualifiedName> {
    [
        "BasicObject",
        "Object",
        "Module",
        "Class",
        "Kernel",
        "Comparable",
        "Enumerable",
    ]
    .into_iter()
    .map(|name| {
        FullyQualifiedName::namespace(vec![RubyConstant::new(name).expect("core constant")])
    })
    .collect()
}

fn declarations(analysis: &FileAnalysis) -> BTreeSet<String> {
    let mut declarations = BTreeSet::new();
    declarations.extend(
        analysis
            .symbols
            .iter()
            .map(|fact| format!("symbol {} {:?} {:?}", fact.fqn, fact.kind, fact.name_range)),
    );
    declarations.extend(analysis.methods.iter().map(|fact| {
        format!(
            "method {} on {} {:?} {:?} {:?}",
            fact.fqn,
            fact.owner,
            fact.owner.namespace_kind(),
            fact.visibility,
            fact.name_range
        )
    }));
    declarations.extend(analysis.method_visibility_overrides.iter().map(|fact| {
        format!(
            "visibility {} {:?} {} {:?}",
            fact.owner,
            fact.owner.namespace_kind(),
            fact.method,
            fact.visibility
        )
    }));
    declarations.extend(
        analysis
            .types
            .iter()
            .filter_map(|fact| match &fact.subject {
                crate::core::TypeSubject::Constant(fqn) => Some(format!("constant type {fqn}")),
                _ => None,
            }),
    );
    declarations.extend(
        analysis
            .graph_nodes
            .iter()
            .map(|fact| format!("node {} {:?}", fact.fqn, fact.kind)),
    );
    declarations.extend(analysis.graph_edges.iter().map(|fact| {
        format!(
            "edge {} {:?} {} {:?}",
            fact.source, fact.kind, fact.target, fact.provenance
        )
    }));
    declarations.extend(analysis.unresolved_graph_edges.iter().map(|fact| {
        let target = fact
            .target_parts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("::");
        format!(
            "unresolved {} {:?} {}{target} in {} {:?}",
            fact.source,
            fact.kind,
            if fact.absolute { "::" } else { "" },
            fact.context,
            fact.provenance
        )
    }));
    declarations
}

#[test]
fn visibility_call_in_ordinary_block_applies_to_lexical_owner() {
    // A block that its method yields keeps the class body's `self`, so the
    // `def` and the `protected` call both act on the enclosing class.
    let source =
        "class Holder\n  configure do\n    def helper; end\n    protected :helper\n  end\nend\n";
    assert_walks_declare(
        source,
        &["visibility Holder Some(Instance) helper Protected"],
    );
}
