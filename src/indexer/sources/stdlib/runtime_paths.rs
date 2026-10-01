//! Exact selected-runtime standard library load-path discovery and identity.

use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RuntimeExecutableIdentity {
    byte_length: u64,
    modified: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeStdlibPathKey {
    executable: PathBuf,
    executable_identity: RuntimeExecutableIdentity,
    java_home: Option<PathBuf>,
}

impl RuntimeStdlibPathKey {
    pub(crate) fn new(executable: &Path, java_home: Option<&Path>) -> Result<Self> {
        let executable = std::fs::canonicalize(executable).with_context(|| {
            format!(
                "failed to canonicalize selected Ruby executable {} for stdlib discovery",
                executable.display()
            )
        })?;
        let executable_identity = runtime_executable_identity(&executable)?;
        let java_home = java_home
            .map(|path| {
                let canonical = std::fs::canonicalize(path).with_context(|| {
                    format!(
                        "failed to canonicalize selected Java home {} for stdlib discovery",
                        path.display()
                    )
                })?;
                if !canonical.is_dir() {
                    return Err(anyhow!(
                        "selected Java home is not a directory: {}",
                        canonical.display()
                    ));
                }
                Ok(canonical)
            })
            .transpose()?;
        Ok(Self {
            executable,
            executable_identity,
            java_home,
        })
    }

    pub(crate) fn discover(&self) -> Result<RuntimeStdlibPaths> {
        let before = runtime_executable_identity(&self.executable)?;
        if before != self.executable_identity {
            return Err(anyhow!(
                "selected Ruby executable changed before stdlib discovery: {}",
                self.executable.display()
            ));
        }

        let mut command = std::process::Command::new(&self.executable);
        command.args([
            "--disable-gems",
            "-e",
            "STDOUT.write($LOAD_PATH.map { |path| File.expand_path(path) + \"\\0\" }.join)",
        ]);
        for name in [
            "RUBYLIB",
            "RUBYOPT",
            "GEM_HOME",
            "GEM_PATH",
            "BUNDLE_GEMFILE",
            "BUNDLE_PATH",
            "BUNDLE_BIN_PATH",
            "BUNDLE_WITH",
            "BUNDLE_WITHOUT",
        ] {
            command.env_remove(name);
        }
        if let Some(java_home) = self.java_home.as_ref() {
            command.env("JAVA_HOME", java_home);
        }
        let output = command.output().with_context(|| {
            format!(
                "failed to query exact Ruby runtime load path from {}",
                self.executable.display()
            )
        })?;
        if !output.status.success() {
            return Err(anyhow!(
                "exact Ruby runtime {} failed while reporting its stdlib load path (status {}): {}",
                self.executable.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let after = runtime_executable_identity(&self.executable)?;
        if after != self.executable_identity {
            return Err(anyhow!(
                "selected Ruby executable changed during stdlib discovery: {}",
                self.executable.display()
            ));
        }

        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        for encoded_path in output.stdout.split(|byte| *byte == 0) {
            if encoded_path.is_empty() {
                continue;
            }
            let path_text = std::str::from_utf8(encoded_path).with_context(|| {
                format!(
                    "exact Ruby runtime {} returned a non-UTF-8 load path",
                    self.executable.display()
                )
            })?;
            let path = PathBuf::from(path_text);
            if !path.is_dir() {
                debug!(
                    "Ignoring missing exact-runtime load path from {}: {}",
                    self.executable.display(),
                    path.display()
                );
                continue;
            }
            let path = std::fs::canonicalize(&path).with_context(|| {
                format!(
                    "failed to canonicalize exact-runtime stdlib path {} from {}",
                    path.display(),
                    self.executable.display()
                )
            })?;
            if seen.insert(path.clone()) {
                debug!("Found exact-runtime stdlib path: {:?}", path);
                paths.push(path);
            }
        }

        info!(
            "Discovered {} stdlib paths from exact runtime {}",
            paths.len(),
            self.executable.display()
        );
        Ok(RuntimeStdlibPaths { paths })
    }
}

fn runtime_executable_identity(path: &Path) -> Result<RuntimeExecutableIdentity> {
    let metadata = std::fs::metadata(path).with_context(|| {
        format!(
            "failed to read selected Ruby executable metadata from {}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(anyhow!(
            "selected Ruby executable is not a regular file: {}",
            path.display()
        ));
    }
    Ok(RuntimeExecutableIdentity {
        byte_length: metadata.len(),
        modified: metadata.modified().with_context(|| {
            format!(
                "failed to read selected Ruby executable modification time from {}",
                path.display()
            )
        })?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeStdlibPaths {
    pub(super) paths: Vec<PathBuf>,
}

impl RuntimeStdlibPaths {
    pub(crate) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(crate) fn estimated_weight_bytes(&self) -> u64 {
        self.paths.iter().fold(256u64, |total, path| {
            total
                .checked_add(u64::try_from(path.as_os_str().len()).expect(
                    "INVARIANT VIOLATED: a runtime stdlib path length does not fit u64. This is a bug because an in-memory path cannot exceed the process address space. Fix: inspect runtime load-path product accounting.",
                ))
                .expect(
                    "INVARIANT VIOLATED: runtime stdlib path-product weight overflowed u64. This is a bug because retained entry count and path storage are bounded. Fix: inspect runtime load-path product accounting.",
                )
        })
    }
}
