//! Inferred types at a point.

use crate::invariant::ExpectInvariant;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::core::{
    FullyQualifiedName, NamespaceKind, RubyMethod, RubyType, TypeResolution, TypeSubject,
};
use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use ruby_prism::{DefNode, Visit};
use tower_lsp::lsp_types::Url;

use crate::server::Server;
use crate::test::harness::fixture::Tag;
use crate::utils::lsp::source_position;

/// What the identifier at a `<type>` point denotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypeKind {
    Return,
    Var,
    Const,
}

impl TypeKind {
    fn parse(kind: &str) -> Self {
        match kind {
            "return" | "->" => Self::Return,
            "var" | ":" => Self::Var,
            "const" => Self::Const,
            other => panic!("<type kind=\"{other}\"> is unknown; use return, var, or const"),
        }
    }
}

/// `<type label="T" kind="...">`: the type inferred for the identifier at the
/// point displays exactly as `T`. `kind` additionally asserts what the
/// identifier is.
pub(super) fn check_types(server: &Server, uri: &Url, content: &str, tags: &[&Tag]) {
    let document = server
        .documents
        .read()
        .get(uri)
        .map(|document| document.read().clone())
        .unwrap_or_else(|| panic!("<type> fixture {uri} is not open"));
    let engine_handle = server.project_for_uri(uri);
    let engine = engine_handle.test_read();

    for tag in tags {
        let expected = tag.attr("label").expect("<type> requires `label`");
        let position = tag.range.start;
        let byte_offset = document.position_to_analysis_offset(source_position(position));
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier, ..) = analyzer.get_identifier_at_position(source_position(position));
        let identifier =
            identifier.unwrap_or_else(|| panic!("<type> at {position:?} is not on an identifier"));

        let (actual_kind, inferred) = match &identifier {
            Identifier::RubyLocalVariable { name, .. } => {
                let scope = document
                    .find_scope_for_variable_at(name, source_position(position))
                    .or_else(|| document.scope_at_position(source_position(position)));
                let inferred = scope.and_then(|scope| {
                    let scope = u32::try_from(scope).expect_invariant(
                        "local variable scope id exceeded u32",
                        "TypeSubject::Local stores u32 scope ids",
                        "widen TypeSubject::Local scope_id",
                    );
                    engine.view().local_variable_type_at(
                        name,
                        scope,
                        document.analysis_file_id(),
                        byte_offset,
                    )
                });
                (TypeKind::Var, inferred)
            }
            Identifier::RubyConstant { iden, .. } => {
                let constant = FullyQualifiedName::constant(iden.clone());
                let inferred = match engine.view().type_at(
                    &TypeSubject::Constant(constant),
                    document.analysis_file_id(),
                    byte_offset,
                ) {
                    TypeResolution::Resolved(fact) => Some(fact.ruby_type),
                    TypeResolution::Ambiguous(_) | TypeResolution::Unresolved => None,
                }
                .unwrap_or_else(|| RubyType::Class(FullyQualifiedName::namespace(iden.clone())));
                (TypeKind::Const, Some(inferred))
            }
            Identifier::RubyMethod {
                iden,
                receiver,
                namespace,
            } => {
                let query = engine.view();
                let method = RubyMethod::new(&iden.to_string()).ok();
                let inferred = if is_def_name_at(content, byte_offset as usize) {
                    method.and_then(|method| {
                        let fqn = FullyQualifiedName::method(namespace.clone(), method);
                        let returns: Vec<RubyType> = query
                            .method_facts_for(&fqn)
                            .iter()
                            .filter_map(|fact| query.method_return_type(fact))
                            .collect();
                        (!returns.is_empty()).then(|| RubyType::union(returns))
                    })
                } else {
                    let receiver_namespace = match receiver {
                        MethodReceiver::None
                        | MethodReceiver::SelfReceiver
                        | MethodReceiver::Super => {
                            let class = if namespace.is_empty() {
                                RubyType::class("Object")
                            } else {
                                RubyType::Class(FullyQualifiedName::from(namespace.clone()))
                            };
                            let RubyType::Class(fqn) = class else {
                                unreachable!("receiver namespace is built as a class type")
                            };
                            Some((fqn, NamespaceKind::Instance))
                        }
                        MethodReceiver::Constant(path) => Some((
                            FullyQualifiedName::constant(path.clone()),
                            NamespaceKind::Singleton,
                        )),
                        _ => None,
                    };
                    receiver_namespace
                        .zip(method)
                        .and_then(|((fqn, kind), method)| {
                            let receiver = FullyQualifiedName::namespace_with_kind(
                                fqn.namespace_parts(),
                                kind,
                            );
                            let request = ruby_analysis::engine::lookup::MethodRequest::new(
                                ruby_analysis::engine::lookup::LookupReceiver::Namespace(&receiver),
                                method,
                                ruby_analysis::engine::lookup::MethodWant::Return,
                            );
                            ruby_analysis::engine::lookup::method(&query, request)
                                .into_return_type()
                        })
                };
                (TypeKind::Return, inferred)
            }
            Identifier::RubyInstanceVariable { name, .. } => (
                TypeKind::Var,
                latest_variable_type(
                    &engine,
                    |subject| matches!(subject, TypeSubject::InstanceVariable { name: n, .. } if n == name),
                ),
            ),
            Identifier::RubyClassVariable { name, .. } => (
                TypeKind::Var,
                latest_variable_type(
                    &engine,
                    |subject| matches!(subject, TypeSubject::ClassVariable { name: n, .. } if n == name),
                ),
            ),
            Identifier::RubyGlobalVariable { name, .. } => (
                TypeKind::Var,
                latest_variable_type(
                    &engine,
                    |subject| matches!(subject, TypeSubject::GlobalVariable(n) if n == name),
                ),
            ),
            other => panic!("<type> at {position:?} is on unsupported identifier {other:?}"),
        };

        if let Some(kind) = tag.attr("kind") {
            assert_eq!(
                TypeKind::parse(kind),
                actual_kind,
                "<type> at {position:?} expected a {kind} identifier"
            );
        }
        let actual = inferred
            .unwrap_or_else(|| panic!("no type inferred at {position:?}; expected {expected}"))
            .to_string();
        assert_eq!(
            actual, expected,
            "type mismatch at {position:?} ({actual_kind:?})"
        );
    }
}

fn latest_variable_type(
    engine: &ruby_analysis::engine::Project,
    subject_matches: impl Fn(&TypeSubject) -> bool,
) -> Option<RubyType> {
    engine
        .view()
        .all_type_facts()
        .into_iter()
        .filter(|fact| fact.ruby_type != RubyType::Unknown && subject_matches(&fact.subject))
        .next_back()
        .map(|fact| fact.ruby_type)
}

/// Whether `byte_offset` is on the name of a method definition.
fn is_def_name_at(content: &str, byte_offset: usize) -> bool {
    struct Finder {
        offset: usize,
        found: bool,
    }
    impl<'pr> Visit<'pr> for Finder {
        fn visit_def_node(&mut self, node: &DefNode<'pr>) {
            let name = node.name_loc();
            if name.start_offset() <= self.offset && self.offset <= name.end_offset() {
                self.found = true;
            }
            ruby_prism::visit_def_node(self, node);
        }
    }
    let parse = ruby_prism::parse(content.as_bytes());
    let mut finder = Finder {
        offset: byte_offset,
        found: false,
    };
    finder.visit(&parse.node());
    finder.found
}
