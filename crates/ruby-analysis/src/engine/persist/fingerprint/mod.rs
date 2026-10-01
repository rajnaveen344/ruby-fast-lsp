//! Semantic export and result fingerprints used to classify file replacements
//! and key persistent caches.

mod stable_hash;

use crate::invariant::ExpectInvariant;
use std::collections::HashMap;
use std::hash::Hash;

use crate::core::equations::method_return_equation::MethodReturnBase;
use crate::core::{FileAnalysis, SourceFileId, SymbolKind, TypeSubject};

use crate::engine::Project;
use stable_hash::{
    export_hash, result_hash, stable_bool, stable_callable_body_summary,
    stable_callable_signatures, stable_diagnostic_severity, stable_direct_yield_call,
    stable_execution_scope_mode, stable_forwarded_block_call, stable_fqn, stable_graph_edge_kind,
    stable_graph_edge_provenance, stable_graph_node_kind, stable_len, stable_method,
    stable_method_availability, stable_method_param_kind, stable_method_reference_access,
    stable_method_visibility, stable_optional_fqn, stable_optional_method, stable_optional_string,
    stable_range_offsets, stable_ruby_type, stable_source_kind, stable_string, stable_strings,
    stable_symbol_kind, stable_type_provenance, stable_type_subject, stable_u64, stable_u8,
};

impl SemanticExportFingerprint {
    pub(in crate::engine) fn from_facts(facts: &FileAnalysis) -> Self {
        let mut exports = Vec::new();

        for fact in &facts.symbols {
            if matches!(
                fact.kind,
                SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
            ) {
                exports.push(export_hash(|hasher| {
                    stable_u8(hasher, 1);
                    stable_fqn(hasher, &fact.fqn);
                    stable_symbol_kind(hasher, fact.kind);
                }));
            }
        }
        for fact in &facts.methods {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 2);
                stable_fqn(hasher, &fact.fqn);
                stable_fqn(hasher, &fact.owner);
                stable_strings(hasher, &fact.params);
                stable_len(hasher, fact.param_facts.len());
                for parameter in &fact.param_facts {
                    stable_string(hasher, &parameter.name);
                    stable_method_param_kind(hasher, parameter.kind);
                    stable_optional_string(hasher, parameter.type_label.as_deref());
                    stable_optional_string(hasher, parameter.documentation.as_deref());
                }
                stable_optional_method(hasher, fact.delegate_receiver);
                stable_method_visibility(hasher, fact.visibility);
                stable_method_availability(hasher, &fact.availability);
                stable_optional_string(hasher, fact.documentation.as_deref());
                stable_optional_string(hasher, fact.return_type_label.as_deref());
                stable_callable_signatures(hasher, fact.callable_signatures());
                stable_forwarded_block_call(hasher, fact.forwarded_block_call());
                stable_direct_yield_call(hasher, fact.direct_yield_call());
            }));
        }
        for fact in &facts.method_visibility_overrides {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 3);
                stable_fqn(hasher, &fact.owner);
                stable_method(hasher, fact.method);
                stable_method_visibility(hasher, fact.visibility);
            }));
        }
        for fact in &facts.types {
            if matches!(
                fact.subject,
                TypeSubject::Constant(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
            ) {
                exports.push(export_hash(|hasher| {
                    stable_u8(hasher, 4);
                    stable_type_subject(hasher, &fact.subject);
                    stable_ruby_type(hasher, &fact.ruby_type);
                    stable_type_provenance(hasher, fact.provenance);
                }));
            }
        }
        for equation in &facts.inference.method_return_equations {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 8);
                stable_fqn(hasher, equation.method());
                match equation.base() {
                    MethodReturnBase::Bottom => stable_u8(hasher, 0),
                    MethodReturnBase::Proven(ruby_type) => {
                        stable_u8(hasher, 1);
                        stable_ruby_type(hasher, ruby_type);
                    }
                    MethodReturnBase::Unknown(reason) => {
                        stable_u8(hasher, 2);
                        stable_string(hasher, reason.code());
                    }
                }
                stable_len(hasher, equation.dependencies().len());
                for dependency in equation.dependencies() {
                    stable_fqn(hasher, dependency);
                }
            }));
        }
        for fact in &facts.inference.constant_callable_bodies {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 9);
                stable_fqn(hasher, &fact.constant);
                stable_callable_body_summary(hasher, &fact.summary);
            }));
        }
        for fact in &facts.graph_nodes {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 5);
                stable_fqn(hasher, &fact.fqn);
                stable_graph_node_kind(hasher, fact.kind);
            }));
        }
        for fact in &facts.graph_edges {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 6);
                stable_fqn(hasher, &fact.source);
                stable_fqn(hasher, &fact.target);
                stable_graph_edge_kind(hasher, fact.kind);
                stable_graph_edge_provenance(hasher, fact.provenance);
            }));
        }
        for fact in &facts.unresolved_graph_edges {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 7);
                stable_fqn(hasher, &fact.source);
                stable_len(hasher, fact.target_parts.len());
                for part in &fact.target_parts {
                    stable_string(hasher, part.as_str());
                }
                stable_bool(hasher, fact.absolute);
                stable_fqn(hasher, &fact.context);
                stable_graph_edge_kind(hasher, fact.kind);
                stable_graph_edge_provenance(hasher, fact.provenance);
            }));
        }

        exports.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        export_hash(|hasher| {
            stable_len(hasher, exports.len());
            for fingerprint in &exports {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }
}

/// Stable identity of the engine's user-visible semantic result.
///
/// Unlike [`SemanticExportFingerprint`], this includes exact declaration and
/// reference ranges, every type fact, resolved graph state, diagnostics, and
/// framework execution contexts. Physical paths and engine-local file/FQN IDs
/// remain excluded so equivalent cold and cached indexing runs can be compared
/// across fresh processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticResultFingerprint {
    high: u64,
    low: u64,
}

impl SemanticResultFingerprint {
    pub fn stable_bytes(self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.high.to_le_bytes());
        bytes[8..].copy_from_slice(&self.low.to_le_bytes());
        bytes
    }
}

impl SemanticChange {
    pub fn classify(
        previous: Option<SemanticExportFingerprint>,
        current: SemanticExportFingerprint,
    ) -> Self {
        match previous {
            None => Self::InitialIndex,
            Some(previous) if previous == current => Self::BodyOnly,
            Some(_) => Self::ExportsChanged,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticExportFingerprint {
    high: u64,
    low: u64,
}

impl SemanticExportFingerprint {
    /// Stable semantic identity bytes for validated derived-product keys.
    ///
    /// This does not expose engine stores. It only makes the already-public
    /// fingerprint portable across process boundaries without relying on
    /// `Debug`, Rust's randomized hashers, or layout-dependent serialization.
    pub fn stable_bytes(self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.high.to_le_bytes());
        bytes[8..].copy_from_slice(&self.low.to_le_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticChange {
    InitialIndex,
    BodyOnly,
    ExportsChanged,
}

impl Project {
    pub fn semantic_export_fingerprint(
        &self,
        file_id: SourceFileId,
    ) -> Option<SemanticExportFingerprint> {
        self.files.export_fingerprint(file_id)
    }

    /// Stable semantic identity for an immutable dependency seed.
    ///
    /// Physical paths and engine-local file IDs are deliberately excluded so
    /// equivalent runtime/core inputs can share one project-neutral producer.
    /// Source kind remains part of identity because definition precedence and
    /// edit/diagnostic policy differ between implementations, stubs, and
    /// signatures.
    pub fn semantic_context_fingerprint(&self) -> SemanticExportFingerprint {
        let mut file_fingerprints = self
            .files
            .export_fingerprints()
            .map(|(file_id, fingerprint)| {
                let source = self.files.get(*file_id).expect_invariant(
                    "semantic export fingerprint has no registered source file",
                    "replace_facts validates every file id before recording semantic state",
                    "remove fingerprints through the same file lifecycle as source registration",
                );
                export_hash(|hasher| {
                    stable_source_kind(hasher, source.kind);
                    stable_u64(hasher, fingerprint.high);
                    stable_u64(hasher, fingerprint.low);
                })
            })
            .collect::<Vec<_>>();
        file_fingerprints.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        export_hash(|hasher| {
            stable_len(hasher, file_fingerprints.len());
            for fingerprint in &file_fingerprints {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }

    /// Stable, path-independent identity of every user-visible semantic fact,
    /// partitioned by its owning source file.
    pub fn semantic_result_file_fingerprints(
        &self,
    ) -> Vec<(SourceFileId, SemanticResultFingerprint)> {
        fn push_component(
            components: &mut HashMap<SourceFileId, Vec<SemanticExportFingerprint>>,
            file_id: SourceFileId,
            component: SemanticExportFingerprint,
        ) {
            components
                .get_mut(&file_id)
                .unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "semantic result fact belongs to unknown file id {:?}",
                        why = "every stored fact must be owned by one registered source",
                        fix = "remove and replace facts through the same file lifecycle",
                        file_id,
                    )
                })
                .push(component);
        }

        let mut components = self
            .files
            .ids()
            .map(|file_id| (file_id, Vec::new()))
            .collect::<HashMap<_, _>>();

        for fact in self.all_symbol_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 1);
                    stable_fqn(hasher, &fact.fqn);
                    stable_symbol_kind(hasher, fact.kind);
                    stable_range_offsets(hasher, fact.name_range);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_method_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 2);
                    stable_fqn(hasher, &fact.fqn);
                    stable_fqn(hasher, &fact.owner);
                    stable_range_offsets(hasher, fact.name_range);
                    stable_range_offsets(hasher, fact.range);
                    stable_strings(hasher, &fact.params);
                    stable_len(hasher, fact.param_facts.len());
                    for parameter in &fact.param_facts {
                        stable_string(hasher, &parameter.name);
                        stable_method_param_kind(hasher, parameter.kind);
                        stable_optional_string(hasher, parameter.type_label.as_deref());
                        stable_optional_string(hasher, parameter.documentation.as_deref());
                    }
                    stable_optional_method(hasher, fact.delegate_receiver);
                    stable_method_visibility(hasher, fact.visibility);
                    stable_method_availability(hasher, &fact.availability);
                    stable_optional_string(hasher, fact.documentation.as_deref());
                    stable_optional_string(hasher, fact.return_type_label.as_deref());
                    stable_callable_signatures(hasher, fact.callable_signatures());
                    stable_forwarded_block_call(hasher, fact.forwarded_block_call());
                    stable_direct_yield_call(hasher, fact.direct_yield_call());
                }),
            );
        }
        for fact in self.decls.method_visibility_overrides() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 3);
                    stable_fqn(hasher, &fact.owner);
                    stable_method(hasher, fact.method);
                    stable_method_visibility(hasher, fact.visibility);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.types.store().all_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 4);
                    stable_type_subject(hasher, &fact.subject);
                    stable_ruby_type(hasher, &fact.ruby_type);
                    stable_type_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_graph_nodes() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 5);
                    stable_fqn(hasher, &fact.fqn);
                    stable_graph_node_kind(hasher, fact.kind);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_graph_edges() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 6);
                    stable_fqn(hasher, &fact.source);
                    stable_fqn(hasher, &fact.target);
                    stable_graph_edge_kind(hasher, fact.kind);
                    stable_graph_edge_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.unresolved_graph_edges() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 7);
                    stable_fqn(hasher, &fact.source);
                    stable_len(hasher, fact.target_parts.len());
                    for part in &fact.target_parts {
                        stable_string(hasher, part.as_str());
                    }
                    stable_bool(hasher, fact.absolute);
                    stable_fqn(hasher, &fact.context);
                    stable_graph_edge_kind(hasher, fact.kind);
                    stable_graph_edge_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for (target, fact) in self.uses.resolved().iter_facts_with_targets() {
            let target = self.names.fqn(target).unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "resolved reference target {:?} has no interned FQN",
                    why = "stored references must retain a valid semantic target",
                    fix = "intern targets before resolving references and remove them only with their facts",
                    target,
                )
            });
            let caller = fact.caller.map(|caller| {
                self.names.fqn(caller).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "resolved reference caller {:?} has no interned FQN",
                        why = "caller provenance must remain valid while the reference exists",
                        fix = "intern callers before resolving references and remove them only with their facts",
                        caller,
                    )
                })
            });
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 8);
                    stable_fqn(hasher, target);
                    stable_optional_fqn(hasher, caller);
                    stable_method_reference_access(hasher, fact.access);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.diagnostics.all_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 9);
                    stable_diagnostic_severity(hasher, fact.severity);
                    stable_string(hasher, &fact.code);
                    stable_string(hasher, &fact.message);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for (file_id, contexts) in self.decls.execution_contexts_by_file() {
            for context in contexts {
                push_component(
                    &mut components,
                    file_id,
                    export_hash(|hasher| {
                        stable_u8(hasher, 10);
                        stable_fqn(hasher, &context.lexical_namespace);
                        stable_fqn(hasher, &context.implicit_receiver);
                        stable_fqn(hasher, &context.method_definition_owner);
                        stable_execution_scope_mode(hasher, context.lexical_scope);
                        stable_execution_scope_mode(hasher, context.local_scope);
                        stable_string(hasher, &context.extension_id);
                        stable_range_offsets(hasher, context.range);
                    }),
                );
            }
        }
        for (file_id, reads) in self.types.local_read_types_by_file() {
            for (range, ruby_type) in reads {
                push_component(
                    &mut components,
                    file_id,
                    export_hash(|hasher| {
                        stable_u8(hasher, 11);
                        stable_range_offsets(hasher, range);
                        stable_ruby_type(hasher, ruby_type);
                    }),
                );
            }
        }

        components
            .into_iter()
            .map(|(file_id, mut facts)| {
                let source = self.files.get(file_id).expect_invariant(
                    "semantic result component owner has no registered source file",
                    "the component map is seeded exclusively from registered sources",
                    "keep source removal and semantic fact removal atomic",
                );
                facts.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
                (
                    file_id,
                    result_hash(|hasher| {
                        stable_source_kind(hasher, source.kind);
                        stable_len(hasher, facts.len());
                        for fact in &facts {
                            stable_u64(hasher, fact.high);
                            stable_u64(hasher, fact.low);
                        }
                    }),
                )
            })
            .collect()
    }

    /// Stable per-file fingerprints for the three resolution-owned result
    /// categories that are not already isolated by semantic export and
    /// diagnostic manifests: resolved references, framework execution
    /// contexts, and proven local-read types.
    pub fn semantic_resolution_file_fingerprints(
        &self,
    ) -> HashMap<SourceFileId, [SemanticResultFingerprint; 3]> {
        let category_hash = |tag: u8, mut components: Vec<SemanticExportFingerprint>| {
            components.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
            result_hash(|hasher| {
                stable_u8(hasher, tag);
                stable_len(hasher, components.len());
                for component in &components {
                    stable_u64(hasher, component.high);
                    stable_u64(hasher, component.low);
                }
            })
        };
        let mut components = self
            .files
            .ids()
            .map(|file_id| (file_id, [Vec::new(), Vec::new(), Vec::new()]))
            .collect::<HashMap<_, _>>();

        for (target, fact) in self.uses.resolved().iter_facts_with_targets() {
            let target = self.names.fqn(target).unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "per-file reference fingerprint target {:?} has no interned FQN",
                    why = "resolved references retain their target identity",
                    fix = "remove references before removing interned names",
                    target,
                )
            });
            let caller = fact.caller.map(|caller| {
                self.names.fqn(caller).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "per-file reference fingerprint caller {:?} has no interned FQN",
                        why = "resolved references retain caller provenance",
                        fix = "remove references before removing interned names",
                        caller,
                    )
                })
            });
            let component = export_hash(|hasher| {
                stable_fqn(hasher, target);
                stable_optional_fqn(hasher, caller);
                stable_method_reference_access(hasher, fact.access);
                stable_range_offsets(hasher, fact.range);
            });
            components.get_mut(&fact.range.file_id).unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "per-file reference fingerprint belongs to unknown file {:?}",
                    why = "resolved references cannot outlive their registered source",
                    fix = "remove references before unregistering files",
                    fact.range.file_id,
                )
            })[0]
                .push(component);
        }
        for (file_id, contexts) in self.decls.execution_contexts_by_file() {
            let output = &mut components.get_mut(&file_id).unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "execution-context fingerprint belongs to unknown file {:?}",
                    why = "execution contexts cannot outlive their registered source",
                    fix = "remove contexts before unregistering files",
                    file_id,
                )
            })[1];
            output.extend(contexts.iter().map(|context| {
                export_hash(|hasher| {
                    stable_fqn(hasher, &context.lexical_namespace);
                    stable_fqn(hasher, &context.implicit_receiver);
                    stable_fqn(hasher, &context.method_definition_owner);
                    stable_execution_scope_mode(hasher, context.lexical_scope);
                    stable_execution_scope_mode(hasher, context.local_scope);
                    stable_string(hasher, &context.extension_id);
                    stable_range_offsets(hasher, context.range);
                })
            }));
        }
        for (file_id, reads) in self.types.local_read_types_by_file() {
            let output = &mut components.get_mut(&file_id).unwrap_or_else(|| {
                unreachable_invariant!(
                    what = "local-read fingerprint belongs to unknown file {:?}",
                    why = "flow evidence cannot outlive its registered source",
                    fix = "remove inference evidence before unregistering files",
                    file_id,
                )
            })[2];
            output.extend(reads.map(|(range, ruby_type)| {
                export_hash(|hasher| {
                    stable_range_offsets(hasher, range);
                    stable_ruby_type(hasher, ruby_type);
                })
            }));
        }

        components
            .into_iter()
            .map(|(file_id, [references, contexts, local_reads])| {
                (
                    file_id,
                    [
                        category_hash(1, references),
                        category_hash(2, contexts),
                        category_hash(3, local_reads),
                    ],
                )
            })
            .collect()
    }

    /// Stable, path-independent identity of every user-visible semantic fact.
    ///
    /// This is intended for cross-process correctness evidence, not query
    /// lookup. Each fact is reduced to an order-independent stable component;
    /// the final multiset preserves file ownership and source-kind precedence
    /// without retaining engine-local IDs or physical paths.
    pub fn semantic_result_fingerprint(&self) -> SemanticResultFingerprint {
        let mut file_fingerprints = self
            .semantic_result_file_fingerprints()
            .into_iter()
            .map(|(_, fingerprint)| fingerprint)
            .collect::<Vec<_>>();
        file_fingerprints.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        result_hash(|hasher| {
            stable_len(hasher, file_fingerprints.len());
            for fingerprint in &file_fingerprints {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }
}
