//! JRuby Java-import navigation inputs and deferred plan materialization.

use super::collection::replace_file_analysis;
use super::FileProcessor;
use super::{FileResolution, JrubyNavigationResolution};
use crate::environment::runtime::jruby::imports::StaticJavaNavigationPlan;
use crate::environment::runtime::jruby::source_navigation::java_source_navigation_facts_with_declaration;
use crate::invariant::ExpectInvariant;
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use ruby_analysis::core::{FileAnalysis, FullyQualifiedName, SourceKind};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, SourceFileInput};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::Url;

fn extend_unique<T: PartialEq>(target: &mut Vec<T>, source: Vec<T>) {
    for value in source {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

impl FileProcessor {
    pub(super) fn ensure_jruby_navigation_inputs(
        &self,
        content: &str,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    ) -> Result<()> {
        let Some(provider) = &self.jruby_import_provider else {
            return Ok(());
        };
        let plan = provider
            .static_navigation_plan(content)
            .map_err(|message| {
                anyhow!("failed to resolve static JRuby Java dependencies: {message}")
            })?;
        self.materialize_jruby_navigation_plan(
            plan,
            analysis_engine,
            JrubyNavigationResolution::Immediate,
        )
    }

    pub(crate) fn materialize_jruby_navigation_plan_as_deferred_resolution(
        &self,
        plan: StaticJavaNavigationPlan,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Result<()> {
        self.materialize_jruby_navigation_plan(
            plan,
            analysis_engine,
            JrubyNavigationResolution::Deferred { known_namespaces },
        )
    }

    fn materialize_jruby_navigation_plan(
        &self,
        plan: StaticJavaNavigationPlan,
        analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
        resolution: JrubyNavigationResolution,
    ) -> Result<()> {
        if plan.signature_class_names.is_empty() {
            return Ok(());
        }
        let provider = self.jruby_import_provider.as_ref().expect_invariant(
            "a JRuby navigation plan was materialized without an owning JRuby provider",
            "plans are derived from one exact project classpath catalog",
            "keep plan collection and materialization on the same project FileProcessor",
        );
        let cache_root = provider.signature_cache_root().ok_or_else(|| {
            anyhow!(
                "JRuby provider for classpath {} has no isolated signature cache root",
                provider.classpath_fingerprint()
            )
        })?;
        let signature_class_names = plan
            .signature_class_names
            .into_iter()
            .collect::<BTreeSet<_>>();
        let implementation_class_names = plan
            .implementation_class_names
            .into_iter()
            .collect::<BTreeSet<_>>();
        let (deferred_signature_known_namespaces, file_resolution) = match resolution {
            JrubyNavigationResolution::Immediate => (None, FileResolution::Full),
            JrubyNavigationResolution::Deferred { known_namespaces } => {
                (Some(known_namespaces), FileResolution::Deferred)
            }
        };
        std::fs::create_dir_all(cache_root).with_context(|| {
            format!(
                "failed to create isolated JRuby signature cache {}",
                cache_root.display()
            )
        })?;

        let mut exact_sources = BTreeMap::<
            PathBuf,
            (
                String,
                Vec<(
                    String,
                    ruby_fast_lsp_jvm_metadata::JavaSourceClassLocation,
                    bool,
                )>,
            ),
        >::new();
        let mut signature_generation_wall = Duration::default();
        let mut signature_cache_io_wall = Duration::default();
        let mut signature_index_wall = Duration::default();
        let mut implementation_resolution_wall = Duration::default();
        let mut generated_signatures = 0usize;
        let mut indexed_signatures = 0usize;
        for class_name in signature_class_names {
            let signature_generation_started = Instant::now();
            let Some((internal_name, signature)) =
                provider.generated_signature(&class_name).map_err(|error| {
                    anyhow!("failed to generate signature for Java class `{class_name}`: {error:?}")
                })?
            else {
                signature_generation_wall += signature_generation_started.elapsed();
                continue;
            };
            signature_generation_wall += signature_generation_started.elapsed();
            generated_signatures += 1;
            let signature_cache_io_started = Instant::now();
            let signature_path = cache_root.join(format!("{internal_name}.rb"));
            let signature_parent = signature_path.parent().expect_invariant(
                "generated JRuby signature path has no parent",
                "validated JVM names always produce a cache-relative path",
                "retain the isolated cache root and validated internal class name",
            );
            std::fs::create_dir_all(signature_parent).with_context(|| {
                format!(
                    "failed to create JRuby signature directory {}",
                    signature_parent.display()
                )
            })?;
            if !std::fs::read_to_string(&signature_path).is_ok_and(|existing| existing == signature)
            {
                std::fs::write(&signature_path, &signature).with_context(|| {
                    format!(
                        "failed to materialize JRuby signature {}",
                        signature_path.display()
                    )
                })?;
            }
            signature_cache_io_wall += signature_cache_io_started.elapsed();
            let signature_uri = Url::from_file_path(&signature_path).map_err(|_| {
                anyhow!(
                    "generated JRuby signature is not a valid file URI: {}",
                    signature_path.display()
                )
            })?;
            let signature_already_indexed =
                analysis_engine.read().file_id(&signature_path).is_some();
            if !signature_already_indexed {
                let signature_index_started = Instant::now();
                match &deferred_signature_known_namespaces {
                    Some(known_namespaces) => {
                        self.collect_file_facts_as_deferred_resolution_with_known_namespaces_in_engine(
                            &signature_uri,
                            &signature,
                            analysis_engine.clone(),
                            SourceKind::Signature,
                            known_namespaces.clone(),
                        )?;
                    }
                    None => {
                        self.collect_file_facts_as_with_resolution(
                            &signature_uri,
                            &signature,
                            analysis_engine.clone(),
                            SourceKind::Signature,
                            true,
                            None,
                            false,
                            true,
                        )?;
                    }
                }
                signature_index_wall += signature_index_started.elapsed();
                indexed_signatures += 1;
            }

            if provider.has_registered_navigation_class(&internal_name) {
                continue;
            }
            if !implementation_class_names.contains(&internal_name) {
                continue;
            }
            let implementation_resolution_started = Instant::now();
            let resolved_sources = match provider
                .resolved_navigation_implementations(&internal_name)
            {
                Ok(resolved) => resolved,
                Err(error) => {
                    warn!(
                        "Java implementation source unavailable for {} during JRuby navigation materialization: {:?}; using generated signature fallback",
                        internal_name, error
                    );
                    Vec::new()
                }
            };
            implementation_resolution_wall += implementation_resolution_started.elapsed();
            if resolved_sources.is_empty() {
                continue;
            }
            for (index, resolved) in resolved_sources.into_iter().enumerate() {
                let entry = exact_sources
                    .entry(resolved.path)
                    .or_insert_with(|| (resolved.content.clone(), Vec::new()));
                invariant_eq!(
                    entry.0,
                    resolved.content,
                    what =
                        "one Java source path resolved to different content in one classpath pass",
                    why = "classpath and source fingerprints are fixed for the pass",
                    fix = "retain one verified source identity per materialized path",
                );
                entry
                    .1
                    .push((internal_name.clone(), resolved.location, index == 0));
            }
        }

        let exact_source_insertion_started = Instant::now();
        let exact_source_files = exact_sources.len();
        for (path, (content, mut classes)) in exact_sources {
            classes.sort_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.internal_name.cmp(&right.1.internal_name))
            });
            classes.dedup_by(|left, right| left.0 == right.0);
            let (file_id, mut facts) = {
                let mut engine = analysis_engine.write();
                let file_id = engine.register_file(SourceFileInput {
                    path,
                    content,
                    kind: SourceKind::External,
                });
                let query = AnalysisQuery::new(&engine);
                (
                    file_id,
                    FileAnalysis {
                        symbols: query.symbol_facts_in_file(file_id),
                        methods: query.method_facts_in_file(file_id),
                        method_visibility_overrides: query
                            .method_visibility_overrides_in_file(file_id),
                        types: query.type_facts_in_file(file_id),
                        graph_nodes: query.graph_nodes_in_file(file_id),
                        graph_edges: query.graph_edges_in_file(file_id),
                        diagnostics: query.diagnostic_facts_in_file(file_id),
                        ..FileAnalysis::default()
                    },
                )
            };
            for (internal_name, location, include_class_declaration) in classes {
                let declaration = provider.class_declaration(&internal_name).expect_invariant(
                    "exact Java implementation resolved for a class absent from its owning catalog",
                    "resolution starts from that exact catalog declaration",
                    "keep provider catalog and resolver transactionally paired",
                );
                provider.register_method_navigation_ranges(&internal_name, &location, file_id);
                let new_facts = java_source_navigation_facts_with_declaration(
                    &declaration.class,
                    &location,
                    file_id,
                    include_class_declaration,
                );
                extend_unique(&mut facts.symbols, new_facts.symbols);
                extend_unique(&mut facts.methods, new_facts.methods);
                extend_unique(&mut facts.types, new_facts.types);
            }
            replace_file_analysis(analysis_engine, file_id, facts, file_resolution);
        }
        info!(
            "[PERF][JRuby navigation materialization] classpath={} generated_signatures={} \
             indexed_signatures={} exact_source_files={} signature_generation={:?} \
             signature_cache_io={:?} signature_index={:?} implementation_resolution={:?} \
             exact_source_insertion={:?}",
            provider.classpath_fingerprint(),
            generated_signatures,
            indexed_signatures,
            exact_source_files,
            signature_generation_wall,
            signature_cache_io_wall,
            signature_index_wall,
            implementation_resolution_wall,
            exact_source_insertion_started.elapsed()
        );
        Ok(())
    }
}
