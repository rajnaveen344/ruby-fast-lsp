use super::*;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(
        path.parent()
            .expect("test fixture file must have a parent directory"),
    )
    .expect("test fixture parent must be created");
    fs::write(path, bytes).expect("test fixture file must be written");
}

fn jar_with_manifest(class_path: Option<&str>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    if let Some(class_path) = class_path {
        writer
            .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        write!(
            writer,
            "Manifest-Version: 1.0\r\nClass-Path: {class_path}\r\n\r\n"
        )
        .unwrap();
    }
    writer
        .start_file("fixture.txt", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"fixture").unwrap();
    writer.finish().unwrap().into_inner()
}

fn fixture_inputs(root: &Path) -> ClasspathInputs {
    let project = root.join("project");
    let runtime = root.join("jruby-9.2.21.0");
    let java_home = root.join("jdk-17");
    let repository = root.join("m2");
    fs::create_dir_all(&project).expect("project fixture root must be created");
    write(&runtime.join("bin/jruby"), b"fixture executable");
    write(&runtime.join("lib/jruby.jar"), b"jruby runtime");
    write(
        &runtime.join("lib/ruby/stdlib/jopenssl.jar"),
        b"jruby stdlib",
    );
    write(&java_home.join("jmods/java.base.jmod"), b"java base");
    write(&java_home.join("lib/src.zip"), b"jdk sources");
    write(
        &repository.join("com/example/demo/1.2/demo-1.2.jar"),
        b"locked demo",
    );
    write(
        &project.join("Jars.lock"),
        b"com.example:demo:jar:1.2:runtime:\ncom.missing:absent:jar:9.0:runtime:\n",
    );
    write(&project.join("vendor/jars/explicit.jar"), b"explicit");
    fs::create_dir_all(project.join("java-src")).expect("explicit source fixture must be created");

    ClasspathInputs {
        project_root: project,
        jruby_executable: runtime.join("bin/jruby"),
        java_home,
        maven_repository: Some(repository),
        java_gem_roots: Vec::new(),
        additional_classpath: vec!["vendor/jars/*.jar".to_string()],
        additional_sources: vec!["java-src".to_string()],
    }
}

fn sibling_project_inputs(shared: &ClasspathInputs, root: &Path) -> ClasspathInputs {
    let project = root.join("project");
    fs::create_dir_all(&project).expect("sibling project fixture root must be created");
    write(
        &project.join("Jars.lock"),
        b"com.example:demo:jar:1.2:runtime:\ncom.missing:absent:jar:9.0:runtime:\n",
    );
    write(
        &project.join("vendor/jars/explicit.jar"),
        b"sibling explicit",
    );
    fs::create_dir_all(project.join("java-src"))
        .expect("sibling explicit source fixture must be created");
    ClasspathInputs {
        project_root: project,
        jruby_executable: shared.jruby_executable.clone(),
        java_home: shared.java_home.clone(),
        maven_repository: shared.maven_repository.clone(),
        java_gem_roots: shared.java_gem_roots.clone(),
        additional_classpath: shared.additional_classpath.clone(),
        additional_sources: shared.additional_sources.clone(),
    }
}

#[test]
fn reuses_exact_file_products_without_merging_project_classpaths() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let left_inputs = fixture_inputs(&fixture.path().join("shared"));
    let right_inputs = sibling_project_inputs(&left_inputs, &fixture.path().join("right"));
    let cache = ClasspathFileProductCache::new(128, 1024 * 1024);

    let left =
        discover_project_classpath_with_cache(&left_inputs, ClasspathLimits::default(), &cache)
            .expect("left cached classpath must be discovered");
    let after_left = cache.snapshot();
    assert!(after_left.lookups > 0);
    assert_eq!(after_left.producers, after_left.lookups);
    assert_eq!(after_left.hits, 0);
    assert_eq!(after_left.joined_flights, 0);

    let right =
        discover_project_classpath_with_cache(&right_inputs, ClasspathLimits::default(), &cache)
            .expect("right cached classpath must be discovered");
    let after_right = cache.snapshot();

    assert_ne!(left.project_root, right.project_root);
    assert_ne!(left.fingerprint_sha256, right.fingerprint_sha256);
    assert!(
        after_right.hits >= 5,
        "expected shared runtime/JDK/Maven reuse"
    );
    assert!(after_right.producers < after_right.lookups);
    assert!(left
        .artifacts
        .iter()
        .any(|artifact| artifact.path.starts_with(&left.project_root)));
    assert!(left
        .artifacts
        .iter()
        .all(|artifact| !artifact.path.starts_with(&right.project_root)));
    assert!(right
        .artifacts
        .iter()
        .any(|artifact| artifact.path.starts_with(&right.project_root)));
    assert!(right
        .artifacts
        .iter()
        .all(|artifact| !artifact.path.starts_with(&left.project_root)));
}

#[test]
fn concurrent_identical_discovery_has_one_producer_per_file_identity() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let inputs = fixture_inputs(fixture.path());
    let cache = ClasspathFileProductCache::new(128, 1024 * 1024);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let inputs = inputs.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
                .expect("concurrent cached classpath must be discovered")
        }));
    }
    barrier.wait();
    let left = workers
        .remove(0)
        .join()
        .expect("left worker must not panic");
    let right = workers
        .remove(0)
        .join()
        .expect("right worker must not panic");
    let snapshot = cache.snapshot();

    assert_eq!(left, right);
    assert!(snapshot.producers > 0);
    assert_eq!(snapshot.lookups, snapshot.producers * 2);
    assert_eq!(snapshot.hits + snapshot.joined_flights, snapshot.producers);
}

#[test]
fn changed_files_miss_the_cache_and_hit_paths_still_enforce_consumer_limits() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let inputs = fixture_inputs(fixture.path());
    let cache = ClasspathFileProductCache::new(128, 1024 * 1024);
    let first = discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
        .expect("initial cached classpath must be discovered");
    let after_first = cache.snapshot();
    let explicit_path = inputs.project_root.join("vendor/jars/explicit.jar");
    let first_fingerprint = first
        .artifacts
        .iter()
        .find(|artifact| artifact.path == explicit_path.canonicalize().unwrap())
        .expect("initial explicit artifact must exist")
        .fingerprint_sha256
        .clone();

    write(
        &explicit_path,
        b"changed explicit artifact with a new length",
    );
    let second = discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
        .expect("changed cached classpath must be rediscovered");
    let after_second = cache.snapshot();
    let second_fingerprint = second
        .artifacts
        .iter()
        .find(|artifact| artifact.path == explicit_path.canonicalize().unwrap())
        .expect("changed explicit artifact must exist")
        .fingerprint_sha256
        .clone();

    assert_ne!(first_fingerprint, second_fingerprint);
    assert_eq!(after_second.producers, after_first.producers + 1);
    assert!(after_second.hits > after_first.hits);

    let mut restrictive = ClasspathLimits::default();
    restrictive.max_total_bytes = 1;
    assert_eq!(
        discover_project_classpath_with_cache(&inputs, restrictive, &cache),
        Err(ClasspathError::LimitExceeded("total classpath bytes"))
    );
}

#[test]
fn file_product_retention_obeys_entry_and_weight_bounds() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let inputs = fixture_inputs(fixture.path());
    let cache = ClasspathFileProductCache::new(2, 1_200);

    discover_project_classpath_with_cache(&inputs, ClasspathLimits::default(), &cache)
        .expect("bounded cached classpath must be discovered");
    let snapshot = cache.snapshot();

    assert!(snapshot.entries <= 2);
    assert!(cache.retained_weight_bytes() <= 1_200);
    assert!(snapshot.evictions > 0);
}

#[test]
fn discovers_one_project_in_precedence_order_with_content_identity() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let inputs = fixture_inputs(fixture.path());
    let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
        .expect("fixture classpath must be discovered");

    assert_eq!(
        classpath
            .artifacts
            .iter()
            .map(|artifact| artifact.origin)
            .collect::<Vec<_>>(),
        vec![
            ArtifactOrigin::JrubyRuntime,
            ArtifactOrigin::JrubyRuntime,
            ArtifactOrigin::JdkRuntime,
            ArtifactOrigin::Lockfile,
            ArtifactOrigin::Explicit,
        ]
    );
    assert!(classpath
        .artifacts
        .iter()
        .all(|artifact| artifact.fingerprint_sha256.len() == 64));
    assert_eq!(classpath.sources.len(), 2);
    assert_eq!(
        classpath.unresolved,
        vec![UnresolvedCoordinate {
            coordinate: "com.missing:absent:9.0".to_string(),
            origin: ArtifactOrigin::Lockfile,
        }]
    );
    assert_eq!(classpath.fingerprint_sha256.len(), 64);
}

#[test]
fn discovers_installed_project_local_jar_repository_without_a_lockfile() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    fs::remove_file(inputs.project_root.join("Jars.lock"))
        .expect("fixture lockfile must be removed");
    write(
        &inputs
            .project_root
            .join("lib/jars/com/example/transitive/4.5/transitive-4.5.jar"),
        b"project-local transitive jar",
    );
    inputs.maven_repository = None;

    let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
        .expect("project-local installed jars must be discovered");
    let project_repository = classpath
        .artifacts
        .iter()
        .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
        .collect::<Vec<_>>();

    assert_eq!(project_repository.len(), 1);
    assert!(project_repository[0]
        .path
        .ends_with("lib/jars/com/example/transitive/4.5/transitive-4.5.jar"));
}

#[test]
fn indexes_jars_only_below_the_exact_selected_java_gem_root() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    inputs.maven_repository = None;
    fs::remove_file(inputs.project_root.join("Jars.lock")).unwrap();
    inputs.additional_classpath.clear();
    let selected = fixture
        .path()
        .join("gems/jruby-9.2.21.0/gems/bson-4.14.1-java");
    let unrelated = fixture
        .path()
        .join("gems/jruby-9.2.21.0/gems/bson-4.14.0-java");
    write(&selected.join("lib/bson.jar"), b"selected Java gem");
    write(&unrelated.join("lib/bson.jar"), b"unrelated Java gem");
    inputs.java_gem_roots = vec![selected.clone()];

    let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
        .expect("exact Java gem classpath must be discovered");
    let java_gems = classpath
        .artifacts
        .iter()
        .filter(|artifact| artifact.origin == ArtifactOrigin::JavaGem)
        .collect::<Vec<_>>();

    assert_eq!(java_gems.len(), 1);
    assert_eq!(
        java_gems[0].path,
        selected.join("lib/bson.jar").canonicalize().unwrap()
    );
    assert!(!classpath
        .artifacts
        .iter()
        .any(|artifact| artifact.path.starts_with(&unrelated)));
}

#[test]
fn expands_bounded_manifest_class_path_without_cycles_or_parent_traversal() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    inputs.maven_repository = None;
    fs::remove_file(inputs.project_root.join("Jars.lock")).unwrap();
    inputs.additional_classpath = vec!["vendor/jars/root.jar".to_string()];
    let root_jar = inputs.project_root.join("vendor/jars/root.jar");
    let dependency_jar = inputs.project_root.join("vendor/jars/dependency.jar");
    write(&root_jar, &jar_with_manifest(Some("dependency.jar")));
    write(&dependency_jar, &jar_with_manifest(Some("root.jar")));

    let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
        .expect("manifest classpath must expand deterministically");
    assert_eq!(
        classpath
            .artifacts
            .iter()
            .filter(|artifact| artifact.path == root_jar.canonicalize().unwrap())
            .count(),
        1
    );
    assert!(classpath.artifacts.iter().any(|artifact| {
        artifact.path == dependency_jar.canonicalize().unwrap()
            && artifact.origin == ArtifactOrigin::ManifestClassPath
    }));

    write(&root_jar, &jar_with_manifest(Some("../escape.jar")));
    assert!(matches!(
        discover_project_classpath(&inputs, ClasspathLimits::default()),
        Err(ClasspathError::InvalidManifestEntry { artifact, entry })
            if artifact == root_jar.canonicalize().unwrap() && entry == "../escape.jar"
    ));
}

#[test]
fn discovers_exact_attached_and_project_java_sources_without_indexing_source_jars_as_code() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    write(
        &inputs
            .maven_repository
            .as_ref()
            .expect("fixture has a Maven repository")
            .join("com/example/demo/1.2/demo-1.2-sources.jar"),
        b"locked demo sources",
    );
    write(
        &inputs.project_root.join("lib/jars/local-2.0.jar"),
        b"local binary",
    );
    write(
        &inputs.project_root.join("lib/jars/local-2.0-sources.jar"),
        b"local sources",
    );
    write(
        &inputs
            .project_root
            .join("src/main/java/com/example/ProjectType.java"),
        b"package com.example; class ProjectType {}",
    );
    inputs.additional_classpath.clear();
    inputs.additional_sources.clear();

    let classpath = discover_project_classpath(&inputs, ClasspathLimits::default())
        .expect("attached and project sources must be discovered");
    assert!(
        classpath
            .artifacts
            .iter()
            .all(|artifact| !artifact.path.to_string_lossy().contains("-sources.jar")),
        "source archives must never be parsed as Java bytecode artifacts"
    );
    for suffix in [
        "demo-1.2-sources.jar",
        "local-2.0-sources.jar",
        "src/main/java",
        "lib/src.zip",
    ] {
        assert!(
            classpath
                .sources
                .iter()
                .any(|source| source.path.ends_with(suffix)),
            "missing discovered Java source root {suffix}: {:?}",
            classpath.sources
        );
    }
}

#[test]
fn project_local_jar_repositories_are_isolated_and_do_not_follow_symlinks() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let left_root = fixture.path().join("left");
    let right_root = fixture.path().join("right");
    let mut left = fixture_inputs(&left_root);
    let mut right = fixture_inputs(&right_root);
    for inputs in [&mut left, &mut right] {
        fs::remove_file(inputs.project_root.join("Jars.lock"))
            .expect("fixture lockfile must be removed");
        inputs.maven_repository = None;
    }
    write(
        &left.project_root.join("lib/jars/left-only.jar"),
        b"left-only",
    );
    write(
        &right.project_root.join("lib/jars/right-only.jar"),
        b"right-only",
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            right.project_root.join("lib/jars"),
            left.project_root.join("lib/jars/linked-right"),
        )
        .expect("fixture repository symlink must be created");
    }

    let left = discover_project_classpath(&left, ClasspathLimits::default())
        .expect("left classpath must be discovered");
    let right = discover_project_classpath(&right, ClasspathLimits::default())
        .expect("right classpath must be discovered");
    let left_paths = left
        .artifacts
        .iter()
        .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
        .map(|artifact| artifact.path.as_path())
        .collect::<Vec<_>>();
    let right_paths = right
        .artifacts
        .iter()
        .filter(|artifact| artifact.origin == ArtifactOrigin::ProjectRepository)
        .map(|artifact| artifact.path.as_path())
        .collect::<Vec<_>>();

    assert_eq!(left_paths.len(), 1);
    assert!(left_paths[0].ends_with("lib/jars/left-only.jar"));
    assert_eq!(right_paths.len(), 1);
    assert!(right_paths[0].ends_with("lib/jars/right-only.jar"));
}

#[test]
fn project_local_repository_walk_counts_every_visited_entry() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    fs::remove_file(inputs.project_root.join("Jars.lock"))
        .expect("fixture lockfile must be removed");
    inputs.maven_repository = None;
    for index in 0..8 {
        fs::create_dir_all(
            inputs
                .project_root
                .join(format!("lib/jars/empty/{index}/nested")),
        )
        .expect("fixture empty directory must be created");
    }
    let mut limits = ClasspathLimits::default();
    limits.max_walk_entries = 4;

    assert_eq!(
        discover_project_classpath(&inputs, limits),
        Err(ClasspathError::LimitExceeded("classpath walk entries"))
    );
}

#[test]
fn keeps_conflicting_project_classpaths_isolated() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let left_root = fixture.path().join("left");
    let right_root = fixture.path().join("right");
    let left = fixture_inputs(&left_root);
    let right = fixture_inputs(&right_root);
    write(
        &right
            .maven_repository
            .as_ref()
            .expect("fixture has repository")
            .join("com/example/demo/1.2/demo-1.2.jar"),
        b"different locked demo",
    );

    let left = discover_project_classpath(&left, ClasspathLimits::default())
        .expect("left fixture classpath must be discovered");
    let right = discover_project_classpath(&right, ClasspathLimits::default())
        .expect("right fixture classpath must be discovered");
    assert_ne!(left.project_root, right.project_root);
    assert_ne!(left.fingerprint_sha256, right.fingerprint_sha256);
    let left_demo = left
        .artifacts
        .iter()
        .find(|artifact| artifact.origin == ArtifactOrigin::Lockfile)
        .expect("left lock artifact must exist");
    let right_demo = right
        .artifacts
        .iter()
        .find(|artifact| artifact.origin == ArtifactOrigin::Lockfile)
        .expect("right lock artifact must exist");
    assert_ne!(left_demo.fingerprint_sha256, right_demo.fingerprint_sha256);
    let left_root = fs::canonicalize(left_root).expect("left fixture root must canonicalize");
    let right_root = fs::canonicalize(right_root).expect("right fixture root must canonicalize");
    assert!(left
        .artifacts
        .iter()
        .all(|artifact| artifact.path.starts_with(&left_root)));
    assert!(right
        .artifacts
        .iter()
        .all(|artifact| artifact.path.starts_with(&right_root)));
}

#[test]
fn rejects_escaping_patterns_and_applies_artifact_bounds() {
    let fixture = tempfile::tempdir().expect("classpath fixture root must be created");
    let mut inputs = fixture_inputs(fixture.path());
    inputs.additional_classpath = vec!["../outside.jar".to_string()];
    assert_eq!(
        discover_project_classpath(&inputs, ClasspathLimits::default()),
        Err(ClasspathError::InvalidProjectPattern(
            "../outside.jar".to_string()
        ))
    );

    inputs.additional_classpath = vec!["vendor/jars/*.jar".to_string()];
    let mut limits = ClasspathLimits::default();
    limits.max_artifacts = 1;
    assert_eq!(
        discover_project_classpath(&inputs, limits),
        Err(ClasspathError::LimitExceeded("classpath artifacts"))
    );
}

#[test]
fn parses_literal_jarfile_only_without_executing_ruby() {
    assert_eq!(
        parse_jarfile_coordinate("jar 'org.example:demo', '2.0'"),
        Ok(Some(MavenCoordinate {
            group: "org.example".to_string(),
            artifact: "demo".to_string(),
            classifier: None,
            version: "2.0".to_string(),
        }))
    );
    assert_eq!(
        parse_jarfile_coordinate("jar dynamic_coordinate, ENV['VERSION']"),
        Err(ClasspathError::InvalidLockEntry(
            "jar dynamic_coordinate, ENV['VERSION']".to_string()
        ))
    );
}
