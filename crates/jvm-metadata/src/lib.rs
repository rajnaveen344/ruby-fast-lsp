//! Pure, bounded JVM declaration metadata.
//!
//! This crate must remain independent from Ruby analysis, LSP protocol types,
//! editors, workspace configuration, and runtime execution.

#[macro_use]
#[allow(unused_macros)]
#[path = "../../ruby-analysis/src/invariant.rs"]
mod invariant;

mod archive;
mod classfile;
mod descriptor;
mod java_source;

pub use archive::{
    parse_archive, ArchiveClass, ArchiveError, ArchiveKind, ArchiveLimits, ArchiveMetadata,
    ARCHIVE_PRODUCT_SEMANTIC_VERSION,
};
pub use classfile::{
    parse_class, parse_class_with_interner, ClassFile, ClassKind, ClassLimits, JvmStringInterner,
    MemberInfo, MetadataError, MethodParameter, Visibility,
};
pub use descriptor::{parse_field_descriptor, parse_method_descriptor, JvmType, MethodDescriptor};
pub use java_source::{
    locate_java_source_declarations, JavaSourceClassLocation, JavaSourceError, JavaSourceLimits,
    JavaSourceMemberLocation, SourceByteRange,
};
