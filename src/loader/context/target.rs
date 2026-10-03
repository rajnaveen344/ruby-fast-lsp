//! The engine a load reads and writes, and the named writes it applies.
//!
//! A load addresses either a project, through its owner's handle, or a
//! loader-private scratch engine (the dependency seed, a core template, a
//! pre-collection snapshot). Both are a [`LoadTarget`]. The loader reads a
//! target through [`view`](#method.view) and changes it only through the
//! named operations implemented on `dyn LoadTarget` in this module: the owner
//! implements two primitives and never hands the loader its lock, and only
//! this module can supply the [`NamedWrite`] proof the write primitive takes.
use crate::environment::extensions::ExtensionSemanticSeed;
use crate::invariant::ExpectInvariant;
use crate::loader::cache::dependency_product::{
    GemDependencyBinding, GemDependencyManifest, GemDependencyProduct,
};
use anyhow::Result;
use log::info;
use parking_lot::RwLock;
use ruby_analysis::core::{FileAnalysis, SourceFileId, SourceKind};
use ruby_analysis::engine::{
    Project, ProjectNeutralFileFactsTemplate, ResolveMode, SemanticChange, SourceFileInput,
    SourceFileSnapshot, View,
};
use ruby_analysis::inference::semantics::Semantics;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Proof that an engine write comes from a named operation in this module.
/// Implementors receive it; only this module constructs it.
pub struct NamedWrite(());

/// The engine a load reads and writes: a project through its owner, or a
/// loader-private scratch engine.
pub trait LoadTarget: Send + Sync {
    /// Run `read` over the engine under one read guard.
    fn read_engine(&self, read: &mut dyn FnMut(&Project));
    /// Run `write` over the engine under one write guard. Only the named
    /// operations on `dyn LoadTarget` can call this.
    fn write_engine(&self, proof: NamedWrite, write: &mut dyn FnMut(&mut Project));
    /// Read-only semantics over this engine for a file walk.
    fn semantics(self: Arc<Self>) -> Arc<dyn Semantics>;
    /// The identity of the engine this target addresses: two targets address
    /// the same engine exactly when their identities share one allocation.
    fn engine_identity(self: Arc<Self>) -> Arc<dyn Send + Sync>;
}

/// A loader-private scratch engine is its own target.
impl LoadTarget for RwLock<Project> {
    fn read_engine(&self, read: &mut dyn FnMut(&Project)) {
        read(&self.read());
    }

    fn write_engine(&self, _proof: NamedWrite, write: &mut dyn FnMut(&mut Project)) {
        write(&mut self.write());
    }

    fn semantics(self: Arc<Self>) -> Arc<dyn Semantics> {
        self
    }

    fn engine_identity(self: Arc<Self>) -> Arc<dyn Send + Sync> {
        self
    }
}

/// How a fact replacement resolves references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileResolution {
    /// Resolve the whole engine now.
    Full,
    /// Resolve only the replaced file now.
    CurrentFile,
    /// Leave resolution to a later batch resolve.
    Deferred,
}

/// Facts to install at a registered path.
pub(crate) enum PathFacts {
    Facts(FileAnalysis),
    Template(ProjectNeutralFileFactsTemplate),
}

/// One project source offered to [`LoadTarget`] batch registration.
pub(crate) struct ProjectSourceCandidate<'a> {
    pub path: &'a Path,
    pub content: &'a str,
    /// The source came from an open editor buffer.
    pub open_document: bool,
    /// Registration of `path` observed when the source was read.
    pub expected_snapshot: Option<SourceFileSnapshot>,
}

impl dyn LoadTarget + '_ {
    /// Run `read` over one view of the engine.
    pub fn view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R {
        self.read(|engine| read(&engine.view()))
    }

    /// An owned copy of the engine taken under the same read guard as
    /// `select`, when `select` returns `Some`.
    pub(crate) fn snapshot_with<T>(
        &self,
        select: impl FnOnce(&View<'_>) -> Option<T>,
    ) -> Option<(Project, T)> {
        self.read(|engine| select(&engine.view()).map(|selected| (engine.clone(), selected)))
    }

    fn read<R>(&self, read: impl FnOnce(&Project) -> R) -> R {
        let mut read = Some(read);
        let mut result = None;
        self.read_engine(&mut |engine| {
            let read = read.take().expect_invariant(
                "a load target ran one read twice",
                "each read primitive call applies its closure exactly once",
                "call the read closure once in LoadTarget::read_engine",
            );
            result = Some(read(engine));
        });
        result.expect_invariant(
            "a load target skipped a read",
            "each read primitive call applies its closure exactly once",
            "call the read closure once in LoadTarget::read_engine",
        )
    }

    fn write<R>(&self, write: impl FnOnce(&mut Project) -> R) -> R {
        let mut write = Some(write);
        let mut result = None;
        self.write_engine(NamedWrite(()), &mut |engine| {
            let write = write.take().expect_invariant(
                "a load target ran one named write twice",
                "each write primitive call applies its closure exactly once",
                "call the write closure once in LoadTarget::write_engine",
            );
            result = Some(write(engine));
        });
        result.expect_invariant(
            "a load target skipped a named write",
            "each write primitive call applies its closure exactly once",
            "call the write closure once in LoadTarget::write_engine",
        )
    }

    /// Replace the facts of `file_id` and resolve as `resolution` asks.
    pub(crate) fn replace_file_facts(
        &self,
        file_id: SourceFileId,
        facts: FileAnalysis,
        resolution: FileResolution,
    ) -> SemanticChange {
        self.write(|engine| match resolution {
            FileResolution::Full => engine.update(file_id, facts, ResolveMode::Immediate),
            FileResolution::CurrentFile => {
                let semantic_change = engine.update(file_id, facts, ResolveMode::Deferred);
                engine.resolve_file(file_id);
                semantic_change
            }
            FileResolution::Deferred => engine.update(file_id, facts, ResolveMode::Deferred),
        })
    }

    /// Replace the facts at `path` with deferred resolution only while
    /// `path` is still registered as `source_snapshot`. The check and the
    /// replacement hold one write guard. Returns whether facts were replaced.
    pub(crate) fn replace_facts_if_source_snapshot(
        &self,
        path: &Path,
        source_snapshot: SourceFileSnapshot,
        facts: FileAnalysis,
    ) -> bool {
        self.write(|engine| {
            if engine.view().source_snapshot_for_path(path) != Some(source_snapshot) {
                return false;
            }
            engine
                .update_if_snapshot(source_snapshot, facts, ResolveMode::Deferred)
                .is_some()
        })
    }

    /// Register the extension semantic seed source and replace its facts
    /// with `seed`. Resolution is deferred: the seed only has to be visible
    /// to the file walk that follows it. The registry hands a seed over only
    /// while it records that seed as the one this engine holds, so the loader
    /// calls this synchronously inside that window and it is never a late
    /// write.
    pub(crate) fn commit_extension_seed(&self, seed: ExtensionSemanticSeed) {
        self.write(|engine| {
            let file_id = engine.register_file(seed.source());
            engine.update(file_id, seed.analysis(file_id), ResolveMode::Deferred);
        });
    }

    /// Register `content` at `path`, replacing the source of a file already
    /// registered there.
    pub(crate) fn register_source_borrowed(
        &self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        self.write(|engine| engine.register_file_borrowed(path, content, kind))
    }

    /// The file registered at `path`, registering an empty `kind` source
    /// there when none is.
    pub(crate) fn register_path_if_absent(&self, path: PathBuf, kind: SourceKind) -> SourceFileId {
        self.write(|engine| {
            if let Some(file_id) = engine.view().file_id(&path) {
                return file_id;
            }
            engine.register_file(SourceFileInput {
                path,
                content: String::new(),
                kind,
            })
        })
    }

    /// Register every source, in order, under one write guard.
    pub(crate) fn register_sources(&self, sources: impl IntoIterator<Item = SourceFileInput>) {
        self.write(|engine| {
            for source in sources {
                engine.register_file(source);
            }
        });
    }

    /// Register every `(path, content)` as `kind`, in order, under one write
    /// guard.
    pub(crate) fn register_sources_borrowed<'a>(
        &self,
        sources: impl IntoIterator<Item = (&'a Path, &'a str)>,
        kind: SourceKind,
    ) {
        self.write(|engine| {
            for (path, content) in sources {
                engine.register_file_borrowed(path.to_path_buf(), content, kind);
            }
        });
    }

    /// Register an empty project source at each path not yet registered,
    /// then return a copy of the engine taken under the same write guard.
    pub(crate) fn register_project_paths_and_snapshot(&self, paths: &[PathBuf]) -> Project {
        self.write(|engine| {
            for path in paths {
                if engine.view().file_id(path).is_none() {
                    engine.register_file_borrowed(path.clone(), "", SourceKind::Project);
                }
            }
            engine.clone()
        })
    }

    /// Register `source` and return its identity with the facts it already
    /// holds, read under the same write guard.
    pub(crate) fn register_source_with_facts(
        &self,
        source: SourceFileInput,
    ) -> (SourceFileId, FileAnalysis) {
        self.write(|engine| {
            let file_id = engine.register_file(source);
            let query = engine.view();
            (
                file_id,
                FileAnalysis {
                    symbols: query.symbol_facts_in_file(file_id),
                    methods: query.method_facts_in_file(file_id),
                    method_visibility_overrides: query.method_visibility_overrides_in_file(file_id),
                    types: query.type_facts_in_file(file_id),
                    graph_nodes: query.graph_nodes_in_file(file_id),
                    graph_edges: query.graph_edges_in_file(file_id),
                    diagnostics: query.diagnostic_facts_in_file(file_id),
                    ..FileAnalysis::default()
                },
            )
        })
    }

    /// Register a batch of project sources in order under one write guard.
    ///
    /// An open-document candidate whose registered content differs is
    /// stale and skipped; a candidate whose registration changed since it
    /// was read is superseded and skipped. Each registered source is also
    /// registered into `mirror`, locked after this target, which must assign
    /// the same file identity. Returns each candidate's registered snapshot,
    /// or `None` when it was skipped.
    pub(crate) fn register_project_sources_if_snapshot(
        &self,
        candidates: &[ProjectSourceCandidate<'_>],
        mirror: Option<&RwLock<Project>>,
    ) -> Vec<Option<SourceFileSnapshot>> {
        self.write(|engine| {
            let mut mirror = mirror.map(RwLock::write);
            candidates
                .iter()
                .map(|candidate| {
                    if candidate.open_document {
                        if let Some(file_id) = engine.view().file_id(candidate.path) {
                            if !engine
                                .view()
                                .file_content_matches(file_id, candidate.content)
                            {
                                info!(
                                    "Skipping stale project snapshot for open document {}",
                                    candidate.path.display()
                                );
                                return None;
                            }
                        }
                    }
                    let Some(source_snapshot) = engine.register_file_borrowed_if_snapshot(
                        candidate.path.to_path_buf(),
                        candidate.content,
                        SourceKind::Project,
                        candidate.expected_snapshot,
                    ) else {
                        info!(
                            "Skipping project snapshot superseded before registration: {}",
                            candidate.path.display()
                        );
                        return None;
                    };
                    if let Some(mirror) = mirror.as_mut() {
                        let mirror_id = mirror.register_file_borrowed(
                            candidate.path.to_path_buf(),
                            candidate.content,
                            SourceKind::Project,
                        );
                        invariant_eq!(
                            engine.view().file_id(candidate.path).unwrap(),
                            mirror_id,
                            what = "immutable semantic context assigned a different file id for {}",
                            why = "retained FileAnalysis ranges must be valid in the live engine",
                            fix = "pre-register the tail in identical order in both engines",
                            candidate.path.display(),
                        );
                    }
                    Some(source_snapshot)
                })
                .collect()
        })
    }

    /// Register signature `content` at `path` and replace its facts, with
    /// deferred resolution, by `index` of the registered file, under one
    /// write guard. Nothing is replaced when `index` fails.
    pub(crate) fn commit_signature_source(
        &self,
        path: PathBuf,
        content: &str,
        index: impl FnOnce(SourceFileId) -> Result<FileAnalysis>,
    ) -> Result<()> {
        self.write(|engine| {
            let file_id = engine.register_file_borrowed(path, content, SourceKind::Signature);
            let facts = index(file_id)?;
            engine.update(file_id, facts, ResolveMode::Deferred);
            Ok(())
        })
    }

    /// Replace the facts at each registered path with deferred resolution
    /// under one write guard, then resolve the whole engine when `resolve`.
    pub(crate) fn replace_facts_by_path<'a>(
        &self,
        entries: impl IntoIterator<Item = (&'a Path, PathFacts)>,
        resolve: bool,
    ) {
        self.write(|engine| {
            for (path, facts) in entries {
                let file_id = engine.view().file_id(path).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "a batch fact commit lost the registered identity of {}",
                        why = "every batch registers its sources before collecting their facts",
                        fix = "preserve batch registration through the ordered commit",
                        path.display(),
                    )
                });
                let facts = match facts {
                    PathFacts::Facts(facts) => facts,
                    PathFacts::Template(template) => template.instantiate(file_id),
                };
                engine.update(file_id, facts, ResolveMode::Deferred);
            }
            if resolve {
                engine.resolve();
            }
        });
    }

    /// Resolve every deferred registration in the engine.
    pub(crate) fn resolve(&self) {
        self.write(Project::resolve);
    }

    /// Resolve the references of `file_ids` only.
    pub(crate) fn resolve_files(&self, file_ids: &[SourceFileId]) {
        self.write(|engine| engine.resolve_files(file_ids));
    }

    /// Release spare capacity once a load completes.
    pub(crate) fn compact(&self) {
        self.write(Project::shrink_to_fit);
    }

    /// Install `template` when the engine holds no file yet. Returns whether
    /// it was installed.
    pub(crate) fn install_template_if_empty(&self, template: &Project) -> bool {
        self.write(|engine| {
            if engine.view().file_count() != 0 {
                return false;
            }
            *engine = template.clone();
            true
        })
    }

    /// Bind one locked gem's shared dependency product with deferred
    /// resolution.
    pub(crate) fn bind_gem_product(
        &self,
        product: &GemDependencyProduct,
        manifest: GemDependencyManifest,
    ) -> Result<GemDependencyBinding> {
        self.write(|engine| product.bind_owned_deferred_into_measured(manifest, engine))
    }
}
