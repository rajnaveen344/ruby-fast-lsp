//! Java classpath catalogs.
//!
//! `product` owns immutable per-artifact class metadata and its persistent
//! and process caches. `catalog` composes ordered products into a project's
//! classpath-precedence view and shares identical compositions across
//! projects.

mod catalog;
mod product;

#[cfg(test)]
mod persistence_tests;
#[cfg(test)]
mod tests;

use super::classpath::{ArtifactKind, ClasspathArtifact, ProjectClasspath};
use ruby_fast_lsp_jvm_metadata::{ArchiveKind, ArchiveLimits};
use std::fs;
use std::path::PathBuf;

pub use catalog::{
    DuplicateJavaClass, JavaClassDeclaration, JavaProxyTargets, ProjectJavaCatalog,
    ProjectJavaCatalogBuilder,
};
pub use product::{JavaArtifactProduct, JavaArtifactProductCache, JavaArtifactProductKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaCatalogError {
    Read { path: PathBuf, message: String },
    Archive { path: PathBuf, message: String },
    ArtifactFingerprintMismatch { path: PathBuf },
}

pub fn build_project_java_catalog(
    classpath: &ProjectClasspath,
    jdk_feature: u16,
    archive_limits: ArchiveLimits,
) -> Result<ProjectJavaCatalog, JavaCatalogError> {
    let mut builder = ProjectJavaCatalogBuilder::new(classpath);
    for artifact in &classpath.artifacts {
        let key = JavaArtifactProductKey::new(artifact, jdk_feature, archive_limits);
        builder.push(&JavaArtifactProduct::build(artifact, &key, archive_limits)?)?;
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
    products: &[JavaArtifactProduct],
) -> Result<ProjectJavaCatalog, JavaCatalogError> {
    invariant_eq!(
        classpath.artifacts.len(),
        products.len(),
        what = "Java artifact product count differs from the ordered classpath artifact count",
        why = "missing or extra products would change Java classpath precedence",
        fix = "resolve exactly one immutable product for every ordered classpath artifact",
    );
    let mut builder = ProjectJavaCatalogBuilder::new(classpath);
    for product in products {
        builder.push(product)?;
    }
    Ok(builder.finish())
}

fn artifact_kind(kind: ArtifactKind) -> ArchiveKind {
    match kind {
        ArtifactKind::Jar => ArchiveKind::Jar,
        ArtifactKind::Jmod => ArchiveKind::Jmod,
    }
}
