use super::*;
use crate::core::{
    GraphEdgeKind, GraphNodeKind, MethodParamKind, RubyMethod, SymbolKind, TypeSubject,
};
use crate::invariant::ExpectInvariant;
use std::collections::HashMap;

fn file() -> SourceFileId {
    SourceFileId(1)
}

#[test]
fn indexes_class_module_method_and_mixin_facts() {
    let index = AnalysisIndexer::new(file()).index_source(
            "module Auth\nend\nclass User\n  include Auth\n  def name\n  end\n  def self.find\n  end\nend\n",
        );

    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    assert!(index
        .graph_nodes
        .iter()
        .any(|fact| fact.fqn == user && fact.kind == GraphNodeKind::Class));
    assert!(index.graph_edges.iter().any(|fact| fact.source == user
        && fact.target == auth
        && fact.kind == GraphEdgeKind::Include));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#name"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#find"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
    }));
}

#[test]
fn indexes_method_param_names() {
    let index = AnalysisIndexer::new(file()).index_source(
            "class User\n  def find(id, name = nil, *rest, tail, active:, role: nil, **opts, &block)\n  end\nend\n",
        );

    let method = index
        .methods
        .iter()
        .find(|fact| fact.fqn.to_string() == "User#find")
        .expect_invariant(
            "analysis indexer did not emit User#find",
            "def nodes must produce method facts",
            "keep visit_def_node method fact emission active",
        );
    assert_eq!(
        method.param_names().collect::<Vec<_>>(),
        ["id", "name", "rest", "tail", "active", "role", "opts", "block"]
    );
    let kinds = method
        .param_facts
        .iter()
        .map(|param| param.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            MethodParamKind::Required,
            MethodParamKind::Optional,
            MethodParamKind::Rest,
            MethodParamKind::Required,
            MethodParamKind::RequiredKeyword,
            MethodParamKind::OptionalKeyword,
            MethodParamKind::KeywordRest,
            MethodParamKind::Block,
        ]
    );
}

#[test]
fn indexes_singleton_class_attr_and_module_function_methods() {
    let index = AnalysisIndexer::new(file()).index_source(
            "module Utils\n  def helper\n  end\n  module_function :helper\nend\nclass User\n  attr_accessor :name\n  class << self\n    attr_reader :count\n    def build\n    end\n  end\nend\n",
        );

    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "Utils#helper"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#name"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#name="
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#count"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "User#build"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
    }));
}

#[test]
fn indexes_bare_module_function_following_methods() {
    let index = AnalysisIndexer::new(file())
        .index_source("module Utils\n  module_function\n  def helper\n  end\nend\n");

    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "Utils#helper"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Instance)
    }));
    assert!(index.methods.iter().any(|fact| {
        fact.fqn.to_string() == "Utils#helper"
            && fact.owner.namespace_kind() == Some(crate::core::NamespaceKind::Singleton)
    }));
}

#[test]
fn indexes_class_attribute_methods() {
    let index = AnalysisIndexer::new(file())
        .index_source("class Worker\n  class_attribute :queue_config\nend\n");

    for kind in [
        crate::core::NamespaceKind::Instance,
        crate::core::NamespaceKind::Singleton,
    ] {
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "Worker#queue_config"
                && fact.owner.namespace_kind() == Some(kind)
        }));
        assert!(index.methods.iter().any(|fact| {
            fact.fqn.to_string() == "Worker#queue_config="
                && fact.owner.namespace_kind() == Some(kind)
        }));
    }
}

#[test]
fn indexes_variable_write_symbol_facts() {
    let index = AnalysisIndexer::new(file())
        .index_source("name = 1\n@name = name\n@@count = 1\n$debug = true\n");

    assert!(index
        .symbols
        .iter()
        .any(|fact| { fact.fqn.to_string() == "name" && fact.kind == SymbolKind::LocalVariable }));
    assert!(index.symbols.iter().any(|fact| {
        fact.fqn.to_string() == "@name" && fact.kind == SymbolKind::InstanceVariable
    }));
    assert!(index.symbols.iter().any(|fact| {
        fact.fqn.to_string() == "@@count" && fact.kind == SymbolKind::ClassVariable
    }));
    assert!(index.symbols.iter().any(|fact| {
        fact.fqn.to_string() == "$debug" && fact.kind == SymbolKind::GlobalVariable
    }));
}

#[test]
fn indexes_literal_assignment_type_facts() {
    let index = AnalysisIndexer::new(file())
        .index_source("A = 1\nname = \"Ada\"\n@active = true\n@@count = 1\n$debug = false\n");

    assert!(index.types.iter().any(|fact| {
        fact.subject
            == TypeSubject::Constant(FullyQualifiedName::constant(vec![
                RubyConstant::new("A").unwrap()
            ]))
            && fact.ruby_type == RubyType::integer()
    }));
    assert!(index.types.iter().any(|fact| {
        fact.subject
            == TypeSubject::Local {
                scope_id: 0,
                name: "name".to_string(),
            }
            && fact.ruby_type == RubyType::string()
    }));
    assert!(index.types.iter().any(|fact| {
        matches!(
            &fact.subject,
            TypeSubject::InstanceVariable { name, .. } if name == "@active"
        ) && fact.ruby_type == RubyType::true_class()
    }));
    assert!(index.types.iter().any(|fact| {
        matches!(
            &fact.subject,
            TypeSubject::ClassVariable { name, .. } if name == "@@count"
        ) && fact.ruby_type == RubyType::integer()
    }));
    assert!(index.types.iter().any(|fact| {
        fact.subject == TypeSubject::GlobalVariable("$debug".to_string())
            && fact.ruby_type == RubyType::false_class()
    }));
}

#[test]
fn seeds_regexp_constant_types_before_body_collection() {
    for source in [
        "PATTERN = /entry/\n",
        "PATTERN = /entry#{1}/\n",
        "PATTERN = /entry/.freeze\n",
    ] {
        let index = AnalysisIndexer::new(file()).index_source(source);
        let actual = index
                .types
                .iter()
                .filter(|fact| matches!(&fact.subject, TypeSubject::Constant(fqn) if fqn.to_string() == "PATTERN"))
                .map(|fact| fact.ruby_type.clone())
                .collect::<Vec<_>>();
        assert_eq!(
                actual,
                vec![RubyType::Class(FullyQualifiedName::try_from("Regexp").unwrap())],
                "Regexp literal declarations must seed their value type before earlier source calls are collected: {source}"
            );
    }
}

#[test]
fn unresolved_collection_members_do_not_publish_partial_types() {
    let index = AnalysisIndexer::new(file())
        .index_source("values = [1, dynamic_value]\nmapping = {known: 1, **dynamic_hash}\n");

    assert!(index.types.iter().any(|fact| {
        fact.subject
            == TypeSubject::Local {
                scope_id: 0,
                name: "values".to_string(),
            }
            && fact.ruby_type == RubyType::Array(vec![RubyType::Unknown])
    }));
    assert!(index.types.iter().any(|fact| {
        fact.subject
            == TypeSubject::Local {
                scope_id: 0,
                name: "mapping".to_string(),
            }
            && fact.ruby_type == RubyType::Hash(vec![RubyType::Unknown], vec![RubyType::Unknown])
    }));
}

#[test]
fn indexes_namespace_constant_type_facts() {
    let index = AnalysisIndexer::new(file()).index_source("module Auth\nend\nclass User\nend\n");

    let auth = FullyQualifiedName::constant(vec![RubyConstant::new("Auth").unwrap()]);
    let user = FullyQualifiedName::constant(vec![RubyConstant::new("User").unwrap()]);
    assert!(index.types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(auth.clone())
            && matches!(fact.ruby_type, RubyType::ModuleReference(_))
    }));
    assert!(index.types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(user.clone())
            && matches!(fact.ruby_type, RubyType::ClassReference(_))
    }));
}

#[test]
fn indexes_constant_object_assignment_type_fact() {
    let index = AnalysisIndexer::new(file()).index_source("class User\nend\nMODEL = User\n");

    let model = FullyQualifiedName::constant(vec![RubyConstant::new("MODEL").unwrap()]);
    assert!(index.types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(model.clone())
            && fact.ruby_type
                == RubyType::ClassReference(FullyQualifiedName::constant(vec![RubyConstant::new(
                    "User",
                )
                .unwrap()]))
    }));
}

#[test]
fn unresolved_constant_alias_keeps_an_unknown_equation_target() {
    let index = AnalysisIndexer::new(file()).index_source("CHOICE = RemoteValues::ITEM\n");
    let subject = TypeSubject::Constant(FullyQualifiedName::constant(vec![RubyConstant::new(
        "CHOICE",
    )
    .unwrap()]));
    let facts = index
        .types
        .iter()
        .filter(|fact| fact.subject == subject)
        .collect::<Vec<_>>();
    assert_eq!(
        facts.len(),
        1,
        "a deferred constant needs exactly one declaration target"
    );
    assert_eq!(
        facts[0].ruby_type,
        RubyType::Unknown,
        "unresolved constant syntax is not proof of a class object"
    );
    assert_eq!(facts[0].range, TextRange::new(file(), 0, 6));
}

#[test]
fn class_reopening_through_a_constant_alias_keeps_the_original_owner_identity() {
    let index = AnalysisIndexer::new(file()).index_source(
        "module Types\n\
             \x20 class Original\n\
             \x20 end\n\
             \x20 Alias = Original\n\
             \x20 class Alias\n\
             \x20   def from_alias\n\
             \x20   end\n\
             \x20 end\n\
             end\n",
    );

    let types = RubyConstant::new("Types").unwrap();
    let original = RubyConstant::new("Original").unwrap();
    let alias = RubyConstant::new("Alias").unwrap();
    let expected_method = FullyQualifiedName::method(
        vec![types.clone(), original.clone()],
        RubyMethod::new("from_alias").unwrap(),
    );
    let shadow_class = FullyQualifiedName::namespace(vec![types, alias]);

    assert!(
        index.methods.iter().any(|fact| fact.fqn == expected_method),
        "methods declared through a class-object alias must retain the original class identity"
    );
    assert!(
        !index
            .graph_nodes
            .iter()
            .any(|fact| fact.fqn == shadow_class),
        "a class-object alias must not become a second graph class"
    );
}

#[test]
fn class_reopening_through_a_known_cross_file_alias_keeps_the_original_owner_identity() {
    let types = RubyConstant::new("Types").unwrap();
    let original = RubyConstant::new("Original").unwrap();
    let alias = RubyConstant::new("Alias").unwrap();
    let original_fqn = FullyQualifiedName::namespace(vec![types.clone(), original.clone()]);
    let alias_fqn = FullyQualifiedName::constant(vec![types.clone(), alias.clone()]);
    let known_namespaces = HashSet::from([
        original_fqn.clone(),
        original_fqn.to_singleton_namespace().unwrap(),
    ]);
    let known_constant_types = HashMap::from([(alias_fqn, RubyType::ClassReference(original_fqn))]);

    let index =
        AnalysisIndexer::with_known_semantics(file(), &(known_namespaces, known_constant_types))
            .index_source(
                "module Types\n\
             \x20 class Alias\n\
             \x20   def from_other_file\n\
             \x20   end\n\
             \x20 end\n\
             end\n",
            );

    let expected_method = FullyQualifiedName::method(
        vec![types.clone(), original],
        RubyMethod::new("from_other_file").unwrap(),
    );
    let shadow_class = FullyQualifiedName::namespace(vec![types, alias]);
    assert!(index.methods.iter().any(|fact| fact.fqn == expected_method));
    assert!(!index
        .graph_nodes
        .iter()
        .any(|fact| fact.fqn == shadow_class));
}

#[test]
fn nested_class_declaration_does_not_reopen_a_same_named_lexical_ancestor() {
    let io_streams = RubyConstant::new("IOStreams").unwrap();
    let writer = RubyConstant::new("Writer").unwrap();
    let outer_writer = FullyQualifiedName::namespace(vec![io_streams.clone(), writer.clone()]);
    let outer_writer_constant =
        FullyQualifiedName::constant(vec![io_streams.clone(), writer.clone()]);
    let known_namespaces = HashSet::from([
        FullyQualifiedName::namespace(vec![io_streams.clone()]),
        outer_writer.clone(),
        outer_writer.to_singleton_namespace().unwrap(),
    ]);
    let known_constant_types = HashMap::from([(
        outer_writer_constant,
        RubyType::ClassReference(outer_writer.clone()),
    )]);

    let index =
        AnalysisIndexer::with_known_semantics(file(), &(known_namespaces, known_constant_types))
            .index_source(
                "module IOStreams\n\
                     \x20 module Gzip\n\
                     \x20   class Writer < IOStreams::Writer\n\
                     \x20     def output_stream\n\
                     \x20       super\n\
                     \x20     end\n\
                     \x20   end\n\
                     \x20 end\n\
                     end\n",
            );

    let nested_writer =
        FullyQualifiedName::namespace(vec![io_streams, RubyConstant::new("Gzip").unwrap(), writer]);
    assert!(
            index
                .graph_nodes
                .iter()
                .any(|fact| fact.fqn == nested_writer),
            "a simple class declaration must define its exact lexical child even when an outer constant has the same name"
        );
    assert!(index.graph_edges.iter().any(|fact| {
        fact.source == nested_writer
            && fact.target == outer_writer
            && fact.kind == GraphEdgeKind::Superclass
    }));
    assert!(
        index
            .graph_edges
            .iter()
            .all(|fact| fact.source != fact.target),
        "declaration lookup must never collapse a subclass onto its superclass"
    );
}

#[test]
fn module_reopening_through_a_constant_alias_keeps_the_original_owner_identity() {
    let index = AnalysisIndexer::new(file()).index_source(
        "module Original\n\
             end\n\
             Alias = Original\n\
             module Alias\n\
             \x20 def from_alias\n\
             \x20 end\n\
             end\n",
    );
    let original = RubyConstant::new("Original").unwrap();
    let alias = RubyConstant::new("Alias").unwrap();
    let expected_method =
        FullyQualifiedName::method(vec![original], RubyMethod::new("from_alias").unwrap());
    let shadow_module = FullyQualifiedName::namespace(vec![alias]);

    assert!(index.methods.iter().any(|fact| fact.fqn == expected_method));
    assert!(!index
        .graph_nodes
        .iter()
        .any(|fact| fact.fqn == shadow_module));
}

#[test]
fn indexes_constructor_assignment_type_fact() {
    let index = AnalysisIndexer::new(file()).index_source("class User\nend\n@user = User.new\n");

    assert!(index.types.iter().any(|fact| {
        matches!(
            &fact.subject,
            TypeSubject::InstanceVariable { name, .. } if name == "@user"
        ) && fact.ruby_type
            == RubyType::Class(FullyQualifiedName::constant(vec![RubyConstant::new(
                "User",
            )
            .unwrap()]))
    }));
}

#[test]
fn indexes_exact_constant_declaration_name_ranges() {
    let index =
        AnalysisIndexer::new(file()).index_source("class Outer::User\nend\nOuter::LIMIT = 1\n");

    let user = index
        .symbols
        .iter()
        .find(|fact| fact.fqn.to_string() == "Outer::User")
        .expect("class symbol should be indexed");
    assert_eq!(
        (user.name_range.start_byte, user.name_range.end_byte),
        (13, 17)
    );

    let limit = index
        .symbols
        .iter()
        .find(|fact| fact.fqn.to_string() == "Outer::LIMIT")
        .expect("value constant should be indexed");
    assert_eq!(
        (limit.name_range.start_byte, limit.name_range.end_byte),
        (29, 34)
    );
}

#[test]
fn frozen_literal_constant_retains_receiver_type_in_direct_facts() {
    let index = AnalysisIndexer::new(file())
        .index_source("class User\n  NAMES = [\"admin\", \"guest\"].freeze\nend\n");
    let names = FullyQualifiedName::constant(vec![
        RubyConstant::new("User").unwrap(),
        RubyConstant::new("NAMES").unwrap(),
    ]);

    assert!(index.types.iter().any(|fact| {
        fact.subject == TypeSubject::Constant(names.clone())
            && matches!(fact.ruby_type, RubyType::Array(_))
    }));
}
