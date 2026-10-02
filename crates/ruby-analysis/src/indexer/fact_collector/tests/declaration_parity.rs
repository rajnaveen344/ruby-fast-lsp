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

/// Both walks' declarations for `source`, seed first. Both start from the
/// core namespaces, and the collector also sees every namespace the seed
/// declares in the file, as the loader wires it once the seed has run.
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
    collector.extend_direct_known_namespaces(seed.graph_nodes.iter().map(|fact| fact.fqn.clone()));
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

#[test]
fn multi_write_constant_targets_follow_lexical_paths() {
    // `Inner::DEPTH` finds `Inner` lexically, `::TOP` is top level, and
    // `self::LEFT` is the current module. A splat target collects the rest
    // into an array, so it takes no positional element type.
    let source = "module Shapes\n  module Inner; end\n  WIDTH, HEIGHT = 1, 2\n  \
                  Inner::DEPTH, ::TOP, self::LEFT = 3, 4, 5\n  FIRST, *REST = [1, 2, 3]\nend\n";
    assert_walks_declare(
        source,
        &[
            "symbol Shapes::WIDTH Constant",
            "symbol Shapes::HEIGHT Constant",
            "symbol Shapes::Inner::DEPTH Constant",
            "symbol TOP Constant",
            "symbol Shapes::LEFT Constant",
            "symbol Shapes::FIRST Constant",
            "symbol Shapes::REST Constant",
            "constant type Shapes::Inner::DEPTH",
        ],
    );
}

#[test]
fn constant_path_write_finds_parent_lexically() {
    // `Kernel::LIMIT = 1` inside `Shapes` writes `::Kernel::LIMIT` when no
    // `Shapes::Kernel` exists, since `Kernel` resolves through lexical scope.
    let source = "module Shapes\n  Kernel::LIMIT = 1\nend\n";
    assert_walks_declare(source, &["symbol Kernel::LIMIT Constant"]);
}

#[test]
fn nested_defs_land_on_the_lexical_definition_owner() {
    // A `def` inside a method body defines on the class the outer method was
    // written in, whatever the outer method's receiver; `def self.name` needs
    // `self` to be a class object, which it is in a singleton method body.
    let source = "module Shapes\n  class Other; end\n  class Base\n    def self.build\n      \
                  def area; end\n    end\n    class << self\n      def plain\n        \
                  def self.scaled; end\n        def nested; end\n      end\n    end\n    \
                  def Other.make\n      def made; end\n    end\n  end\nend\n";
    assert_walks_declare(
        source,
        &[
            "method Shapes::Base#area on Shapes::Base Some(Instance)",
            "method Shapes::Base#scaled on #<Class:Shapes::Base> Some(Singleton)",
            "method Shapes::Base#nested on #<Class:Shapes::Base> Some(Singleton)",
            "method Shapes::Base#made on Shapes::Base Some(Instance)",
        ],
    );
}

#[test]
fn self_receiver_def_needs_a_class_object_self() {
    // `self` is the singleton class in a `class << self` body and an instance
    // in an instance method, so neither `def self.name` defines a singleton
    // method of the enclosing class.
    let source = "module Shapes\n  class Base\n    class << self\n      def self.meta; end\n    \
                  end\n    def outer\n      def self.solo; end\n    end\n  end\nend\n";
    assert_walks_declare(source, &["method Shapes::Base#outer on Shapes::Base"]);
    let (seed, _) = walk_declarations(source);
    assert!(
        !seed
            .iter()
            .any(|declaration| declaration.contains("#meta") || declaration.contains("#solo")),
        "{seed:#?}"
    );
}

#[test]
fn class_body_edges_bind_namespaces_declared_so_far() {
    // Ruby evaluates a superclass and a mixin argument when the class body
    // runs, so `Error` binds the `Shapes::Error` declared above it, not the
    // `Shapes::Inner::Error` declared below it. `Later` is not yet defined,
    // so the include stays an unresolved lexical reference.
    let source = "module Shapes\n  class Error < StandardError; end\n  module Inner\n    \
                  class Err < Error\n      include Later\n    end\n    \
                  class Error < StandardError; end\n    module Later; end\n  end\nend\n";
    assert_walks_declare(
        source,
        &[
            "edge Shapes::Inner::Err Superclass Shapes::Error",
            "unresolved Shapes::Inner::Err Include Later in Shapes::Inner::Err",
        ],
    );
}

#[test]
fn declaration_calls_in_blocks_act_on_the_definition_owner() {
    // A receiverless declaration call in an ordinary block lands where a
    // `def` in that block lands. In an include hook that is the hook's
    // module, which models `klass.class_eval { include Sized }` as giving
    // every includer `Sized`, as the hook does at run time.
    let source = "module Shapes\n  module Sized; end\n  module Marker\n    \
                  def self.append_features(klass)\n      super\n      \
                  klass.class_eval do\n        include(Sized)\n      end\n    end\n  \
                  end\n  class Holder\n    configure do\n      attr_reader :size\n    \
                  end\n  end\nend\n";
    assert_walks_declare(
        source,
        &[
            "edge Shapes::Marker Include Shapes::Sized",
            "method Shapes::Holder#size on Shapes::Holder Some(Instance)",
        ],
    );
}

#[test]
fn dynamic_definitions_need_a_class_object_self() {
    // `define_method` and `define_singleton_method` are sent to `self`. In a
    // singleton method body `self` is the class, so both define on it. In
    // `initialize`, which the index records as `new`, `self` is still an
    // instance, and an eval block on an unknown receiver proves nothing, so
    // neither `hidden` nor `solo` lands on the class.
    let source = "module Shapes\n  class Base\n    def self.build\n      \
                  define_singleton_method(:made) { 1 }\n      define_method(:shaped) { 1 }\n    \
                  end\n    def initialize(target)\n      target.singleton_class.class_eval do\n        \
                  private\n        define_method(:hidden) { 1 }\n      end\n      \
                  define_singleton_method(:solo) { 1 }\n    end\n  end\nend\n";
    assert_walks_declare(
        source,
        &[
            "method Shapes::Base#made on #<Class:Shapes::Base> Some(Singleton)",
            "method Shapes::Base#shaped on Shapes::Base Some(Instance)",
            "method Shapes::Base#new on #<Class:Shapes::Base>",
        ],
    );
    let (seed, _) = walk_declarations(source);
    assert!(
        !seed
            .iter()
            .any(|declaration| declaration.contains("#hidden") || declaration.contains("#solo")),
        "{seed:#?}"
    );
}

#[test]
fn extend_edges_do_not_join_the_instance_ancestry() {
    // `extend` adds a module to the singleton class's ancestors, not the
    // receiver's own, so `extend self` and a module that extends one of its
    // includers form no inheritance cycle.
    let source = "module Shapes\n  module Tools\n    extend self\n  end\n  module Sized\n    \
                  extend Tools\n  end\n  module Tools\n    include Sized\n  end\nend\n";
    assert_walks_declare(
        source,
        &[
            "edge Shapes::Tools Extend Shapes::Tools",
            "edge Shapes::Sized Extend Shapes::Tools",
            "edge Shapes::Tools Include Shapes::Sized",
        ],
    );
}
