use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use globset::GlobSet;
use parking_lot::Mutex;
use ruby_analysis::core::{MethodCalleeResolution, RubyMethod};
use ruby_analysis::indexer as utils;
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_extension_api::{CallContext, ExtensionEvent};
use ruby_prism::CallNode;
use semver::Version;

use crate::environment::extensions::dispatch::call_context::resolved_core_callees_for_call;
use crate::environment::extensions::loading::manifest::{
    build_watched_file_matcher, ExtensionGemRequirement, ExtensionMetadata, ExtensionMethodTarget,
    ExtensionNamespaceTarget, ExtensionProjectContextDelivery,
};
use crate::environment::extensions::loading::wasm::{
    lifecycle_output_is_empty, require_empty_lifecycle_output,
};
use crate::environment::extensions::registry::state::extension_target_owner_exists;
use crate::environment::extensions::registry::status::{
    ExtensionStatus, ExtensionStatusReport, ExtensionTelemetry, GuestCallKind,
};

pub(in crate::environment::extensions) struct LoadedWasmExtension {
    pub(in crate::environment::extensions) metadata: ExtensionMetadata,
    extension: Mutex<ruby_fast_lsp_extension_wasm_host::WasmExtension>,
    project_extensions: Mutex<BTreeMap<String, ruby_fast_lsp_extension_wasm_host::WasmExtension>>,
    compiled_extension: ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension,
    activation_settings: Mutex<Option<serde_json::Value>>,
    status: Mutex<ExtensionStatus>,
    pub(super) indexed_call_names: BTreeSet<String>,
    pub(super) frame_call_names: BTreeSet<String>,
    pub(super) semantic_targets: Vec<ExtensionMethodTarget>,
    pub(super) semantic_namespaces: Vec<ExtensionNamespaceTarget>,
    pub(in crate::environment::extensions) watched_file_matcher: GlobSet,
    applicability: Vec<ExtensionGemRequirement>,
    pub(in crate::environment::extensions) project_context_delivery:
        ExtensionProjectContextDelivery,
    telemetry: ExtensionTelemetry,
    #[cfg(test)]
    applicability_evaluations: AtomicU64,
}

pub(in crate::environment::extensions) fn guest_call_context<'a>(
    delivery: ExtensionProjectContextDelivery,
    project: &'a ruby_fast_lsp_extension_api::ProjectContext,
    context: &'a CallContext,
) -> Cow<'a, CallContext> {
    match delivery {
        ExtensionProjectContextDelivery::Activation if context.project.is_none() => {
            Cow::Borrowed(context)
        }
        ExtensionProjectContextDelivery::Activation => {
            let mut compact = context.clone();
            compact.project = None;
            Cow::Owned(compact)
        }
        ExtensionProjectContextDelivery::PerCall if context.project.is_some() => {
            Cow::Borrowed(context)
        }
        ExtensionProjectContextDelivery::PerCall => {
            let mut complete = context.clone();
            complete.project = Some(project.clone());
            Cow::Owned(complete)
        }
    }
}

impl LoadedWasmExtension {
    pub(in crate::environment::extensions) fn new(
        metadata: ExtensionMetadata,
        extension: ruby_fast_lsp_extension_wasm_host::WasmExtension,
        compiled_extension: ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension,
        semantic_targets: Vec<ExtensionMethodTarget>,
        semantic_namespaces: Vec<ExtensionNamespaceTarget>,
        frame_call_names: BTreeSet<String>,
        applicability: Vec<ExtensionGemRequirement>,
        project_context_delivery: ExtensionProjectContextDelivery,
    ) -> Self {
        let indexed_call_names = extension
            .indexed_call_names()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let watched_file_matcher = build_watched_file_matcher(
            &metadata.id,
            &metadata.watched_files,
        )
        .expect("INVARIANT VIOLATED: validated extension watcher globs failed to compile while constructing a loaded extension. This is a bug because manifest validation and runtime matching use the same compiler. Fix: keep watcher validation before Wasm instantiation.");
        Self {
            metadata,
            extension: Mutex::new(extension),
            project_extensions: Mutex::new(BTreeMap::new()),
            compiled_extension,
            activation_settings: Mutex::new(None),
            status: Mutex::new(ExtensionStatus::Discovered),
            indexed_call_names,
            frame_call_names,
            semantic_targets,
            semantic_namespaces,
            watched_file_matcher,
            applicability,
            project_context_delivery,
            telemetry: ExtensionTelemetry::default(),
            #[cfg(test)]
            applicability_evaluations: AtomicU64::new(0),
        }
    }

    pub(in crate::environment::extensions) fn is_loaded(&self) -> bool {
        *self.status.lock() == ExtensionStatus::Loaded
    }

    pub(super) fn handle_lifecycle_event(
        &self,
        event_name: &str,
        settings: Option<serde_json::Value>,
    ) {
        let started = Instant::now();
        if matches!(event_name, "lifecycle.activate" | "settings.changed") {
            *self.activation_settings.lock() = settings.clone();
        }
        let event = ExtensionEvent {
            event: event_name.to_string(),
            call: None,
            document: None,
            project: None,
            settings,
            files: None,
            process_results: None,
        };
        let base_result = self.extension.lock().handle_event(&event);
        let project_result = if lifecycle_output_is_empty(&base_result) {
            let mut projects = self.project_extensions.lock();
            projects.values_mut().try_for_each(|extension| {
                extension
                    .handle_event(&event)
                    .and_then(require_empty_lifecycle_output)
            })
        } else {
            Ok(())
        };
        let failure = base_result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .or_else(|| project_result.as_ref().err().map(ToString::to_string));
        self.telemetry.record_call(
            GuestCallKind::Lifecycle,
            started.elapsed(),
            base_result.as_ref().ok(),
            failure.as_deref(),
        );
        match (base_result, project_result) {
            (Ok(output), Ok(()))
                if output.index_patches.is_empty()
                    && output.execution_contexts.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty()
                    && output.process_requests.is_empty()
                    && output.reindex_files.is_empty() =>
            {
                let mut status = self.status.lock();
                *status = match event_name {
                    "lifecycle.activate" | "settings.changed" => ExtensionStatus::Loaded,
                    "lifecycle.deactivate" => ExtensionStatus::Deactivated,
                    other => panic!(
                        "INVARIANT VIOLATED: unsupported extension lifecycle event `{other}`. This is a bug because lifecycle state transitions must be explicit. Fix: add the event and its resulting state to handle_lifecycle_event."
                    ),
                };
            }
            (Ok(_), Ok(())) => self.reject(format!(
                "extension `{}` returned patches from `{event_name}`; lifecycle events must not mutate semantic or editor state",
                self.metadata.id
            )),
            (Err(err), _) | (_, Err(err)) => self.fail(format!(
                "extension `{}` {event_name} failed: {err}",
                self.metadata.id
            )),
        }
        if event_name == "lifecycle.deactivate" {
            self.project_extensions.lock().clear();
        }
    }

    #[cfg(test)]
    pub(in crate::environment::extensions) fn index_call_output(
        &self,
        context: &CallContext,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        self.index_call_output_for_project(context.project.as_ref(), context)
    }

    pub(in crate::environment::extensions) fn index_call_output_for_project(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
        context: &CallContext,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        assert!(
            context
                .project
                .as_ref()
                .is_none_or(|context_project| Some(context_project) == project),
            "INVARIANT VIOLATED: an extension call payload disagrees with its explicit owning project. This is a host bug because project instance selection and guest-visible context must describe one source owner. Fix: construct the call context from the same FactCollector project passed to index_call_output_for_project."
        );
        let (result, elapsed) = if let Some(project) = project {
            let mut extensions = self.project_extensions.lock();
            if !extensions.contains_key(&project.project_uri) {
                let creation_started = Instant::now();
                let created = (|| {
                    let mut extension =
                        ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
                            self.metadata.id.clone(),
                            self.compiled_extension.clone(),
                        )?;
                    let activation = ExtensionEvent {
                        event: "lifecycle.activate".to_string(),
                        call: None,
                        document: None,
                        project: Some(project.clone()),
                        settings: self.activation_settings.lock().clone(),
                        files: None,
                        process_results: None,
                    };
                    require_empty_lifecycle_output(extension.handle_event(&activation)?)?;
                    Ok::<_, anyhow::Error>(extension)
                })();
                let creation_failure = created.as_ref().err().map(ToString::to_string);
                self.telemetry.record_project_instance_creation(
                    creation_started.elapsed(),
                    creation_failure.as_deref(),
                );
                extensions.insert(project.project_uri.clone(), created?);
            }
            let started = Instant::now();
            let guest_context = guest_call_context(self.project_context_delivery, project, context);
            let result = extensions
                .get_mut(&project.project_uri)
                .expect(
                    "INVARIANT VIOLATED: project Wasm instance disappeared immediately after insertion. This is a host registry bug because the instance map is locked for the entire operation. Fix: keep lookup and insertion under one project-extension lock.",
                )
                .index_call_output(guest_context.as_ref());
            (result, started.elapsed())
        } else {
            let started = Instant::now();
            let result = self.extension.lock().index_call_output(context);
            (result, started.elapsed())
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        self.telemetry.record_call(
            GuestCallKind::Index,
            elapsed,
            result.as_ref().ok(),
            failure.as_deref(),
        );
        result
    }

    pub(in crate::environment::extensions) fn handle_event_for_project(
        &self,
        event: &ExtensionEvent,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput> {
        let (result, elapsed) = if let Some(project) = project {
            let mut extensions = self.project_extensions.lock();
            if !extensions.contains_key(&project.project_uri) {
                let creation_started = Instant::now();
                let created = (|| {
                    let mut extension =
                        ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
                            self.metadata.id.clone(),
                            self.compiled_extension.clone(),
                        )?;
                    let activation = ExtensionEvent {
                        event: "lifecycle.activate".to_string(),
                        call: None,
                        document: None,
                        project: Some(project.clone()),
                        settings: self.activation_settings.lock().clone(),
                        files: None,
                        process_results: None,
                    };
                    require_empty_lifecycle_output(extension.handle_event(&activation)?)?;
                    Ok::<_, anyhow::Error>(extension)
                })();
                let creation_failure = created.as_ref().err().map(ToString::to_string);
                self.telemetry.record_project_instance_creation(
                    creation_started.elapsed(),
                    creation_failure.as_deref(),
                );
                extensions.insert(project.project_uri.clone(), created?);
            }
            let started = Instant::now();
            let result = extensions
                .get_mut(&project.project_uri)
                .expect(
                    "INVARIANT VIOLATED: project Wasm instance disappeared immediately after insertion. This is a host registry bug because the instance map is locked for the entire event. Fix: keep lookup and insertion under one project-extension lock.",
                )
                .handle_event(event);
            (result, started.elapsed())
        } else {
            let started = Instant::now();
            let result = self.extension.lock().handle_event(event);
            (result, started.elapsed())
        };
        let failure = result.as_ref().err().map(ToString::to_string);
        self.telemetry.record_call(
            GuestCallKind::Event,
            elapsed,
            result.as_ref().ok(),
            failure.as_deref(),
        );
        result
    }

    pub(in crate::environment::extensions) fn fail(&self, reason: impl Into<String>) {
        let failure = ExtensionStatus::from_failure(reason);
        let mut status = self.status.lock();
        if matches!(
            *status,
            ExtensionStatus::Discovered | ExtensionStatus::Loaded
        ) {
            self.telemetry.record_disablement();
            *status = failure;
        }
    }

    pub(in crate::environment::extensions) fn reject(&self, reason: impl Into<String>) {
        self.telemetry.record_rejected_output();
        self.fail(reason);
    }

    pub(in crate::environment::extensions) fn reject_conflict(&self, reason: impl Into<String>) {
        self.telemetry.record_rejected_output();
        self.telemetry.record_patch_conflict();
        self.fail(reason);
    }

    pub(in crate::environment::extensions) fn status_report(&self) -> ExtensionStatusReport {
        let project_instances = self.project_extensions.lock().len();
        let status_guard = self.status.lock();
        let (status, last_error) = match &*status_guard {
            ExtensionStatus::Discovered => ("discovered", None),
            ExtensionStatus::Loaded => ("loaded", None),
            ExtensionStatus::Deactivated => ("deactivated", None),
            ExtensionStatus::Slow { reason } => ("slow", Some(reason.clone())),
            ExtensionStatus::Failed { reason } => ("failed", Some(reason.clone())),
        };
        ExtensionStatusReport {
            id: self.metadata.id.clone(),
            name: self.metadata.name.clone(),
            version: self.metadata.version.clone(),
            status: status.to_string(),
            last_error,
            capabilities: self.metadata.capabilities.clone(),
            permissions: self.metadata.permissions.clone(),
            watched_files: self.metadata.watched_files.clone(),
            process_commands: self.metadata.process_commands.clone(),
            indexed_call_names: self.indexed_call_names.iter().cloned().collect(),
            telemetry: self.telemetry.report(project_instances),
        }
    }

    pub(in crate::environment::extensions) fn handles_call(&self, method_name: &str) -> bool {
        self.indexed_call_names.contains(method_name)
    }

    pub(in crate::environment::extensions) fn applies_to(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        #[cfg(test)]
        self.applicability_evaluations
            .fetch_add(1, Ordering::Relaxed);
        if self.applicability.is_empty() {
            return true;
        }
        let Some(project) = project else {
            return false;
        };
        if !project.lockfile_present || !project.locked_gems_complete {
            return false;
        }
        self.applicability.iter().all(|required| {
            project.locked_gems.iter().any(|locked| {
                locked.name == required.name
                    && Version::parse(&locked.version)
                        .ok()
                        .is_some_and(|version| required.version.matches(&version))
            })
        })
    }

    #[cfg(test)]
    pub(in crate::environment::extensions) fn test_applicability_evaluations(&self) -> u64 {
        self.applicability_evaluations.load(Ordering::Relaxed)
    }

    pub(in crate::environment::extensions) fn applies_to_source(
        &self,
        project: Option<&ruby_fast_lsp_extension_api::ProjectContext>,
    ) -> bool {
        if !self.applies_to(project) {
            return false;
        }
        project.is_none_or(|project| {
            matches!(
                project.source_kind,
                ruby_fast_lsp_extension_api::ProjectSourceKind::Project
                    | ruby_fast_lsp_extension_api::ProjectSourceKind::Excluded
            )
        })
    }

    pub(in crate::environment::extensions) fn has_semantic_targets(&self) -> bool {
        !self.semantic_targets.is_empty()
    }

    pub(in crate::environment::extensions) fn semantically_matches_call(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
    ) -> bool {
        if !self.has_semantic_targets() {
            return self.handles_call(utils::utf8_str(node.name().as_slice()));
        }

        let method_name = utils::utf8_str(node.name().as_slice());
        if !self.handles_call(method_name) {
            return false;
        }

        let Ok(method) = RubyMethod::new(method_name) else {
            return false;
        };
        let callees = resolved_core_callees_for_call(visitor, node);
        self.semantic_targets.iter().any(|target| {
            extension_target_owner_exists(visitor, target)
                && target.method == method
                && callees.iter().any(|callee| {
                    callee.resolution != MethodCalleeResolution::ReceiverOnly
                        && target.owner == callee.owner.namespace_parts()
                        && Some(target.owner_kind) == callee.owner.namespace_kind()
                        && target.method == callee.method
                })
        })
    }

    pub(super) fn semantically_matches_frame_call(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
    ) -> bool {
        if !self.has_semantic_targets() {
            return self
                .frame_call_names
                .contains(utils::utf8_str(node.name().as_slice()));
        }

        let method_name = utils::utf8_str(node.name().as_slice());
        let Ok(method) = RubyMethod::new(method_name) else {
            return false;
        };
        let callees = resolved_core_callees_for_call(visitor, node);
        self.semantic_targets.iter().any(|target| {
            target.frame
                && extension_target_owner_exists(visitor, target)
                && target.method == method
                && callees.iter().any(|callee| {
                    callee.resolution != MethodCalleeResolution::ReceiverOnly
                        && target.owner == callee.owner.namespace_parts()
                        && Some(target.owner_kind) == callee.owner.namespace_kind()
                        && target.method == callee.method
                })
        })
    }

    pub(in crate::environment::extensions) fn can_run_inside_extension_frame(
        &self,
        visitor: &FactCollector,
        node: &CallNode,
    ) -> bool {
        self.handles_call(utils::utf8_str(node.name().as_slice()))
            && visitor.enclosing_extension_calls().iter().any(|call| {
                call.frame_extension_ids
                    .iter()
                    .any(|id| id == &self.metadata.id)
            })
    }
}
