//! Lockfiles that lock one gem for several native platforms.

use super::*;

const NATIVE_ONLY_LOCKFILE: &str = concat!(
    "GEM\n",
    "  remote: https://rubygems.org/\n",
    "  specs:\n",
    "    native (1.2.3-aarch64-linux-gnu)\n",
    "    native (1.2.3-arm64-darwin)\n",
    "    native (1.2.3-x86_64-darwin)\n",
    "    native (1.2.3-x86_64-linux-musl)\n",
    "    plain (2.0.0)\n",
    "\n",
    "PLATFORMS\n",
    "  aarch64-linux-gnu\n",
    "  arm64-darwin\n",
    "  x86_64-darwin\n",
    "  x86_64-linux-musl\n",
);

fn installed(name: &str, version: &str, platform: &str, source: GemSource) -> GemInfo {
    let locked_version = if platform == "ruby" {
        version.to_string()
    } else {
        format!("{version}-{platform}")
    };
    let path = crate::test::harness::fixture_path(&format!("/tmp/{name}-{locked_version}"));
    GemInfo {
        name: name.to_string(),
        version: version.to_string(),
        platform: platform.to_string(),
        locked_version,
        source,
        lib_paths: vec![path.join("lib")],
        path,
        dependencies: Vec::new(),
        is_default: false,
    }
}

fn indexer_with_lockfile(workspace: &TempDir, lockfile: &str) -> IndexerGem {
    std::fs::write(workspace.path().join("Gemfile.lock"), lockfile).unwrap();
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer
        .load_locked_gems()
        .expect("Bundler multi-platform native variants are a valid lockfile");
    indexer
}

#[test]
fn native_only_variants_resolve_to_the_variant_bundler_installed() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(&workspace, NATIVE_ONLY_LOCKFILE);
    indexer.discovered_gems.insert(
        "native".to_string(),
        vec![installed(
            "native",
            "1.2.3",
            "arm64-darwin",
            GemSource::BundlerInstalled,
        )],
    );

    assert_eq!(
        indexer.get_gem("native").map(|gem| gem.platform.as_str()),
        Some("arm64-darwin"),
        "the variant the selected runtime's Bundler resolved is the locked identity"
    );
}

#[test]
fn ruby_and_native_variants_resolve_to_the_installed_variant() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(
        &workspace,
        "GEM\n  specs:\n    mixed (1.0.0)\n    mixed (1.0.0-arm64-darwin)\n\nPLATFORMS\n  arm64-darwin\n  ruby\n",
    );
    indexer.discovered_gems.insert(
        "mixed".to_string(),
        vec![installed(
            "mixed",
            "1.0.0",
            "ruby",
            GemSource::BundlerInstalled,
        )],
    );

    assert_eq!(
        indexer.get_gem("mixed").map(|gem| gem.platform.as_str()),
        Some("ruby")
    );
}

#[test]
fn a_variant_the_lockfile_does_not_name_is_never_selected() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(&workspace, NATIVE_ONLY_LOCKFILE);
    indexer.discovered_gems.insert(
        "native".to_string(),
        vec![
            installed("native", "1.2.3", "ruby", GemSource::GlobalInstalled),
            installed(
                "native",
                "1.2.4",
                "arm64-darwin",
                GemSource::GlobalInstalled,
            ),
        ],
    );

    assert!(indexer.get_gem("native").is_none());
}

#[test]
fn several_installed_locked_variants_stay_unselected() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(&workspace, NATIVE_ONLY_LOCKFILE);
    indexer.discovered_gems.insert(
        "native".to_string(),
        vec![
            installed(
                "native",
                "1.2.3",
                "arm64-darwin",
                GemSource::GlobalInstalled,
            ),
            installed(
                "native",
                "1.2.3",
                "x86_64-darwin",
                GemSource::GlobalInstalled,
            ),
        ],
    );

    assert!(
        indexer.get_gem("native").is_none(),
        "two installed locked variants give no evidence of the runtime's platform"
    );
}

#[test]
fn variants_locked_at_different_versions_are_rejected() {
    let workspace = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    native (1.2.3-arm64-darwin)\n    native (1.2.4-x86_64-darwin)\n",
    )
    .unwrap();
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));

    let error = indexer.load_locked_gems().unwrap_err().to_string();
    assert!(
        error.contains("conflicting versions"),
        "unexpected error: {error}"
    );
}

#[test]
fn the_same_locked_identity_installed_twice_still_resolves() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(&workspace, NATIVE_ONLY_LOCKFILE);
    for name in ["native", "plain"] {
        let (version, platform) = if name == "native" {
            ("1.2.3", "arm64-darwin")
        } else {
            ("2.0.0", "ruby")
        };
        indexer.discovered_gems.insert(
            name.to_string(),
            vec![
                installed(name, version, platform, GemSource::GlobalInstalled),
                installed(name, version, platform, GemSource::GlobalInstalled),
            ],
        );
        assert_eq!(
            indexer.get_gem(name).map(|gem| gem.platform.as_str()),
            Some(platform)
        );
    }
}

#[test]
fn vendor_archive_is_read_only_when_one_locked_variant_is_vendored() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(workspace.path().join("Gemfile.lock"), NATIVE_ONLY_LOCKFILE).unwrap();
    let cache = workspace.path().join("vendor/cache");
    exact_sources::create_cached_gem(
        &cache.join("native-1.2.3-arm64-darwin.gem"),
        "native",
        "1.2.3",
        "arm64-darwin",
    );
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.set_cached_gem_root_for_test(extraction_cache.path().to_path_buf());

    indexer.discover_cached_gem_archives().unwrap();
    assert_eq!(
        indexer.discovered_gems["native"][0].locked_version,
        "1.2.3-arm64-darwin"
    );

    exact_sources::create_cached_gem(
        &cache.join("native-1.2.3-x86_64-darwin.gem"),
        "native",
        "1.2.3",
        "x86_64-darwin",
    );
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.set_cached_gem_root_for_test(extraction_cache.path().to_path_buf());

    indexer.discover_cached_gem_archives().unwrap();
    assert!(
        !indexer.discovered_gems.contains_key("native"),
        "several vendored variants give no evidence of the runtime's platform"
    );
}

#[test]
fn java_gem_roots_ignore_native_variants_of_other_gems() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let rvm = fixture.path().join(".rvm");
    let executable = rvm.join("rubies/jruby-9.2.21.0/bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        NATIVE_ONLY_LOCKFILE.replace(
            "    plain (2.0.0)\n",
            "    plain (2.0.0)\n    plain (2.0.0-java)\n",
        ),
    )
    .unwrap();
    let exact = rvm.join("gems/jruby-9.2.21.0/gems/plain-2.0.0-java");
    fs::create_dir_all(&exact).unwrap();

    assert_eq!(
        discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
        vec![dunce::canonicalize(exact).unwrap()]
    );
}

#[test]
fn a_locked_gem_without_library_folders_still_requires_its_dependencies() {
    let workspace = TempDir::new().unwrap();
    let mut indexer = indexer_with_lockfile(
        &workspace,
        "GEM\n  specs:\n    part (1.0.0)\n    umbrella (1.0.0)\n      part (= 1.0.0)\n",
    );
    let mut umbrella = installed("umbrella", "1.0.0", "ruby", GemSource::BundlerInstalled);
    umbrella.lib_paths.clear();
    umbrella.dependencies = vec!["part".to_string()];
    indexer
        .discovered_gems
        .insert("umbrella".to_string(), vec![umbrella]);
    indexer.discovered_gems.insert(
        "part".to_string(),
        vec![installed(
            "part",
            "1.0.0",
            "ruby",
            GemSource::BundlerInstalled,
        )],
    );
    indexer.set_required_gems(HashSet::from(["umbrella".to_string()]));

    assert_eq!(
        indexer.required_gems_with_dependencies(),
        vec!["umbrella", "part"],
        "a gem that only groups other gems has no code of its own to index"
    );
    assert!(
        indexer.get_gem("umbrella").is_none(),
        "a gem without library folders stays unavailable for indexing"
    );
}
