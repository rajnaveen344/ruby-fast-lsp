use anyhow::{anyhow, Result};
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{
    AnalysisEngine, ProjectNeutralFileFactsSnapshot, ProjectNeutralFileFactsTemplate, ResolveMode,
    SemanticExportFingerprint, SourceFileInput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::Url;

const GEM_DEPENDENCY_PRODUCT_SCHEMA: u32 = 6;
const PERSISTENT_GEM_DEPENDENCY_PRODUCT_SCHEMA: u32 = 6;
const RUBY_PRISM_SEMANTIC_VERSION: &str = "1.4.0";
const ANALYZER_DEPENDENCY_LOCK: &[u8] = include_bytes!("../Cargo.lock");
include!(concat!(env!("OUT_DIR"), "/gem_fact_producer_identity.rs"));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GemDependencyProductKey {
    schema: u32,
    analyzer_version: String,
    parser_version: String,
    analyzer_dependency_lock_sha256: [u8; 32],
    semantic_producer_source_sha256: [u8; 32],
    seed: SemanticExportFingerprint,
    closure_sha256: [u8; 32],
}

impl GemDependencyProductKey {
    fn cache_id(&self) -> String {
        let mut identity = Sha256::new();
        identity.update(self.schema.to_le_bytes());
        hash_field(&mut identity, self.analyzer_version.as_bytes());
        hash_field(&mut identity, self.parser_version.as_bytes());
        identity.update(self.analyzer_dependency_lock_sha256);
        identity.update(self.semantic_producer_source_sha256);
        identity.update(self.seed.stable_bytes());
        identity.update(self.closure_sha256);
        encode_hex(&identity.finalize())
    }

    fn persistent_identity(&self) -> PersistentGemDependencyProductKey {
        PersistentGemDependencyProductKey {
            schema: self.schema,
            analyzer_version: self.analyzer_version.clone(),
            parser_version: self.parser_version.clone(),
            analyzer_dependency_lock_sha256: self.analyzer_dependency_lock_sha256,
            semantic_producer_source_sha256: self.semantic_producer_source_sha256,
            seed: self.seed.stable_bytes(),
            closure_sha256: self.closure_sha256,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GemDependencySource {
    pub batch: u32,
    pub logical_path: String,
    pub physical_path: PathBuf,
    pub content: Arc<String>,
    pub content_sha256: [u8; 32],
    pub package_name: String,
    pub package_version: String,
}

impl GemDependencySource {
    pub fn new(
        batch: u32,
        logical_path: String,
        physical_path: PathBuf,
        content: String,
        package_name: impl Into<String>,
        package_version: impl Into<String>,
    ) -> Result<Self> {
        validate_logical_path(&logical_path)?;
        if !physical_path.is_absolute() {
            return Err(anyhow!(
                "dependency source path must be absolute: {}",
                physical_path.display()
            ));
        }
        let package_name = package_name.into();
        let package_version = package_version.into();
        assert!(
            !package_name.is_empty(),
            "INVARIANT VIOLATED: gem dependency source package name is empty. \
             This is a bug because locked gem sources must carry GemInfo.name. \
             Fix: pass gem_info.name into GemDependencySource::new."
        );
        assert!(
            !package_version.is_empty(),
            "INVARIANT VIOLATED: gem dependency source package version is empty. \
             This is a bug because locked gem sources must carry GemInfo.locked_version. \
             Fix: pass gem_info.locked_version into GemDependencySource::new."
        );
        let content_sha256 = Sha256::digest(content.as_bytes()).into();
        Ok(Self {
            batch,
            logical_path,
            physical_path,
            content: Arc::new(content),
            content_sha256,
            package_name,
            package_version,
        })
    }

    pub fn library_package(&self) -> ruby_analysis::core::LibraryPackageId {
        ruby_analysis::core::LibraryPackageId::new(
            self.package_name.clone(),
            self.package_version.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct GemDependencyManifest {
    key: GemDependencyProductKey,
    sources: Vec<GemDependencySource>,
}

impl GemDependencyManifest {
    pub fn new(
        seed: SemanticExportFingerprint,
        runtime_provider_fingerprint: Option<&str>,
        closure_identities: &[String],
        sources: Vec<GemDependencySource>,
    ) -> Result<Self> {
        Self::new_with_semantic_producer_identity(
            seed,
            runtime_provider_fingerprint,
            closure_identities,
            sources,
            GEM_FACT_PRODUCER_SOURCE_SHA256,
        )
    }

    fn new_with_semantic_producer_identity(
        seed: SemanticExportFingerprint,
        runtime_provider_fingerprint: Option<&str>,
        closure_identities: &[String],
        mut sources: Vec<GemDependencySource>,
        semantic_producer_source_sha256: [u8; 32],
    ) -> Result<Self> {
        sources.sort_by(|left, right| {
            left.batch
                .cmp(&right.batch)
                .then_with(|| left.logical_path.cmp(&right.logical_path))
        });
        let mut logical_paths = HashSet::new();
        for source in &sources {
            if !logical_paths.insert(source.logical_path.clone()) {
                return Err(anyhow!(
                    "dependency manifest contains duplicate logical path `{}`",
                    source.logical_path
                ));
            }
        }

        let mut closure = Sha256::new();
        closure.update(GEM_DEPENDENCY_PRODUCT_SCHEMA.to_le_bytes());
        hash_field(&mut closure, env!("CARGO_PKG_VERSION").as_bytes());
        hash_field(&mut closure, RUBY_PRISM_SEMANTIC_VERSION.as_bytes());
        let analyzer_dependency_lock_sha256: [u8; 32] =
            Sha256::digest(ANALYZER_DEPENDENCY_LOCK).into();
        closure.update(analyzer_dependency_lock_sha256);
        closure.update(semantic_producer_source_sha256);
        match runtime_provider_fingerprint {
            Some(fingerprint) => {
                closure.update([1]);
                hash_field(&mut closure, fingerprint.as_bytes());
            }
            None => closure.update([0]),
        }
        closure.update(
            u64::try_from(closure_identities.len())
                .expect(
                    "INVARIANT VIOLATED: dependency closure identity count exceeded u64. This is a bug because one process cannot hold that many locked gems. Fix: reject oversized lockfiles during discovery.",
                )
                .to_le_bytes(),
        );
        for identity in closure_identities {
            hash_field(&mut closure, identity.as_bytes());
        }
        for source in &sources {
            closure.update(source.batch.to_le_bytes());
            hash_field(&mut closure, source.logical_path.as_bytes());
            closure.update(source.content_sha256);
            closure.update(
                u64::try_from(source.content.len())
                    .expect(
                        "INVARIANT VIOLATED: dependency source length exceeded u64. This is a bug because one process cannot hold a source larger than u64. Fix: reject oversized dependency sources during discovery.",
                    )
                    .to_le_bytes(),
            );
        }
        let key = GemDependencyProductKey {
            schema: GEM_DEPENDENCY_PRODUCT_SCHEMA,
            analyzer_version: env!("CARGO_PKG_VERSION").to_string(),
            parser_version: RUBY_PRISM_SEMANTIC_VERSION.to_string(),
            analyzer_dependency_lock_sha256,
            semantic_producer_source_sha256,
            seed,
            closure_sha256: closure.finalize().into(),
        };
        Ok(Self { key, sources })
    }

    pub fn key(&self) -> &GemDependencyProductKey {
        &self.key
    }

    pub fn sources(&self) -> &[GemDependencySource] {
        &self.sources
    }

    pub(crate) fn cache_id(&self) -> String {
        self.key.cache_id()
    }
}

#[derive(Debug, Clone)]
pub struct GemDependencyFileTemplate {
    logical_path: String,
    content_sha256: [u8; 32],
    facts: ProjectNeutralFileFactsTemplate,
}

impl GemDependencyFileTemplate {
    pub fn new(
        logical_path: String,
        content_sha256: [u8; 32],
        facts: ProjectNeutralFileFactsTemplate,
    ) -> Self {
        Self {
            logical_path,
            content_sha256,
            facts,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GemDependencyProduct {
    key: GemDependencyProductKey,
    files: Vec<GemDependencyFileTemplate>,
    estimated_weight_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentGemDependencyProduct {
    schema: u32,
    cache_id: String,
    key: PersistentGemDependencyProductKey,
    files: Vec<PersistentGemDependencyFile>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PersistentGemDependencyProductKey {
    schema: u32,
    analyzer_version: String,
    parser_version: String,
    analyzer_dependency_lock_sha256: [u8; 32],
    semantic_producer_source_sha256: [u8; 32],
    seed: [u8; 16],
    closure_sha256: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentGemDependencyFile {
    logical_path: String,
    content_sha256: [u8; 32],
    facts: ProjectNeutralFileFactsSnapshot,
}

pub struct GemDependencyBinding {
    pub uris: Vec<Url>,
    pub validation_wall: Duration,
    pub insertion_wall: Duration,
}

#[derive(Debug, Default)]
pub struct GemDependencyBindingCounters {
    attempts: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    files: AtomicU64,
    validation_wall_ns: AtomicU64,
    validation_max_wall_ns: AtomicU64,
    insertion_wall_ns: AtomicU64,
    insertion_max_wall_ns: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemDependencyBindingSnapshot {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub files: u64,
    pub validation_wall_ns: u64,
    pub validation_max_wall_ns: u64,
    pub insertion_wall_ns: u64,
    pub insertion_max_wall_ns: u64,
}

impl GemDependencyBindingCounters {
    pub fn record_success(&self, binding: &GemDependencyBinding) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        self.successes.fetch_add(1, Ordering::Relaxed);
        self.files.fetch_add(
            u64::try_from(binding.uris.len()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        record_duration(
            &self.validation_wall_ns,
            &self.validation_max_wall_ns,
            binding.validation_wall,
        );
        record_duration(
            &self.insertion_wall_ns,
            &self.insertion_max_wall_ns,
            binding.insertion_wall,
        );
    }

    pub fn record_failure(&self) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> GemDependencyBindingSnapshot {
        GemDependencyBindingSnapshot {
            attempts: self.attempts.load(Ordering::Relaxed),
            successes: self.successes.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            validation_wall_ns: self.validation_wall_ns.load(Ordering::Relaxed),
            validation_max_wall_ns: self.validation_max_wall_ns.load(Ordering::Relaxed),
            insertion_wall_ns: self.insertion_wall_ns.load(Ordering::Relaxed),
            insertion_max_wall_ns: self.insertion_max_wall_ns.load(Ordering::Relaxed),
        }
    }
}

impl GemDependencyProduct {
    pub fn new(
        manifest: &GemDependencyManifest,
        mut files: Vec<GemDependencyFileTemplate>,
    ) -> Result<Self> {
        files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
        let mut expected = manifest
            .sources
            .iter()
            .map(|source| (source.logical_path.as_str(), source.content_sha256))
            .collect::<Vec<_>>();
        expected.sort_by_key(|(logical_path, _)| *logical_path);
        let actual = files
            .iter()
            .map(|file| (file.logical_path.as_str(), file.content_sha256))
            .collect::<Vec<_>>();
        if actual != expected {
            return Err(anyhow!(
                "dependency product files do not exactly match the checksum-verified manifest"
            ));
        }
        let estimated_weight_bytes = files
            .iter()
            .try_fold(0u64, |total, file| {
                let template = u64::try_from(file.facts.estimated_heap_bytes()).ok()?;
                let path = u64::try_from(file.logical_path.capacity()).ok()?;
                total.checked_add(template)?.checked_add(path)
            })
            .ok_or_else(|| anyhow!("dependency product weight exceeded u64"))?;
        Ok(Self {
            key: manifest.key.clone(),
            files,
            estimated_weight_bytes,
        })
    }

    pub fn estimated_weight_bytes(&self) -> u64 {
        self.estimated_weight_bytes
    }

    pub(crate) fn cache_id(&self) -> String {
        self.key.cache_id()
    }

    pub(crate) fn encode_persistent_payload(&self) -> Result<Vec<u8>> {
        let files = self
            .files
            .iter()
            .map(|file| {
                Ok(PersistentGemDependencyFile {
                    logical_path: file.logical_path.clone(),
                    content_sha256: file.content_sha256,
                    facts: file
                        .facts
                        .to_persistent_snapshot()
                        .map_err(anyhow::Error::msg)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        postcard::to_allocvec(&PersistentGemDependencyProduct {
            schema: PERSISTENT_GEM_DEPENDENCY_PRODUCT_SCHEMA,
            cache_id: self.key.cache_id(),
            key: self.key.persistent_identity(),
            files,
        })
        .map_err(|error| anyhow!("failed to encode persistent dependency product: {error}"))
    }

    pub(crate) fn decode_persistent_payload(
        manifest: &GemDependencyManifest,
        payload: &[u8],
    ) -> Result<Self> {
        let persisted: PersistentGemDependencyProduct = postcard::from_bytes(payload)
            .map_err(|error| anyhow!("failed to decode persistent dependency product: {error}"))?;
        if persisted.schema != PERSISTENT_GEM_DEPENDENCY_PRODUCT_SCHEMA {
            return Err(anyhow!(
                "persistent dependency product schema {} does not match {}",
                persisted.schema,
                PERSISTENT_GEM_DEPENDENCY_PRODUCT_SCHEMA
            ));
        }
        let expected_cache_id = manifest.cache_id();
        if persisted.cache_id != expected_cache_id {
            return Err(anyhow!(
                "persistent dependency product identity does not match requesting manifest"
            ));
        }
        if persisted.key != manifest.key.persistent_identity() {
            return Err(anyhow!(
                "persistent dependency product key components do not match requesting manifest"
            ));
        }
        if persisted.files.len() != manifest.sources.len() {
            return Err(anyhow!(
                "persistent dependency product contains {} files; manifest requires {}",
                persisted.files.len(),
                manifest.sources.len()
            ));
        }
        let files = persisted
            .files
            .into_iter()
            .map(|file| {
                Ok(GemDependencyFileTemplate::new(
                    file.logical_path,
                    file.content_sha256,
                    ProjectNeutralFileFactsTemplate::try_from_persistent_snapshot(file.facts)
                        .map_err(anyhow::Error::msg)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::new(manifest, files)
    }

    pub fn bind_into(
        &self,
        manifest: &GemDependencyManifest,
        engine: &mut AnalysisEngine,
    ) -> Result<Vec<Url>> {
        Ok(self.bind_into_measured(manifest, engine)?.uris)
    }

    pub fn bind_into_measured(
        &self,
        manifest: &GemDependencyManifest,
        engine: &mut AnalysisEngine,
    ) -> Result<GemDependencyBinding> {
        let validation_started = Instant::now();
        if self.key != manifest.key {
            return Err(anyhow!(
                "dependency product key does not match the requesting manifest"
            ));
        }
        let templates = self
            .files
            .iter()
            .map(|file| (file.logical_path.as_str(), file))
            .collect::<std::collections::HashMap<_, _>>();
        if templates.len() != self.files.len() || templates.len() != manifest.sources.len() {
            return Err(anyhow!(
                "dependency product source count does not match the requesting manifest"
            ));
        }

        // Validate the complete binding before registering the first consumer
        // file. A malformed or mismatched product must not leave a partially
        // populated isolated engine.
        let mut prepared = Vec::with_capacity(manifest.sources.len());
        for source in &manifest.sources {
            let template = templates.get(source.logical_path.as_str()).ok_or_else(|| {
                anyhow!(
                    "dependency product is missing logical source `{}`",
                    source.logical_path
                )
            })?;
            if template.content_sha256 != source.content_sha256 {
                return Err(anyhow!(
                    "dependency product checksum changed for logical source `{}`",
                    source.logical_path
                ));
            }
            let uri = Url::from_file_path(&source.physical_path).map_err(|_| {
                anyhow!(
                    "dependency source is not a valid file URI: {}",
                    source.physical_path.display()
                )
            })?;
            prepared.push((source, *template, uri));
        }
        let validation_wall = validation_started.elapsed();

        let insertion_started = Instant::now();
        let mut uris = Vec::with_capacity(prepared.len());
        for (source, template, uri) in prepared {
            let file_id = engine.register_gem_file(
                SourceFileInput {
                    path: source.physical_path.clone(),
                    content: source.content.to_string(),
                    kind: SourceKind::Gem,
                },
                source.library_package(),
            );
            engine.replace_facts(
                file_id,
                template.facts.instantiate(file_id),
                ResolveMode::Deferred,
            );
            uris.push(uri);
        }
        if !manifest.sources.is_empty() {
            engine.resolve();
        }
        Ok(GemDependencyBinding {
            uris,
            validation_wall,
            insertion_wall: insertion_started.elapsed(),
        })
    }

    pub fn bind_owned_into_measured(
        &self,
        manifest: GemDependencyManifest,
        engine: &mut AnalysisEngine,
    ) -> Result<GemDependencyBinding> {
        self.bind_owned_into_measured_with_resolution(manifest, engine, true)
    }

    pub fn bind_owned_deferred_into_measured(
        &self,
        manifest: GemDependencyManifest,
        engine: &mut AnalysisEngine,
    ) -> Result<GemDependencyBinding> {
        self.bind_owned_into_measured_with_resolution(manifest, engine, false)
    }

    fn bind_owned_into_measured_with_resolution(
        &self,
        manifest: GemDependencyManifest,
        engine: &mut AnalysisEngine,
        resolve: bool,
    ) -> Result<GemDependencyBinding> {
        let validation_started = Instant::now();
        if self.key != manifest.key {
            return Err(anyhow!(
                "dependency product key does not match the requesting manifest"
            ));
        }
        let templates = self
            .files
            .iter()
            .map(|file| (file.logical_path.as_str(), file))
            .collect::<std::collections::HashMap<_, _>>();
        if templates.len() != self.files.len() || templates.len() != manifest.sources.len() {
            return Err(anyhow!(
                "dependency product source count does not match the requesting manifest"
            ));
        }

        let mut prepared = Vec::with_capacity(manifest.sources.len());
        for source in manifest.sources {
            let template = templates.get(source.logical_path.as_str()).ok_or_else(|| {
                anyhow!(
                    "dependency product is missing logical source `{}`",
                    source.logical_path
                )
            })?;
            if template.content_sha256 != source.content_sha256 {
                return Err(anyhow!(
                    "dependency product checksum changed for logical source `{}`",
                    source.logical_path
                ));
            }
            let uri = Url::from_file_path(&source.physical_path).map_err(|_| {
                anyhow!(
                    "dependency source is not a valid file URI: {}",
                    source.physical_path.display()
                )
            })?;
            prepared.push((source, *template, uri));
        }
        let validation_wall = validation_started.elapsed();

        let insertion_started = Instant::now();
        let mut uris = Vec::with_capacity(prepared.len());
        for (source, template, uri) in prepared {
            let package = source.library_package();
            let content =
                Arc::try_unwrap(source.content).unwrap_or_else(|shared| shared.as_ref().clone());
            let file_id = engine.register_gem_file(
                SourceFileInput {
                    path: source.physical_path,
                    content,
                    kind: SourceKind::Gem,
                },
                package,
            );
            engine.replace_facts(
                file_id,
                template.facts.instantiate(file_id),
                ResolveMode::Deferred,
            );
            uris.push(uri);
        }
        if resolve && !uris.is_empty() {
            engine.resolve();
        }
        Ok(GemDependencyBinding {
            uris,
            validation_wall,
            insertion_wall: insertion_started.elapsed(),
        })
    }
}

fn record_duration(total: &AtomicU64, maximum: &AtomicU64, elapsed: Duration) {
    let nanoseconds = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    let _ = total.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(nanoseconds))
    });
    maximum.fetch_max(nanoseconds, Ordering::Relaxed);
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
    hasher.update(
        u64::try_from(field.len())
            .expect(
                "INVARIANT VIOLATED: dependency product key field exceeded u64. This is a bug because one process cannot hold a key field larger than u64. Fix: reject oversized dependency metadata.",
            )
            .to_le_bytes(),
    );
    hasher.update(field);
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn validate_logical_path(logical_path: &str) -> Result<()> {
    if logical_path.is_empty() {
        return Err(anyhow!("dependency logical path must not be empty"));
    }
    let path = Path::new(logical_path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return Err(anyhow!(
            "dependency logical path must be normalized and relative: `{logical_path}`"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::{
        FullyQualifiedName, GraphNodeFact, GraphNodeKind, RubyConstant, SourceFileId, SymbolFact,
        SymbolKind, TextRange,
    };
    use ruby_analysis::engine::{AnalysisQuery, FileFacts, ProjectNeutralFileFactsTemplate};

    fn empty_seed() -> SemanticExportFingerprint {
        AnalysisEngine::new().semantic_context_fingerprint()
    }

    fn source(path: &str, physical: &str, content: &str) -> GemDependencySource {
        GemDependencySource::new(
            0,
            path.to_string(),
            PathBuf::from(physical),
            content.to_string(),
            "widget",
            "1.0.0",
        )
        .unwrap()
    }

    fn widget_template() -> ProjectNeutralFileFactsTemplate {
        let file_id = SourceFileId(91);
        let range = TextRange::new(file_id, 0, 12);
        let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
        ProjectNeutralFileFactsTemplate::try_new(
            file_id,
            FileFacts {
                symbols: vec![SymbolFact::new(fqn.clone(), SymbolKind::Class, range)],
                graph_nodes: vec![GraphNodeFact::new(fqn, GraphNodeKind::Class, range)],
                ..FileFacts::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn manifest_identity_changes_with_content_order_batch_or_seed() {
        let seed = empty_seed();
        let first = GemDependencyManifest::new(
            seed,
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source("widget/1/lib/widget.rb", "/a/widget.rb", "one")],
        )
        .unwrap();
        let changed_content = GemDependencyManifest::new(
            seed,
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source("widget/1/lib/widget.rb", "/b/widget.rb", "two")],
        )
        .unwrap();
        assert_ne!(first.key(), changed_content.key());

        let mut different_batch_source = source("widget/1/lib/widget.rb", "/a/widget.rb", "one");
        different_batch_source.batch = 1;
        let changed_batch = GemDependencyManifest::new(
            seed,
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![different_batch_source],
        )
        .unwrap();
        assert_ne!(first.key(), changed_batch.key());

        let mut seed_engine = AnalysisEngine::new();
        let seed_file = seed_engine.register_file(SourceFileInput {
            path: PathBuf::from("/seed.rb"),
            content: "class Seed; end".to_string(),
            kind: SourceKind::Stub,
        });
        let seed_range = TextRange::new(seed_file, 0, 12);
        let seed_fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Seed").unwrap()]);
        seed_engine.replace_facts(
            seed_file,
            FileFacts {
                symbols: vec![SymbolFact::new(seed_fqn, SymbolKind::Class, seed_range)],
                ..FileFacts::default()
            },
            ResolveMode::Deferred,
        );
        let changed_seed = GemDependencyManifest::new(
            seed_engine.semantic_context_fingerprint(),
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source("widget/1/lib/widget.rb", "/a/widget.rb", "one")],
        )
        .unwrap();
        assert_ne!(first.key(), changed_seed.key());

        let changed_provider = GemDependencyManifest::new(
            seed,
            Some("jruby-classpath-a"),
            &["widget:1:ruby:registry".to_string()],
            vec![source("widget/1/lib/widget.rb", "/a/widget.rb", "one")],
        )
        .unwrap();
        assert_ne!(first.key(), changed_provider.key());

        let changed_closure = GemDependencyManifest::new(
            seed,
            None,
            &[
                "widget:1:ruby:registry".to_string(),
                "missing:optional".to_string(),
            ],
            vec![source("widget/1/lib/widget.rb", "/a/widget.rb", "one")],
        )
        .unwrap();
        assert_ne!(first.key(), changed_closure.key());
        assert_eq!(first.key().parser_version, RUBY_PRISM_SEMANTIC_VERSION);
        assert!(
            std::str::from_utf8(ANALYZER_DEPENDENCY_LOCK)
                .unwrap()
                .contains(&format!(
                    "name = \"ruby-prism\"\nversion = \"{RUBY_PRISM_SEMANTIC_VERSION}\""
                )),
            "the explicit parser semantic identity must match the locked parser version"
        );
    }

    #[test]
    fn manifest_identity_changes_with_semantic_producer_source() {
        let seed = empty_seed();
        let first = GemDependencyManifest::new_with_semantic_producer_identity(
            seed,
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source(
                "widget/1/lib/widget.rb",
                "/a/widget.rb",
                "class Widget; end",
            )],
            [0x11; 32],
        )
        .unwrap();
        let changed_producer = GemDependencyManifest::new_with_semantic_producer_identity(
            seed,
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source(
                "widget/1/lib/widget.rb",
                "/a/widget.rb",
                "class Widget; end",
            )],
            [0x22; 32],
        )
        .unwrap();

        assert_ne!(first.cache_id(), changed_producer.cache_id());

        let product = GemDependencyProduct::new(
            &first,
            vec![GemDependencyFileTemplate::new(
                "widget/1/lib/widget.rb".to_string(),
                first.sources()[0].content_sha256,
                widget_template(),
            )],
        )
        .unwrap();
        let payload = product.encode_persistent_payload().unwrap();
        let error =
            GemDependencyProduct::decode_persistent_payload(&changed_producer, payload.as_slice())
                .unwrap_err();
        assert!(error
            .to_string()
            .contains("identity does not match requesting manifest"));
    }

    #[test]
    fn one_product_binds_navigation_to_each_consumers_exact_path() {
        let first_manifest = GemDependencyManifest::new(
            empty_seed(),
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source(
                "widget/1/lib/widget.rb",
                "/projects/one/vendor/widget.rb",
                "class Widget; end",
            )],
        )
        .unwrap();
        let second_manifest = GemDependencyManifest::new(
            empty_seed(),
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source(
                "widget/1/lib/widget.rb",
                "/projects/two/vendor/widget.rb",
                "class Widget; end",
            )],
        )
        .unwrap();
        assert_eq!(first_manifest.key(), second_manifest.key());
        let product = GemDependencyProduct::new(
            &first_manifest,
            vec![GemDependencyFileTemplate::new(
                "widget/1/lib/widget.rb".to_string(),
                first_manifest.sources()[0].content_sha256,
                widget_template(),
            )],
        )
        .unwrap();

        let mut first = AnalysisEngine::new();
        product.bind_into(&first_manifest, &mut first).unwrap();
        let mut second = AnalysisEngine::new();
        second.register_file(SourceFileInput {
            path: PathBuf::from("/projects/two/project.rb"),
            content: String::new(),
            kind: SourceKind::Project,
        });
        product.bind_into(&second_manifest, &mut second).unwrap();

        let parts = [RubyConstant::new("Widget").unwrap()];
        let first_definition =
            AnalysisQuery::new(&first).constant_definition_ranges(&parts, &[])[0];
        let second_definition =
            AnalysisQuery::new(&second).constant_definition_ranges(&parts, &[])[0];
        assert_eq!(
            first.file(first_definition.file_id).unwrap().path,
            PathBuf::from("/projects/one/vendor/widget.rb")
        );
        assert_eq!(
            second.file(second_definition.file_id).unwrap().path,
            PathBuf::from("/projects/two/vendor/widget.rb")
        );
        assert_ne!(first_definition.file_id, second_definition.file_id);
    }

    #[test]
    fn binding_validates_the_complete_product_before_mutating_consumer_engine() {
        let manifest = GemDependencyManifest::new(
            empty_seed(),
            None,
            &["widget:1:ruby:registry".to_string()],
            vec![source(
                "widget/1/lib/widget.rb",
                "/projects/one/vendor/widget.rb",
                "class Widget; end",
            )],
        )
        .unwrap();
        let mut product = GemDependencyProduct::new(
            &manifest,
            vec![GemDependencyFileTemplate::new(
                "widget/1/lib/widget.rb".to_string(),
                manifest.sources()[0].content_sha256,
                widget_template(),
            )],
        )
        .unwrap();
        product.files.clear();

        let mut engine = AnalysisEngine::new();
        let error = product.bind_into(&manifest, &mut engine).unwrap_err();

        assert!(error
            .to_string()
            .contains("source count does not match the requesting manifest"));
        assert_eq!(engine.file_count(), 0);
    }
}
