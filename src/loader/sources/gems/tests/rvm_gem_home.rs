//! Gem folders of a runtime installed by RVM, derived from the selected
//! executable instead of the inherited shell environment.

#![cfg(unix)]

use super::*;

const RUNTIME: &str = "ruby-9.9.9";

struct Fixture {
    workspace: TempDir,
    project: PathBuf,
    gem_dir: PathBuf,
    invocations: PathBuf,
    seen_environment: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let workspace = TempDir::new().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        let project = root.join("project");
        let gem_dir = root.join("installed/example-1.2.3");
        fs::create_dir_all(gem_dir.join("lib")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(gem_dir.join("example.gemspec"), "# example\n").unwrap();
        fs::write(project.join("Gemfile"), "gem 'example'\n").unwrap();
        fs::write(
            project.join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
        Self {
            invocations: root.join("invocations"),
            seen_environment: root.join("seen-environment"),
            workspace,
            project,
            gem_dir,
        }
    }

    fn root(&self) -> PathBuf {
        dunce::canonicalize(self.workspace.path()).unwrap()
    }

    fn rvm_gems(&self) -> PathBuf {
        self.root().join(".rvm/gems")
    }

    /// A runtime that records the gem environment it was started with.
    fn runtime_at(&self, executable: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(
            executable,
            format!(
                "#!/bin/sh\nprintf x >> '{log}'\nprintf '%s\\n%s' \"${{GEM_HOME-unset}}\" \"${{GEM_PATH-unset}}\" > '{seen}'\nprintf '%s' 'RUBY_FAST_LSP_GEM_DISCOVERY={{\"source\":\"bundler\",\"gems\":[{{\"name\":\"example\",\"version\":\"1.2.3\",\"platform\":\"ruby\",\"gem_dir\":\"{dir}\",\"lib_dirs\":[\"{dir}/lib\"],\"dependencies\":[],\"default_gem\":false}}],\"watched\":null}}'\n",
                log = self.invocations.display(),
                seen = self.seen_environment.display(),
                dir = self.gem_dir.display(),
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).unwrap();
    }

    fn rvm_runtime(&self) -> PathBuf {
        let executable = self
            .root()
            .join(".rvm/rubies")
            .join(RUNTIME)
            .join("bin/ruby");
        self.runtime_at(&executable);
        executable
    }

    /// Run discovery and return the `GEM_HOME` and `GEM_PATH` the runtime saw.
    fn discover(&self, executable: &Path, cache_root: Option<&Path>) -> (String, String) {
        let mut indexer = IndexerGem::new(Some(self.project.clone()));
        indexer.set_selected_runtime(executable.to_path_buf(), RuntimeImplementation::Mri, None);
        if let Some(cache_root) = cache_root {
            indexer.set_discovery_cache_root(cache_root.to_path_buf());
        }
        indexer.discover_auto_gems().unwrap();
        assert_eq!(indexer.discovered_gems["example"][0].version, "1.2.3");
        let seen = fs::read_to_string(&self.seen_environment).unwrap();
        let (home, path) = seen.split_once('\n').unwrap();
        (home.to_string(), path.to_string())
    }

    fn invocation_count(&self) -> usize {
        fs::read_to_string(&self.invocations).unwrap().len()
    }
}

fn joined(paths: &[PathBuf]) -> String {
    std::env::join_paths(paths)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn rvm_runtime_discovers_gems_in_its_own_gem_folders() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();
    let home = fixture.rvm_gems().join(RUNTIME);
    let global = fixture.rvm_gems().join(format!("{RUNTIME}@global"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&global).unwrap();

    assert_eq!(
        fixture.discover(&executable, None),
        (home.display().to_string(), joined(&[home, global])),
        "the selected RVM runtime installs its gems outside its own prefix"
    );
}

#[test]
fn rvm_runtime_without_a_global_gemset_uses_only_its_gem_home() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();
    let home = fixture.rvm_gems().join(RUNTIME);
    fs::create_dir_all(&home).unwrap();

    assert_eq!(
        fixture.discover(&executable, None),
        (home.display().to_string(), joined(&[home]))
    );
}

#[test]
fn rvm_runtime_selected_through_an_alias_uses_the_real_runtime_name() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();
    let rubies = fixture.root().join(".rvm/rubies");
    std::os::unix::fs::symlink(rubies.join(RUNTIME), rubies.join("default")).unwrap();
    let home = fixture.rvm_gems().join(RUNTIME);
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(fixture.rvm_gems().join("default")).unwrap();
    drop(executable);

    assert_eq!(
        fixture.discover(&rubies.join("default/bin/ruby"), None).0,
        home.display().to_string()
    );
}

#[test]
fn project_gemset_selects_that_gemset_and_never_the_default_one() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();
    let default_home = fixture.rvm_gems().join(RUNTIME);
    let global = fixture.rvm_gems().join(format!("{RUNTIME}@global"));
    fs::create_dir_all(&default_home).unwrap();
    fs::create_dir_all(&global).unwrap();
    fs::write(fixture.project.join(".ruby-gemset"), "service\n").unwrap();

    assert_eq!(
        fixture.discover(&executable, None),
        ("unset".to_string(), "unset".to_string()),
        "a missing project gemset must not fall back to the default gemset"
    );

    let gemset = fixture.rvm_gems().join(format!("{RUNTIME}@service"));
    fs::create_dir_all(&gemset).unwrap();
    let selected = (
        gemset.display().to_string(),
        joined(&[gemset, global.clone()]),
    );
    assert_eq!(fixture.discover(&executable, None), selected);

    // The same gemset named by the version file's `@` suffix.
    fs::remove_file(fixture.project.join(".ruby-gemset")).unwrap();
    fs::write(
        fixture.project.join(".ruby-version"),
        format!("{RUNTIME}@service\n"),
    )
    .unwrap();
    assert_eq!(fixture.discover(&executable, None), selected);

    fs::write(
        fixture.project.join(".ruby-version"),
        format!("{RUNTIME}@absent\n"),
    )
    .unwrap();
    assert_eq!(
        fixture.discover(&executable, None),
        ("unset".to_string(), "unset".to_string())
    );
}

#[test]
fn runtime_outside_rvm_keeps_its_own_gem_folders() {
    let fixture = Fixture::new();
    let executable = fixture.root().join("opt/ruby/bin/ruby");
    fixture.runtime_at(&executable);
    fs::create_dir_all(fixture.root().join("gems/ruby")).unwrap();

    assert_eq!(
        fixture.discover(&executable, None),
        ("unset".to_string(), "unset".to_string())
    );
}

#[test]
fn rvm_runtime_without_a_gem_folder_keeps_its_own_gem_folders() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();

    assert_eq!(
        fixture.discover(&executable, None),
        ("unset".to_string(), "unset".to_string())
    );
}

#[test]
fn saved_discovery_is_not_reused_once_the_rvm_gem_folders_change() {
    let fixture = Fixture::new();
    let executable = fixture.rvm_runtime();
    let cache_root = fixture.root().join("cache");

    fixture.discover(&executable, Some(&cache_root));
    fixture.discover(&executable, Some(&cache_root));
    assert_eq!(fixture.invocation_count(), 1, "unchanged inputs are reused");

    let home = fixture.rvm_gems().join(RUNTIME);
    fs::create_dir_all(&home).unwrap();
    assert_eq!(
        fixture.discover(&executable, Some(&cache_root)).0,
        home.display().to_string()
    );
    assert_eq!(fixture.invocation_count(), 2);

    fs::create_dir_all(fixture.rvm_gems().join(format!("{RUNTIME}@global"))).unwrap();
    fixture.discover(&executable, Some(&cache_root));
    assert_eq!(fixture.invocation_count(), 3);
}
