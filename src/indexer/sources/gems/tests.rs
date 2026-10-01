use super::java::discover_locked_java_gem_roots;
use super::lockfile::{compare_versions, parse_gemfile_gem_statement, parse_locked_gems};
use super::products::GEM_PRODUCT_TRANSIENT_MEMORY_BYTES;
use super::vendor_cache::{
    cached_gem_project_digest, cached_gem_project_extraction_root, cached_gem_project_identity,
    CACHED_GEM_PROJECT_DIGEST_MARKER, CACHED_GEM_PROJECT_DIGEST_PREFIX_CHARS,
};
use super::*;
use crate::indexing_resources::IndexingResourcePriority;
use crate::indexing_resources::IndexingWorkSpec;
use crate::server::RubyLanguageServer;
use flate2::write::GzEncoder;
use flate2::Compression;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyMethod, RubyType, SourceKind};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery, FileFacts, ResolveMode};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tar::{Builder, Header};
use tempfile::TempDir;

fn create_test_indexer() -> IndexerGem {
    let temp_dir = TempDir::new().unwrap();
    IndexerGem::new(Some(temp_dir.path().to_path_buf()))
}

#[test]
fn test_gem_indexer_creation() {
    let indexer = create_test_indexer();
    assert_eq!(indexer.gem_count(), 0);
    assert!(indexer.get_required_gems().is_empty());
}

fn shared_dependency_indexer(project_root: &Path, gem_root: &Path) -> IndexerGem {
    let mut indexer = IndexerGem::new(Some(project_root.to_path_buf()));
    indexer.set_required_gems(HashSet::from(["shared_widget".to_string()]));
    indexer.discovered_gems.insert(
        "shared_widget".to_string(),
        vec![GemInfo {
            name: "shared_widget".to_string(),
            version: "1.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.0.0".to_string(),
            source: GemSource::BundlerInstalled,
            path: gem_root.to_path_buf(),
            lib_paths: vec![gem_root.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );
    indexer.set_file_processor(FileProcessor::new());
    indexer.set_dependency_seed_engine(AnalysisEngine::new());
    indexer
}

fn assert_shared_dependency_semantics(engine: &AnalysisEngine, expected_path: &Path) {
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("SharedWidget").unwrap()]);
    let method = RubyMethod::new("label").unwrap();
    let query = AnalysisQuery::new(engine);
    let definitions =
        query.constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[]);
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        engine.file(definitions[0].file_id).unwrap().path,
        expected_path
    );
    assert_eq!(
        engine.file(definitions[0].file_id).unwrap().kind,
        SourceKind::Gem
    );

    let signatures = query.resolve_method_signature_facts(&owner, &method);
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].params, ["prefix"]);
    assert_eq!(signatures[0].return_type_label.as_deref(), Some("String"));
    let value = FullyQualifiedName::constant(vec![
        RubyConstant::new("SharedWidget").unwrap(),
        RubyConstant::new("DEFAULT_LABEL").unwrap(),
    ]);
    assert_eq!(query.constant_value_type(&value), Some(RubyType::string()));
}

#[test]
fn ordinary_gem_products_ignore_unrelated_jruby_classpaths_but_java_gems_do_not() {
    let fixture = TempDir::new().unwrap();
    let first_project = fixture.path().join("first");
    let second_project = fixture.path().join("second");
    let first_gem = first_project.join("bundle/shared_widget-1.0.0");
    let second_gem = second_project.join("bundle/shared_widget-1.0.0");
    fs::create_dir_all(first_gem.join("lib")).unwrap();
    fs::create_dir_all(second_gem.join("lib")).unwrap();
    let ordinary = "module SharedWidget\nend\n";
    fs::write(first_gem.join("lib/shared_widget.rb"), ordinary).unwrap();
    fs::write(second_gem.join("lib/shared_widget.rb"), ordinary).unwrap();

    let mut first = shared_dependency_indexer(&first_project, &first_gem);
    first.set_runtime_provider_fingerprint(Some("classpath-a".to_string()));
    let mut second = shared_dependency_indexer(&second_project, &second_gem);
    second.set_runtime_provider_fingerprint(Some("classpath-b".to_string()));
    let seed = AnalysisEngine::new().semantic_context_fingerprint();
    let first_key = first.required_gem_manifests(seed).unwrap()[0].key().clone();
    let second_key = second.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();
    assert_eq!(
        first_key, second_key,
        "ordinary Ruby gem facts must not be invalidated by an unrelated project classpath"
    );

    fs::write(
        first_gem.join("lib/shared_widget.rb"),
        "java_import 'fixtures.RichFixture'\n",
    )
    .unwrap();
    fs::write(
        second_gem.join("lib/shared_widget.rb"),
        "java_import 'fixtures.RichFixture'\n",
    )
    .unwrap();
    let first_key = first.required_gem_manifests(seed).unwrap()[0].key().clone();
    let second_key = second.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();
    assert_ne!(
        first_key, second_key,
        "a gem using JRuby interop must retain the exact owning classpath identity"
    );
}

#[tokio::test]
async fn concurrent_isolated_projects_share_one_flight_with_exact_provenance() {
    let fixture = TempDir::new().unwrap();
    let first_project = fixture.path().join("first");
    let second_project = fixture.path().join("second");
    let third_project = fixture.path().join("third");
    let first_gem = first_project.join("bundle/shared_widget-1.0.0");
    let second_gem = second_project.join("bundle/shared_widget-1.0.0");
    let third_gem = third_project.join("bundle/shared_widget-1.0.0");
    let content = concat!(
        "class SharedWidget\n",
        "  DEFAULT_LABEL = \"label\"\n",
        "  # @param prefix [String]\n",
        "  # @return [String]\n",
        "  def label(prefix)\n",
        "    \"label\"\n",
        "  end\n",
        "end\n",
    );
    for (project, gem) in [
        (&first_project, &first_gem),
        (&second_project, &second_gem),
        (&third_project, &third_gem),
    ] {
        fs::create_dir_all(gem.join("lib")).unwrap();
        fs::write(project.join("Gemfile"), "gem 'shared_widget'\n").unwrap();
        fs::write(gem.join("lib/shared_widget.rb"), content).unwrap();
    }

    let first_indexer = shared_dependency_indexer(&first_project, &first_gem);
    let second_indexer = shared_dependency_indexer(&second_project, &second_gem);
    let server = RubyLanguageServer::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");
    let first_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let second_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));

    let (first_result, second_result) = tokio::join!(
        first_indexer.index_required_gems_with_shared_product(&server, first_engine.clone()),
        second_indexer.index_required_gems_with_shared_product(&server, second_engine.clone()),
    );
    assert_eq!(first_result.unwrap().len(), 1);
    assert_eq!(second_result.unwrap().len(), 1);
    let cache = server.products.gem_dependencies().snapshot();
    assert_eq!(cache.lookups, 2);
    assert_eq!(cache.producers, 1);
    assert_eq!(cache.hits + cache.joined_flights, 1);
    assert_eq!(cache.entries, 0);

    let first_path = first_gem.join("lib/shared_widget.rb");
    let second_path = second_gem.join("lib/shared_widget.rb");
    assert_shared_dependency_semantics(&first_engine.read(), &first_path);
    assert_shared_dependency_semantics(&second_engine.read(), &second_path);
    let first_file_id = first_engine.read().file_id(&first_path).unwrap();
    let second_file_id = second_engine.read().file_id(&second_path).unwrap();
    assert!(first_engine.read().file_id(&second_path).is_none());
    assert!(second_engine.read().file_id(&first_path).is_none());
    assert_eq!(
        first_engine.read().file(first_file_id).unwrap().path,
        first_path
    );
    assert_eq!(
        second_engine.read().file(second_file_id).unwrap().path,
        second_path
    );

    first_engine
        .write()
        .replace_facts(first_file_id, FileFacts::default(), ResolveMode::Immediate);
    assert!(
        AnalysisQuery::new(&first_engine.read())
            .constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[],)
            .is_empty(),
        "ordinary replacement must remove rebound facts from only the first consumer"
    );
    assert_shared_dependency_semantics(&second_engine.read(), &second_path);

    let third_indexer = shared_dependency_indexer(&third_project, &third_gem);
    let third_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    assert_eq!(
        third_indexer
            .index_required_gems_with_shared_product(&server, third_engine.clone())
            .await
            .unwrap()
            .len(),
        1
    );
    let after_sequential_consumer = server.products.gem_dependencies().snapshot();
    assert_eq!(after_sequential_consumer.lookups, 3);
    assert_eq!(after_sequential_consumer.producers, 2);
    assert_eq!(after_sequential_consumer.entries, 0);
    let persistent = server.products.persistent().gem_product_snapshot();
    assert_eq!(persistent.producers, 1);
    assert_eq!(persistent.publications, 1);
    assert_eq!(persistent.hits, 1);
    assert_shared_dependency_semantics(
        &third_engine.read(),
        &third_gem.join("lib/shared_widget.rb"),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cold_active_gem_product_overlaps_the_jruby_runtime_companion() {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    fs::create_dir_all(gem_root.join("lib")).unwrap();
    fs::write(project_root.join("Gemfile"), "gem 'shared_widget'\n").unwrap();
    fs::write(
        gem_root.join("lib/shared_widget.rb"),
        "class SharedWidget\nend\n",
    )
    .unwrap();

    let indexer = shared_dependency_indexer(&project_root, &gem_root);
    let mut server = RubyLanguageServer::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");
    server
        .indexing
        .set_resources(crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                6,
                2,
                512 * 1024 * 1024,
                2,
            ),
        ));
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&project_root, true);
    let server = Arc::new(server);

    let (runtime_started_tx, runtime_started_rx) = tokio::sync::oneshot::channel();
    let (runtime_release_tx, runtime_release_rx) = std::sync::mpsc::channel();
    let runtime_server = server.clone();
    let runtime_root = project_root.clone();
    let runtime = tokio::spawn(async move {
        runtime_server
            .indexing
            .resources()
            .run_partitioned_parallel_with_resources(
                "simulated JRuby runtime companion",
                IndexingWorkSpec::new(
                    Some(runtime_root),
                    IndexingResourcePriority::Background,
                    1,
                    GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
                    1,
                )
                .as_project_parallel(),
                None,
                move || {
                    runtime_started_tx.send(()).unwrap();
                    runtime_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });
    runtime_started_rx.await.unwrap();

    let engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        indexer.index_required_gems_with_shared_product(&server, engine),
    )
    .await;
    runtime_release_tx.send(()).unwrap();
    runtime.await.unwrap();

    assert!(
        result.is_ok(),
        "cold gem-product construction must use the active project's five-lane partition so \
             it can finish while the one-lane JRuby companion owns the persistent-cache \
             maintenance path"
    );
    assert_eq!(result.unwrap().unwrap().len(), 1);
    let resources = server.indexing.resources().snapshot();
    assert_eq!(resources.peak_active_cpu_lanes, 6);
    assert_eq!(
        resources.peak_active_transient_memory_bytes,
        512 * 1024 * 1024
    );
}

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

#[test]
fn discovers_only_exact_locked_java_platform_gem_roots_for_selected_jruby() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let rvm = fixture.path().join(".rvm");
    let runtime = rvm.join("rubies/jruby-9.2.21.0");
    let executable = runtime.join("bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "    bson (4.14.1)\n",
            "    rack (3.0.0)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    let exact = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
    let wrong = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.0-java");
    let ruby = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1");
    fs::create_dir_all(&exact).unwrap();
    fs::create_dir_all(&wrong).unwrap();
    fs::create_dir_all(&ruby).unwrap();

    assert_eq!(
        discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
        vec![exact.canonicalize().unwrap()]
    );
}

#[test]
fn project_local_locked_java_gem_precedes_the_selected_rvm_runtime_copy() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let rvm = fixture.path().join(".rvm");
    let executable = rvm.join("rubies/jruby-9.2.21.0/bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    let local = project.join("vendor/bundle/jruby/2.5.0/gems/bson-4.14.1-java");
    let global = rvm.join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&global).unwrap();

    assert_eq!(
        discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap(),
        vec![local.canonicalize().unwrap()]
    );
}

#[test]
fn duplicate_project_local_locked_java_gem_installations_fail_closed() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"fixture").unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Gemfile.lock"),
        concat!(
            "GEM\n",
            "  remote: https://rubygems.org/\n",
            "  specs:\n",
            "    bson (4.14.1-java)\n",
            "PLATFORMS\n",
            "  java\n",
        ),
    )
    .unwrap();
    for compatibility in ["2.5.0", "3.1.0"] {
        fs::create_dir_all(
            project
                .join("vendor/bundle/jruby")
                .join(compatibility)
                .join("gems/bson-4.14.1-java"),
        )
        .unwrap();
    }

    let error = discover_locked_java_gem_roots(&project, &executable, "2.5").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous installations in project vendor/bundle"),
        "unexpected error: {error:?}"
    );
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
        RubyImplementation::Mri,
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
    indexer.set_selected_runtime(fake_ruby, RubyImplementation::Mri, None);
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
    let seed = AnalysisEngine::new().semantic_context_fingerprint();
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
    indexer.set_selected_runtime(fake_ruby, RubyImplementation::JRuby, None);
    indexer.discover_auto_gems().unwrap();

    assert_eq!(std::fs::read_to_string(invocation_log).unwrap(), "x");
    let gem = &indexer.discovered_gems["example"][0];
    assert_eq!(gem.source, GemSource::GlobalInstalled);
    assert_eq!(gem.version, "1.2.3");
}
