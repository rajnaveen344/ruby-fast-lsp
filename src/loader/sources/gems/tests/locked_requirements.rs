//! Lockfile parsing, version ordering, and required-gem closure.

use super::*;

#[test]
fn lock_parser_preserves_source_identity_and_dependencies() {
    let identities = parse_locked_gems(
        "GIT\n  remote: https://example.test/tool.git\n  revision: abcdef123456\n  specs:\n    tool (2.0.0)\n      rack\n\nPATH\n  remote: components/local\n  specs:\n    local (0.4.0)\n\nGEM\n  specs:\n    rack (3.1.0)\n      base64\n",
    )
    .unwrap();

    assert_eq!(
        identities,
        vec![
            LockedGemIdentity {
                name: "tool".to_string(),
                locked_version: "2.0.0".to_string(),
                source: LockedGemSource::Git,
                dependencies: vec!["rack".to_string()],
            },
            LockedGemIdentity {
                name: "local".to_string(),
                locked_version: "0.4.0".to_string(),
                source: LockedGemSource::Path,
                dependencies: Vec::new(),
            },
            LockedGemIdentity {
                name: "rack".to_string(),
                locked_version: "3.1.0".to_string(),
                source: LockedGemSource::Registry,
                dependencies: vec!["base64".to_string()],
            },
        ]
    );
}

#[test]
fn valid_ruby_and_java_lock_variants_are_not_duplicate_source_identities() {
    let workspace = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    bcrypt-ruby (3.0.1)\n    bcrypt-ruby (3.0.1-java)\n\nPLATFORMS\n  java\n  ruby\n",
    )
    .unwrap();
    let mut indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    indexer.active_ruby_engine = ActiveRubyEngine::JRuby;

    indexer
        .load_locked_gems()
        .expect("Bundler multi-platform variants must be accepted");
    assert_eq!(
        indexer.locked_gems["bcrypt-ruby"].locked_version, "3.0.1-java",
        "JRuby must select the java lock variant"
    );
}

#[test]
fn test_version_comparison() {
    assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("1.0.1", "1.0.0"), Ordering::Greater);
    assert_eq!(compare_versions("1.0.0", "1.0.1"), Ordering::Less);
    assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
}

#[test]
fn gemfile_required_roots_parse_direct_gem_statements_in_stable_order() {
    let workspace = TempDir::new().unwrap();
    std::fs::write(
        workspace.path().join("Gemfile"),
        r#"
source 'https://rubygems.org'

# ignored
gem 'rack', '~> 2.0'
gem "rails"
gem 'rack'
group :test do
  gem 'rspec'
end
"#,
    )
    .unwrap();

    let indexer = IndexerGem::new(Some(workspace.path().to_path_buf()));
    assert_eq!(
        indexer.gemfile_required_roots_blocking().unwrap(),
        vec!["rack".to_string(), "rails".to_string(), "rspec".to_string()]
    );
    assert_eq!(
        parse_gemfile_gem_statement("  gem 'sinatra', require: false"),
        Some("sinatra".to_string())
    );
    assert_eq!(parse_gemfile_gem_statement("# gem 'nope'"), None);
}

#[test]
fn test_required_gems_include_transitive_dependencies() {
    let mut indexer = create_test_indexer();
    indexer.set_required_gems(HashSet::from(["rspec".to_string()]));
    indexer.discovered_gems.insert(
        "rspec".to_string(),
        vec![GemInfo {
            name: "rspec".to_string(),
            version: "3.13.2".to_string(),
            platform: "ruby".to_string(),
            locked_version: "3.13.2".to_string(),
            source: GemSource::BundlerInstalled,
            path: crate::test::harness::fixture_path("/tmp/rspec"),
            lib_paths: vec![crate::test::harness::fixture_path("/tmp/rspec/lib")],
            dependencies: vec![
                "rspec-core".to_string(),
                "rspec-expectations".to_string(),
                "rspec-mocks".to_string(),
            ],
            is_default: false,
        }],
    );
    indexer.discovered_gems.insert(
        "rspec-core".to_string(),
        vec![GemInfo {
            name: "rspec-core".to_string(),
            version: "3.13.6".to_string(),
            platform: "ruby".to_string(),
            locked_version: "3.13.6".to_string(),
            source: GemSource::BundlerInstalled,
            path: crate::test::harness::fixture_path("/tmp/rspec-core"),
            lib_paths: vec![crate::test::harness::fixture_path("/tmp/rspec-core/lib")],
            dependencies: vec!["rspec-support".to_string()],
            is_default: false,
        }],
    );
    indexer.discovered_gems.insert(
        "rspec-support".to_string(),
        vec![GemInfo {
            name: "rspec-support".to_string(),
            version: "3.13.7".to_string(),
            platform: "ruby".to_string(),
            locked_version: "3.13.7".to_string(),
            source: GemSource::BundlerInstalled,
            path: crate::test::harness::fixture_path("/tmp/rspec-support"),
            lib_paths: vec![crate::test::harness::fixture_path("/tmp/rspec-support/lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );

    let gems = indexer.required_gems_with_dependencies();

    assert_eq!(
        gems,
        vec![
            "rspec".to_string(),
            "rspec-core".to_string(),
            "rspec-expectations".to_string(),
            "rspec-mocks".to_string(),
            "rspec-support".to_string(),
        ]
    );
}

#[test]
fn required_gem_manifest_preparation_is_lazy_and_root_first() {
    let fixture = TempDir::new().unwrap();
    let project = fixture.path().join("project");
    let first = fixture.path().join("gems/first-1.0.0");
    let later = fixture.path().join("gems/later-1.0.0");
    fs::create_dir_all(first.join("lib")).unwrap();
    fs::create_dir_all(later.join("lib")).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(first.join("lib/first.rb"), "module FirstDependency\nend\n").unwrap();
    fs::write(later.join("lib/later.rb"), [0xff, 0xfe]).unwrap();

    let mut indexer = IndexerGem::new(Some(project));
    indexer.set_required_gems(HashSet::from(["later".to_string(), "first".to_string()]));
    for (name, root) in [("first", first), ("later", later)] {
        indexer.discovered_gems.insert(
            name.to_string(),
            vec![GemInfo {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                platform: "ruby".to_string(),
                locked_version: "1.0.0".to_string(),
                source: GemSource::BundlerInstalled,
                path: root.clone(),
                lib_paths: vec![root.join("lib")],
                dependencies: Vec::new(),
                is_default: false,
            }],
        );
    }

    let names = indexer.required_gems_with_dependencies();
    assert_eq!(names, ["first", "later"]);
    let seed = AnalysisEngine::new().view().semantic_context_fingerprint();
    let first_manifest = indexer
        .required_gem_manifest("first", seed)
        .unwrap()
        .expect("the first direct dependency must produce a manifest");
    assert_eq!(first_manifest.sources().len(), 1);
    assert!(
        indexer.required_gem_manifest("later", seed).is_err(),
        "a later invalid dependency must fail only when its own manifest is prepared"
    );
}

#[test]
fn test_excluded_gems_win_over_roots_and_transitive_dependencies() {
    let mut indexer = create_test_indexer();
    indexer.set_required_gems(HashSet::from(["rails".to_string(), "debug".to_string()]));
    indexer.set_excluded_gems(HashSet::from([
        "debug".to_string(),
        "activesupport".to_string(),
    ]));
    indexer.discovered_gems.insert(
        "rails".to_string(),
        vec![GemInfo {
            name: "rails".to_string(),
            version: "8.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "8.0.0".to_string(),
            source: GemSource::BundlerInstalled,
            path: crate::test::harness::fixture_path("/tmp/rails"),
            lib_paths: vec![crate::test::harness::fixture_path("/tmp/rails/lib")],
            dependencies: vec!["activesupport".to_string()],
            is_default: false,
        }],
    );

    assert_eq!(indexer.required_gems_with_dependencies(), vec!["rails"]);
}
