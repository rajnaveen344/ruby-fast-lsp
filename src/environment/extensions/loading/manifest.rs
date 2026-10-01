use std::collections::BTreeSet;
use std::path::{Component, Path};

use globset::{Glob, GlobSet, GlobSetBuilder};
use log::warn;
use ruby_analysis::core::{GraphNodeKind, NamespaceKind, RubyConstant, RubyMethod};

use semver::{Version, VersionReq};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::environment::extensions::loading::config::ExtensionLoadError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::environment::extensions) struct ExtensionMethodTarget {
    pub(in crate::environment::extensions) owner: Vec<RubyConstant>,
    pub(in crate::environment::extensions) owner_kind: NamespaceKind,
    pub(in crate::environment::extensions) method: RubyMethod,
    pub(in crate::environment::extensions) frame: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::environment::extensions) struct ExtensionNamespaceTarget {
    pub(in crate::environment::extensions) owner: Vec<RubyConstant>,
    pub(in crate::environment::extensions) declaration_kind: GraphNodeKind,
}

#[derive(Clone, Debug)]
pub(in crate::environment::extensions) struct ExtensionMetadata {
    pub(in crate::environment::extensions) id: String,
    pub(in crate::environment::extensions) name: Option<String>,
    pub(in crate::environment::extensions) version: Option<String>,
    pub(in crate::environment::extensions) capabilities: Vec<String>,
    pub(in crate::environment::extensions) permissions: Vec<String>,
    pub(in crate::environment::extensions) watched_files: Vec<String>,
    pub(in crate::environment::extensions) process_commands: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(in crate::environment::extensions) struct ExtensionManifest {
    pub(super) id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    abi_version: u32,
    #[serde(default)]
    server_version: Option<String>,
    runtime: String,
    pub(super) wasm: Option<String>,
    #[serde(default)]
    checksum_sha256: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    settings_schema: Option<toml::Value>,
    pub(super) build: Option<ExtensionBuildManifest>,
    pub(in crate::environment::extensions) indexing: Option<ExtensionIndexingManifest>,
    watching: Option<ExtensionWatchingManifest>,
    process: Option<ExtensionProcessManifest>,
    applicability: Option<ExtensionApplicabilityManifest>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionApplicabilityManifest {
    locked_gems: Vec<ExtensionGemRequirementManifest>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionGemRequirementManifest {
    name: String,
    version: String,
}

#[derive(Clone, Debug)]
pub(in crate::environment::extensions) struct ExtensionGemRequirement {
    pub(in crate::environment::extensions) name: String,
    pub(in crate::environment::extensions) version: VersionReq,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct ExtensionBuildManifest {
    pub(super) output: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(in crate::environment::extensions) struct ExtensionIndexingManifest {
    call_names: Vec<String>,
    #[serde(default)]
    pub(super) project_context: ExtensionProjectContextDelivery,
    #[serde(default)]
    pub(in crate::environment::extensions) frame_call_names: Vec<String>,
    #[serde(default)]
    targets: Vec<ExtensionMethodTargetManifest>,
    #[serde(default)]
    namespaces: Vec<ExtensionNamespaceTargetManifest>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(in crate::environment::extensions) enum ExtensionProjectContextDelivery {
    #[default]
    PerCall,
    Activation,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionMethodTargetManifest {
    owner: Vec<String>,
    owner_kind: String,
    method: String,
    #[serde(default)]
    frame: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionNamespaceTargetManifest {
    owner: Vec<String>,
    declaration_kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionWatchingManifest {
    globs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExtensionProcessManifest {
    commands: Vec<String>,
}

pub(super) fn parse_manifest_applicability(
    manifest: &ExtensionManifest,
) -> Result<Vec<ExtensionGemRequirement>, ExtensionLoadError> {
    let Some(applicability) = &manifest.applicability else {
        return Ok(Vec::new());
    };
    applicability
        .locked_gems
        .iter()
        .map(|gem| {
            let version = VersionReq::parse(&gem.version).map_err(|error| {
                ExtensionLoadError::new(format!(
                    "extension `{}` applicability for gem `{}` has invalid version requirement `{}`: {error}",
                    manifest.id, gem.name, gem.version
                ))
            })?;
            Ok(ExtensionGemRequirement {
                name: gem.name.clone(),
                version,
            })
        })
        .collect()
}

pub(in crate::environment::extensions) fn validate_manifest(
    manifest: &ExtensionManifest,
) -> Result<(), ExtensionLoadError> {
    validate_manifest_id(&manifest.id)?;
    validate_optional_non_empty("name", &manifest.id, manifest.name.as_deref())?;
    validate_optional_non_empty("version", &manifest.id, manifest.version.as_deref())?;
    validate_manifest_list("capability", &manifest.id, &manifest.capabilities)?;
    validate_manifest_list("permission", &manifest.id, &manifest.permissions)?;
    if manifest.abi_version != ruby_fast_lsp_extension_api::ABI_VERSION {
        return Err(ExtensionLoadError::new(format!(
            "extension manifest `{}` ABI version {} != host ABI version {}",
            manifest.id,
            manifest.abi_version,
            ruby_fast_lsp_extension_api::ABI_VERSION
        )));
    }
    if !matches!(manifest.runtime.as_str(), "wasm" | "mruby-wasm") {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` runtime `{}` is unsupported",
            manifest.id, manifest.runtime
        )));
    }
    if let Some(applicability) = &manifest.applicability {
        if applicability.locked_gems.is_empty() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` applicability.locked_gems must not be empty",
                manifest.id
            )));
        }
        let mut names = BTreeSet::new();
        for gem in &applicability.locked_gems {
            if gem.name.is_empty()
                || gem.name.len() > 128
                || !gem
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` applicability gem name `{}` is invalid",
                    manifest.id, gem.name
                )));
            }
            if !names.insert(gem.name.as_str()) {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` applicability declares gem `{}` more than once",
                    manifest.id, gem.name
                )));
            }
            VersionReq::parse(&gem.version).map_err(|error| {
                ExtensionLoadError::new(format!(
                    "extension `{}` applicability for gem `{}` has invalid version requirement `{}`: {error}",
                    manifest.id, gem.name, gem.version
                ))
            })?;
        }
    }
    if let Some(server_version) = &manifest.server_version {
        validate_server_version(&manifest.id, server_version)?;
    }
    if manifest.settings_schema.is_some()
        && !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "settings")
    {
        warn!(
            "Extension `{}` declares settings_schema without `settings` capability",
            manifest.id
        );
    }
    if let Some(indexing) = &manifest.indexing {
        validate_manifest_list("indexing call name", &manifest.id, &indexing.call_names)?;
        validate_manifest_list(
            "indexing frame call name",
            &manifest.id,
            &indexing.frame_call_names,
        )?;
        for name in &indexing.frame_call_names {
            RubyMethod::new(name).map_err(|err| {
                ExtensionLoadError::new(format!(
                    "extension `{}` indexing frame call name `{name}` is invalid: {err}",
                    manifest.id
                ))
            })?;
        }
    }
    if let Some(watching) = &manifest.watching {
        validate_manifest_list("watched file glob", &manifest.id, &watching.globs)?;
        if !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "watching")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares watched files without `watching` capability",
                manifest.id
            )));
        }
        build_watched_file_matcher(&manifest.id, &watching.globs)?;
    }
    if let Some(process) = &manifest.process {
        validate_manifest_list("process command", &manifest.id, &process.commands)?;
        if !manifest
            .capabilities
            .iter()
            .any(|capability| capability == "process")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares process commands without `process` capability",
                manifest.id
            )));
        }
        if !manifest
            .permissions
            .iter()
            .any(|permission| permission == "process.exec")
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` declares process commands without `process.exec` permission",
                manifest.id
            )));
        }
        for command in &process.commands {
            let path = Path::new(command);
            if path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` process command `{command}` must be a bare executable or workspace-relative path without traversal",
                    manifest.id
                )));
            }
        }
    }
    Ok(())
}

pub(in crate::environment::extensions) fn build_watched_file_matcher(
    extension_id: &str,
    globs: &[String],
) -> Result<GlobSet, ExtensionLoadError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in globs {
        let path = Path::new(pattern);
        let has_windows_drive_prefix = pattern
            .as_bytes()
            .get(1)
            .is_some_and(|character| *character == b':');
        if path.is_absolute()
            || pattern.starts_with('/')
            || pattern.starts_with('\\')
            || pattern.contains('\\')
            || has_windows_drive_prefix
            || pattern.split('/').any(|component| component == "..")
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` watched file glob `{pattern}` must be workspace-relative and cannot contain parent traversal"
            )));
        }
        let glob = Glob::new(pattern).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` has invalid watched file glob `{pattern}`: {err}"
            ))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{extension_id}` failed to compile watched file globs: {err}"
        ))
    })
}

pub(super) fn validate_manifest_checksum(
    manifest: &ExtensionManifest,
    wasm_bytes: &[u8],
) -> Result<(), ExtensionLoadError> {
    let Some(expected) = &manifest.checksum_sha256 else {
        return Ok(());
    };
    if expected.len() != 64
        || !expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` checksum_sha256 must be 64 hex characters",
            manifest.id
        )));
    }
    let actual = format!("{:x}", Sha256::digest(wasm_bytes));
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` checksum mismatch: manifest {} != actual {}",
            manifest.id, expected, actual
        )));
    }
    Ok(())
}

fn validate_manifest_id(id: &str) -> Result<(), ExtensionLoadError> {
    if id.trim().is_empty() || id.chars().any(char::is_whitespace) {
        return Err(ExtensionLoadError::new(format!(
            "extension manifest id `{}` must be non-empty and contain no whitespace",
            id
        )));
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &str,
    id: &str,
    value: Option<&str>,
) -> Result<(), ExtensionLoadError> {
    if matches!(value, Some(value) if value.trim().is_empty()) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` manifest field `{}` must not be empty",
            id, field
        )));
    }
    Ok(())
}

fn validate_manifest_list(
    label: &str,
    id: &str,
    values: &[String],
) -> Result<(), ExtensionLoadError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` manifest {} must not be empty",
                id, label
            )));
        }
        if !seen.insert(value) {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` manifest has duplicate {} `{}`",
                id, label, value
            )));
        }
    }
    Ok(())
}

fn validate_server_version(id: &str, requirement: &str) -> Result<(), ExtensionLoadError> {
    let requirement = VersionReq::parse(requirement).map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{}` has invalid server_version `{}`: {}",
            id, requirement, err
        ))
    })?;
    let server_version = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|err| {
        ExtensionLoadError::new(format!(
            "host server version `{}` is invalid semver: {}",
            env!("CARGO_PKG_VERSION"),
            err
        ))
    })?;
    if !requirement.matches(&server_version) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` requires server_version `{}` but host is `{}`",
            id, requirement, server_version
        )));
    }
    Ok(())
}

pub(super) fn extension_metadata(
    id: &str,
    manifest: Option<&ExtensionManifest>,
) -> ExtensionMetadata {
    match manifest {
        Some(manifest) => ExtensionMetadata {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            capabilities: manifest.capabilities.clone(),
            permissions: manifest.permissions.clone(),
            watched_files: manifest
                .watching
                .as_ref()
                .map(|watching| watching.globs.clone())
                .unwrap_or_default(),
            process_commands: manifest
                .process
                .as_ref()
                .map(|process| process.commands.clone())
                .unwrap_or_default(),
        },
        None => ExtensionMetadata {
            id: id.to_string(),
            name: None,
            version: None,
            capabilities: Vec::new(),
            permissions: Vec::new(),
            watched_files: Vec::new(),
            process_commands: Vec::new(),
        },
    }
}

pub(super) fn validate_manifest_call_names(
    manifest: &ExtensionManifest,
    guest_call_names: &[String],
) -> Result<(), ExtensionLoadError> {
    let Some(indexing) = &manifest.indexing else {
        return Ok(());
    };
    let manifest_names: BTreeSet<&String> = indexing.call_names.iter().collect();
    let guest_names: BTreeSet<&String> = guest_call_names.iter().collect();
    if manifest_names != guest_names {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` manifest call names {:?} != guest call names {:?}",
            manifest.id, indexing.call_names, guest_call_names
        )));
    }
    Ok(())
}

pub(in crate::environment::extensions) fn parse_manifest_method_targets(
    manifest: &ExtensionManifest,
) -> Result<Vec<ExtensionMethodTarget>, ExtensionLoadError> {
    let Some(indexing) = &manifest.indexing else {
        return Ok(Vec::new());
    };
    indexing
        .targets
        .iter()
        .map(|target| parse_manifest_method_target(&manifest.id, target))
        .collect()
}

pub(in crate::environment::extensions) fn parse_manifest_namespace_targets(
    manifest: &ExtensionManifest,
) -> Result<Vec<ExtensionNamespaceTarget>, ExtensionLoadError> {
    let Some(indexing) = &manifest.indexing else {
        return Ok(Vec::new());
    };
    indexing
        .namespaces
        .iter()
        .map(|target| {
            let owner = target
                .owner
                .iter()
                .map(|part| {
                    RubyConstant::new(part).map_err(|err| {
                        ExtensionLoadError::new(format!(
                            "extension `{}` indexing namespace owner part `{}` is invalid: {}",
                            manifest.id, part, err
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if owner.is_empty() {
                return Err(ExtensionLoadError::new(format!(
                    "extension `{}` indexing namespace owner must not be empty",
                    manifest.id
                )));
            }
            let declaration_kind = match target.declaration_kind.as_str() {
                "class" => GraphNodeKind::Class,
                "module" => GraphNodeKind::Module,
                other => {
                    return Err(ExtensionLoadError::new(format!(
                        "extension `{}` indexing namespace declaration_kind `{}` is invalid; expected `class` or `module`",
                        manifest.id, other
                    )))
                }
            };
            Ok(ExtensionNamespaceTarget {
                owner,
                declaration_kind,
            })
        })
        .collect()
}

fn parse_manifest_method_target(
    extension_id: &str,
    target: &ExtensionMethodTargetManifest,
) -> Result<ExtensionMethodTarget, ExtensionLoadError> {
    let owner = target
        .owner
        .iter()
        .map(|part| {
            RubyConstant::new(part).map_err(|err| {
                ExtensionLoadError::new(format!(
                    "extension `{}` indexing target owner part `{}` is invalid: {}",
                    extension_id, part, err
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let owner_kind = match target.owner_kind.as_str() {
        "instance" => NamespaceKind::Instance,
        "singleton" => NamespaceKind::Singleton,
        other => {
            return Err(ExtensionLoadError::new(format!(
                "extension `{}` indexing target owner_kind `{}` is invalid; expected `instance` or `singleton`",
                extension_id, other
            )))
        }
    };
    let method = RubyMethod::new(&target.method).map_err(|err| {
        ExtensionLoadError::new(format!(
            "extension `{}` indexing target method `{}` is invalid: {}",
            extension_id, target.method, err
        ))
    })?;
    Ok(ExtensionMethodTarget {
        owner,
        owner_kind,
        method,
        frame: target.frame,
    })
}

pub(super) fn wasm_file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_else(|| {
            unreachable_invariant!(
                what = "Wasm extension path `{}` has no valid UTF-8 file stem",
                why = "direct wasm loads default extension IDs to file stems",
                fix = "rename the wasm file or load it through an extension.toml manifest",
                path.display(),
            )
        })
        .to_string()
}
