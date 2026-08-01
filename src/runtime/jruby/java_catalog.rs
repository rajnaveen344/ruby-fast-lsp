use super::classpath::{ArtifactKind, ClasspathArtifact, ProjectClasspath};
use crate::single_flight::{BlockingBoundedSingleFlightCache, SingleFlightSnapshot};
use anyhow::{anyhow, Context, Result as AnyResult};
use ruby_fast_lsp_jvm_metadata::{
    parse_archive, ArchiveKind, ArchiveLimits, ArchiveMetadata, ClassFile,
    ARCHIVE_PRODUCT_SEMANTIC_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::Arc;

const JAVA_ARTIFACT_PRODUCT_SCHEMA: u32 = 1;
const DEFAULT_JAVA_ARTIFACT_CACHE_ENTRIES: usize = 256;
const DEFAULT_JAVA_ARTIFACT_CACHE_WEIGHT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaClassDeclaration {
    pub class: Arc<ClassFile>,
    pub artifact_path: PathBuf,
    pub artifact_fingerprint_sha256: String,
    pub entry_name: String,
    pub release: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateJavaClass {
    pub name: String,
    pub winner: PathBuf,
    pub shadowed: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectJavaCatalog {
    pub classpath_fingerprint_sha256: String,
    pub classes: BTreeMap<String, JavaClassDeclaration>,
    pub duplicates: Vec<DuplicateJavaClass>,
}

pub struct ProjectJavaCatalogBuilder<'a> {
    classpath: &'a ProjectClasspath,
    next_artifact: usize,
    classes: BTreeMap<String, JavaClassDeclaration>,
    duplicates: Vec<DuplicateJavaClass>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaCatalogError {
    Read { path: PathBuf, message: String },
    Archive { path: PathBuf, message: String },
    ArtifactFingerprintMismatch { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JavaArtifactProductKey {
    cache_id: String,
    artifact_fingerprint_sha256: String,
    artifact_kind: ArtifactKind,
    jdk_feature: u16,
    limits_fingerprint_sha256: String,
}

#[derive(Debug, Clone)]
pub struct JavaArtifactProduct {
    key: JavaArtifactProductKey,
    archive: ArchiveMetadata,
    estimated_weight_bytes: u64,
}

/// Bounded process-owned reuse for exact immutable Java artifact metadata.
/// Project classpath order, paths, duplicate selection, catalogs, and engines
/// stay consumer-owned; only parsed `ClassFile` allocations are shared.
#[derive(Clone)]
pub struct JavaArtifactProductCache {
    inner: BlockingBoundedSingleFlightCache<JavaArtifactProductKey, JavaArtifactProduct, String>,
}

impl Default for JavaArtifactProductCache {
    fn default() -> Self {
        Self::new(
            DEFAULT_JAVA_ARTIFACT_CACHE_ENTRIES,
            DEFAULT_JAVA_ARTIFACT_CACHE_WEIGHT_BYTES,
        )
    }
}

impl JavaArtifactProductCache {
    pub fn new(max_entries: usize, max_weight_bytes: u64) -> Self {
        Self {
            inner: BlockingBoundedSingleFlightCache::new(
                max_entries,
                max_weight_bytes,
                JavaArtifactProduct::estimated_weight_bytes,
            ),
        }
    }

    pub fn get_or_try_init(
        &self,
        key: JavaArtifactProductKey,
        producer: impl FnOnce() -> Result<JavaArtifactProduct, String>,
    ) -> Result<Arc<JavaArtifactProduct>, String> {
        self.inner.get_or_try_init(key, producer)
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        self.inner.snapshot()
    }

    pub fn retained_weight_bytes(&self) -> u64 {
        self.inner.retained_weight()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistentJavaArtifactProduct {
    schema: u32,
    cache_id: String,
    artifact_fingerprint_sha256: String,
    artifact_kind: u8,
    jdk_feature: u16,
    limits_fingerprint_sha256: String,
    archive: ArchiveMetadata,
}

impl JavaArtifactProductKey {
    pub fn new(artifact: &ClasspathArtifact, jdk_feature: u16, limits: ArchiveLimits) -> Self {
        let limits_fingerprint_sha256 = archive_limits_fingerprint(limits);
        let mut identity = Sha256::new();
        identity.update(JAVA_ARTIFACT_PRODUCT_SCHEMA.to_le_bytes());
        hash_field(&mut identity, env!("CARGO_PKG_VERSION").as_bytes());
        hash_field(&mut identity, ARCHIVE_PRODUCT_SEMANTIC_VERSION.as_bytes());
        hash_field(&mut identity, artifact.fingerprint_sha256.as_bytes());
        identity.update([artifact_kind_tag(artifact.kind)]);
        identity.update(jdk_feature.to_le_bytes());
        hash_field(&mut identity, limits_fingerprint_sha256.as_bytes());
        Self {
            cache_id: format!("{:x}", identity.finalize()),
            artifact_fingerprint_sha256: artifact.fingerprint_sha256.clone(),
            artifact_kind: artifact.kind,
            jdk_feature,
            limits_fingerprint_sha256,
        }
    }

    pub fn cache_id(&self) -> &str {
        &self.cache_id
    }
}

impl JavaArtifactProduct {
    pub fn cache_id(&self) -> &str {
        self.key.cache_id()
    }

    pub fn build(
        artifact: &ClasspathArtifact,
        key: &JavaArtifactProductKey,
        archive_limits: ArchiveLimits,
    ) -> Result<Self, JavaCatalogError> {
        let expected = JavaArtifactProductKey::new(artifact, key.jdk_feature, archive_limits);
        assert_eq!(
            key, &expected,
            "INVARIANT VIOLATED: Java artifact product key does not describe the artifact and \
             parser limits being built. This is a bug because persistent metadata would be \
             published under the wrong semantic identity. Fix: derive the key from the exact \
             artifact, JDK feature, and archive limits immediately before construction."
        );
        let bytes = fs::read(&artifact.path).map_err(|error| JavaCatalogError::Read {
            path: artifact.path.clone(),
            message: error.to_string(),
        })?;
        if format!("{:x}", Sha256::digest(&bytes)) != artifact.fingerprint_sha256 {
            return Err(JavaCatalogError::ArtifactFingerprintMismatch {
                path: artifact.path.clone(),
            });
        }
        let archive = parse_archive(
            &bytes,
            artifact_kind(artifact.kind),
            key.jdk_feature,
            archive_limits,
        )
        .map_err(|error| JavaCatalogError::Archive {
            path: artifact.path.clone(),
            message: format!("{error:?}"),
        })?;
        assert_eq!(
            archive.fingerprint_sha256, artifact.fingerprint_sha256,
            "INVARIANT VIOLATED: archive parser content identity differs from the pre-parse \
             SHA-256. This is a bug because both hashes cover the same immutable bytes. Fix: keep \
             artifact and archive fingerprint algorithms identical."
        );
        let estimated_weight_bytes = estimate_product_weight(key, &archive);
        Ok(Self {
            key: key.clone(),
            archive,
            estimated_weight_bytes,
        })
    }

    pub fn estimated_weight_bytes(&self) -> u64 {
        self.estimated_weight_bytes
    }

    pub fn encode_persistent_payload(&self) -> AnyResult<Vec<u8>> {
        postcard::to_allocvec(&PersistentJavaArtifactProduct {
            schema: JAVA_ARTIFACT_PRODUCT_SCHEMA,
            cache_id: self.key.cache_id.clone(),
            artifact_fingerprint_sha256: self.key.artifact_fingerprint_sha256.clone(),
            artifact_kind: artifact_kind_tag(self.key.artifact_kind),
            jdk_feature: self.key.jdk_feature,
            limits_fingerprint_sha256: self.key.limits_fingerprint_sha256.clone(),
            archive: self.archive.clone(),
        })
        .context("serializing persistent Java artifact metadata")
    }

    pub fn decode_persistent_payload(
        key: &JavaArtifactProductKey,
        payload: &[u8],
    ) -> AnyResult<Self> {
        let persisted: PersistentJavaArtifactProduct = postcard::from_bytes(payload)
            .context("deserializing persistent Java artifact metadata")?;
        if persisted.schema != JAVA_ARTIFACT_PRODUCT_SCHEMA {
            return Err(anyhow!(
                "persistent Java artifact schema {} does not match {}",
                persisted.schema,
                JAVA_ARTIFACT_PRODUCT_SCHEMA
            ));
        }
        if persisted.cache_id != key.cache_id
            || persisted.artifact_fingerprint_sha256 != key.artifact_fingerprint_sha256
            || persisted.artifact_kind != artifact_kind_tag(key.artifact_kind)
            || persisted.jdk_feature != key.jdk_feature
            || persisted.limits_fingerprint_sha256 != key.limits_fingerprint_sha256
            || persisted.archive.fingerprint_sha256 != key.artifact_fingerprint_sha256
        {
            return Err(anyhow!(
                "persistent Java artifact metadata identity does not match the requested product"
            ));
        }
        let estimated_weight_bytes = estimate_product_weight(key, &persisted.archive);
        Ok(Self {
            key: key.clone(),
            archive: persisted.archive,
            estimated_weight_bytes,
        })
    }
}

fn estimate_product_weight(key: &JavaArtifactProductKey, archive: &ArchiveMetadata) -> u64 {
    let mut bytes = size_of::<JavaArtifactProduct>();
    for capacity in [
        key.cache_id.capacity(),
        key.artifact_fingerprint_sha256.capacity(),
        key.limits_fingerprint_sha256.capacity(),
        archive.fingerprint_sha256.capacity(),
    ] {
        add_product_weight(&mut bytes, capacity, "identity string");
    }
    add_product_weight(
        &mut bytes,
        archive
            .classes
            .capacity()
            .checked_mul(size_of::<ruby_fast_lsp_jvm_metadata::ArchiveClass>())
            .expect(
                "INVARIANT VIOLATED: Java archive class-vector weight overflowed usize. This is a bug because archive class counts are bounded. Fix: inspect archive metadata capacity accounting.",
            ),
        "archive class vector",
    );
    for archived in &archive.classes {
        add_product_weight(
            &mut bytes,
            archived.entry_name.capacity(),
            "archive entry name",
        );
        add_product_weight(
            &mut bytes,
            size_of::<usize>() * 2,
            "shared ClassFile control block",
        );
        add_product_weight(
            &mut bytes,
            usize::try_from(archived.class.estimated_heap_bytes()).expect(
                "INVARIANT VIOLATED: a ClassFile heap estimate does not fit usize. This is a bug because the parsed class exists in this process. Fix: inspect cross-architecture metadata weight conversion.",
            ),
            "shared ClassFile metadata",
        );
    }
    u64::try_from(bytes).expect(
        "INVARIANT VIOLATED: Java artifact product weight does not fit u64. This is a bug because one product cannot exceed the process address space. Fix: inspect product weight arithmetic.",
    )
}

fn add_product_weight(bytes: &mut usize, additional: usize, label: &'static str) {
    *bytes = bytes.checked_add(additional).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: Java artifact {label} weight overflowed usize. This is a bug because archive inputs and class counts are bounded. Fix: inspect product weight accounting."
        )
    });
}

impl<'a> ProjectJavaCatalogBuilder<'a> {
    pub fn new(classpath: &'a ProjectClasspath) -> Self {
        Self {
            classpath,
            next_artifact: 0,
            classes: BTreeMap::new(),
            duplicates: Vec::new(),
        }
    }

    pub fn push(&mut self, product: JavaArtifactProduct) -> Result<(), JavaCatalogError> {
        let artifact = self.classpath.artifacts.get(self.next_artifact).expect(
            "INVARIANT VIOLATED: Java catalog received more products than ordered classpath \
                 artifacts. This is a bug because extra products have no defensible precedence. \
                 Fix: produce exactly one product per classpath artifact in order.",
        );
        add_artifact_product(artifact, product, &mut self.classes, &mut self.duplicates)?;
        self.next_artifact += 1;
        Ok(())
    }

    pub fn finish(mut self) -> ProjectJavaCatalog {
        assert_eq!(
            self.next_artifact,
            self.classpath.artifacts.len(),
            "INVARIANT VIOLATED: Java catalog completed before every ordered classpath artifact \
             supplied one product. This is a bug because missing products silently change Java \
             lookup semantics. Fix: resolve and push one immutable product for every artifact."
        );
        self.duplicates.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.winner.cmp(&right.winner))
                .then_with(|| left.shadowed.cmp(&right.shadowed))
        });
        ProjectJavaCatalog {
            classpath_fingerprint_sha256: self.classpath.fingerprint_sha256.clone(),
            classes: self.classes,
            duplicates: self.duplicates,
        }
    }
}

pub fn build_project_java_catalog(
    classpath: &ProjectClasspath,
    jdk_feature: u16,
    archive_limits: ArchiveLimits,
) -> Result<ProjectJavaCatalog, JavaCatalogError> {
    let mut builder = ProjectJavaCatalogBuilder::new(classpath);
    for artifact in &classpath.artifacts {
        let key = JavaArtifactProductKey::new(artifact, jdk_feature, archive_limits);
        builder.push(JavaArtifactProduct::build(artifact, &key, archive_limits)?)?;
    }
    Ok(builder.finish())
}

pub fn verify_artifact_discovery_identity(
    artifact: &ClasspathArtifact,
) -> Result<(), JavaCatalogError> {
    let metadata = fs::metadata(&artifact.path).map_err(|error| JavaCatalogError::Read {
        path: artifact.path.clone(),
        message: error.to_string(),
    })?;
    let modified = metadata
        .modified()
        .map_err(|error| JavaCatalogError::Read {
            path: artifact.path.clone(),
            message: error.to_string(),
        })?;
    if metadata.len() != artifact.file_identity.byte_length
        || modified != artifact.file_identity.modified
    {
        return Err(JavaCatalogError::ArtifactFingerprintMismatch {
            path: artifact.path.clone(),
        });
    }
    Ok(())
}

pub fn build_project_java_catalog_from_products(
    classpath: &ProjectClasspath,
    products: Vec<JavaArtifactProduct>,
) -> Result<ProjectJavaCatalog, JavaCatalogError> {
    assert_eq!(
        classpath.artifacts.len(),
        products.len(),
        "INVARIANT VIOLATED: Java artifact product count differs from the ordered classpath \
         artifact count. This is a bug because missing or extra products would change Java \
         classpath precedence. Fix: resolve exactly one immutable product for every ordered \
         classpath artifact."
    );
    let mut builder = ProjectJavaCatalogBuilder::new(classpath);
    for product in products {
        builder.push(product)?;
    }
    Ok(builder.finish())
}

fn add_artifact_product(
    artifact: &ClasspathArtifact,
    product: JavaArtifactProduct,
    classes: &mut BTreeMap<String, JavaClassDeclaration>,
    duplicates: &mut Vec<DuplicateJavaClass>,
) -> Result<(), JavaCatalogError> {
    assert_eq!(
        product.key.artifact_fingerprint_sha256, artifact.fingerprint_sha256,
        "INVARIANT VIOLATED: Java artifact product is bound to a different content identity. \
         This is a bug because cached class metadata must never be substituted across artifacts. \
         Fix: validate the exact artifact key before composing the project catalog."
    );
    assert_eq!(
        product.key.artifact_kind, artifact.kind,
        "INVARIANT VIOLATED: Java artifact product kind differs from the consumer classpath kind. \
         This is a bug because JAR and JMOD entry policies are not interchangeable. Fix: include \
         and validate artifact kind in the persistent product identity."
    );
    for archived in product.archive.classes {
        let name = archived.class.name.clone();
        let declaration = JavaClassDeclaration {
            class: archived.class,
            artifact_path: artifact.path.clone(),
            artifact_fingerprint_sha256: artifact.fingerprint_sha256.clone(),
            entry_name: archived.entry_name,
            release: archived.release,
        };
        if let Some(existing) = classes.get(&name) {
            duplicates.push(DuplicateJavaClass {
                name,
                winner: existing.artifact_path.clone(),
                shadowed: artifact.path.clone(),
            });
        } else {
            classes.insert(name, declaration);
        }
    }
    Ok(())
}

fn artifact_kind(kind: ArtifactKind) -> ArchiveKind {
    match kind {
        ArtifactKind::Jar => ArchiveKind::Jar,
        ArtifactKind::Jmod => ArchiveKind::Jmod,
    }
}

fn artifact_kind_tag(kind: ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::Jar => 1,
        ArtifactKind::Jmod => 2,
    }
}

fn hash_field(identity: &mut Sha256, value: &[u8]) {
    identity.update(
        u64::try_from(value.len())
            .expect(
                "INVARIANT VIOLATED: Java artifact identity field length exceeded u64. This is a \
                 bug because one process cannot hold such a field. Fix: reject oversized \
                 classpath identity input before hashing.",
            )
            .to_le_bytes(),
    );
    identity.update(value);
}

fn archive_limits_fingerprint(limits: ArchiveLimits) -> String {
    let mut identity = Sha256::new();
    for value in [
        limits.max_archive_bytes,
        limits.max_entries,
        limits.max_entry_bytes,
        limits.max_total_decompressed_bytes,
        limits.max_class_count,
        limits.class.max_class_bytes,
        limits.class.max_constant_pool_entries,
        limits.class.max_members,
        limits.class.max_attributes,
        limits.class.max_attribute_bytes,
        limits.class.max_annotations,
        limits.class.max_annotation_depth,
    ] {
        identity.update(
            u64::try_from(value)
                .expect(
                    "INVARIANT VIOLATED: Java archive limit exceeded u64. This is a bug because \
                     persistent product identities must be architecture-independent. Fix: keep \
                     bounded archive limits representable as u64.",
                )
                .to_le_bytes(),
        );
    }
    format!("{:x}", identity.finalize())
}

#[cfg(test)]
mod tests {
    use super::super::classpath::{
        ArtifactOrigin, ClasspathArtifact, ProjectClasspath, SourceRoot, UnresolvedCoordinate,
    };
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::{Cursor, Write};
    use std::sync::Arc;
    use zip::write::SimpleFileOptions;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn jar(entry: &str, contents: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(entry, SimpleFileOptions::default())
            .expect("fixture JAR entry must start");
        writer
            .write_all(contents)
            .expect("fixture JAR entry must write");
        writer
            .finish()
            .expect("fixture JAR must finish")
            .into_inner()
    }

    fn jar_with_marker(contents: &[u8], marker: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("com/example/Demo.class", SimpleFileOptions::default())
            .expect("fixture JAR class entry must start");
        writer
            .write_all(contents)
            .expect("fixture JAR class entry must write");
        writer
            .start_file(format!("META-INF/{marker}"), SimpleFileOptions::default())
            .expect("fixture JAR marker entry must start");
        writer
            .write_all(marker.as_bytes())
            .expect("fixture JAR marker entry must write");
        writer
            .finish()
            .expect("fixture JAR must finish")
            .into_inner()
    }

    fn artifact(path: PathBuf, bytes: &[u8], origin: ArtifactOrigin) -> ClasspathArtifact {
        fs::write(&path, bytes).expect("fixture artifact must be written");
        let file_identity = super::super::classpath::SourceFileIdentity {
            byte_length: bytes.len() as u64,
            modified: fs::metadata(&path).unwrap().modified().unwrap(),
        };
        ClasspathArtifact {
            path,
            origin,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(bytes)),
            byte_length: bytes.len() as u64,
            file_identity,
        }
    }

    fn classpath(root: PathBuf, artifacts: Vec<ClasspathArtifact>) -> ProjectClasspath {
        ProjectClasspath {
            project_root: root,
            artifacts,
            sources: Vec::<SourceRoot>::new(),
            unresolved: Vec::<UnresolvedCoordinate>::new(),
            fingerprint_sha256: "fixture-classpath".to_string(),
        }
    }

    #[test]
    fn preserves_classpath_precedence_and_reports_duplicate_class_identity() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let first_bytes = jar("com/example/Demo.class", &class);
        let second_bytes = jar("com/example/Demo.class", &class);
        let first = artifact(
            fixture.path().join("first.jar"),
            &first_bytes,
            ArtifactOrigin::JrubyRuntime,
        );
        let second = artifact(
            fixture.path().join("second.jar"),
            &second_bytes,
            ArtifactOrigin::Explicit,
        );
        let classpath = classpath(
            fixture.path().to_path_buf(),
            vec![first.clone(), second.clone()],
        );

        let catalog = build_project_java_catalog(&classpath, 17, ArchiveLimits::default())
            .expect("fixture Java catalog must build");
        assert_eq!(catalog.classes.len(), 1);
        assert_eq!(
            catalog.classes["com/example/Demo"].artifact_path,
            first.path
        );
        assert_eq!(
            catalog.duplicates,
            vec![DuplicateJavaClass {
                name: "com/example/Demo".to_string(),
                winner: first.path,
                shadowed: second.path,
            }]
        );
    }

    #[test]
    fn rejects_artifact_changed_after_classpath_discovery() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let bytes = jar("com/example/Demo.class", &class);
        let artifact = artifact(
            fixture.path().join("changed.jar"),
            &bytes,
            ArtifactOrigin::Explicit,
        );
        fs::write(&artifact.path, b"changed after discovery")
            .expect("fixture artifact must be changed");
        let classpath = classpath(fixture.path().to_path_buf(), vec![artifact.clone()]);
        assert_eq!(
            build_project_java_catalog(&classpath, 17, ArchiveLimits::default()),
            Err(JavaCatalogError::ArtifactFingerprintMismatch {
                path: artifact.path
            })
        );
    }

    #[test]
    fn persistent_artifact_product_rebinds_to_the_consumer_classpath_path() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let bytes = jar("com/example/Demo.class", &class);
        let first = artifact(
            fixture.path().join("producer.jar"),
            &bytes,
            ArtifactOrigin::JrubyRuntime,
        );
        let limits = ArchiveLimits::default();
        let first_key = JavaArtifactProductKey::new(&first, 17, limits);
        let product = JavaArtifactProduct::build(&first, &first_key, limits)
            .expect("producer artifact product must build");
        let payload = product
            .encode_persistent_payload()
            .expect("artifact product must encode");

        let second = artifact(
            fixture.path().join("consumer.jar"),
            &bytes,
            ArtifactOrigin::Explicit,
        );
        let second_key = JavaArtifactProductKey::new(&second, 17, limits);
        assert_eq!(
            first_key.cache_id(),
            second_key.cache_id(),
            "artifact product identity must be independent of the consumer path and origin"
        );
        let decoded = JavaArtifactProduct::decode_persistent_payload(&second_key, &payload)
            .expect("consumer must decode the immutable artifact product");
        let catalog = build_project_java_catalog_from_products(
            &classpath(fixture.path().to_path_buf(), vec![second.clone()]),
            vec![decoded],
        )
        .expect("consumer catalog must compose");

        assert_eq!(
            catalog.classes["com/example/Demo"].artifact_path, second.path,
            "persistent metadata must bind to the exact consumer artifact path"
        );
    }

    #[test]
    fn product_clones_share_class_metadata_but_rebind_project_paths() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let bytes = jar("com/example/Demo.class", &class);
        let first = artifact(
            fixture.path().join("project-one.jar"),
            &bytes,
            ArtifactOrigin::JrubyRuntime,
        );
        let second = artifact(
            fixture.path().join("project-two.jar"),
            &bytes,
            ArtifactOrigin::Explicit,
        );
        let limits = ArchiveLimits::default();
        let key = JavaArtifactProductKey::new(&first, 17, limits);
        let product = JavaArtifactProduct::build(&first, &key, limits)
            .expect("shared artifact product must build");

        let first_catalog = build_project_java_catalog_from_products(
            &classpath(fixture.path().join("project-one"), vec![first.clone()]),
            vec![product.clone()],
        )
        .expect("first project catalog must compose");
        let second_catalog = build_project_java_catalog_from_products(
            &classpath(fixture.path().join("project-two"), vec![second.clone()]),
            vec![product],
        )
        .expect("second project catalog must compose");
        let first_declaration = &first_catalog.classes["com/example/Demo"];
        let second_declaration = &second_catalog.classes["com/example/Demo"];

        assert_eq!(first_declaration.artifact_path, first.path);
        assert_eq!(second_declaration.artifact_path, second.path);
        assert!(
            Arc::ptr_eq(&first_declaration.class, &second_declaration.class),
            "isolated project declarations should share only immutable parsed class metadata"
        );
    }

    #[test]
    fn process_cache_evicts_completed_artifact_products_to_both_bounds() {
        let fixture = tempfile::tempdir().expect("catalog fixture must be created");
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/minimal_class.hex"
        ));
        let first_bytes = jar_with_marker(&class, "first");
        let second_bytes = jar_with_marker(&class, "second");
        let first = artifact(
            fixture.path().join("first.jar"),
            &first_bytes,
            ArtifactOrigin::Explicit,
        );
        let second = artifact(
            fixture.path().join("second.jar"),
            &second_bytes,
            ArtifactOrigin::Explicit,
        );
        let limits = ArchiveLimits::default();
        let first_key = JavaArtifactProductKey::new(&first, 17, limits);
        let second_key = JavaArtifactProductKey::new(&second, 17, limits);
        let cache = JavaArtifactProductCache::new(1, 1024 * 1024);

        for (artifact, key) in [(&first, first_key), (&second, second_key)] {
            let key_for_build = key.clone();
            cache
                .get_or_try_init(key, || {
                    JavaArtifactProduct::build(artifact, &key_for_build, limits)
                        .map_err(|error| format!("{error:?}"))
                })
                .expect("bounded Java artifact product must build");
        }

        assert_eq!(cache.snapshot().entries, 1);
        assert_eq!(cache.snapshot().producers, 2);
        assert_eq!(cache.snapshot().evictions, 1);
        assert!(cache.retained_weight_bytes() > 0);
        assert!(cache.retained_weight_bytes() <= 1024 * 1024);

        let overweight_cache = JavaArtifactProductCache::new(1, 1);
        let key = JavaArtifactProductKey::new(&first, 17, limits);
        let key_for_build = key.clone();
        let product = overweight_cache
            .get_or_try_init(key, || {
                JavaArtifactProduct::build(&first, &key_for_build, limits)
                    .map_err(|error| format!("{error:?}"))
            })
            .expect("an overweight product must still serve its current consumer");
        assert!(product.estimated_weight_bytes() > 1);
        assert_eq!(overweight_cache.snapshot().entries, 0);
        assert_eq!(overweight_cache.snapshot().evictions, 1);
        assert_eq!(overweight_cache.retained_weight_bytes(), 0);
    }
}
