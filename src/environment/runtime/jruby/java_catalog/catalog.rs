//! Classpath-ordered Java catalogs composed from immutable artifact products.
//!
//! Catalog content is a sorted, index-based view over shared archives. It is
//! a pure function of the ordered product identities, so projects whose
//! classpaths resolve to the same artifacts share one content value; each
//! project keeps only its classpath fingerprint and artifact paths.

use super::super::classpath::ProjectClasspath;
use super::product::{hash_field, CatalogContentIdentity, JavaArtifactProductCache};
use super::{JavaArtifactProduct, JavaCatalogError};
use crate::invariant::ExpectInvariant;
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::{archive_entry_name, ArchiveKind, ArchiveMetadata, ClassFile};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One Java class selected by classpath precedence, borrowed from its catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JavaClassDeclaration<'a> {
    pub class: &'a Arc<ClassFile>,
    pub artifact_path: &'a Path,
    pub artifact_fingerprint_sha256: &'a str,
    pub archive_kind: ArchiveKind,
    /// Multi-release JAR version directory the class was selected from.
    pub release: Option<u16>,
}

impl JavaClassDeclaration<'_> {
    /// The archive entry holding this class.
    pub fn entry_name(&self) -> String {
        archive_entry_name(self.archive_kind, self.release, &self.class.name)
    }
}

/// A class definition hidden by an earlier classpath artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DuplicateJavaClass<'a> {
    pub name: &'a str,
    pub winner: &'a Path,
    pub shadowed: &'a Path,
}

#[derive(Debug, Clone)]
pub struct ProjectJavaCatalog {
    classpath_fingerprint_sha256: String,
    /// Consumer paths of the ordered classpath artifacts, parallel to
    /// `content.archives`. Paths are project-owned so identical artifacts at
    /// different locations still share content.
    artifact_paths: Box<[PathBuf]>,
    content: Arc<JavaCatalogContent>,
}

#[derive(Debug)]
pub(super) struct JavaCatalogContent {
    archives: Box<[Arc<ArchiveMetadata>]>,
    /// Classpath winners sorted by internal name.
    classes: Box<[CatalogClass]>,
    /// Shadowed definitions grouped by winner in name order.
    duplicates: Box<[CatalogDuplicate]>,
    /// JRuby proxy constant paths sorted by path, then class index. More than
    /// one entry for a path is an ambiguous proxy.
    proxies: Box<[CatalogProxy]>,
    /// Sorted first segments of every class name.
    top_level_packages: Box<[Box<str>]>,
}

#[derive(Debug)]
struct CatalogClass {
    class: Arc<ClassFile>,
    artifact: u32,
    release: Option<u16>,
}

#[derive(Debug)]
struct CatalogDuplicate {
    class: u32,
    shadowed: u32,
}

#[derive(Debug)]
struct CatalogProxy {
    ruby_path: Box<str>,
    class: u32,
}

/// The catalog classes one JRuby proxy constant path names.
#[derive(Clone, Copy)]
pub struct JavaProxyTargets<'a> {
    catalog: &'a ProjectJavaCatalog,
    targets: &'a [CatalogProxy],
}

impl<'a> JavaProxyTargets<'a> {
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// The selected class when exactly one classpath identity owns the proxy.
    pub fn unique(&self) -> Option<JavaClassDeclaration<'a>> {
        match self.targets {
            [only] => Some(self.catalog.declaration(only.class)),
            _ => None,
        }
    }

    /// Internal names of every class behind the proxy, comma separated.
    pub fn joined_class_names(&self) -> String {
        self.targets
            .iter()
            .map(|target| self.catalog.content.class_name(target.class))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl ProjectJavaCatalog {
    pub fn classpath_fingerprint(&self) -> &str {
        &self.classpath_fingerprint_sha256
    }

    pub fn class_count(&self) -> usize {
        self.content.classes.len()
    }

    pub fn class(&self, internal_name: &str) -> Option<JavaClassDeclaration<'_>> {
        self.content
            .class_index(internal_name)
            .map(|index| self.declaration(index))
    }

    pub fn contains_class(&self, internal_name: &str) -> bool {
        self.content.class_index(internal_name).is_some()
    }

    /// Internal names starting with `prefix`, in sorted order.
    pub fn class_names_with_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = &'a str> + 'a {
        let classes = &self.content.classes;
        let start = classes.partition_point(|entry| *entry.class.name < *prefix);
        classes[start..]
            .iter()
            .map(|entry| &*entry.class.name)
            .take_while(move |name| name.starts_with(prefix))
    }

    pub fn duplicate_count(&self) -> usize {
        self.content.duplicates.len()
    }

    /// Shadowed definitions sorted by name, winner, then shadowed path.
    pub fn duplicates(&self) -> Vec<DuplicateJavaClass<'_>> {
        let mut duplicates = self
            .content
            .duplicates
            .iter()
            .map(|duplicate| DuplicateJavaClass {
                name: self.content.class_name(duplicate.class),
                winner: self.artifact_path(self.content.classes[index(duplicate.class)].artifact),
                shadowed: self.artifact_path(duplicate.shadowed),
            })
            .collect::<Vec<_>>();
        duplicates.sort();
        duplicates
    }

    pub fn proxy_targets(&self, ruby_path: &str) -> JavaProxyTargets<'_> {
        let proxies = &self.content.proxies;
        let start = proxies.partition_point(|proxy| *proxy.ruby_path < *ruby_path);
        let len = proxies[start..]
            .iter()
            .take_while(|proxy| *proxy.ruby_path == *ruby_path)
            .count();
        JavaProxyTargets {
            catalog: self,
            targets: &proxies[start..start + len],
        }
    }

    pub fn has_proxy(&self, ruby_path: &str) -> bool {
        !self.proxy_targets(ruby_path).is_empty()
    }

    /// Whether any class name starts with the package segment `root`.
    pub fn has_top_level_package(&self, root: &str) -> bool {
        self.content
            .top_level_packages
            .binary_search_by(|package| (**package).cmp(root))
            .is_ok()
    }

    fn artifact_path(&self, artifact: u32) -> &Path {
        &self.artifact_paths[index(artifact)]
    }

    fn declaration(&self, class: u32) -> JavaClassDeclaration<'_> {
        let entry = &self.content.classes[index(class)];
        let archive = &self.content.archives[index(entry.artifact)];
        JavaClassDeclaration {
            class: &entry.class,
            artifact_path: self.artifact_path(entry.artifact),
            artifact_fingerprint_sha256: &archive.fingerprint_sha256,
            archive_kind: archive.kind,
            release: entry.release,
        }
    }

    /// Whether both catalogs hold the same shared content value.
    #[cfg(test)]
    pub(crate) fn shares_content_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.content, &other.content)
    }

    /// A one-artifact catalog over `classes`, for provider fixtures.
    #[cfg(test)]
    pub(crate) fn from_test_classes(
        classpath_fingerprint_sha256: &str,
        artifact_path: PathBuf,
        classes: Vec<ClassFile>,
    ) -> Self {
        let mut classes = classes
            .into_iter()
            .map(|class| ruby_fast_lsp_jvm_metadata::ArchiveClass {
                release: None,
                class: Arc::new(class),
            })
            .collect::<Vec<_>>();
        classes.sort_by(|left, right| left.class.name.cmp(&right.class.name));
        let archive = ArchiveMetadata {
            fingerprint_sha256: "fixture".to_string(),
            kind: ArchiveKind::Jar,
            classes,
        };
        Self {
            classpath_fingerprint_sha256: classpath_fingerprint_sha256.to_string(),
            artifact_paths: Box::new([artifact_path]),
            content: Arc::new(JavaCatalogContent::compose(vec![Arc::new(archive)])),
        }
    }
}

impl JavaCatalogContent {
    fn compose(archives: Vec<Arc<ArchiveMetadata>>) -> Self {
        let mut definitions = Vec::new();
        for (artifact_index, archive) in archives.iter().enumerate() {
            let artifact_index = to_index(artifact_index, "classpath artifacts");
            definitions.extend(
                archive
                    .classes
                    .iter()
                    .map(|archived| (artifact_index, archived)),
            );
        }
        // Stable: equal names keep classpath order, so the first is the winner.
        definitions.sort_by(|(_, left), (_, right)| left.class.name.cmp(&right.class.name));

        let mut classes = Vec::<CatalogClass>::with_capacity(definitions.len());
        let mut duplicates = Vec::new();
        for (artifact, archived) in definitions {
            if let Some(winner) = classes.last() {
                if winner.class.name == archived.class.name {
                    duplicates.push(CatalogDuplicate {
                        class: to_index(classes.len() - 1, "catalog classes"),
                        shadowed: artifact,
                    });
                    continue;
                }
            }
            classes.push(CatalogClass {
                class: Arc::clone(&archived.class),
                artifact,
                release: archived.release,
            });
        }

        let mut proxies = Vec::new();
        let mut top_level_packages = Vec::<Box<str>>::new();
        for (class_index, entry) in classes.iter().enumerate() {
            let name = &*entry.class.name;
            if let Some(package) = name.split('/').next() {
                if top_level_packages
                    .last()
                    .is_none_or(|last| **last != *package)
                {
                    top_level_packages.push(package.into());
                }
            }
            // JVM classfiles may legitimately contain anonymous and
            // compiler-generated names such as `Outer$1`. They are classpath
            // truth, but JRuby cannot expose them as ordinary Ruby proxy
            // constants. Keep them for exact descriptor relationships and omit
            // only the invalid Ruby proxy projection.
            let Ok(java_name) = JavaClassName::parse(name) else {
                continue;
            };
            proxies.push(CatalogProxy {
                ruby_path: java_name.ruby_fqn().into_boxed_str(),
                class: to_index(class_index, "catalog classes"),
            });
        }
        proxies.sort_by(|left, right| {
            left.ruby_path
                .cmp(&right.ruby_path)
                .then_with(|| left.class.cmp(&right.class))
        });
        // Sorted class names keep equal first segments adjacent except where
        // a longer segment sorts between them (`a/x`, `a$b`, `a/y`).
        top_level_packages.sort();
        top_level_packages.dedup();

        Self {
            archives: archives.into_boxed_slice(),
            classes: classes.into_boxed_slice(),
            duplicates: duplicates.into_boxed_slice(),
            proxies: proxies.into_boxed_slice(),
            top_level_packages: top_level_packages.into_boxed_slice(),
        }
    }

    fn class_index(&self, internal_name: &str) -> Option<u32> {
        self.classes
            .binary_search_by(|entry| (*entry.class.name).cmp(internal_name))
            .ok()
            .map(|found| to_index(found, "catalog classes"))
    }

    fn class_name(&self, class: u32) -> &str {
        &self.classes[index(class)].class.name
    }
}

/// Composes one project's catalog from its ordered artifact products.
pub struct ProjectJavaCatalogBuilder<'a> {
    classpath: &'a ProjectClasspath,
    archives: Vec<Arc<ArchiveMetadata>>,
    identity: Sha256,
}

impl<'a> ProjectJavaCatalogBuilder<'a> {
    pub fn new(classpath: &'a ProjectClasspath) -> Self {
        Self {
            classpath,
            archives: Vec::with_capacity(classpath.artifacts.len()),
            identity: Sha256::new(),
        }
    }

    pub fn push(&mut self, product: &JavaArtifactProduct) -> Result<(), JavaCatalogError> {
        let artifact = self
            .classpath
            .artifacts
            .get(self.archives.len())
            .expect_invariant(
                "Java catalog received more products than ordered classpath artifacts",
                "extra products have no defensible precedence",
                "produce exactly one product per classpath artifact in order",
            );
        invariant_eq!(
            product.key.artifact_fingerprint_sha256,
            artifact.fingerprint_sha256,
            what = "Java artifact product is bound to a different content identity",
            why = "cached class metadata must never be substituted across artifacts",
            fix = "validate the exact artifact key before composing the project catalog",
        );
        invariant_eq!(
            product.key.artifact_kind,
            artifact.kind,
            what = "Java artifact product kind differs from the consumer classpath kind",
            why = "JAR and JMOD entry policies are not interchangeable",
            fix = "include and validate artifact kind in the persistent product identity",
        );
        hash_field(&mut self.identity, product.cache_id().as_bytes());
        self.archives.push(Arc::clone(&product.archive));
        Ok(())
    }

    /// A catalog with its own content.
    pub fn finish(self) -> ProjectJavaCatalog {
        let (classpath_fingerprint_sha256, artifact_paths, archives, _) = self.complete();
        ProjectJavaCatalog {
            classpath_fingerprint_sha256,
            artifact_paths,
            content: Arc::new(JavaCatalogContent::compose(archives)),
        }
    }

    /// A catalog sharing content with live catalogs of the same ordered
    /// artifact products, wherever each project finds those artifacts.
    pub fn finish_shared(self, cache: &JavaArtifactProductCache) -> ProjectJavaCatalog {
        let (classpath_fingerprint_sha256, artifact_paths, archives, identity) = self.complete();
        let content = cache.shared_content(identity, || JavaCatalogContent::compose(archives));
        invariant_eq!(
            content.archives.len(),
            artifact_paths.len(),
            what = "shared Java catalog content has a different artifact count than its consumer",
            why = "the identity hashes every ordered artifact product identity",
            fix = "derive the content identity from exactly the products that compose it",
        );
        ProjectJavaCatalog {
            classpath_fingerprint_sha256,
            artifact_paths,
            content,
        }
    }

    fn complete(
        self,
    ) -> (
        String,
        Box<[PathBuf]>,
        Vec<Arc<ArchiveMetadata>>,
        CatalogContentIdentity,
    ) {
        invariant_eq!(
            self.archives.len(),
            self.classpath.artifacts.len(),
            what = "Java catalog completed before every ordered classpath artifact supplied one product",
            why = "missing products silently change Java lookup semantics",
            fix = "resolve and push one immutable product for every artifact",
        );
        let artifact_paths = self
            .classpath
            .artifacts
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect();
        (
            self.classpath.fingerprint_sha256.clone(),
            artifact_paths,
            self.archives,
            self.identity.finalize().into(),
        )
    }
}

fn to_index(value: usize, label: &'static str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| {
        unreachable_invariant!(
            what = "Java catalog {label} index exceeded u32",
            why = "archive class counts and classpath artifacts are bounded far below u32",
            fix = "inspect archive and classpath limits",
            label = label,
        )
    })
}

fn index(value: u32) -> usize {
    usize::try_from(value).expect_invariant(
        "a u32 Java catalog index does not fit usize",
        "supported targets have at least 32-bit pointers",
        "inspect target pointer width",
    )
}
