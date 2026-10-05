//! Persisted gem discovery results, keyed by every input that selects them.
//!
//! Starting the selected runtime only to ask Bundler for the locked specs is
//! the slowest part of dependency discovery. The answer is a function of the
//! runtime, the Gemfile and lockfile, Bundler configuration, the environment
//! the runtime reads, and the gemspecs Bundler evaluates from source. When
//! Bundler cannot load the bundle, the runtime falls back to every installed
//! gem; that answer also depends on the installed specification folders and
//! RubyGems configuration, which the runtime reports and which are checked
//! again before reuse. A saved answer is reused only while all of those still
//! match; anything unreadable is a miss.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const SCHEMA: &str = "rflsp-gem-discovery-v2";

/// Project files Bundler or a runtime version manager reads at startup.
const PROJECT_INPUTS: &[&str] = &[
    "Gemfile.lock",
    ".bundle/config",
    ".ruby-version",
    ".tool-versions",
];

/// Environment read by Bundler, RubyGems, the runtime, or a version manager.
/// The discovery command removes `GEM_HOME`, `GEM_PATH`, and `RUBY_VERSION`.
const ENVIRONMENT_PREFIXES: &[&str] = &[
    "BUNDLE_", "BUNDLER_", "GEM_", "RUBY", "JRUBY_", "JAVA_", "RBENV_", "ASDF_", "MISE_",
];
const ENVIRONMENT_NAMES: &[&str] = &["HOME", "PATH"];
const REMOVED_ENVIRONMENT: &[&str] = &["GEM_HOME", "GEM_PATH", "RUBY_VERSION"];

/// Which branch of the discovery command produced a gem array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum DiscoverySource {
    /// Bundler loaded the locked bundle.
    Bundler,
    /// Bundler could not load the bundle; every installed gem was listed.
    Global,
}

pub(super) struct GemDiscoveryCache {
    file: PathBuf,
    key: String,
    inputs: Inputs,
}

struct Inputs {
    project_root: PathBuf,
    gemfile: PathBuf,
    ruby_executable: PathBuf,
    java_home: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    key: String,
    source: DiscoverySource,
    /// Fingerprints of the top-level gemspecs in every discovered gem folder,
    /// which Bundler evaluates for path and git sources.
    gem_dirs: Vec<(PathBuf, String)>,
    /// Fingerprints of the specification folders and configuration files the
    /// runtime reported reading when it fell back to installed gems.
    watched: Vec<(PathBuf, String)>,
    /// The gem array exactly as the runtime printed it.
    gems: String,
}

impl GemDiscoveryCache {
    pub(super) fn open(
        cache_root: &Path,
        project_root: &Path,
        gemfile: &Path,
        ruby_executable: &Path,
        java_home: Option<&Path>,
    ) -> Result<Self> {
        let project_root = dunce::canonicalize(project_root).with_context(|| {
            format!(
                "failed to canonicalize {} for gem discovery reuse",
                project_root.display()
            )
        })?;
        let file = cache_root.join("gem-discovery").join(format!(
            "{:x}.json",
            Sha256::digest(project_root.to_string_lossy().as_bytes())
        ));
        let inputs = Inputs {
            project_root,
            gemfile: gemfile.to_path_buf(),
            ruby_executable: ruby_executable.to_path_buf(),
            java_home: java_home.map(Path::to_path_buf),
        };
        let key = inputs.key()?;
        Ok(Self { file, key, inputs })
    }

    /// The saved gem array and the branch that produced it, if every input
    /// still matches.
    pub(super) fn load(&self) -> Option<(DiscoverySource, Vec<u8>)> {
        let bytes = std::fs::read(&self.file).ok()?;
        let saved = serde_json::from_slice::<Saved>(&bytes).ok()?;
        if saved.key != self.key {
            return None;
        }
        for (gem_dir, fingerprint) in &saved.gem_dirs {
            if gemspec_fingerprint(gem_dir).ok()?.as_deref() != Some(fingerprint.as_str()) {
                return None;
            }
        }
        for (path, fingerprint) in &saved.watched {
            if watched_fingerprint(path).ok()? != *fingerprint {
                return None;
            }
        }
        Some((saved.source, saved.gems.into_bytes()))
    }

    /// Save a gem array produced while the inputs matched this key. The key is
    /// computed again so an input edited while the runtime ran is never saved
    /// under the older identity. An installed-gem fallback is saved only with
    /// the folders and files the runtime reported reading.
    pub(super) fn store(
        &self,
        source: DiscoverySource,
        gems: &[u8],
        watched: Option<&[PathBuf]>,
    ) -> Result<()> {
        if self.inputs.key()? != self.key {
            return Ok(());
        }
        let mut gem_dirs = Vec::new();
        let mut watched_fingerprints = Vec::new();
        match source {
            DiscoverySource::Bundler => {
                // Without a lockfile Bundler resolves against whatever is
                // installed, which the gem folders alone do not identify.
                if !self.inputs.project_root.join("Gemfile.lock").is_file() {
                    return Ok(());
                }
                #[derive(Deserialize)]
                struct GemDir {
                    gem_dir: PathBuf,
                }
                for gem in serde_json::from_slice::<Vec<GemDir>>(gems)
                    .context("Bundler discovery result has no gem folders")?
                {
                    // A missing folder means the discovery cannot be reused.
                    let Some(fingerprint) = gemspec_fingerprint(&gem.gem_dir)? else {
                        return Ok(());
                    };
                    gem_dirs.push((gem.gem_dir, fingerprint));
                }
            }
            DiscoverySource::Global => {
                let Some(watched) = watched else {
                    return Ok(());
                };
                for path in watched {
                    watched_fingerprints.push((path.clone(), watched_fingerprint(path)?));
                }
            }
        }
        let saved = Saved {
            key: self.key.clone(),
            source,
            gem_dirs,
            watched: watched_fingerprints,
            gems: String::from_utf8(gems.to_vec()).context("gem discovery result is not UTF-8")?,
        };
        let parent = self
            .file
            .parent()
            .ok_or_else(|| anyhow!("gem discovery file has no parent folder"))?;
        std::fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(
            ".{}.{}.tmp",
            self.file
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default(),
            std::process::id()
        ));
        std::fs::write(&temporary, serde_json::to_vec(&saved)?)?;
        std::fs::rename(&temporary, &self.file)?;
        Ok(())
    }
}

impl Inputs {
    fn key(&self) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut field = |label: &str, value: &[u8]| {
            hasher.update(label.as_bytes());
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(value);
        };
        field("schema", SCHEMA.as_bytes());
        field("root", self.project_root.to_string_lossy().as_bytes());
        field("gemfile", &read_optional(&self.gemfile)?);
        for input in PROJECT_INPUTS {
            field(input, &read_optional(&self.project_root.join(input))?);
        }
        // `gemspec` in a Gemfile evaluates the project's own gemspecs.
        for (path, contents) in root_gemspecs(&self.project_root)? {
            field("gemspec", path.to_string_lossy().as_bytes());
            field("gemspec contents", &contents);
        }
        field("runtime", self.ruby_executable.to_string_lossy().as_bytes());
        field(
            "runtime file",
            file_identity(&self.ruby_executable).as_bytes(),
        );
        if let Ok(resolved) = dunce::canonicalize(&self.ruby_executable) {
            field("runtime target", file_identity(&resolved).as_bytes());
        }
        field(
            "java home",
            self.java_home
                .as_deref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_bytes(),
        );

        let mut environment = std::env::vars_os()
            .filter(|(name, _)| {
                let name = name.to_string_lossy();
                !REMOVED_ENVIRONMENT.contains(&name.as_ref())
                    && (ENVIRONMENT_NAMES.contains(&name.as_ref())
                        || ENVIRONMENT_PREFIXES
                            .iter()
                            .any(|prefix| name.starts_with(prefix)))
            })
            .collect::<Vec<_>>();
        environment.sort();
        for (name, value) in &environment {
            field("env", name.to_string_lossy().as_bytes());
            field("env value", value.to_string_lossy().as_bytes());
        }
        for config in user_bundler_configs(&environment) {
            field("bundler config", &read_optional(&config)?);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

/// Bundler's application and user configuration files, as selected by the
/// same variables Bundler reads.
fn user_bundler_configs(environment: &[(std::ffi::OsString, std::ffi::OsString)]) -> Vec<PathBuf> {
    let variable = |name: &str| {
        environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| PathBuf::from(value))
    };
    let mut configs = Vec::new();
    if let Some(app_config) = variable("BUNDLE_APP_CONFIG") {
        configs.push(app_config.join("config"));
    }
    if let Some(user_config) = variable("BUNDLE_USER_CONFIG") {
        configs.push(user_config);
    } else if let Some(user_home) = variable("BUNDLE_USER_HOME") {
        configs.push(user_home.join("config"));
    } else if let Some(home) = variable("HOME") {
        configs.push(home.join(".bundle/config"));
    }
    configs
}

fn read_optional(path: &Path) -> Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(b"<missing>".to_vec()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read gem discovery input {}", path.display())),
    }
}

fn root_gemspecs(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut gemspecs = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "gemspec")
        {
            let contents = read_optional(&path)?;
            gemspecs.push((path, contents));
        }
    }
    gemspecs.sort();
    Ok(gemspecs)
}

/// Size, modification time, and inode of a file, or a marker when absent.
fn file_identity(path: &Path) -> String {
    let Ok(metadata) = std::fs::metadata(path) else {
        return "<missing>".to_string();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    };
    #[cfg(not(unix))]
    let inode: Option<(u64, u64)> = None;
    format!("{}:{modified:?}:{inode:?}", metadata.len())
}

/// Fingerprint of a reported specification folder (every entry's identity) or
/// configuration file, with a marker when it does not exist yet.
fn watched_fingerprint(path: &Path) -> Result<String> {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(file_identity(path));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to list gem discovery input {}", path.display()))
        }
    };
    let mut listing = Vec::new();
    for entry in entries {
        let path = entry?.path();
        listing.push(format!(
            "{}={}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            file_identity(&path)
        ));
    }
    listing.sort();
    Ok(format!("dir:{}", listing.join(";")))
}

/// Fingerprint of a gem folder's top-level gemspecs, or `None` when the
/// folder no longer exists.
fn gemspec_fingerprint(gem_dir: &Path) -> Result<Option<String>> {
    let entries = match std::fs::read_dir(gem_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut gemspecs = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "gemspec")
        {
            gemspecs.push(format!(
                "{}={}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                file_identity(&path)
            ));
        }
    }
    gemspecs.sort();
    Ok(Some(gemspecs.join(";")))
}
