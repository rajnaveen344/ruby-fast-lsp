use crate::invariant::ExpectInvariant;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use log::warn;
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_extension_api::Extension;
use ruby_prism::CallNode;

use crate::environment::extensions::dispatch::apply::apply_patch;
use crate::environment::extensions::dispatch::call_context::call_context;
use crate::environment::extensions::dispatch::execution_context::apply_execution_context;
use crate::environment::extensions::loading::manifest::ExtensionProjectContextDelivery;
use crate::environment::extensions::patches::conflicts::{
    resolve_execution_context_conflicts, resolve_index_patch_conflicts,
};
use crate::environment::extensions::patches::validation::{
    index_patch_extension_id, index_patch_requires_project_context, validate_execution_contexts,
    validate_execution_contexts_for_project, validate_index_patch_payloads,
    validate_index_patch_provenance,
};
use crate::environment::extensions::registry::handle::ExtensionRegistryHandle;
use crate::environment::extensions::registry::state::ExtensionApplicabilitySnapshot;

pub(in crate::environment::extensions) fn process_call_node_with_registry(
    registry: &ExtensionRegistryHandle,
    visitor: &mut FactCollector,
    node: &CallNode,
    applicability: Option<&ExtensionApplicabilitySnapshot>,
    tracked_call_prechecked: bool,
) -> bool {
    let method_name = utils::utf8_str(node.name().as_slice());
    if !tracked_call_prechecked
        && !registry
            .inner
            .read()
            .tracked_call_names
            .contains(method_name)
    {
        return false;
    }
    if process_wasm_call_node(registry, visitor, node, applicability) {
        return true;
    }
    if registry.inner.read().has_loaded_wasm_for_call(method_name) {
        return false;
    }

    let rspec = ruby_fast_lsp_extension_rspec::extension();

    invariant!(
        rspec.abi_version() == ruby_fast_lsp_extension_api::ABI_VERSION,
        what = "extension ABI version mismatch for {}",
        why = "extension patches cannot be safely interpreted across ABI versions",
        fix = "rebuild extension against current ruby-fast-lsp-extension-api",
        rspec.id(),
    );

    if !rspec.indexed_call_names().contains(&method_name) {
        return false;
    }

    let ctx = call_context(visitor, node, true);
    let output = rspec.index_call_output(&ctx);
    validate_index_patch_provenance(rspec.id(), &output.index_patches).expect_invariant(
        "bundled native extension spoofed index patch provenance",
        "bundled and Wasm extensions must obey the same public trust contract",
        "emit the compiled extension ID in every PatchSource",
    );
    validate_index_patch_payloads(&output.index_patches).expect_invariant(
        "bundled native extension emitted an invalid index patch",
        "native adapters must use the same validated ABI as Wasm guests",
        "correct the extension payload",
    );
    invariant!(
        ctx.project.is_some()
            || !output
                .index_patches
                .iter()
                .any(index_patch_requires_project_context),
        what = "native extension emitted a project-generated owner without a ProjectContext",
        why = "project-scoped identity needs a project",
        fix = "emit source-scoped owners or require project context",
    );
    validate_execution_contexts(rspec.id(), &ctx, &output.execution_contexts).expect_invariant(
        "bundled native extension emitted an invalid execution context",
        "native adapters must use the same validated ABI as Wasm guests",
        "correct the context ranges, owners, targets, or provenance",
    );
    let handled = !output.index_patches.is_empty() || !output.execution_contexts.is_empty();
    for patch in output.index_patches {
        apply_patch(visitor, node, patch);
    }
    for context in output.execution_contexts {
        apply_execution_context(visitor, context);
    }
    handled
}

fn process_wasm_call_node(
    registry: &ExtensionRegistryHandle,
    visitor: &mut FactCollector,
    node: &CallNode,
    applicability: Option<&ExtensionApplicabilitySnapshot>,
) -> bool {
    let method_name = utils::utf8_str(node.name().as_slice());
    let project = visitor.extension_project_context();
    let extensions = registry.extensions_with_applicability(project, applicability);
    let mut emitted = Vec::new();
    let mut emitted_contexts = Vec::new();
    let mut emitters = BTreeMap::new();
    let active_frame_ids = visitor
        .enclosing_extension_calls()
        .iter()
        .flat_map(|call| call.frame_extension_ids.iter())
        .collect::<BTreeSet<_>>();
    let explicitly_switches_receiver = node
        .receiver()
        .is_some_and(|receiver| receiver.as_self_node().is_none());
    let mut shared_call_context = None;
    let mut shared_project_call_context = None;

    for (loaded, applies_to_source) in extensions {
        if !loaded.is_loaded() {
            continue;
        }
        if !loaded.handles_call(method_name) {
            continue;
        }
        if !applies_to_source {
            continue;
        }
        let owns_active_frame = active_frame_ids.contains(&loaded.metadata.id);
        if !active_frame_ids.is_empty() && !owns_active_frame && !explicitly_switches_receiver {
            continue;
        }
        if loaded.has_semantic_targets()
            && !loaded.semantically_matches_call(visitor, node)
            && !loaded.can_run_inside_extension_frame(visitor, node)
        {
            continue;
        }
        let compact_ctx =
            shared_call_context.get_or_insert_with(|| call_context(visitor, node, false));
        let ctx = match (loaded.project_context_delivery, project) {
            (ExtensionProjectContextDelivery::PerCall, Some(project)) => {
                shared_project_call_context.get_or_insert_with(|| {
                    let mut complete = compact_ctx.clone();
                    complete.project = Some(project.clone());
                    complete
                })
            }
            (
                ExtensionProjectContextDelivery::Activation
                | ExtensionProjectContextDelivery::PerCall,
                None,
            )
            | (ExtensionProjectContextDelivery::Activation, Some(_)) => compact_ctx,
        };
        let output = match loaded.index_call_output_for_project(project, ctx) {
            Ok(output) => output,
            Err(err) => {
                warn!(
                    "Disabling Ruby Fast LSP extension `{}` after indexing failure on `{}`: {}",
                    loaded.metadata.id, method_name, err
                );
                let reason = err.to_string();
                loaded.fail(reason);
                continue;
            }
        };
        if output.index_patches.is_empty() && output.execution_contexts.is_empty() {
            continue;
        }
        if let Err(spoofed_id) =
            validate_index_patch_provenance(&loaded.metadata.id, &output.index_patches)
        {
            loaded.reject(format!(
                "extension `{}` emitted an index patch attributed to `{spoofed_id}`; patch provenance must match the loaded manifest id",
                loaded.metadata.id
            ));
            continue;
        }
        if let Err(err) = validate_index_patch_payloads(&output.index_patches) {
            loaded.reject(format!(
                "extension `{}` emitted an invalid index patch: {err}",
                loaded.metadata.id
            ));
            continue;
        }
        if project.is_none()
            && output
                .index_patches
                .iter()
                .any(index_patch_requires_project_context)
        {
            loaded.reject(format!(
                "extension `{}` emitted a project-generated owner without an owning ProjectContext",
                loaded.metadata.id
            ));
            continue;
        }
        if let Err(err) = validate_execution_contexts_for_project(
            &loaded.metadata.id,
            ctx,
            project.is_some(),
            &output.execution_contexts,
        ) {
            loaded.reject(format!(
                "extension `{}` emitted an invalid block execution context: {err}",
                loaded.metadata.id
            ));
            continue;
        }
        emitters.insert(loaded.metadata.id.clone(), Arc::clone(&loaded));
        emitted.extend(output.index_patches);
        emitted_contexts.extend(output.execution_contexts);
    }

    if emitted.is_empty() && emitted_contexts.is_empty() {
        return false;
    }
    let mut pending = emitted;
    let mut pending_contexts = emitted_contexts;
    let (patches, contexts) = loop {
        let conflict = match resolve_index_patch_conflicts(pending.clone()) {
            Ok(patches) => match resolve_execution_context_conflicts(pending_contexts.clone()) {
                Ok(contexts) => break (patches, contexts),
                Err(conflict) => conflict,
            },
            Err(conflict) => conflict,
        };
        let rejected_ids = conflict
            .extension_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for extension_id in &conflict.extension_ids {
            let loaded = emitters.get(extension_id).expect_invariant(
                "conflicting patch source has no emitting extension",
                "provenance is validated before conflict resolution",
                "keep emitter registration adjacent to accepted patch collection",
            );
            loaded.reject_conflict(conflict.message.clone());
        }
        warn!(
            "Rejecting conflicting extension index patches: {}",
            conflict.message
        );
        pending.retain(|patch| !rejected_ids.contains(index_patch_extension_id(patch)));
        pending_contexts
            .retain(|context| !rejected_ids.contains(context.source.extension_id.as_str()));
        if pending.is_empty() && pending_contexts.is_empty() {
            return false;
        }
    };
    for patch in patches {
        apply_patch(visitor, node, patch);
    }
    for context in contexts {
        apply_execution_context(visitor, context);
    }
    true
}
