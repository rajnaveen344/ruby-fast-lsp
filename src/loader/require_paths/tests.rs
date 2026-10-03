use super::*;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::{Project, SourceFileInput};

#[test]
fn finds_static_require_string_under_cursor() {
    let source = "require \"foo/bar\"\n";
    let offset = source.find("foo").unwrap();
    let target = find_require_string_at_offset(source, offset).unwrap();
    assert_eq!(target.kind, RequireKind::Require);
    assert_eq!(target.argument, "foo/bar");
}

#[test]
fn content_byte_range_excludes_quotes() {
    let source = "require 'platform/helpers/json'\n";
    let target = find_require_string_at_offset(source, source.find("json").unwrap()).unwrap();
    let (start, end) = target.content_byte_range(source);
    assert_eq!(&source[start..end], "platform/helpers/json");
    assert_eq!(
        &source[target.start_byte..target.end_byte],
        "'platform/helpers/json'"
    );
}

#[test]
fn rejects_interpolated_require_string() {
    let source = "require \"a#{b}\"\n";
    let offset = source.find('a').unwrap();
    assert!(find_require_string_at_offset(source, offset).is_none());
}

#[test]
fn collects_all_static_requires_in_file() {
    let source = "require \"a\"\nrequire_relative \"./b\"\nautoload :C, \"c\"\n";
    let targets = find_all_require_strings(source);
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].argument, "a");
    assert_eq!(targets[1].argument, "./b");
    assert_eq!(targets[1].kind, RequireKind::RequireRelative);
}

#[test]
fn unresolved_require_emits_diagnostic_on_string() {
    let source = "require \"missing\"\n";
    let file_id = SourceFileId(1);
    let diagnostics = unresolved_require_diagnostics(
        source,
        file_id,
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, UNRESOLVED_REQUIRE_CODE);
    assert_eq!(diagnostics[0].range.file_id, file_id);
    assert!(diagnostics[0].message.contains("missing"));
    let start = diagnostics[0].range.start_byte as usize;
    let end = diagnostics[0].range.end_byte as usize;
    assert_eq!(&source[start..end], "missing");
}

#[test]
fn resolved_require_emits_no_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("lib").join("foo.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# foo\n").unwrap();
    let source = "require \"foo\"\n";
    let diagnostics = unresolved_require_diagnostics(
        source,
        SourceFileId(1),
        &dir.path().join("main.rb"),
        dir.path(),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn require_relative_resolves_beside_current_file() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("app").join("main.rb");
    let target = dir.path().join("app").join("foo.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&current, "require_relative \"./foo\"\n").unwrap();
    std::fs::write(&target, "# foo\n").unwrap();

    let resolved = resolve_require_path(
        RequireKind::RequireRelative,
        "./foo",
        &current,
        dir.path(),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    )
    .unwrap();
    assert_eq!(resolved, target);
}

#[test]
fn require_relative_does_not_use_the_feature_index() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("app").join("main.rb");
    let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
    let gem_target = gem_lib.join("foo.rb");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();
    std::fs::create_dir_all(gem_target.parent().unwrap()).unwrap();
    std::fs::write(&current, "require_relative \"./foo\"\n").unwrap();
    std::fs::write(&gem_target, "# gem foo\n").unwrap();
    let index = RequireFeatureIndex::build(&[gem_lib], None);

    assert!(
        resolve_require_path(
            RequireKind::RequireRelative,
            "./foo",
            &current,
            dir.path(),
            &[],
            &index,
            None,
        )
        .is_none(),
        "require_relative must stay dirname-only even when the feature index has foo"
    );
}

#[test]
fn require_prefers_configured_load_path_before_lib() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("custom").join("foo.rb");
    let lib = dir.path().join("lib").join("foo.rb");
    std::fs::create_dir_all(custom.parent().unwrap()).unwrap();
    std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
    std::fs::write(&custom, "# custom\n").unwrap();
    std::fs::write(&lib, "# lib\n").unwrap();

    let resolved = resolve_require_path(
        RequireKind::Require,
        "foo",
        &dir.path().join("main.rb"),
        dir.path(),
        &["custom".to_string()],
        &RequireFeatureIndex::empty(),
        None,
    )
    .unwrap();
    assert_eq!(resolved, custom);
}

#[test]
fn require_can_resolve_engine_registered_virtual_files() {
    let mut engine = Project::new();
    let path = PathBuf::from("/project/lib/foo.rb");
    engine.register_file(SourceFileInput {
        path: path.clone(),
        content: "# foo\n".to_string(),
        kind: SourceKind::Project,
    });

    let resolved = resolve_require_path(
        RequireKind::Require,
        "foo",
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        Some(&engine.view()),
    )
    .unwrap();
    assert_eq!(resolved, path);
}

#[test]
fn require_resolves_through_dependency_require_roots() {
    let dir = tempfile::tempdir().unwrap();
    let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
    let target = gem_lib.join("platform/helpers/json.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# gem json\n").unwrap();
    let index = RequireFeatureIndex::build(&[gem_lib], None);

    let resolved = resolve_require_path(
        RequireKind::Require,
        "platform/helpers/json",
        &dir.path().join("main.rb"),
        dir.path(),
        &[],
        &index,
        None,
    )
    .unwrap();
    assert_eq!(resolved, target);
    assert_eq!(
        index.lookup("platform/helpers/json.rb"),
        Some(target.as_path())
    );
}

#[test]
fn feature_index_first_published_root_wins() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("gems/first-1.0.0/lib");
    let second = dir.path().join("gems/second-1.0.0/lib");
    let first_target = first.join("dup.rb");
    let second_target = second.join("dup.rb");
    std::fs::create_dir_all(first).unwrap();
    std::fs::create_dir_all(second).unwrap();
    std::fs::write(&first_target, "# first\n").unwrap();
    std::fs::write(&second_target, "# second\n").unwrap();
    let index = RequireFeatureIndex::build(
        &[
            dir.path().join("gems/first-1.0.0/lib"),
            dir.path().join("gems/second-1.0.0/lib"),
        ],
        None,
    );

    assert_eq!(index.lookup("dup"), Some(first_target.as_path()));
    let resolved = resolve_require_path(
        RequireKind::Require,
        "dup",
        &dir.path().join("main.rb"),
        dir.path(),
        &[],
        &index,
        None,
    )
    .unwrap();
    assert_eq!(resolved, first_target);
}

#[test]
fn feature_index_miss_does_not_need_dependency_roots_on_disk() {
    let index = RequireFeatureIndex::build(&[PathBuf::from("/missing/gem/lib")], None);
    assert!(index.lookup("still_missing").is_none());
    assert!(resolve_require_path(
        RequireKind::Require,
        "still_missing",
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &index,
        None,
    )
    .is_none());
}

#[test]
fn project_lib_still_wins_before_dependency_roots() {
    let dir = tempfile::tempdir().unwrap();
    let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
    let gem_target = gem_lib.join("foo.rb");
    let project_target = dir.path().join("lib/foo.rb");
    std::fs::create_dir_all(gem_target.parent().unwrap()).unwrap();
    std::fs::create_dir_all(project_target.parent().unwrap()).unwrap();
    std::fs::write(&gem_target, "# gem\n").unwrap();
    std::fs::write(&project_target, "# project\n").unwrap();
    let index = RequireFeatureIndex::build(&[gem_lib], None);

    let resolved = resolve_require_path(
        RequireKind::Require,
        "foo",
        &dir.path().join("main.rb"),
        dir.path(),
        &[],
        &index,
        None,
    )
    .unwrap();
    assert_eq!(resolved, project_target);
}

#[test]
fn location_spans_entire_target_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foo.rb");
    let content = "# line1\nclass Foo\nend\n";
    std::fs::write(&path, content).unwrap();

    let location = location_for_require_target(&path, None).unwrap();
    assert_eq!(location.range.start, Position::new(0, 0));
    assert_eq!(location.range.end, Position::new(3, 0));
}

#[test]
fn reresolve_clears_stored_require_when_dependency_root_appears() {
    let dir = tempfile::tempdir().unwrap();
    let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
    let target = gem_lib.join("platform/helpers/json.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# gem json\n").unwrap();

    let source = "require 'platform/helpers/json'\n";
    let file_id = SourceFileId(1);
    let current = dir.path().join("main.rb");
    let existing = unresolved_require_diagnostics(
        source,
        file_id,
        &current,
        dir.path(),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(existing.len(), 1);

    let index = RequireFeatureIndex::build(&[gem_lib], None);
    let refreshed = reresolve_unresolved_require_diagnostics(
        &current,
        dir.path(),
        &[],
        &index,
        None,
        &existing,
    );
    assert!(
        refreshed.is_empty(),
        "stored unresolved-require must clear once the gem root exists, got {refreshed:?}"
    );
    assert!(target.exists());
}

#[test]
fn reresolve_keeps_stored_require_when_path_is_still_missing() {
    let source = "require \"missing\"\n";
    let file_id = SourceFileId(1);
    let existing = unresolved_require_diagnostics(
        source,
        file_id,
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(existing.len(), 1);

    let refreshed = reresolve_unresolved_require_diagnostics(
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
        &existing,
    );
    assert_eq!(refreshed, existing);
}

#[test]
fn reresolve_clears_only_the_require_that_gained_a_root() {
    let dir = tempfile::tempdir().unwrap();
    let gem_lib = dir.path().join("gems/demo-1.0.0/lib");
    let target = gem_lib.join("found.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# found\n").unwrap();

    let source = "require \"found\"\nrequire \"still_missing\"\n";
    let file_id = SourceFileId(1);
    let current = dir.path().join("main.rb");
    let existing = unresolved_require_diagnostics(
        source,
        file_id,
        &current,
        dir.path(),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(existing.len(), 2);

    let index = RequireFeatureIndex::build(&[gem_lib], None);
    let refreshed = reresolve_unresolved_require_diagnostics(
        &current,
        dir.path(),
        &[],
        &index,
        None,
        &existing,
    );
    assert_eq!(refreshed.len(), 1);
    assert!(refreshed[0].message.contains("still_missing"));
}

#[test]
fn reresolve_keeps_require_relative_kind_from_the_stored_message() {
    let source = "require_relative \"./missing\"\n";
    let file_id = SourceFileId(1);
    let existing = unresolved_require_diagnostics(
        source,
        file_id,
        Path::new("/project/app/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(existing.len(), 1);
    assert!(existing[0].message.contains("require_relative"));

    let refreshed = reresolve_unresolved_require_diagnostics(
        Path::new("/project/app/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
        &existing,
    );
    assert_eq!(refreshed, existing);
}

#[test]
fn reresolve_clears_from_engine_indexed_files_without_disk() {
    let mut engine = Project::new();
    let gem_lib = PathBuf::from("/gems/demo-1.0.0/lib");
    let path = gem_lib.join("platform/helpers/json.rb");
    engine.register_file(SourceFileInput {
        path: path.clone(),
        content: "# json\n".to_string(),
        kind: SourceKind::Project,
    });

    let source = "require 'platform/helpers/json'\n";
    let existing = unresolved_require_diagnostics(
        source,
        SourceFileId(1),
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &RequireFeatureIndex::empty(),
        None,
    );
    assert_eq!(existing.len(), 1);

    let index = RequireFeatureIndex::build(&[gem_lib], Some(&engine.view()));
    let refreshed = reresolve_unresolved_require_diagnostics(
        Path::new("/project/main.rb"),
        Path::new("/project"),
        &[],
        &index,
        Some(&engine.view()),
        &existing,
    );
    assert!(
        refreshed.is_empty(),
        "engine-indexed gem files must clear stored unresolved-require without a disk hit, got {refreshed:?}"
    );
}

#[test]
fn engine_present_build_skips_unindexed_disk_files() {
    let dir = tempfile::tempdir().unwrap();
    let gem_lib = dir.path().join("lib");
    let disk_only = gem_lib.join("disk_only.rb");
    let engine_path = gem_lib.join("in_engine.rb");
    std::fs::create_dir_all(&gem_lib).unwrap();
    std::fs::write(&disk_only, "# disk\n").unwrap();
    std::fs::write(&engine_path, "# engine\n").unwrap();
    let mut engine = Project::new();
    engine.register_file(SourceFileInput {
        path: engine_path.clone(),
        content: "# engine\n".to_string(),
        kind: SourceKind::Gem,
    });
    let index = RequireFeatureIndex::build(&[gem_lib], Some(&engine.view()));
    assert_eq!(index.lookup("in_engine"), Some(engine_path.as_path()));
    assert!(
        index.lookup("disk_only").is_none(),
        "publish-time index must not readdir unindexed gem files"
    );
}
