//! Project-local vendor cache discovery and bounded gem archive extraction.

use super::lockfile::locked_version_for;
use super::CachedGemMetadata;
use super::GemInfo;
use super::GemSource;
use super::IndexerGem;
use super::LockedGemIdentity;
use super::LockedGemSource;
use crate::invariant::ExpectInvariant;
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use log::{info, warn};
use serde_yaml::Value as YamlValue;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tar::EntryType;

const MAX_CACHED_GEM_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

const MAX_CACHED_GEM_METADATA_BYTES: u64 = 8 * 1024 * 1024;

const MAX_CACHED_GEM_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;

const MAX_CACHED_GEM_FILES: usize = 100_000;

pub(super) const CACHED_GEM_PROJECT_DIGEST_PREFIX_CHARS: usize = 12;

pub(super) const CACHED_GEM_PROJECT_DIGEST_MARKER: &str = ".project-digest";

pub(super) fn validate_archive_identity_component(kind: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(anyhow!(
            "Gemfile.lock {kind} `{value}` cannot safely identify a vendor cache archive"
        ));
    }
    Ok(())
}

pub(super) fn cached_gem_project_digest(canonical_project_root: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(canonical_project_root.to_string_lossy().as_bytes())
    )
}

fn is_safe_cache_identity_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn cached_gem_project_leaf(canonical_project_root: &Path) -> &str {
    canonical_project_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| is_safe_cache_identity_component(name))
        .unwrap_or("project")
}

pub(super) fn cached_gem_project_identity(canonical_project_root: &Path) -> String {
    format!(
        "{}-{}",
        cached_gem_project_leaf(canonical_project_root),
        &cached_gem_project_digest(canonical_project_root)
            [..CACHED_GEM_PROJECT_DIGEST_PREFIX_CHARS]
    )
}

pub(super) fn cached_gem_project_extraction_root(
    cache_root: &Path,
    canonical_project_root: &Path,
) -> PathBuf {
    let digest = cached_gem_project_digest(canonical_project_root);
    let leaf = cached_gem_project_leaf(canonical_project_root);
    let gems_root = cache_root.join("gems");
    let short_root = gems_root.join(cached_gem_project_identity(canonical_project_root));
    match std::fs::read_to_string(short_root.join(CACHED_GEM_PROJECT_DIGEST_MARKER)) {
        Ok(existing) if existing == digest => short_root,
        Ok(_) => gems_root.join(format!("{leaf}-{digest}")),
        Err(_) => short_root,
    }
}

fn bind_cached_gem_project_digest(extraction_root: &Path, project_digest: &str) -> Result<()> {
    let marker = extraction_root.join(CACHED_GEM_PROJECT_DIGEST_MARKER);
    match std::fs::read_to_string(&marker) {
        Ok(existing) => {
            invariant_eq!(
                existing,
                project_digest,
                what = "gem extraction root {} is bound to digest {existing}, expected {project_digest}",
                why = "an identity directory belongs to one project",
                fix = "resolve the free short identity or full-digest fallback before extracting",
                extraction_root.display(),
                existing = existing,
                project_digest = project_digest,
            );
            Ok(())
        }
        Err(_) => std::fs::write(&marker, project_digest).with_context(|| {
            format!(
                "failed to write cached gem project identity marker {}",
                marker.display()
            )
        }),
    }
}

fn extract_cached_gem_archive(
    extraction_root: &Path,
    archive_path: &Path,
    locked: &LockedGemIdentity,
    project_digest: &str,
) -> Result<GemInfo> {
    let archive_metadata = std::fs::metadata(archive_path)
        .with_context(|| format!("failed to inspect {}", archive_path.display()))?;
    let archive_size = archive_metadata.len();
    if archive_size > MAX_CACHED_GEM_ARCHIVE_BYTES {
        return Err(anyhow!(
            "archive is {archive_size} bytes, exceeding the {MAX_CACHED_GEM_ARCHIVE_BYTES}-byte limit"
        ));
    }
    let gem_root = extraction_root.join(format!("{}-{}", locked.name, locked.locked_version));
    let archive_stat = ArchiveStat::of(&archive_metadata);
    if let Some(stamp) = ExtractionStamp::read_matching(&gem_root, &archive_stat) {
        bind_cached_gem_project_digest(extraction_root, project_digest)?;
        return extracted_gem_info(gem_root, locked, stamp.metadata);
    }
    let archive_bytes = std::fs::read(archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;
    let metadata_gzip =
        read_gem_package_member(&archive_bytes, "metadata.gz", MAX_CACHED_GEM_METADATA_BYTES)?;
    let metadata_yaml = decompress_bounded(
        &metadata_gzip,
        MAX_CACHED_GEM_METADATA_BYTES,
        "gem metadata",
    )?;
    let metadata = parse_cached_gem_metadata(&metadata_yaml)?;
    if metadata.name != locked.name || metadata.locked_version != locked.locked_version {
        return Err(anyhow!(
            "archive metadata identifies {} v{}, but Gemfile.lock requires {} v{}",
            metadata.name,
            metadata.locked_version,
            locked.name,
            locked.locked_version
        ));
    }

    let checksum = format!("{:x}", Sha256::digest(&archive_bytes));
    let completion_marker = gem_root.join(".complete");
    let marker_matches =
        std::fs::read_to_string(&completion_marker).is_ok_and(|contents| contents == checksum);
    if !marker_matches {
        extract_cached_gem_data(
            &archive_bytes,
            extraction_root,
            &gem_root,
            &checksum,
            &metadata.require_paths,
        )?;
    }
    bind_cached_gem_project_digest(extraction_root, project_digest)?;
    let stamp = ExtractionStamp {
        stat: archive_stat,
        checksum,
        metadata,
    };
    stamp.write(&gem_root)?;
    extracted_gem_info(gem_root, locked, stamp.metadata)
}

fn extracted_gem_info(
    gem_root: PathBuf,
    locked: &LockedGemIdentity,
    metadata: CachedGemMetadata,
) -> Result<GemInfo> {
    let lib_paths = metadata
        .require_paths
        .iter()
        .map(|path| gem_root.join(path))
        .collect::<Vec<_>>();
    if lib_paths.iter().any(|path| !path.is_dir()) {
        return Err(anyhow!(
            "archive did not contain every declared require path for {} v{}",
            locked.name,
            locked.locked_version
        ));
    }

    Ok(GemInfo {
        name: locked.name.clone(),
        version: metadata.version,
        platform: metadata.platform,
        locked_version: locked.locked_version.clone(),
        source: GemSource::VendorArchive,
        path: gem_root,
        lib_paths,
        dependencies: locked.dependencies.clone(),
        is_default: false,
    })
}

const EXTRACTION_STAMP: &str = ".archive-stamp";
const EXTRACTION_STAMP_SCHEMA: &str = "rflsp-gem-stamp-v1";

/// The file-system identity of an archive. A changed size, modification time,
/// or inode means the archive must be read and hashed again.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveStat {
    size: u64,
    modified_nanos: Option<u128>,
    inode: Option<(u64, u64)>,
}

impl ArchiveStat {
    fn of(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let inode = None;
        Self {
            size: metadata.len(),
            modified_nanos: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_nanos()),
            inode,
        }
    }

    /// Without a modification time and inode, size alone cannot identify the
    /// archive, so it is always read again.
    fn is_identifying(&self) -> bool {
        self.modified_nanos.is_some() && self.inode.is_some()
    }
}

/// What a verified extraction recorded about its archive, so a later start
/// with the same archive skips reading and hashing it.
struct ExtractionStamp {
    stat: ArchiveStat,
    checksum: String,
    metadata: CachedGemMetadata,
}

impl ExtractionStamp {
    fn read_matching(gem_root: &Path, stat: &ArchiveStat) -> Option<Self> {
        if !stat.is_identifying() {
            return None;
        }
        let text = std::fs::read_to_string(gem_root.join(EXTRACTION_STAMP)).ok()?;
        let stamp = Self::parse(&text)?;
        let completed = std::fs::read_to_string(gem_root.join(".complete")).ok()?;
        (stamp.stat == *stat && completed == stamp.checksum).then_some(stamp)
    }

    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != EXTRACTION_STAMP_SCHEMA {
            return None;
        }
        let mut field = |key: &str| lines.next()?.strip_prefix(key)?.strip_prefix('=');
        let size = field("size")?.parse().ok()?;
        let modified_nanos = Some(field("modified")?.parse().ok()?);
        let (dev, ino) = field("inode")?.split_once(':')?;
        let inode = Some((dev.parse().ok()?, ino.parse().ok()?));
        let checksum = field("checksum")?.to_owned();
        let name = field("name")?.to_owned();
        let version = field("version")?.to_owned();
        let platform = field("platform")?.to_owned();
        let locked_version = field("locked")?.to_owned();
        let require_paths = lines
            .map(|line| line.strip_prefix("require=").map(PathBuf::from))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            stat: ArchiveStat {
                size,
                modified_nanos,
                inode,
            },
            checksum,
            metadata: CachedGemMetadata {
                name,
                version,
                platform,
                locked_version,
                require_paths,
            },
        })
    }

    /// Publish the stamp atomically; a reader sees the old stamp or this one.
    fn write(&self, gem_root: &Path) -> Result<()> {
        let (Some(modified), Some((dev, ino))) = (self.stat.modified_nanos, self.stat.inode) else {
            return Ok(());
        };
        let metadata = &self.metadata;
        let mut text = format!(
            "{EXTRACTION_STAMP_SCHEMA}\nsize={}\nmodified={modified}\ninode={dev}:{ino}\n\
             checksum={}\nname={}\nversion={}\nplatform={}\nlocked={}\n",
            self.stat.size,
            self.checksum,
            metadata.name,
            metadata.version,
            metadata.platform,
            metadata.locked_version,
        );
        for path in &metadata.require_paths {
            let path = path
                .to_str()
                .ok_or_else(|| anyhow!("gem require path {} is not valid UTF-8", path.display()))?;
            text.push_str("require=");
            text.push_str(path);
            text.push('\n');
        }
        let temporary = gem_root.join(format!("{EXTRACTION_STAMP}.tmp-{}", std::process::id()));
        std::fs::write(&temporary, text)
            .and_then(|()| std::fs::rename(&temporary, gem_root.join(EXTRACTION_STAMP)))
            .with_context(|| {
                format!(
                    "failed to record cached gem extraction stamp in {}",
                    gem_root.display()
                )
            })
    }
}

fn read_gem_package_member(
    archive_bytes: &[u8],
    member_name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    let mut package = tar::Archive::new(Cursor::new(archive_bytes));
    for entry in package
        .entries()
        .context("failed to read gem package tar")?
    {
        let mut entry = entry.context("failed to read gem package entry")?;
        let path = entry
            .path()
            .context("gem package entry has an invalid path")?;
        if path != Path::new(member_name) {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(anyhow!("gem package member `{member_name}` is not a file"));
        }
        let size = entry.size();
        if size > max_bytes {
            return Err(anyhow!(
                "gem package member `{member_name}` is {size} bytes, exceeding the {max_bytes}-byte limit"
            ));
        }
        let mut bytes = Vec::with_capacity(
            usize::try_from(size)
                .context("gem package member size cannot be represented on this platform")?,
        );
        entry
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read gem package member `{member_name}`"))?;
        return Ok(bytes);
    }
    Err(anyhow!(
        "gem package is missing required member `{member_name}`"
    ))
}

fn decompress_bounded(compressed: &[u8], max_bytes: u64, label: &str) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(Cursor::new(compressed));
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut output)
        .with_context(|| format!("failed to decompress {label}"))?;
    if u64::try_from(output.len()).context("decompressed length cannot fit u64")? > max_bytes {
        return Err(anyhow!(
            "{label} exceeds the {max_bytes}-byte decompression limit"
        ));
    }
    Ok(output)
}

fn parse_cached_gem_metadata(metadata_yaml: &[u8]) -> Result<CachedGemMetadata> {
    let value: YamlValue =
        serde_yaml::from_slice(metadata_yaml).context("failed to parse gem metadata YAML")?;
    let name = yaml_string_field(&value, "name")?;
    let version_value = yaml_field(&value, "version")?;
    let version = match untag_yaml(version_value) {
        YamlValue::String(version) => version.clone(),
        YamlValue::Mapping(_) => yaml_string_field(version_value, "version")?,
        other => {
            return Err(anyhow!(
                "gem metadata version has unsupported YAML shape: {other:?}"
            ));
        }
    };
    let platform = yaml_string_field(&value, "platform")?;
    let platform = if platform.is_empty() {
        "ruby".to_string()
    } else {
        platform
    };
    let locked_version = locked_version_for(&version, &platform);
    let require_paths_value = yaml_field(&value, "require_paths")?;
    let YamlValue::Sequence(paths) = untag_yaml(require_paths_value) else {
        return Err(anyhow!("gem metadata require_paths must be a sequence"));
    };
    if paths.is_empty() {
        return Err(anyhow!("gem metadata require_paths must not be empty"));
    }
    let require_paths = paths
        .iter()
        .map(|value| {
            let YamlValue::String(path) = untag_yaml(value) else {
                return Err(anyhow!("gem metadata require path must be a string"));
            };
            validate_relative_archive_path(path)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(CachedGemMetadata {
        name,
        version,
        platform,
        locked_version,
        require_paths,
    })
}

fn untag_yaml(value: &YamlValue) -> &YamlValue {
    match value {
        YamlValue::Tagged(tagged) => untag_yaml(&tagged.value),
        YamlValue::Null
        | YamlValue::Bool(_)
        | YamlValue::Number(_)
        | YamlValue::String(_)
        | YamlValue::Sequence(_)
        | YamlValue::Mapping(_) => value,
    }
}

fn yaml_field<'a>(value: &'a YamlValue, field: &str) -> Result<&'a YamlValue> {
    let YamlValue::Mapping(mapping) = untag_yaml(value) else {
        return Err(anyhow!("gem metadata root must be a mapping"));
    };
    mapping
        .get(YamlValue::String(field.to_string()))
        .ok_or_else(|| anyhow!("gem metadata is missing `{field}`"))
}

fn yaml_string_field(value: &YamlValue, field: &str) -> Result<String> {
    let field_value = yaml_field(value, field)?;
    match untag_yaml(field_value) {
        YamlValue::String(value) => Ok(value.clone()),
        other => Err(anyhow!(
            "gem metadata `{field}` must be a string, found {other:?}"
        )),
    }
}

fn validate_relative_archive_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(anyhow!(
            "gem archive path `{}` is not a safe relative path",
            path.display()
        ));
    }
    Ok(path.to_path_buf())
}

fn extract_cached_gem_data(
    package_bytes: &[u8],
    cache_root: &Path,
    gem_root: &Path,
    checksum: &str,
    require_paths: &[PathBuf],
) -> Result<()> {
    let data_gzip =
        read_gem_package_member(package_bytes, "data.tar.gz", MAX_CACHED_GEM_ARCHIVE_BYTES)?;
    let temporary_root = cache_root.join(format!(".{checksum}.tmp-{}", std::process::id()));
    if temporary_root.exists() {
        std::fs::remove_dir_all(&temporary_root).with_context(|| {
            format!(
                "failed to clear incomplete cached gem extraction {}",
                temporary_root.display()
            )
        })?;
    }
    std::fs::create_dir_all(&temporary_root).with_context(|| {
        format!(
            "failed to create cached gem extraction {}",
            temporary_root.display()
        )
    })?;

    let extraction = (|| -> Result<()> {
        let decoder = GzDecoder::new(Cursor::new(data_gzip));
        let mut archive = tar::Archive::new(decoder);
        let mut file_count = 0usize;
        let mut extracted_bytes = 0u64;
        for entry in archive.entries().context("failed to read gem data tar")? {
            let mut entry = entry.context("failed to read gem data entry")?;
            let path = entry.path().context("gem data entry has an invalid path")?;
            let path_text = path
                .to_str()
                .ok_or_else(|| anyhow!("gem data entry path is not valid UTF-8"))?;
            let path = validate_relative_archive_path(path_text)?;
            if !require_paths
                .iter()
                .any(|require_path| path.starts_with(require_path))
            {
                continue;
            }

            match entry.header().entry_type() {
                EntryType::Directory => continue,
                EntryType::Regular => {}
                entry_type => {
                    return Err(anyhow!(
                        "gem data entry {} uses unsupported type {entry_type:?}",
                        path.display()
                    ));
                }
            }
            file_count += 1;
            if file_count > MAX_CACHED_GEM_FILES {
                return Err(anyhow!(
                    "gem data contains more than {MAX_CACHED_GEM_FILES} files"
                ));
            }
            extracted_bytes = extracted_bytes
                .checked_add(entry.size())
                .ok_or_else(|| anyhow!("gem extracted byte count overflowed u64"))?;
            if extracted_bytes > MAX_CACHED_GEM_EXTRACTED_BYTES {
                return Err(anyhow!(
                    "gem data exceeds the {MAX_CACHED_GEM_EXTRACTED_BYTES}-byte extraction limit"
                ));
            }

            let destination = temporary_root.join(&path);
            let parent = destination.parent().ok_or_else(|| {
                anyhow!(
                    "gem data destination {} has no parent",
                    destination.display()
                )
            })?;
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cached gem directory {}", parent.display())
            })?;
            let mut output = std::fs::File::create(&destination).with_context(|| {
                format!("failed to create cached gem file {}", destination.display())
            })?;
            let copied = std::io::copy(&mut entry, &mut output)
                .with_context(|| format!("failed to extract cached gem file {}", path.display()))?;
            if copied != entry.size() {
                return Err(anyhow!(
                    "cached gem file {} declared {} bytes but yielded {copied}",
                    path.display(),
                    entry.size()
                ));
            }
        }
        if file_count == 0 {
            return Err(anyhow!(
                "gem data contains no regular files under its declared require paths"
            ));
        }
        std::fs::write(temporary_root.join(".complete"), checksum)
            .context("failed to write cached gem completion marker")?;
        Ok(())
    })();

    if let Err(error) = extraction {
        let _ = std::fs::remove_dir_all(&temporary_root);
        return Err(error);
    }

    let extraction_parent = gem_root.parent().ok_or_else(|| {
        anyhow!(
            "cached gem destination {} has no project extraction parent",
            gem_root.display()
        )
    })?;
    std::fs::create_dir_all(extraction_parent).with_context(|| {
        format!(
            "failed to create cached gem project extraction directory {}",
            extraction_parent.display()
        )
    })?;
    if gem_root.exists() {
        std::fs::remove_dir_all(gem_root).with_context(|| {
            format!(
                "failed to replace incomplete cached gem destination {}",
                gem_root.display()
            )
        })?;
    }
    std::fs::rename(&temporary_root, gem_root).with_context(|| {
        format!(
            "failed to publish cached gem extraction {}",
            gem_root.display()
        )
    })?;
    Ok(())
}

impl IndexerGem {
    /// Add extracted Bundler Git caches using only lockfile metadata. Gemfiles
    /// and gemspecs are project code and must not be executed by this fallback.
    pub(super) fn discover_cached_git_gems(&mut self) -> Result<()> {
        self.load_locked_gems()?;
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let lockfile_path = root.join("Gemfile.lock");
        let lockfile = match std::fs::read_to_string(&lockfile_path) {
            Ok(lockfile) => lockfile,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read owning project lockfile {}",
                        lockfile_path.display()
                    )
                });
            }
        };
        let cache_root = root.join("vendor/cache");
        if !cache_root.is_dir() {
            return Ok(());
        }

        let mut lines = lockfile.lines().peekable();
        while let Some(line) = lines.next() {
            if line != "GIT" {
                continue;
            }
            let mut remote = None;
            let mut revision = None;
            let mut specs = Vec::new();
            while let Some(section_line) = lines.peek().copied() {
                if !section_line.is_empty() && !section_line.starts_with(' ') {
                    break;
                }
                let section_line = lines.next().expect_invariant(
                    "peeked lockfile line disappeared",
                    "iterator state must remain stable between peek and next",
                    "keep lockfile parsing single-threaded",
                );
                if let Some(value) = section_line.strip_prefix("  remote: ") {
                    remote = Some(value.to_string());
                } else if let Some(value) = section_line.strip_prefix("  revision: ") {
                    revision = Some(value.to_string());
                } else if section_line.starts_with("    ") && !section_line.starts_with("      ") {
                    let spec = section_line.trim();
                    if let Some((name, version)) = spec.split_once(" (") {
                        if let Some(version) = version.strip_suffix(')') {
                            specs.push((name.to_string(), version.to_string()));
                        }
                    }
                }
            }

            let (Some(remote), Some(revision)) = (remote, revision) else {
                continue;
            };
            if revision.len() < 7 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                warn!(
                    "Skipping cached Git dependency with invalid lockfile revision: {}",
                    revision
                );
                continue;
            }
            let repository = remote
                .rsplit(['/', ':'])
                .next()
                .map(|name| name.strip_suffix(".git").unwrap_or(name))
                .filter(|name| !name.is_empty());
            let Some(repository) = repository else {
                continue;
            };
            let revision_prefix = revision.get(..revision.len().min(12)).expect_invariant(
                "Git revision prefix is not a UTF-8 boundary",
                "lockfile revisions must be ASCII hexadecimal",
                "validate Bundler lockfile revision syntax before slicing",
            );
            let cache_path = cache_root.join(format!("{repository}-{revision_prefix}"));
            let lib_path = cache_path.join("lib");
            if !cache_path.is_dir() || !lib_path.is_dir() {
                continue;
            }
            for (name, version) in specs {
                let locked = self.locked_gems.get(&name).ok_or_else(|| {
                    anyhow!(
                        "Gemfile.lock Git cache parser produced `{name}` without a locked identity"
                    )
                })?;
                if locked.source != LockedGemSource::Git || locked.locked_version != version {
                    return Err(anyhow!(
                        "Gemfile.lock Git cache identity for `{name}` disagrees with the unified lock parser"
                    ));
                }
                self.discovered_gems
                    .entry(name.clone())
                    .or_default()
                    .push(GemInfo {
                        name,
                        version: version.clone(),
                        platform: "ruby".to_string(),
                        locked_version: version,
                        source: GemSource::VendorGit,
                        path: cache_path.clone(),
                        lib_paths: vec![lib_path.clone()],
                        dependencies: locked.dependencies.clone(),
                        is_default: false,
                    });
            }
        }
        Ok(())
    }

    pub(super) fn discover_cached_gem_archives(&mut self) -> Result<()> {
        self.load_locked_gems()?;
        self.discover_cached_gem_archives_matching(None)
    }

    pub(super) fn discover_priority_cached_gem_archives(
        &mut self,
        priority_keys: &HashSet<String>,
    ) -> Result<()> {
        invariant!(
            !priority_keys.is_empty(),
            what = "priority vendor-archive discovery received no keys",
            why = "an empty navigation frontier cannot select bounded work",
            fix = "skip the priority phase when the active document exposes no constant roots",
        );
        // Archives are selected only by locked identity; a project without a
        // lockfile (or with an empty one) locks no archive to read.
        if self.locked_gems.is_empty() {
            return Ok(());
        }
        self.discover_cached_gem_archives_matching(Some(priority_keys))
    }

    fn discover_cached_gem_archives_matching(
        &mut self,
        priority_keys: Option<&HashSet<String>>,
    ) -> Result<()> {
        let Some(root) = &self.workspace_root else {
            return Ok(());
        };
        let cache_root = root.join("vendor/cache");
        if !cache_root.is_dir() {
            return Ok(());
        }
        let (extraction_root, project_digest) = self.cached_gem_extraction_context(root)?;

        let mut locked_registry_gems = self
            .locked_gems
            .values()
            .filter(|identity| identity.source == LockedGemSource::Registry)
            .cloned()
            .collect::<Vec<_>>();
        locked_registry_gems.sort_by(|left, right| left.name.cmp(&right.name));

        for locked in &locked_registry_gems {
            if priority_keys.is_some_and(|keys| {
                !keys.contains(&crate::loader::coordinator::dependency_priority_key(
                    &locked.name,
                ))
            }) {
                continue;
            }
            let exact_installed_source_exists =
                self.discovered_gems
                    .get(&locked.name)
                    .is_some_and(|candidates| {
                        candidates.iter().any(|candidate| {
                            matches!(
                                candidate.source,
                                GemSource::BundlerInstalled
                                    | GemSource::GlobalInstalled
                                    | GemSource::VendorArchive
                            ) && candidate.locked_version == locked.locked_version
                                && candidate.lib_paths.iter().any(|path| path.is_dir())
                        })
                    });
            if exact_installed_source_exists {
                continue;
            }

            let archive_name = format!("{}-{}.gem", locked.name, locked.locked_version);
            let archive_path = cache_root.join(&archive_name);
            if !archive_path.is_file() {
                continue;
            }

            match extract_cached_gem_archive(
                &extraction_root,
                &archive_path,
                locked,
                &project_digest,
            ) {
                Ok(gem) => {
                    info!(
                        "Discovered locked vendor cache gem: {} v{}",
                        gem.name, gem.version
                    );
                    self.discovered_gems
                        .entry(gem.name.clone())
                        .or_default()
                        .push(gem);
                }
                Err(error) => {
                    warn!(
                        "Skipping invalid cached gem archive {}: {error:#}",
                        archive_path.display()
                    );
                }
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub(super) fn cached_gem_extraction_root(&self, project_root: &Path) -> Result<PathBuf> {
        Ok(self.cached_gem_extraction_context(project_root)?.0)
    }

    fn cached_gem_extraction_context(&self, project_root: &Path) -> Result<(PathBuf, String)> {
        let cache_root = match &self.cached_gem_root_override {
            Some(root) => root.clone(),
            None => crate::utils::cache::ruby_fast_lsp_user_cache_root()?,
        };
        let canonical_project_root = project_root.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize Ruby project root {} for cached gem isolation",
                project_root.display()
            )
        })?;
        let project_digest = cached_gem_project_digest(&canonical_project_root);
        Ok((
            cached_gem_project_extraction_root(&cache_root, &canonical_project_root),
            project_digest,
        ))
    }
}
