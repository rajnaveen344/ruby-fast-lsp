//! Gem folders of a runtime installed by RVM.
//!
//! RVM keeps a runtime at `<rvm>/rubies/<name>` and its gems at
//! `<rvm>/gems/<name>[@<gemset>]`, outside the runtime's own prefix. Only
//! `GEM_HOME` and `GEM_PATH` connect the two, and the server does not trust
//! the inherited shell for them. They are derived here from the project's
//! selected executable and its own gemset file, and from nothing else.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The gem environment RVM gives a runtime.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct RvmGemEnvironment {
    pub(super) gem_home: PathBuf,
    pub(super) gem_path: OsString,
}

/// `<rvm>/gems` and the runtime name, when `executable` is the canonical
/// path `<rvm>/rubies/<name>/bin/<executable>`.
pub(super) fn rvm_gem_repository(executable: &Path) -> Option<(PathBuf, OsString)> {
    let runtime_home = executable.parent()?.parent()?;
    let rubies = runtime_home.parent()?;
    if rubies.file_name()? != "rubies" {
        return None;
    }
    Some((
        rubies.parent()?.join("gems"),
        runtime_home.file_name()?.to_os_string(),
    ))
}

/// The gem folders RVM selects for `executable` in `project_root`: the
/// project's gemset, or the runtime's default one, followed by the runtime's
/// `@global` gemset. `None` leaves the runtime on its own gem folders: the
/// runtime is not an RVM one, or the selected gemset is not installed. A
/// project gemset that is missing never falls back to the default gemset.
pub(super) fn rvm_gem_environment(
    executable: &Path,
    project_root: Option<&Path>,
) -> Option<RvmGemEnvironment> {
    let executable = dunce::canonicalize(executable).ok()?;
    let (repository, runtime) = rvm_gem_repository(&executable)?;
    let gemset = |name: &str| {
        let mut folder = runtime.clone();
        folder.push("@");
        folder.push(name);
        repository.join(folder)
    };
    let gem_home = match project_root.map(project_gemset) {
        Some(ProjectGemset::Named(name)) => gemset(&name),
        Some(ProjectGemset::Unreadable) => return None,
        Some(ProjectGemset::Default) | None => repository.join(&runtime),
    };
    if !gem_home.is_dir() {
        return None;
    }
    let global = gemset("global");
    let mut folders = vec![gem_home.clone()];
    if global.is_dir() && global != gem_home {
        folders.push(global);
    }
    let gem_path = std::env::join_paths(folders).ok()?;
    Some(RvmGemEnvironment { gem_home, gem_path })
}

enum ProjectGemset {
    Default,
    Named(String),
    /// The project names a gemset that cannot be read as one folder name.
    Unreadable,
}

/// The gemset the project selects: its own `.ruby-gemset`, or else the
/// `@<gemset>` suffix of its `.ruby-version`.
fn project_gemset(project_root: &Path) -> ProjectGemset {
    let read = |file: &str| match std::fs::read(project_root.join(file)) {
        Ok(contents) => String::from_utf8(contents).map(Some).map_err(|_| ()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    };
    let contents = match (read(".ruby-gemset"), read(".ruby-version")) {
        (Err(()), _) | (Ok(None), Err(())) => return ProjectGemset::Unreadable,
        (Ok(Some(gemset)), _) => gemset,
        (Ok(None), Ok(version)) => version
            .as_deref()
            .and_then(|version| version.trim().split_once('@'))
            .map(|(_, gemset)| gemset.to_string())
            .unwrap_or_default(),
    };
    let name = contents.trim();
    if name.is_empty() || name == "default" {
        return ProjectGemset::Default;
    }
    if name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
        && name != "."
        && name != ".."
    {
        ProjectGemset::Named(name.to_string())
    } else {
        ProjectGemset::Unreadable
    }
}
