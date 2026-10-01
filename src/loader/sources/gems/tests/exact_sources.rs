//! Exact locked sources: vendor-cache archives, cached gem identities, and installed registry gems.

use super::*;

#[test]
fn locked_git_gem_uses_extracted_vendor_cache_without_executing_gemspec() {
    let workspace = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GIT\n  remote: git@github.com:emerose/pbkdf2-ruby.git\n  revision: b8c9fd171c32d4abcab52be629996448cf0bf63a\n  specs:\n    pbkdf2 (0.2.0)\n\nGEM\n",
    )
    .unwrap();
    let cache = workspace
        .path()
        .join("vendor/cache/pbkdf2-ruby-b8c9fd171c32");
    std::fs::create_dir_all(cache.join("lib")).unwrap();
    std::fs::write(cache.join("lib/pbkdf2.rb"), "class PBKDF2; end\n").unwrap();
    std::fs::write(
        cache.join("pbkdf2.gemspec"),
        "raise 'must never execute project gemspec'\n",
    )
    .unwrap();
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));

    indexer.discover_cached_git_gems().unwrap();

    let gem = &indexer.discovered_gems["pbkdf2"][0];
    assert_eq!(gem.version, "0.2.0");
    assert_eq!(gem.platform, "ruby");
    assert_eq!(gem.source, GemSource::VendorGit);
    assert_eq!(gem.path, cache);
    assert_eq!(gem.lib_paths, [cache.join("lib")]);
}

fn append_tar_file(builder: &mut Builder<Vec<u8>>, path: &str, content: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(u64::try_from(content.len()).unwrap());
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, Cursor::new(content))
        .unwrap();
}

fn gzip(content: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::copy(&mut Cursor::new(content), &mut encoder).unwrap();
    encoder.finish().unwrap()
}

fn create_cached_gem_with_require_path(
    path: &Path,
    name: &str,
    version: &str,
    platform: &str,
    require_path: &str,
) {
    let metadata = format!(
        "--- !ruby/object:Gem::Specification\n\
             name: {name}\n\
             version: !ruby/object:Gem::Version\n\
             \x20 version: {version}\n\
             platform: {platform}\n\
             require_paths:\n\
             - {require_path}\n"
    );
    let mut data = Builder::new(Vec::new());
    append_tar_file(
        &mut data,
        "lib/example.rb",
        b"module Example; class Cached; end; end\n",
    );
    let data = data.into_inner().unwrap();

    let mut package = Builder::new(Vec::new());
    append_tar_file(&mut package, "metadata.gz", &gzip(metadata.as_bytes()));
    append_tar_file(&mut package, "data.tar.gz", &gzip(&data));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, package.into_inner().unwrap()).unwrap();
}

fn create_cached_gem(path: &Path, name: &str, version: &str, platform: &str) {
    create_cached_gem_with_require_path(path, name, version, platform, "lib");
}

fn create_cached_gem_indexer(project_root: &Path, cache_root: &Path) -> IndexerGem {
    let mut indexer = IndexerGem::new(Some(project_root.to_path_buf()));
    indexer.set_cached_gem_root_for_test(cache_root.to_path_buf());
    indexer
}

fn extraction_path_contains_sha256_directory(path: &Path) -> bool {
    path.components().any(|component| {
        component.as_os_str().to_str().is_some_and(|name| {
            name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    })
}

#[test]
fn locked_registry_gem_uses_project_local_vendor_cache_archive() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    example (1.2.3-java)\n\nPLATFORMS\n  java\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
        "example",
        "1.2.3",
        "java",
    );
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

    indexer.discover_cached_gem_archives().unwrap();

    let gem = &indexer.discovered_gems["example"][0];
    assert_eq!(gem.version, "1.2.3");
    assert_eq!(gem.platform, "java");
    assert_eq!(gem.locked_version, "1.2.3-java");
    assert_eq!(gem.source, GemSource::VendorArchive);
    assert!(
        gem.path.starts_with(extraction_cache.path().join("gems")),
        "cached archive extraction must use the external cache"
    );
    assert!(
        !gem.path.starts_with(workspace.path()),
        "cached archive extraction must not mutate the Ruby project"
    );
    assert_eq!(gem.lib_paths, [gem.path.join("lib")]);
    assert_eq!(
        std::fs::read_to_string(gem.path.join("lib/example.rb")).unwrap(),
        "module Example; class Cached; end; end\n"
    );
    assert_eq!(
        gem.path.file_name().and_then(|name| name.to_str()),
        Some("example-1.2.3-java"),
        "editors display this path, so the gem identity must be the extracted root"
    );
    assert!(
        !extraction_path_contains_sha256_directory(&gem.path),
        "SHA-256 directory names hide the gem identity in editor breadcrumbs: {}",
        gem.path.display()
    );
    let project_leaf = workspace.path().file_name().unwrap().to_str().unwrap();
    let relative = gem
        .path
        .strip_prefix(extraction_cache.path().join("gems"))
        .unwrap();
    let project_component = relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .unwrap();
    assert!(
        project_component.starts_with(&format!("{project_leaf}-")),
        "project identity `{project_component}` should start with the project directory name"
    );
    assert_eq!(
        std::fs::read_to_string(gem.path.join(".complete"))
            .unwrap()
            .len(),
        64,
        "archive checksum belongs in the completion marker, not the displayed path"
    );
}

#[test]
fn cached_gem_project_identity_uses_directory_name_and_short_digest() {
    let identity = cached_gem_project_identity(Path::new("/workspace/server"));
    assert!(
        identity.starts_with("server-"),
        "project identity should lead with the directory name, got {identity}"
    );
    assert_eq!(
        identity.len(),
        "server-".len() + CACHED_GEM_PROJECT_DIGEST_PREFIX_CHARS
    );
    assert!(
        identity["server-".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()),
        "project identity suffix should be a digest prefix, got {identity}"
    );
    assert_ne!(
        identity,
        cached_gem_project_identity(Path::new("/other/server")),
        "distinct project paths must not share an extraction identity"
    );
    assert!(
        cached_gem_project_identity(Path::new("/workspace/weird name")).starts_with("project-"),
        "unsafe directory names must fall back to a stable project leaf"
    );
}

#[test]
fn cached_gem_project_identity_disambiguates_short_digest_collisions() {
    let cache = TempDir::new().unwrap();
    let project = Path::new("/workspace/server");
    let short_root = cache
        .path()
        .join("gems")
        .join(cached_gem_project_identity(project));
    std::fs::create_dir_all(&short_root).unwrap();
    std::fs::write(
        short_root.join(CACHED_GEM_PROJECT_DIGEST_MARKER),
        "0".repeat(64),
    )
    .unwrap();

    let resolved = cached_gem_project_extraction_root(cache.path(), project);
    let expected = format!("server-{}", cached_gem_project_digest(project));
    assert_ne!(
        resolved, short_root,
        "a colliding short digest must not reuse the other project's extraction root"
    );
    assert_eq!(
        resolved.file_name().and_then(|name| name.to_str()),
        Some(expected.as_str())
    );
}

#[test]
fn priority_vendor_archive_discovery_defers_unrelated_locked_archives() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    active_gem (1.2.3)\n    background_gem (4.5.6)\n\nPLATFORMS\n  ruby\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/active_gem-1.2.3.gem"),
        "active_gem",
        "1.2.3",
        "ruby",
    );
    let deferred_archive = workspace
        .path()
        .join("vendor/cache/background_gem-4.5.6.gem");
    std::fs::write(&deferred_archive, b"not a gem archive").unwrap();
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.load_locked_gems().unwrap();

    indexer
        .discover_priority_cached_gem_archives(&HashSet::from(["activegem".to_string()]))
        .unwrap();

    assert!(indexer.discovered_gems.contains_key("active_gem"));
    assert!(
        !indexer.discovered_gems.contains_key("background_gem"),
        "navigation discovery must not read or validate an unrelated archive"
    );

    create_cached_gem(&deferred_archive, "background_gem", "4.5.6", "ruby");
    indexer.discover_cached_gem_archives().unwrap();

    assert_eq!(indexer.discovered_gems["active_gem"].len(), 1);
    assert!(indexer.discovered_gems.contains_key("background_gem"));
}

#[test]
fn cached_registry_gem_must_match_locked_platform_version() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3-java)\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
        "example",
        "1.2.3",
        "ruby",
    );
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

    indexer.discover_cached_gem_archives().unwrap();

    assert!(
        !indexer.discovered_gems.contains_key("example"),
        "an archive whose metadata disagrees with the lockfile must fail closed"
    );
}

#[test]
fn cached_registry_gem_rejects_unsafe_require_path() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    create_cached_gem_with_require_path(
        &workspace.path().join("vendor/cache/example-1.2.3.gem"),
        "example",
        "1.2.3",
        "ruby",
        "../lib",
    );
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());

    indexer.discover_cached_gem_archives().unwrap();

    assert!(
        !indexer.discovered_gems.contains_key("example"),
        "an archive with a traversing require path must fail closed"
    );
    assert!(
        !extraction_cache.path().join("lib").exists(),
        "unsafe archive paths must never escape their extraction destination"
    );
}

#[test]
fn cached_registry_gems_are_isolated_to_the_owning_project() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    let first = workspace.path().join("first");
    let second = workspace.path().join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    for root in [&first, &second] {
        std::fs::write(
            root.join("Gemfile.lock"),
            "GEM\n  specs:\n    example (1.2.3)\n",
        )
        .unwrap();
    }
    create_cached_gem(
        &first.join("vendor/cache/example-1.2.3.gem"),
        "example",
        "1.2.3",
        "ruby",
    );
    let mut first_indexer = create_cached_gem_indexer(&first, extraction_cache.path());
    let mut second_indexer = create_cached_gem_indexer(&second, extraction_cache.path());

    first_indexer.discover_cached_gem_archives().unwrap();
    second_indexer.discover_cached_gem_archives().unwrap();

    assert!(first_indexer.discovered_gems.contains_key("example"));
    let first_path = &first_indexer.discovered_gems["example"][0].path;
    assert!(
        !first_path.starts_with(second_indexer.cached_gem_extraction_root(&second).unwrap()),
        "different projects must have different external extraction roots"
    );
    assert!(
        !second_indexer.discovered_gems.contains_key("example"),
        "a sibling project must not borrow another project's vendor cache"
    );
}

#[test]
fn cached_registry_gem_wins_over_unrelated_global_version() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/example-1.2.3.gem"),
        "example",
        "1.2.3",
        "ruby",
    );
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.discovered_gems.insert(
        "example".to_string(),
        vec![GemInfo {
            name: "example".to_string(),
            version: "9.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "9.0.0".to_string(),
            source: GemSource::GlobalInstalled,
            path: crate::test::harness::fixture_path("/global/example-9.0.0"),
            lib_paths: vec![crate::test::harness::fixture_path(
                "/global/example-9.0.0/lib",
            )],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    indexer.discover_cached_gem_archives().unwrap();

    let selected = indexer.get_gem("example").unwrap();
    assert_eq!(selected.version, "1.2.3");
    assert!(
        selected
            .path
            .starts_with(extraction_cache.path().join("gems")),
        "the owning project's locked cache must beat an unrelated global gem"
    );
}

#[test]
fn exact_installed_registry_gem_wins_before_vendor_archive_fallback() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/example-1.2.3.gem"),
        "example",
        "1.2.3",
        "ruby",
    );
    let installed_path = workspace.path().join("installed/example-1.2.3");
    std::fs::create_dir_all(installed_path.join("lib")).unwrap();
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.discovered_gems.insert(
        "example".to_string(),
        vec![GemInfo {
            name: "example".to_string(),
            version: "1.2.3".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.2.3".to_string(),
            source: GemSource::GlobalInstalled,
            path: installed_path.clone(),
            lib_paths: vec![installed_path.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    indexer.discover_cached_gem_archives().unwrap();

    let selected = indexer.get_gem("example").unwrap();
    assert_eq!(
        selected.path, installed_path,
        "an exact installed gem must win before extracting the project archive fallback"
    );
    assert!(
        !extraction_cache.path().join("gems").exists(),
        "an exact installed gem must avoid unnecessary archive extraction"
    );
}

#[test]
fn wrong_installed_platform_uses_exact_vendor_archive_fallback() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3-java)\n",
    )
    .unwrap();
    create_cached_gem(
        &workspace.path().join("vendor/cache/example-1.2.3-java.gem"),
        "example",
        "1.2.3",
        "java",
    );
    let installed_path = workspace.path().join("installed/example-1.2.3");
    std::fs::create_dir_all(installed_path.join("lib")).unwrap();
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.set_selected_runtime(
        crate::test::harness::fixture_path("/runtimes/jruby/bin/jruby"),
        RubyImplementation::JRuby,
        Some(crate::test::harness::fixture_path("/jdks/17")),
    );
    indexer.detect_active_ruby_engine().unwrap();
    indexer.discovered_gems.insert(
        "example".to_string(),
        vec![GemInfo {
            name: "example".to_string(),
            version: "1.2.3".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.2.3".to_string(),
            source: GemSource::GlobalInstalled,
            path: installed_path,
            lib_paths: vec![workspace.path().join("installed/example-1.2.3/lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    indexer.discover_cached_gem_archives().unwrap();

    let selected = indexer.get_gem("example").unwrap();
    assert_eq!(selected.source, GemSource::VendorArchive);
    assert_eq!(selected.platform, "java");
    assert_eq!(selected.locked_version, "1.2.3-java");
}

#[test]
fn registry_install_cannot_substitute_for_locked_git_dependency() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GIT\n  remote: https://example.test/example.git\n  revision: abcdef1234567890\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    let global_path = workspace.path().join("global/example-1.2.3");
    std::fs::create_dir_all(global_path.join("lib")).unwrap();
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.discovered_gems.insert(
        "example".to_string(),
        vec![GemInfo {
            name: "example".to_string(),
            version: "1.2.3".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.2.3".to_string(),
            source: GemSource::GlobalInstalled,
            path: global_path.clone(),
            lib_paths: vec![global_path.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    indexer.discover_cached_gem_archives().unwrap();

    assert!(
        indexer.get_gem("example").is_none(),
        "a registry installation must never replace a Git-locked dependency"
    );
}

#[test]
fn unrelated_global_version_is_unavailable_without_exact_locked_source() {
    let workspace = TempDir::new().unwrap();
    let extraction_cache = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    example (1.2.3)\n",
    )
    .unwrap();
    let global_path = workspace.path().join("global/example-9.0.0");
    std::fs::create_dir_all(global_path.join("lib")).unwrap();
    let mut indexer = create_cached_gem_indexer(workspace.path(), extraction_cache.path());
    indexer.discovered_gems.insert(
        "example".to_string(),
        vec![GemInfo {
            name: "example".to_string(),
            version: "9.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "9.0.0".to_string(),
            source: GemSource::GlobalInstalled,
            path: global_path.clone(),
            lib_paths: vec![global_path.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    indexer.discover_cached_gem_archives().unwrap();

    assert!(
        indexer.get_gem("example").is_none(),
        "a different globally installed version must never substitute for the lockfile identity"
    );
    assert!(
        !indexer.has_gem("example"),
        "availability accessors must not expose rejected candidates"
    );
}
