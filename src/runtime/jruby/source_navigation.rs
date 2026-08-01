use super::{
    classpath::{SourceOrigin, SourceRoot},
    imports::ruby_type_for_jvm,
    java_catalog::JavaClassDeclaration,
};
use parking_lot::{Mutex, MutexGuard};
use ruby_analysis::core::{
    FullyQualifiedName, MethodFact, MethodParamFact, MethodParamKind, NamespaceKind, RubyConstant,
    RubyMethod, SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance,
    TypeSubject,
};
use ruby_analysis::engine::FileFacts;
use ruby_analysis::method_store::MethodVisibility;
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::{
    locate_java_source_declarations, parse_field_descriptor, parse_method_descriptor, ClassFile,
    ClassKind, JavaSourceClassLocation, JavaSourceError, JavaSourceLimits, SourceByteRange,
    Visibility,
};
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct JavaSourceResolutionLimits {
    pub max_archive_entries: usize,
    pub max_source_bytes: usize,
}

impl Default for JavaSourceResolutionLimits {
    fn default() -> Self {
        Self {
            max_archive_entries: 1_000_000,
            max_source_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedJavaSource {
    pub path: PathBuf,
    pub content: String,
    pub origin: SourceOrigin,
    pub location: JavaSourceClassLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaSourceResolutionError {
    Read { path: PathBuf, message: String },
    FingerprintMismatch { path: PathBuf },
    InvalidArchive { path: PathBuf, message: String },
    InvalidSource { path: PathBuf, message: String },
    Ambiguous { class_name: String, source: PathBuf },
    LimitExceeded(&'static str),
}

#[derive(Debug, Clone)]
pub struct JavaSourceResolver {
    roots: Vec<PreparedSourceRoot>,
    cache_root: PathBuf,
    limits: JavaSourceResolutionLimits,
}

struct PreparedSourceRoot {
    source: SourceRoot,
    archive: OnceLock<Result<Mutex<zip::ZipArchive<fs::File>>, JavaSourceResolutionError>>,
}

impl std::fmt::Debug for PreparedSourceRoot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedSourceRoot")
            .field("source", &self.source)
            .field("archive_initialized", &self.archive.get().is_some())
            .finish()
    }
}

impl Clone for PreparedSourceRoot {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            archive: OnceLock::new(),
        }
    }
}

impl PreparedSourceRoot {
    fn archive(
        &self,
    ) -> Result<MutexGuard<'_, zip::ZipArchive<fs::File>>, JavaSourceResolutionError> {
        verify_source_file_identity(&self.source)?;
        let archive = self.archive.get_or_init(|| {
            let file = open_verified_source_file(&self.source)?;
            zip::ZipArchive::new(file).map(Mutex::new).map_err(|error| {
                JavaSourceResolutionError::InvalidArchive {
                    path: self.source.path.clone(),
                    message: error.to_string(),
                }
            })
        });
        match archive {
            Ok(archive) => Ok(archive.lock()),
            Err(error) => Err(error.clone()),
        }
    }
}

impl JavaSourceResolver {
    pub fn new(
        mut roots: Vec<SourceRoot>,
        cache_root: PathBuf,
        limits: JavaSourceResolutionLimits,
    ) -> Self {
        roots.sort_by(|left, right| {
            source_origin_precedence(left.origin)
                .cmp(&source_origin_precedence(right.origin))
                .then_with(|| left.path.cmp(&right.path))
        });
        Self {
            roots: roots
                .into_iter()
                .map(|source| PreparedSourceRoot {
                    source,
                    archive: OnceLock::new(),
                })
                .collect(),
            cache_root,
            limits,
        }
    }

    pub fn resolve(
        &self,
        declaration: &JavaClassDeclaration,
    ) -> Result<Option<ResolvedJavaSource>, JavaSourceResolutionError> {
        let relative_path = source_relative_path(&declaration.class)?;
        for prepared_root in &self.roots {
            let root = &prepared_root.source;
            if root.path.is_dir() {
                let candidate = root.path.join(&relative_path);
                if !candidate.is_file() {
                    continue;
                }
                let content = read_bounded_utf8(&candidate, self.limits.max_source_bytes)?;
                let Some(location) =
                    locate_verified_source(&declaration.class, &content, &candidate, self.limits)?
                else {
                    continue;
                };
                let canonical_path = fs::canonicalize(&candidate).map_err(|error| {
                    JavaSourceResolutionError::Read {
                        path: candidate,
                        message: error.to_string(),
                    }
                })?;
                return Ok(Some(ResolvedJavaSource {
                    path: canonical_path,
                    content,
                    origin: root.origin,
                    location,
                }));
            }
            if !root.path.is_file() {
                return Err(JavaSourceResolutionError::Read {
                    path: root.path.clone(),
                    message: "source root is neither a file nor a directory".to_string(),
                });
            }
            let expected_fingerprint = root.fingerprint_sha256.as_ref().ok_or_else(|| {
                JavaSourceResolutionError::Read {
                    path: root.path.clone(),
                    message:
                        "file source root has no discovery-time SHA-256 identity; rediscover classpath"
                            .to_string(),
                }
            })?;

            if root
                .path
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("java")
            {
                if root.file_identity.is_some_and(|identity| {
                    identity.byte_length > self.limits.max_source_bytes as u64
                }) {
                    return Err(JavaSourceResolutionError::LimitExceeded(
                        "Java source bytes",
                    ));
                }
                let mut source_file = open_verified_source_file(root)?;
                let mut content = String::new();
                source_file.read_to_string(&mut content).map_err(|error| {
                    JavaSourceResolutionError::InvalidSource {
                        path: root.path.clone(),
                        message: error.to_string(),
                    }
                })?;
                if content.len() > self.limits.max_source_bytes {
                    return Err(JavaSourceResolutionError::LimitExceeded(
                        "Java source bytes",
                    ));
                }
                let Some(location) =
                    locate_verified_source(&declaration.class, &content, &root.path, self.limits)?
                else {
                    continue;
                };
                return Ok(Some(ResolvedJavaSource {
                    path: root.path.clone(),
                    content,
                    origin: root.origin,
                    location,
                }));
            }

            let mut archive = prepared_root.archive()?;
            let Some((content, location)) =
                source_from_archive(declaration, root, &mut archive, &relative_path, self.limits)?
            else {
                continue;
            };
            let path = materialize_archive_source(
                &self.cache_root,
                expected_fingerprint,
                &relative_path,
                &content,
            )?;
            return Ok(Some(ResolvedJavaSource {
                path,
                content,
                origin: root.origin,
                location,
            }));
        }
        Ok(None)
    }
}

fn open_verified_source_file(root: &SourceRoot) -> Result<fs::File, JavaSourceResolutionError> {
    verify_source_file_identity(root)?;
    fs::File::open(&root.path).map_err(|error| JavaSourceResolutionError::Read {
        path: root.path.clone(),
        message: error.to_string(),
    })
}

fn verify_source_file_identity(root: &SourceRoot) -> Result<(), JavaSourceResolutionError> {
    let expected = root
        .file_identity
        .ok_or_else(|| JavaSourceResolutionError::Read {
            path: root.path.clone(),
            message:
                "file source root has no discovery-time filesystem identity; rediscover classpath"
                    .to_string(),
        })?;
    let metadata = fs::metadata(&root.path).map_err(|error| JavaSourceResolutionError::Read {
        path: root.path.clone(),
        message: error.to_string(),
    })?;
    let modified = metadata
        .modified()
        .map_err(|error| JavaSourceResolutionError::Read {
            path: root.path.clone(),
            message: error.to_string(),
        })?;
    if metadata.len() != expected.byte_length || modified != expected.modified {
        return Err(JavaSourceResolutionError::FingerprintMismatch {
            path: root.path.clone(),
        });
    }
    Ok(())
}

fn source_origin_precedence(origin: SourceOrigin) -> u8 {
    match origin {
        SourceOrigin::Project | SourceOrigin::Explicit => 0,
        SourceOrigin::Attached => 1,
        SourceOrigin::Jdk => 2,
        SourceOrigin::Decompiled => 3,
    }
}

fn source_relative_path(class: &ClassFile) -> Result<PathBuf, JavaSourceResolutionError> {
    let (package, simple_name) = class
        .name
        .rsplit_once('/')
        .map_or(("", class.name.as_str()), |(package, name)| (package, name));
    let outer_name = simple_name.split('$').next().expect(
        "INVARIANT VIOLATED: accepted JVM internal class name has no outer component. \
         This is a bug because classfile parsing rejects empty class names. \
         Fix: preserve the validated class name from Java catalog construction.",
    );
    let source_file = class.source_file.as_deref().unwrap_or_else(|| {
        // Stored below after validation; this closure cannot return an owned fallback.
        outer_name
    });
    let fallback;
    let source_file = if class.source_file.is_some() {
        source_file
    } else {
        fallback = format!("{outer_name}.java");
        &fallback
    };
    if source_file.is_empty()
        || source_file == "."
        || source_file == ".."
        || source_file.contains('/')
        || source_file.contains('\\')
        || !source_file.ends_with(".java")
    {
        return Err(JavaSourceResolutionError::InvalidSource {
            path: PathBuf::from(source_file),
            message: "classfile SourceFile is not a safe Java filename".to_string(),
        });
    }
    let mut relative = PathBuf::new();
    for component in package.split('/').filter(|component| !component.is_empty()) {
        if component == "." || component == ".." {
            return Err(JavaSourceResolutionError::InvalidSource {
                path: PathBuf::from(package),
                message: "classfile package is not a safe relative path".to_string(),
            });
        }
        relative.push(component);
    }
    relative.push(source_file);
    Ok(relative)
}

fn read_bounded_utf8(path: &Path, max_bytes: usize) -> Result<String, JavaSourceResolutionError> {
    let metadata = fs::metadata(path).map_err(|error| JavaSourceResolutionError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > max_bytes as u64 {
        return Err(JavaSourceResolutionError::LimitExceeded(
            "Java source bytes",
        ));
    }
    fs::read_to_string(path).map_err(|error| JavaSourceResolutionError::InvalidSource {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn locate_verified_source(
    class: &ClassFile,
    content: &str,
    path: &Path,
    limits: JavaSourceResolutionLimits,
) -> Result<Option<JavaSourceClassLocation>, JavaSourceResolutionError> {
    locate_java_source_declarations(
        class,
        content,
        JavaSourceLimits {
            max_source_bytes: limits.max_source_bytes,
            ..JavaSourceLimits::default()
        },
    )
    .map_err(|error| match error {
        JavaSourceError::LimitExceeded(limit) => JavaSourceResolutionError::LimitExceeded(limit),
        JavaSourceError::ParseFailed => JavaSourceResolutionError::InvalidSource {
            path: path.to_path_buf(),
            message: "Java source parser rejected the document".to_string(),
        },
    })
}

fn source_from_archive<R: Read + Seek>(
    declaration: &JavaClassDeclaration,
    root: &SourceRoot,
    archive: &mut zip::ZipArchive<R>,
    relative_path: &Path,
    limits: JavaSourceResolutionLimits,
) -> Result<Option<(String, JavaSourceClassLocation)>, JavaSourceResolutionError> {
    if archive.len() > limits.max_archive_entries {
        return Err(JavaSourceResolutionError::LimitExceeded(
            "Java source archive entries",
        ));
    }
    let expected = relative_path.to_string_lossy().replace('\\', "/");
    let suffix = format!("/{expected}");
    let mut candidate_indexes = Vec::new();
    for index in 0..archive.len() {
        let name = archive.name_for_index(index).expect(
            "INVARIANT VIOLATED: ZIP central-directory index disappeared while resolving Java \
             source. This is a bug because the archive is immutably borrowed and the index is \
             bounded by ZipArchive::len. Fix: inspect the zip crate archive metadata lifecycle.",
        );
        if name == expected
            || (matches!(root.origin, SourceOrigin::Jdk | SourceOrigin::Explicit)
                && name.ends_with(&suffix))
        {
            candidate_indexes.push(index);
        }
    }
    let mut matches = Vec::new();
    for index in candidate_indexes {
        let mut entry =
            archive
                .by_index(index)
                .map_err(|error| JavaSourceResolutionError::InvalidArchive {
                    path: root.path.clone(),
                    message: error.to_string(),
                })?;
        if entry.size() > limits.max_source_bytes as u64 {
            return Err(JavaSourceResolutionError::LimitExceeded(
                "Java source bytes",
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).map_err(|error| {
            JavaSourceResolutionError::InvalidArchive {
                path: root.path.clone(),
                message: error.to_string(),
            }
        })?;
        if bytes.len() > limits.max_source_bytes {
            return Err(JavaSourceResolutionError::LimitExceeded(
                "Java source bytes",
            ));
        }
        let content =
            String::from_utf8(bytes).map_err(|error| JavaSourceResolutionError::InvalidSource {
                path: root.path.clone(),
                message: error.to_string(),
            })?;
        if let Some(location) =
            locate_verified_source(&declaration.class, &content, &root.path, limits)?
        {
            matches.push((content, location));
        }
    }
    if matches.len() > 1 {
        return Err(JavaSourceResolutionError::Ambiguous {
            class_name: declaration.class.name.clone(),
            source: root.path.clone(),
        });
    }
    Ok(matches.pop())
}

fn materialize_archive_source(
    cache_root: &Path,
    fingerprint: &str,
    relative_path: &Path,
    content: &str,
) -> Result<PathBuf, JavaSourceResolutionError> {
    let path = cache_root
        .join("exact-source")
        .join(fingerprint)
        .join(relative_path);
    let parent = path.parent().expect(
        "INVARIANT VIOLATED: source cache path has no parent. \
         This is a bug because cache root and source-relative path are both non-empty. \
         Fix: preserve both components during source materialization.",
    );
    fs::create_dir_all(parent).map_err(|error| JavaSourceResolutionError::Read {
        path: parent.to_path_buf(),
        message: error.to_string(),
    })?;
    if fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        return Ok(path);
    }
    let temporary = path.with_extension(format!("java.tmp-{}", std::process::id()));
    let mut file =
        fs::File::create(&temporary).map_err(|error| JavaSourceResolutionError::Read {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
    file.write_all(content.as_bytes())
        .map_err(|error| JavaSourceResolutionError::Read {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
    file.sync_all()
        .map_err(|error| JavaSourceResolutionError::Read {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
    fs::rename(&temporary, &path).map_err(|error| JavaSourceResolutionError::Read {
        path: path.clone(),
        message: error.to_string(),
    })?;
    Ok(path)
}

pub fn java_source_navigation_facts(
    class: &ClassFile,
    location: &JavaSourceClassLocation,
    file_id: SourceFileId,
) -> FileFacts {
    java_source_navigation_facts_with_declaration(class, location, file_id, true)
}

pub fn java_source_navigation_facts_with_declaration(
    class: &ClassFile,
    location: &JavaSourceClassLocation,
    file_id: SourceFileId,
    include_class_declaration: bool,
) -> FileFacts {
    assert_eq!(
        class.name, location.internal_name,
        "INVARIANT VIOLATED: Java source location identity differs from classfile metadata. \
         This is a bug because source locations must be verified against the exact winning class. \
         Fix: call java_source_navigation_facts only with the ClassFile used for source matching."
    );
    let java_name = JavaClassName::parse(&class.name).expect(
        "INVARIANT VIOLATED: Java catalog class has an invalid internal name. \
         This is a bug because catalog construction accepts only parsed class metadata. \
         Fix: validate class identity before source-navigation projection.",
    );
    let owner_parts = java_name
        .ruby_namespace_parts()
        .into_iter()
        .map(|part| {
            RubyConstant::new(&part).expect(
                "INVARIANT VIOLATED: validated Java proxy component is not a Ruby constant. \
                 This is a bug because JavaClassName owns proxy validation. \
                 Fix: keep Java source proxy conversion single-sourced.",
            )
        })
        .collect::<Vec<_>>();
    let owner = FullyQualifiedName::constant(owner_parts.clone());
    let declaration_range = text_range(file_id, location.declaration_range);
    let name_range = text_range(file_id, location.name_range);
    let symbol_kind = match class.kind() {
        ClassKind::Interface | ClassKind::Annotation | ClassKind::Module => SymbolKind::Module,
        ClassKind::Class | ClassKind::Enum | ClassKind::Record => SymbolKind::Class,
    };
    let mut facts = FileFacts::default();
    if include_class_declaration {
        facts.symbols.push(
            SymbolFact::new(owner.clone(), symbol_kind, declaration_range)
                .with_name_range(name_range),
        );
    }

    for source_method in &location.methods {
        let method = class
            .methods
            .iter()
            .find(|method| {
                method.name == source_method.name && method.descriptor == source_method.descriptor
            })
            .expect(
                "INVARIANT VIOLATED: verified Java source method has no exact classfile member. \
                 This is a bug because the source locator emits only metadata-backed identities. \
                 Fix: preserve method name+descriptor while projecting source locations.",
            );
        let descriptor = parse_method_descriptor(&method.descriptor).expect(
            "INVARIANT VIOLATED: selected classfile method descriptor is invalid. \
             This is a bug because classfile parsing validates descriptors before catalog insertion. \
             Fix: retain the validated descriptor from JVM metadata.",
        );
        let method_name = if method.name == "<init>" {
            "new"
        } else {
            &method.name
        };
        let Ok(method_name) = RubyMethod::new(method_name) else {
            continue;
        };
        let owner_kind = if method.name == "<init>" || method.is_static() {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        };
        let method_owner = FullyQualifiedName::namespace_with_kind(owner_parts.clone(), owner_kind);
        let method_fqn = FullyQualifiedName::method(owner_parts.clone(), method_name);
        let method_range = text_range(file_id, source_method.declaration_range);
        let method_name_range = text_range(file_id, source_method.name_range);
        let parameters = descriptor
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter_type)| {
                let name = method
                    .parameters
                    .get(index)
                    .map(|parameter| {
                        ruby_fast_lsp_jruby_support::ruby_parameter_name(&parameter.name, index)
                    })
                    .unwrap_or_else(|| format!("arg{index}"));
                let kind = if method.is_varargs() && index + 1 == descriptor.parameters.len() {
                    MethodParamKind::Rest
                } else {
                    MethodParamKind::Required
                };
                MethodParamFact::new(name, kind).with_signature_metadata(
                    Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                        parameter_type,
                    )),
                    None,
                )
            })
            .collect::<Vec<_>>();
        facts.methods.push(
            MethodFact::with_param_facts(
                method_fqn.clone(),
                method_owner,
                method_range,
                parameters,
            )
            .with_name_range(method_name_range)
            .with_visibility(method_visibility(method.visibility()))
            .with_signature_metadata(
                Some(format!(
                    "Java method `{}` with descriptor `{}`.",
                    method.name, method.descriptor
                )),
                Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                    &descriptor.returns,
                )),
            ),
        );
        facts.symbols.push(
            SymbolFact::new(method_fqn.clone(), SymbolKind::Method, method_range)
                .with_name_range(method_name_range),
        );
        facts.types.push(TypeFact::new(
            TypeSubject::MethodReturn(method_fqn),
            ruby_type_for_jvm(&descriptor.returns),
            method_range,
            TypeProvenance::Runtime,
        ));
    }

    for source_field in &location.fields {
        let field = class
            .fields
            .iter()
            .find(|field| {
                field.name == source_field.name && field.descriptor == source_field.descriptor
            })
            .expect(
                "INVARIANT VIOLATED: verified Java source field has no exact classfile member. \
                 This is a bug because the source locator emits only metadata-backed identities. \
                 Fix: preserve field name+descriptor while projecting source locations.",
            );
        let Ok(field_name) = RubyMethod::new(&field.name) else {
            continue;
        };
        let field_type = parse_field_descriptor(&field.descriptor).expect(
            "INVARIANT VIOLATED: selected classfile field descriptor is invalid. \
             This is a bug because classfile parsing validates descriptors before catalog insertion. \
             Fix: retain the validated descriptor from JVM metadata.",
        );
        let field_range = text_range(file_id, source_field.declaration_range);
        let field_name_range = text_range(file_id, source_field.name_range);
        if field.is_static() && (field.is_final() || field.is_enum_constant()) {
            if let Ok(constant) = RubyConstant::new(&field.name) {
                let mut constant_parts = owner_parts.clone();
                constant_parts.push(constant);
                let constant = FullyQualifiedName::constant(constant_parts);
                facts.symbols.push(
                    SymbolFact::new(constant.clone(), SymbolKind::Constant, field_range)
                        .with_name_range(field_name_range),
                );
                facts.types.push(TypeFact::new(
                    TypeSubject::Constant(constant),
                    ruby_type_for_jvm(&field_type),
                    field_range,
                    TypeProvenance::Runtime,
                ));
            }
            continue;
        }
        let owner_kind = if field.is_static() {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        };
        let method_owner = FullyQualifiedName::namespace_with_kind(owner_parts.clone(), owner_kind);
        let getter_fqn = FullyQualifiedName::method(owner_parts.clone(), field_name);
        facts.methods.push(
            MethodFact::new(getter_fqn.clone(), method_owner.clone(), field_range)
                .with_name_range(field_name_range)
                .with_visibility(method_visibility(field.visibility()))
                .with_signature_metadata(
                    Some(format!(
                        "Java field `{}` with descriptor `{}`.",
                        field.name, field.descriptor
                    )),
                    Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                        &field_type,
                    )),
                ),
        );
        facts.types.push(TypeFact::new(
            TypeSubject::MethodReturn(getter_fqn),
            ruby_type_for_jvm(&field_type),
            field_range,
            TypeProvenance::Runtime,
        ));
        if let Ok(writer) = RubyMethod::new(&format!("{}=", field.name)) {
            let writer_fqn = FullyQualifiedName::method(owner_parts.clone(), writer);
            facts.methods.push(
                MethodFact::with_param_facts(
                    writer_fqn,
                    method_owner,
                    field_range,
                    vec![MethodParamFact::new("value", MethodParamKind::Required)
                        .with_signature_metadata(
                            Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                                &field_type,
                            )),
                            None,
                        )],
                )
                .with_name_range(field_name_range)
                .with_visibility(method_visibility(field.visibility())),
            );
        }
    }
    facts
}

fn method_visibility(visibility: Visibility) -> MethodVisibility {
    match visibility {
        Visibility::Public => MethodVisibility::Public,
        Visibility::Protected => MethodVisibility::Protected,
        Visibility::Private | Visibility::Package => MethodVisibility::Private,
    }
}

fn text_range(file_id: SourceFileId, range: SourceByteRange) -> TextRange {
    TextRange::new(file_id, range.start, range.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_analysis::core::SourceKind;
    use ruby_analysis::engine::{AnalysisEngine, ResolveMode, SourceFileInput};
    use ruby_fast_lsp_jvm_metadata::{
        locate_java_source_declarations, parse_class, ClassLimits, JavaSourceLimits,
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::{Cursor, Read, Seek, SeekFrom, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use zip::write::SimpleFileOptions;

    struct CountingReader<R> {
        inner: R,
        bytes_read: Arc<AtomicU64>,
    }

    impl<R: Read> Read for CountingReader<R> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buffer)?;
            self.bytes_read.fetch_add(read as u64, Ordering::Relaxed);
            Ok(read)
        }
    }

    impl<R: Seek> Seek for CountingReader<R> {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(position)
        }
    }

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

    fn rich_declaration(root: &Path) -> JavaClassDeclaration {
        JavaClassDeclaration {
            class: parse_class(
                &decode_hex(include_str!(
                    "../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
                )),
                ClassLimits::default(),
            )
            .expect("checked class fixture must parse")
            .into(),
            artifact_path: root.join("rich.jar"),
            artifact_fingerprint_sha256: "fixture-artifact".to_string(),
            entry_name: "fixtures/RichFixture.class".to_string(),
            release: None,
        }
    }

    fn source_root(path: PathBuf, origin: SourceOrigin) -> SourceRoot {
        let (fingerprint_sha256, file_identity) = if path.is_file() {
            let metadata = fs::metadata(&path).unwrap();
            (
                Some(format!("{:x}", Sha256::digest(fs::read(&path).unwrap()))),
                Some(super::super::classpath::SourceFileIdentity {
                    byte_length: metadata.len(),
                    modified: metadata.modified().unwrap(),
                }),
            )
        } else {
            (None, None)
        };
        SourceRoot {
            path,
            origin,
            fingerprint_sha256,
            file_identity,
        }
    }

    fn source_archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, source) in entries {
            writer
                .start_file(name, SimpleFileOptions::default())
                .expect("source fixture entry must start");
            writer
                .write_all(source.as_bytes())
                .expect("source fixture entry must write");
        }
        writer
            .finish()
            .expect("source fixture archive must finish")
            .into_inner()
    }

    #[test]
    fn archive_resolution_streams_only_the_selected_entry() {
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("padding.bin", stored).unwrap();
        writer.write_all(&vec![0_u8; 8 * 1024 * 1024]).unwrap();
        writer
            .start_file("fixtures/RichFixture.java", stored)
            .unwrap();
        writer.write_all(source.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let bytes_read = Arc::new(AtomicU64::new(0));
        let reader = CountingReader {
            inner: Cursor::new(bytes.clone()),
            bytes_read: Arc::clone(&bytes_read),
        };
        let root = SourceRoot {
            path: PathBuf::from("fixture-sources.jar"),
            origin: SourceOrigin::Attached,
            fingerprint_sha256: Some("fixture".to_string()),
            file_identity: None,
        };

        let mut archive =
            zip::ZipArchive::new(reader).expect("streaming archive fixture must parse");
        let resolved = source_from_archive(
            &rich_declaration(Path::new("/fixture")),
            &root,
            &mut archive,
            Path::new("fixtures/RichFixture.java"),
            JavaSourceResolutionLimits::default(),
        )
        .expect("streaming archive resolution must succeed")
        .expect("selected source entry must resolve");

        assert_eq!(resolved.0, source);
        assert!(
            bytes_read.load(Ordering::Relaxed) < 1024 * 1024,
            "INVARIANT VIOLATED: resolving one Java source entry read the complete source archive. \
             This is a performance bug because classpath discovery already verified the archive identity. \
             Fix: keep ZipArchive backed by a seekable file and read only the selected entry."
        );
    }

    #[test]
    fn resolves_project_source_before_archives_even_when_roots_are_unsorted() {
        let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let project_root = fixture.path().join("src/main/java");
        let project_source = project_root.join("fixtures/RichFixture.java");
        fs::create_dir_all(project_source.parent().unwrap()).unwrap();
        fs::write(&project_source, source).unwrap();
        let attached = fixture.path().join("rich-sources.jar");
        fs::write(
            &attached,
            source_archive(&[("fixtures/RichFixture.java", source)]),
        )
        .unwrap();
        let resolver = JavaSourceResolver::new(
            vec![
                source_root(attached, SourceOrigin::Attached),
                source_root(project_root, SourceOrigin::Project),
            ],
            fixture.path().join("cache"),
            JavaSourceResolutionLimits::default(),
        );

        let resolved = resolver
            .resolve(&rich_declaration(fixture.path()))
            .expect("source resolution must succeed")
            .expect("project source must resolve");

        assert_eq!(resolved.path, project_source.canonicalize().unwrap());
        assert_eq!(resolved.origin, SourceOrigin::Project);
        assert_eq!(resolved.content, source);
        assert_eq!(resolved.location.internal_name, "fixtures/RichFixture");
    }

    #[test]
    fn verifies_and_materializes_attached_source_archive_outside_the_project() {
        let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let attached = fixture.path().join("rich-sources.jar");
        fs::write(
            &attached,
            source_archive(&[("fixtures/RichFixture.java", source)]),
        )
        .unwrap();
        let cache = fixture.path().join("user-cache");
        let resolver = JavaSourceResolver::new(
            vec![source_root(attached, SourceOrigin::Attached)],
            cache.clone(),
            JavaSourceResolutionLimits::default(),
        );

        let resolved = resolver
            .resolve(&rich_declaration(fixture.path()))
            .expect("source resolution must succeed")
            .expect("attached source must resolve");

        assert_eq!(resolved.origin, SourceOrigin::Attached);
        assert!(resolved.path.starts_with(&cache));
        assert!(resolved.path.ends_with("fixtures/RichFixture.java"));
        assert_eq!(fs::read_to_string(&resolved.path).unwrap(), source);
        assert_eq!(resolved.content, source);
    }

    #[test]
    fn reuses_one_parsed_archive_for_repeated_source_resolution() {
        let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let attached = fixture.path().join("rich-sources.jar");
        fs::write(
            &attached,
            source_archive(&[("fixtures/RichFixture.java", source)]),
        )
        .unwrap();
        let resolver = JavaSourceResolver::new(
            vec![source_root(attached, SourceOrigin::Attached)],
            fixture.path().join("cache"),
            JavaSourceResolutionLimits::default(),
        );
        let declaration = rich_declaration(fixture.path());

        resolver
            .resolve(&declaration)
            .expect("first source resolution must succeed")
            .expect("first source resolution must find the class");
        let first_archive = resolver.roots[0]
            .archive
            .get()
            .expect("first source resolution must initialize the archive");

        resolver
            .resolve(&declaration)
            .expect("second source resolution must succeed")
            .expect("second source resolution must find the class");
        let second_archive = resolver.roots[0]
            .archive
            .get()
            .expect("second source resolution must retain the archive");

        assert!(
            std::ptr::eq(first_archive, second_archive),
            "INVARIANT VIOLATED: repeated Java source resolution replaced the parsed archive. \
             This is a performance bug because every replacement reparses the immutable central \
             directory. Fix: retain one verified ZipArchive per prepared source root."
        );
    }

    #[test]
    fn resolves_jdk_module_prefixed_source_and_rejects_ambiguous_matches() {
        let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let jdk_source = fixture.path().join("src.zip");
        fs::write(
            &jdk_source,
            source_archive(&[("java.base/fixtures/RichFixture.java", source)]),
        )
        .unwrap();
        let resolver = JavaSourceResolver::new(
            vec![source_root(jdk_source, SourceOrigin::Jdk)],
            fixture.path().join("cache"),
            JavaSourceResolutionLimits::default(),
        );
        assert_eq!(
            resolver
                .resolve(&rich_declaration(fixture.path()))
                .expect("module-prefixed JDK source must resolve")
                .expect("JDK source must be present")
                .origin,
            SourceOrigin::Jdk
        );

        let ambiguous = fixture.path().join("ambiguous-src.zip");
        fs::write(
            &ambiguous,
            source_archive(&[
                ("java.base/fixtures/RichFixture.java", source),
                ("other.module/fixtures/RichFixture.java", source),
            ]),
        )
        .unwrap();
        let resolver = JavaSourceResolver::new(
            vec![source_root(ambiguous.clone(), SourceOrigin::Jdk)],
            fixture.path().join("cache"),
            JavaSourceResolutionLimits::default(),
        );
        assert!(matches!(
            resolver.resolve(&rich_declaration(fixture.path())),
            Err(JavaSourceResolutionError::Ambiguous {
                class_name,
                source
            }) if class_name == "fixtures/RichFixture" && source == ambiguous
        ));
    }

    #[test]
    fn rejects_source_archive_whose_discovered_fingerprint_changed() {
        let fixture = tempfile::tempdir().expect("source resolver fixture must be created");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let attached = fixture.path().join("rich-sources.jar");
        fs::write(
            &attached,
            source_archive(&[("fixtures/RichFixture.java", source)]),
        )
        .unwrap();
        let root = source_root(attached.clone(), SourceOrigin::Attached);
        fs::write(&attached, b"changed after discovery").unwrap();
        let resolver = JavaSourceResolver::new(
            vec![root],
            fixture.path().join("cache"),
            JavaSourceResolutionLimits::default(),
        );

        assert_eq!(
            resolver.resolve(&rich_declaration(fixture.path())),
            Err(JavaSourceResolutionError::FingerprintMismatch { path: attached })
        );
    }

    #[test]
    fn projects_only_metadata_verified_java_source_locations_into_engine_facts() {
        let class = parse_class(
            &decode_hex(include_str!(
                "../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
            )),
            ClassLimits::default(),
        )
        .expect("checked class fixture must parse");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
            .expect("checked source must parse")
            .expect("checked source must match the class");
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/external/fixtures/RichFixture.java"),
            content: source.to_string(),
            kind: SourceKind::External,
        });
        engine.replace_facts(
            file_id,
            java_source_navigation_facts(&class, &location, file_id),
            ResolveMode::Immediate,
        );
        let combine = FullyQualifiedName::method(
            ["Java", "Fixtures", "RichFixture"]
                .into_iter()
                .map(|part| RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
            RubyMethod::new("combine").unwrap(),
        );
        let methods = engine.query().methods_for_fqn(&combine);
        assert_eq!(methods.len(), 1);
        assert_eq!(
            &source[usize::try_from(methods[0].name_range.start_byte).unwrap()
                ..usize::try_from(methods[0].name_range.end_byte).unwrap()],
            "combine"
        );
        assert_eq!(methods[0].range.file_id, file_id);
        let proxy = FullyQualifiedName::constant(
            ["Java", "Fixtures", "RichFixture"]
                .into_iter()
                .map(|part| RubyConstant::new(part).unwrap())
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            engine.symbol_facts_for(&proxy).len(),
            1,
            "Java implementation class declarations must use the canonical constant identity"
        );
        assert!(engine.query().diagnostic_facts_in_file(file_id).is_empty());
    }

    #[test]
    fn supplemental_implementation_facts_do_not_duplicate_the_class_declaration() {
        let class = parse_class(
            &decode_hex(include_str!(
                "../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
            )),
            ClassLimits::default(),
        )
        .expect("checked class fixture must parse");
        let source = include_str!("../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
        let location = locate_java_source_declarations(&class, source, JavaSourceLimits::default())
            .expect("checked source must parse")
            .expect("checked source must match the class");
        let facts = java_source_navigation_facts_with_declaration(
            &class,
            &location,
            SourceFileId(7),
            false,
        );

        assert!(
            facts
                .symbols
                .iter()
                .all(|fact| !matches!(fact.kind, SymbolKind::Class | SymbolKind::Module)),
            "a decompiled member supplement must not compete with the preferred exact-source class declaration"
        );
        assert!(!facts.methods.is_empty());
    }
}
