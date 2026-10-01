use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

const PRUNED_PROJECT_DISCOVERY_DIRECTORIES: &[&str] = &[
    ".bundle",
    ".git",
    ".ruby-fast-lsp",
    ".ruby-lsp",
    "coverage",
    "log",
    "node_modules",
    "tmp",
    "vendor",
];

/// Discover independent Ruby project roots beneath an editor workspace folder.
///
/// A Gemfile at the workspace root owns the complete folder. Otherwise the
/// nearest nested Gemfiles become independent projects and discovery stops
/// below each one. A folder with no Gemfile remains a single project for
/// compatibility with gem and standalone Ruby workspaces.
pub fn discover_project_roots(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    if workspace_root.join("Gemfile").is_file() {
        return Ok(vec![workspace_root.to_path_buf()]);
    }

    let mut roots = Vec::new();
    discover_nested_project_roots(workspace_root, &mut roots)?;
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        roots.push(workspace_root.to_path_buf());
    }
    Ok(roots)
}

pub fn discover_project_roots_with_explicit(
    workspace_root: &Path,
    explicit_roots: &[String],
) -> Result<Vec<PathBuf>> {
    if explicit_roots.is_empty() {
        return discover_project_roots(workspace_root);
    }

    let canonical_workspace = fs::canonicalize(workspace_root).with_context(|| {
        format!(
            "Failed to canonicalize workspace folder {}",
            workspace_root.display()
        )
    })?;
    let mut roots = Vec::new();
    for configured in explicit_roots {
        let relative = Path::new(configured);
        anyhow::ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "indexing.projectRoots entry must be a non-empty workspace-relative path without traversal: {}",
            configured
        );
        let candidate = canonical_workspace.join(relative);
        let canonical_candidate = fs::canonicalize(&candidate).with_context(|| {
            format!(
                "indexing.projectRoots entry does not exist: {}",
                candidate.display()
            )
        })?;
        anyhow::ensure!(
            canonical_candidate.is_dir() && canonical_candidate.starts_with(&canonical_workspace),
            "indexing.projectRoots entry escapes the workspace or is not a directory: {}",
            configured
        );
        roots.push(canonical_candidate);
    }
    roots.sort();
    roots.dedup();
    for pair in roots.windows(2) {
        anyhow::ensure!(
            !pair[1].starts_with(&pair[0]),
            "indexing.projectRoots entries must not overlap: {} contains {}",
            pair[0].display(),
            pair[1].display()
        );
    }
    Ok(roots)
}

fn discover_nested_project_roots(directory: &Path, roots: &mut Vec<PathBuf>) -> Result<()> {
    let mut children = fs::read_dir(directory)
        .with_context(|| {
            format!(
                "Failed to inspect workspace directory {}",
                directory.display()
            )
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to read workspace directory {}", directory.display()))?;
    children.sort_by_key(|entry| entry.file_name());

    for child in children {
        let file_type = child.file_type().with_context(|| {
            format!(
                "Failed to inspect project candidate {}",
                child.path().display()
            )
        })?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let name = child.file_name();
        if name
            .to_str()
            .is_some_and(|name| PRUNED_PROJECT_DISCOVERY_DIRECTORIES.contains(&name))
        {
            continue;
        }

        let path = child.path();
        if path.join("Gemfile").is_file() {
            roots.push(path);
            continue;
        }
        discover_nested_project_roots(&path, roots)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{discover_project_roots, discover_project_roots_with_explicit};

    fn touch(path: &std::path::Path) {
        fs::create_dir_all(path.parent().expect(
            "INVARIANT VIOLATED: test file must have a parent directory. This is a bug because fixtures require a containing directory. Fix: provide a nested fixture path.",
        ))
        .unwrap();
        fs::write(path, "source 'https://rubygems.org'\n").unwrap();
    }

    #[test]
    fn root_gemfile_owns_the_workspace_and_nested_gemfiles() {
        let workspace = tempdir().unwrap();
        touch(&workspace.path().join("Gemfile"));
        touch(&workspace.path().join("examples/demo/Gemfile"));
        touch(&workspace.path().join("vendor/cache/git-gem/Gemfile"));

        assert_eq!(
            discover_project_roots(workspace.path()).unwrap(),
            [workspace.path()]
        );
    }

    #[test]
    fn container_workspace_discovers_nearest_gemfile_roots_deterministically() {
        let workspace = tempdir().unwrap();
        touch(&workspace.path().join("server/Gemfile"));
        touch(&workspace.path().join("server/examples/demo/Gemfile"));
        touch(&workspace.path().join("admin/Gemfile"));
        touch(&workspace.path().join("vendor/cache/pbkdf2/Gemfile"));
        touch(&workspace.path().join("node_modules/tool/Gemfile"));

        assert_eq!(
            discover_project_roots(workspace.path()).unwrap(),
            [
                workspace.path().join("admin"),
                workspace.path().join("server")
            ]
        );
    }

    #[test]
    fn workspace_without_any_gemfile_remains_a_single_project() {
        let workspace = tempdir().unwrap();
        touch(&workspace.path().join("lib/not_a_gemfile.rb"));

        assert_eq!(
            discover_project_roots(workspace.path()).unwrap(),
            [workspace.path()]
        );
    }

    #[test]
    fn explicit_project_roots_override_an_umbrella_gemfile() {
        let workspace = tempdir().unwrap();
        touch(&workspace.path().join("Gemfile"));
        fs::create_dir_all(workspace.path().join("services/billing")).unwrap();
        fs::create_dir_all(workspace.path().join("services/identity")).unwrap();

        assert_eq!(
            discover_project_roots_with_explicit(
                workspace.path(),
                &[
                    "services/identity".to_string(),
                    "services/billing".to_string()
                ],
            )
            .unwrap(),
            [
                fs::canonicalize(workspace.path().join("services/billing")).unwrap(),
                fs::canonicalize(workspace.path().join("services/identity")).unwrap(),
            ]
        );
    }

    #[test]
    fn explicit_project_roots_reject_escape_and_overlap() {
        let workspace = tempdir().unwrap();
        fs::create_dir_all(workspace.path().join("services/billing/internal")).unwrap();

        assert!(discover_project_roots_with_explicit(
            workspace.path(),
            &["../outside".to_string()],
        )
        .is_err());
        assert!(discover_project_roots_with_explicit(
            workspace.path(),
            &[
                "services/billing".to_string(),
                "services/billing/internal".to_string()
            ],
        )
        .is_err());
    }
}
