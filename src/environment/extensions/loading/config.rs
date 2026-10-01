use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use log::warn;
use walkdir::WalkDir;

use crate::environment::config::RubyFastLspConfig;

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::environment::extensions) struct ExtensionLoadConfig {
    pub(in crate::environment::extensions) package_paths: Vec<ConfiguredExtensionPath>,
    pub(in crate::environment::extensions) directory_paths: Vec<ConfiguredExtensionPath>,
    pub(in crate::environment::extensions) project_package_paths: Vec<ConfiguredExtensionPath>,
    pub(in crate::environment::extensions) settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment::extensions) enum ExtensionPathSource {
    Environment,
    ProjectLocal,
    InitializationOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment::extensions) struct ConfiguredExtensionPath {
    pub(in crate::environment::extensions) path: PathBuf,
    pub(in crate::environment::extensions) source: ExtensionPathSource,
}

#[derive(Debug)]
pub(in crate::environment::extensions) struct ExtensionLoadError {
    message: String,
}

impl ExtensionLoadConfig {
    pub(in crate::environment::extensions) fn from_config(config: &RubyFastLspConfig) -> Self {
        Self::from_config_and_workspace_roots(config, &[])
    }

    pub(in crate::environment::extensions) fn from_config_and_workspace_roots(
        config: &RubyFastLspConfig,
        workspace_roots: &[PathBuf],
    ) -> Self {
        let mut load_config = Self::from_environment();
        load_config
            .package_paths
            .extend(
                config
                    .extension_packages
                    .iter()
                    .map(|path| ConfiguredExtensionPath {
                        path: PathBuf::from(path),
                        source: ExtensionPathSource::InitializationOptions,
                    }),
            );
        load_config.settings = config.extension_settings.clone();
        load_config
            .directory_paths
            .extend(
                config
                    .extension_dirs
                    .iter()
                    .map(|path| ConfiguredExtensionPath {
                        path: PathBuf::from(path),
                        source: ExtensionPathSource::InitializationOptions,
                    }),
            );
        if config.workspace_trusted && config.project_extensions_enabled {
            let mut roots = workspace_roots.to_vec();
            roots.sort();
            roots.dedup();
            for root in roots {
                load_config.project_package_paths.extend(
                    discover_project_extension_packages(&root)
                        .into_iter()
                        .map(|path| ConfiguredExtensionPath {
                            path,
                            source: ExtensionPathSource::ProjectLocal,
                        }),
                );
            }
        }
        load_config
    }

    pub(in crate::environment::extensions) fn from_environment() -> Self {
        let mut config = Self::default();
        if let Some(paths) = std::env::var_os("RUBY_FAST_LSP_EXTENSION_PATHS") {
            for path in std::env::split_paths(&paths) {
                config.package_paths.push(ConfiguredExtensionPath {
                    path,
                    source: ExtensionPathSource::Environment,
                });
            }
        }
        if let Some(paths) = std::env::var_os("RUBY_FAST_LSP_EXTENSION_DIRS") {
            for path in std::env::split_paths(&paths) {
                config.directory_paths.push(ConfiguredExtensionPath {
                    path,
                    source: ExtensionPathSource::Environment,
                });
            }
        }
        config
    }
}

fn discover_project_extension_packages(workspace_root: &Path) -> Vec<PathBuf> {
    let mut packages = Vec::new();
    let hidden_root = workspace_root.join(".ruby-fast-lsp/extensions");
    if hidden_root.is_dir() {
        for entry in WalkDir::new(&hidden_root)
            .min_depth(2)
            .max_depth(2)
            .follow_links(false)
        {
            match entry {
                Ok(entry)
                    if entry.file_type().is_file() && entry.file_name() == "extension.toml" =>
                {
                    packages.push(
                        entry
                            .path()
                            .parent()
                            .expect("INVARIANT VIOLATED: extension.toml discovered without a parent directory. This is a bug because WalkDir entries below a workspace root must have a parent. Fix: preserve the package-directory discovery depth.")
                            .to_path_buf(),
                    );
                }
                Ok(_) => {}
                Err(err) => warn!(
                    "Skipping unreadable project extension entry under `{}`: {}",
                    hidden_root.display(),
                    err
                ),
            }
        }
    }

    let conventional_root = workspace_root.join("ruby_fast_lsp");
    if conventional_root.is_dir() {
        for entry in WalkDir::new(&conventional_root)
            .min_depth(1)
            .follow_links(false)
        {
            match entry {
                Ok(entry)
                    if entry.file_type().is_file() && entry.file_name() == "extension.toml" =>
                {
                    packages.push(
                        entry
                            .path()
                            .parent()
                            .expect("INVARIANT VIOLATED: extension.toml discovered without a parent directory. This is a bug because WalkDir entries below a workspace root must have a parent. Fix: preserve manifest-only project discovery.")
                            .to_path_buf(),
                    );
                }
                Ok(_) => {}
                Err(err) => warn!(
                    "Skipping unreadable project extension entry under `{}`: {}",
                    conventional_root.display(),
                    err
                ),
            }
        }
    }

    packages.sort();
    packages.dedup();
    packages
}

impl ExtensionLoadError {
    pub(in crate::environment::extensions) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ExtensionLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
