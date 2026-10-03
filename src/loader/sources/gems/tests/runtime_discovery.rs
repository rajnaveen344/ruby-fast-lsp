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
