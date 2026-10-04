//! Gemfile ownership, standalone projects, and runtime-backed gem discovery.

use super::*;

#[test]
fn gem_discovery_requires_the_project_roots_own_gemfile() {
    let workspace = TempDir::new().unwrap();
    std::fs::create_dir_all(workspace.path().join("service")).unwrap();
    std::fs::write(workspace.path().join("service/Gemfile"), "").unwrap();
    let indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));

    assert!(
        indexer.find_gemfile().is_err(),
        "a container folder must be expanded into projects before Bundler discovery"
    );
}

#[test]
fn explicitly_included_unlocked_gem_uses_active_ruby_highest_version() {
    let mut indexer = create_test_indexer();
    indexer.set_explicitly_included_gems(HashSet::from(["example".to_string()]));
    indexer.discovered_gems.insert(
        "example".to_string(),
        ["1.2.3", "2.0.0"]
            .into_iter()
            .map(|version| GemInfo {
                name: "example".to_string(),
                version: version.to_string(),
                platform: "ruby".to_string(),
                locked_version: version.to_string(),
                source: GemSource::GlobalInstalled,
                path: PathBuf::from(format!("/global/example-{version}")),
                lib_paths: vec![PathBuf::from(format!("/global/example-{version}/lib"))],
                dependencies: Vec::new(),
                is_default: false,
            })
            .collect(),
    );

    assert_eq!(indexer.get_gem("example").unwrap().version, "2.0.0");
}

#[test]
fn standalone_project_without_explicit_gems_skips_runtime_discovery() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.set_selected_runtime(
        workspace.path().join("missing-ruby"),
        RuntimeImplementation::Mri,
        None,
    );

    assert_eq!(indexer.discover_gems_blocking().unwrap(), 0);
    assert!(
        !indexer.needs_unlocked_explicit_discovery(),
        "a standalone project without explicit included gems must not schedule global discovery"
    );
}

#[cfg(unix)]
#[test]
fn standalone_project_discovers_only_explicitly_included_global_gems() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = TempDir::new().unwrap();
    let invocation_log = workspace.path().join("invocations");
    let fake_ruby = workspace.path().join("ruby");
    std::fs::write(
        &fake_ruby,
        format!(
            "#!/bin/sh\nprintf x >> '{}'\nprintf '%s' '[{{\"name\":\"example\",\"version\":\"1.2.3\",\"platform\":\"ruby\",\"gem_dir\":\"/gems/example-1.2.3\",\"lib_dirs\":[\"/gems/example-1.2.3/lib\"],\"dependencies\":[],\"default_gem\":false}},{{\"name\":\"unrequested\",\"version\":\"9.9.9\",\"platform\":\"ruby\",\"gem_dir\":\"/gems/unrequested-9.9.9\",\"lib_dirs\":[\"/gems/unrequested-9.9.9/lib\"],\"dependencies\":[],\"default_gem\":false}}]'\n",
            invocation_log.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_ruby).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_ruby, permissions).unwrap();

    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.set_selected_runtime(fake_ruby, RuntimeImplementation::Mri, None);
    indexer.set_explicitly_included_gems(HashSet::from(["example".to_string()]));

    assert!(
        indexer.needs_unlocked_explicit_discovery(),
        "the explicit standalone exception must request governed global discovery"
    );
    assert_eq!(indexer.discover_gems_blocking().unwrap(), 2);
    assert_eq!(std::fs::read_to_string(invocation_log).unwrap(), "x");
    assert_eq!(indexer.get_gem("example").unwrap().version, "1.2.3");
    assert!(
        indexer.get_gem("unrequested").is_none(),
        "global discovery must not make an unrequested gem selectable"
    );
}

#[cfg(unix)]
#[test]
fn auto_installed_gem_discovery_uses_one_runtime_process() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile"),
        "source 'https://example.test'\n",
    )
    .unwrap();
    let invocation_log = workspace.path().join("invocations");
    let fake_ruby = workspace.path().join("ruby");
    std::fs::write(
        &fake_ruby,
        format!(
            "#!/bin/sh\nprintf x >> '{}'\nprintf '%s' 'RUBY_FAST_LSP_GEM_DISCOVERY={{\"source\":\"global\",\"gems\":[{{\"name\":\"example\",\"version\":\"1.2.3\",\"platform\":\"ruby\",\"gem_dir\":\"/gems/example-1.2.3\",\"lib_dirs\":[\"/gems/example-1.2.3/lib\"],\"dependencies\":[],\"default_gem\":false}}]}}'\n",
            invocation_log.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_ruby).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_ruby, permissions).unwrap();

    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.set_selected_runtime(fake_ruby, RuntimeImplementation::Jruby, None);
    indexer.discover_auto_gems().unwrap();

    assert_eq!(std::fs::read_to_string(invocation_log).unwrap(), "x");
    let gem = &indexer.discovered_gems["example"][0];
    assert_eq!(gem.source, GemSource::GlobalInstalled);
    assert_eq!(gem.version, "1.2.3");
}

#[cfg(unix)]
fn write_discovery_runtime(path: &Path, invocation_log: &Path, source: &str, gem_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf x >> '{log}'\nprintf '%s' 'RUBY_FAST_LSP_GEM_DISCOVERY={{\"source\":\"{source}\",\"gems\":[{{\"name\":\"example\",\"version\":\"1.2.3\",\"platform\":\"ruby\",\"gem_dir\":\"{dir}\",\"lib_dirs\":[\"{dir}/lib\"],\"dependencies\":[],\"default_gem\":false}}]}}'\n",
            log = invocation_log.display(),
            dir = gem_dir.display(),
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn bundler_discovery_is_reused_until_a_selecting_input_changes() {
    let workspace = TempDir::new().unwrap();
    let project = workspace.path().join("project");
    let gem_dir = workspace.path().join("installed/example-1.2.3");
    std::fs::create_dir_all(gem_dir.join("lib")).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(gem_dir.join("example.gemspec"), "# first\n").unwrap();
    std::fs::write(project.join("Gemfile"), "gem 'example'\n").unwrap();
    std::fs::write(
        project.join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    let invocation_log = workspace.path().join("invocations");
    let fake_ruby = workspace.path().join("ruby");
    write_discovery_runtime(&fake_ruby, &invocation_log, "bundler", &gem_dir);
    let cache_root = workspace.path().join("cache");
    let discover = || {
        let mut indexer = IndexerGem::new(Some(project.clone()));
        indexer.set_selected_runtime(fake_ruby.clone(), RuntimeImplementation::Mri, None);
        indexer.set_discovery_cache_root(cache_root.clone());
        indexer.discover_auto_gems().unwrap();
        let gem = indexer.discovered_gems["example"][0].clone();
        assert_eq!(gem.source, GemSource::BundlerInstalled);
        assert_eq!(gem.version, "1.2.3");
        assert_eq!(gem.path, gem_dir);
        assert_eq!(gem.lib_paths, vec![gem_dir.join("lib")]);
        std::fs::read_to_string(&invocation_log).unwrap().len()
    };

    assert_eq!(discover(), 1);
    assert_eq!(
        discover(),
        1,
        "unchanged inputs must reuse the saved discovery"
    );

    std::fs::write(
        project.join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.4)\n",
    )
    .unwrap();
    assert_eq!(
        discover(),
        2,
        "a lockfile change must run the runtime again"
    );
    assert_eq!(discover(), 2);

    std::fs::write(gem_dir.join("example.gemspec"), "# second gemspec\n").unwrap();
    assert_eq!(
        discover(),
        3,
        "a changed source gemspec must run the runtime again"
    );

    std::fs::write(project.join("Gemfile"), "gem 'example', '1.2.3'\n").unwrap();
    assert_eq!(discover(), 4, "a Gemfile change must run the runtime again");

    std::fs::write(project.join("example.gemspec"), "# project gemspec\n").unwrap();
    assert_eq!(
        discover(),
        5,
        "a project gemspec change must run the runtime again"
    );
    assert_eq!(discover(), 5);

    std::fs::rename(&gem_dir, workspace.path().join("moved")).unwrap();
    std::fs::create_dir_all(gem_dir.join("lib")).unwrap();
    std::fs::write(gem_dir.join("example.gemspec"), "# second gemspec\n").unwrap();
    assert_eq!(
        discover(),
        6,
        "a replaced gem folder must run the runtime again"
    );

    let mut uncached = IndexerGem::new(Some(project.clone()));
    uncached.set_selected_runtime(fake_ruby.clone(), RuntimeImplementation::Mri, None);
    uncached.discover_auto_gems().unwrap();
    assert_eq!(
        std::fs::read_to_string(&invocation_log).unwrap().len(),
        7,
        "reuse is disabled without a cache root"
    );
}

#[cfg(unix)]
#[test]
fn global_fallback_discovery_is_never_reused() {
    let workspace = TempDir::new().unwrap();
    let gem_dir = workspace.path().join("installed/example-1.2.3");
    std::fs::create_dir_all(gem_dir.join("lib")).unwrap();
    std::fs::write(workspace.path().join("Gemfile"), "gem 'example'\n").unwrap();
    std::fs::write(workspace.path().join("Gemfile.lock"), "GEM\n  specs:\n").unwrap();
    let invocation_log = workspace.path().join("invocations");
    let fake_ruby = workspace.path().join("ruby");
    write_discovery_runtime(&fake_ruby, &invocation_log, "global", &gem_dir);

    for expected in 1..=2 {
        let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
        indexer.set_selected_runtime(fake_ruby.clone(), RuntimeImplementation::Mri, None);
        indexer.set_discovery_cache_root(workspace.path().join("cache"));
        indexer.discover_auto_gems().unwrap();
        assert_eq!(
            indexer.discovered_gems["example"][0].source,
            GemSource::GlobalInstalled
        );
        assert_eq!(
            std::fs::read_to_string(&invocation_log).unwrap().len(),
            expected,
            "an installation-wide fallback has no locked identity to reuse"
        );
    }
}
