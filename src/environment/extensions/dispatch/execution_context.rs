use std::collections::BTreeMap;

use ruby_analysis::core::{
    ExecutionContextFact, ExecutionScopeMode, FullyQualifiedName, GeneratedOwnerId, GraphEdgeKind,
    GraphNodeFact, GraphNodeKind, NamespaceKind, RubyConstant,
};
use ruby_analysis::indexer::fact_collector::{BlockExecutionContext, FactCollector};
use ruby_fast_lsp_extension_api::{
    BlockExecutionContextPatch, ExecutionContextTarget, GeneratedOwnerScope,
};

use crate::environment::extensions::patches::conflicts::namespace_kind_from_abi;
use crate::environment::extensions::responses::range_from_abi;

pub(super) fn apply_execution_context(
    visitor: &mut FactCollector,
    context: BlockExecutionContextPatch,
) {
    let source_identity = visitor.document().uri.as_str();
    let project_identity = visitor
        .extension_project_context()
        .map(|project| project.project_uri.as_str());
    let range =
        crate::utils::lsp::text_range(visitor.document(), range_from_abi(context.block_range));
    let mut owners = BTreeMap::new();

    for owner in &context.generated_owners {
        let identity = generated_owner_scope_identity(
            owner.scope,
            source_identity,
            project_identity,
            "execution-context owner",
        );
        let generated = GeneratedOwnerId::new(
            &context.source.extension_id,
            identity,
            &owner.local_id,
        )
        .expect(
            "INVARIANT VIOLATED: invalid generated owner reached extension context application. This is a bug because execution contexts must be validated before fact conversion. Fix: keep validation before apply_execution_context.",
        );
        let namespace = vec![RubyConstant::generated_owner(generated)];
        let previous = owners.insert(
            (owner.scope, owner.local_id.clone()),
            (namespace, namespace_kind_from_abi(owner.owner_kind)),
        );
        assert!(
            previous.is_none(),
            "INVARIANT VIOLATED: duplicate generated owner reached extension context application. This is a bug because duplicate local identities must be rejected at the extension boundary. Fix: keep owner uniqueness validation before fact conversion."
        );
    }

    for owner in &context.generated_owners {
        let (namespace, owner_kind) = owners.get(&(owner.scope, owner.local_id.clone())).expect(
            "INVARIANT VIOLATED: validated generated owner is absent during context application. This is a bug because the owner map is built from the same context. Fix: keep context conversion atomic.",
        );
        let instance_fqn = FullyQualifiedName::namespace(namespace.clone());
        let graph_kind = match owner.declaration_kind {
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class => GraphNodeKind::Class,
            ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module => GraphNodeKind::Module,
        };
        let singleton_fqn = instance_fqn.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: generated owner could not convert to a singleton namespace. This is a bug because generated owners are namespace segments. Fix: construct generated owner graph nodes through FullyQualifiedName::namespace.",
        );
        for node in [
            GraphNodeFact::new(instance_fqn.clone(), graph_kind, range),
            GraphNodeFact::new(singleton_fqn, graph_kind, range),
        ] {
            if !visitor.direct_facts().graph_nodes.contains(&node) {
                visitor.add_graph_node_fact(node);
            }
        }
        if let Some(parent) = &owner.parent {
            let (parent_namespace, parent_kind) = resolve_execution_context_target(parent, &owners);
            let source = FullyQualifiedName::namespace_with_kind(namespace.clone(), *owner_kind);
            let target = FullyQualifiedName::namespace_with_kind(parent_namespace, parent_kind);
            visitor.direct_push_resolved_edge(source, target, GraphEdgeKind::Superclass, range);
        }
    }

    let (implicit_receiver, implicit_receiver_kind) =
        resolve_execution_context_target(&context.implicit_receiver, &owners);
    let (method_definition_owner, method_definition_kind) =
        resolve_execution_context_target(&context.method_definition_owner, &owners);
    let implicit_receiver_fqn =
        FullyQualifiedName::namespace_with_kind(implicit_receiver.clone(), implicit_receiver_kind);
    let method_definition_owner_fqn = FullyQualifiedName::namespace_with_kind(
        method_definition_owner.clone(),
        method_definition_kind,
    );
    visitor.add_execution_context_fact(ExecutionContextFact {
        range,
        lexical_namespace: FullyQualifiedName::namespace(visitor.scope_tracker().get_ns_stack()),
        implicit_receiver: implicit_receiver_fqn,
        method_definition_owner: method_definition_owner_fqn,
        lexical_scope: ExecutionScopeMode::Preserve,
        local_scope: ExecutionScopeMode::Preserve,
        extension_id: context.source.extension_id,
    });
    visitor.set_pending_block_execution_context(BlockExecutionContext {
        block_range: range,
        implicit_receiver,
        implicit_receiver_kind,
        method_definition_owner,
        method_definition_kind,
    });
}

fn resolve_execution_context_target(
    target: &ExecutionContextTarget,
    owners: &BTreeMap<(GeneratedOwnerScope, String), (Vec<RubyConstant>, NamespaceKind)>,
) -> (Vec<RubyConstant>, NamespaceKind) {
    match target {
        ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        } => (
            extension_ruby_constants(namespace, "execution context namespace target"),
            namespace_kind_from_abi(*owner_kind),
        ),
        ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        } => {
            let (namespace, declared_kind) = owners
                .get(&(GeneratedOwnerScope::Source, local_id.clone()))
                .cloned()
                .expect(
                "INVARIANT VIOLATED: undeclared generated target reached context application. This is a bug because every context target must be validated before conversion. Fix: reject undeclared local IDs at the extension boundary.",
            );
            (
                namespace,
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or(declared_kind),
            )
        }
        ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        } => {
            let (namespace, declared_kind) = owners
                .get(&(GeneratedOwnerScope::Project, local_id.clone()))
                .cloned()
                .expect(
                    "INVARIANT VIOLATED: undeclared project-generated target reached context application. This is a bug because every context target must be validated before conversion. Fix: declare the project-scoped owner in the same execution context.",
                );
            (
                namespace,
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or(declared_kind),
            )
        }
    }
}

pub(super) fn generated_owner_scope_identity<'a>(
    scope: GeneratedOwnerScope,
    source_identity: &'a str,
    project_identity: Option<&'a str>,
    label: &str,
) -> &'a str {
    match scope {
        GeneratedOwnerScope::Source => source_identity,
        GeneratedOwnerScope::Project => project_identity.unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: {label} requested project-scoped identity without an owning project. This is a host validation bug because project-generated owners require ProjectContext. Fix: reject the patch before semantic application."
            )
        }),
    }
}

pub(super) fn extension_ruby_constants(parts: &[String], label: &str) -> Vec<RubyConstant> {
    parts
        .iter()
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: validated {label} component `{part}` failed fact conversion: {err}. This is a bug because validation and conversion use the same RubyConstant contract. Fix: keep extension context validation before application."
                )
            })
        })
        .collect()
}
