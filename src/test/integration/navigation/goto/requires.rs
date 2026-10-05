//! Goto definition and unresolved diagnostics for `require` / `require_relative`.

use crate::environment::config::{LoadPathsConfig, ProjectLoadPaths};
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{Location, NumberOrString, Position, Url};

fn filename_to_uri(name: &str) -> Url {
    crate::test::harness::fixture_uri(format!("/{name}"))
}

fn assert_hits_file(locs: &[Location], expected_filename: &str) {
    let expected = filename_to_uri(expected_filename);
    assert!(!locs.is_empty(), "expected ≥1 location, got none");
    assert!(
        locs.iter().any(|l| l.uri == expected),
        "expected a location in {expected_filename}, got {locs:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cold_runtime_require_roots_refresh_open_diagnostics_and_preserve_project_precedence() {
    use crate::environment::config::runtime::{
        ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
        SelectedRuntimeDescriptor,
    };
    use crate::environment::config::RubyFastLspConfig;
    use crate::environment::runtime::catalog::{RuntimeDiscoverySource, RuntimeImplementation};
    use crate::loader::coordinator::IndexingCoordinator;
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(fixture.path()).unwrap();
    let root = base.join("project");
    let runtime = base.join("runtime");
    let stdlib = runtime.join("lib/ruby/stdlib");
    let default_gem = runtime.join("lib/ruby/gems/uri/lib");
    for directory in [
        &root.join("lib"),
        &runtime.join("bin"),
        &stdlib,
        &default_gem,
    ] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let json = stdlib.join("json.rb");
    let uri = default_gem.join("uri.rb");
    let local = root.join("lib/shared.rb");
    for file in [&json, &uri, &local, &stdlib.join("shared.rb")] {
        std::fs::write(file, "# require target\n").unwrap();
    }
    let executable = runtime.join("bin/ruby");
    std::fs::write(&executable, concat!(
        "#!/bin/sh\n",
        "runtime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\n",
        "printf 'probe\\n' >> \"$runtime_root/probe-count\"\n",
        "printf '%s\\0' \"$runtime_root/lib/ruby/stdlib\" \"$runtime_root/lib/ruby/gems/uri/lib\"\n",
    )).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root.to_str().unwrap().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Mri,
                    family: "3.3".to_string(),
                    engine_version: "3.3.11".to_string(),
                    compatibility_version: "3.3".to_string(),
                    executable,
                    discovery_source: RuntimeDiscoverySource::Path,
                    java_home: None,
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    let source = "require 'json'\nrequire 'uri'\nrequire 'still_missing'\nrequire 'shared'\nclass Service\n  def value; 'ready'; end\nend\nService.new.value\n";
    let main = root.join("main.rb");
    std::fs::write(&main, source).unwrap();
    let filename = main.to_str().unwrap().trim_start_matches('/');
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(root.to_str().unwrap().trim_start_matches('/'));
    editor.server().replace_configuration(config.clone());
    editor.open(filename, source).await;
    let unresolved_lines = |diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>| {
        diagnostics.into_iter().filter(|diagnostic| {
            matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }).map(|diagnostic| diagnostic.range.start.line).collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        unresolved_lines(editor.published_diagnostics(filename)),
        [0, 1, 2].into_iter().collect()
    );

    let ctx = editor.server().load_context_for_project(&root);
    IndexingCoordinator::new(root, config)
        .run_complete_indexing(&ctx)
        .await
        .unwrap();

    assert_eq!(
        unresolved_lines(editor.published_diagnostics(filename)),
        [2].into_iter().collect(),
        "cold indexing must clear resolved imports without an edit and retain a true miss"
    );
    for (line, target) in [(0, &json), (1, &uri), (3, &local)] {
        let locations = editor.goto_def_at(filename, line, 10).await;
        assert_eq!(
            locations.len(),
            1,
            "one deterministic require target: {locations:?}"
        );
        assert_eq!(locations[0].uri.to_file_path().unwrap(), *target);
    }
    assert_eq!(std::fs::read_to_string(runtime.join("probe-count")).unwrap().lines().count(), 1,
        "standalone projects must use one exact runtime load-path probe and no global gem discovery");
}

/// Any feature the selected runtime's stdlib ships resolves, not only a
/// curated list of well-known module names. Nested features resolve by path,
/// and the required file's declarations join the project.
#[cfg(unix)]
#[tokio::test]
async fn runtime_stdlib_resolves_every_shipped_feature() {
    use crate::environment::config::runtime::{
        ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
        SelectedRuntimeDescriptor,
    };
    use crate::environment::config::RubyFastLspConfig;
    use crate::environment::runtime::catalog::{RuntimeDiscoverySource, RuntimeImplementation};
    use crate::loader::coordinator::IndexingCoordinator;
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(fixture.path()).unwrap();
    let root = base.join("project");
    let runtime = base.join("runtime");
    let stdlib = runtime.join("lib/ruby/stdlib");
    for directory in [&root, &runtime.join("bin"), &stdlib.join("toolkit")] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let feature = stdlib.join("runtime_only_feature.rb");
    let nested = stdlib.join("toolkit/part.rb");
    std::fs::write(&feature, "module RuntimeOnlyFeature\nend\n").unwrap();
    std::fs::write(&nested, "module Toolkit\n  class Part\n  end\nend\n").unwrap();
    std::fs::write(runtime.join("lib/ruby/outside.rb"), "# not a feature\n").unwrap();
    let executable = runtime.join("bin/ruby");
    std::fs::write(
        &executable,
        concat!(
            "#!/bin/sh\n",
            "runtime_root=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\n",
            "printf '%s\\0' \"$runtime_root/lib/ruby/stdlib\"\n",
        ),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root.to_str().unwrap().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Mri,
                    family: "3.3".to_string(),
                    engine_version: "3.3.11".to_string(),
                    compatibility_version: "3.3".to_string(),
                    executable,
                    discovery_source: RuntimeDiscoverySource::Path,
                    java_home: None,
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    let source = "require 'runtime_only_feature'\nrequire 'toolkit/part'\nrequire '../outside'\nRuntimeOnlyFeature\nToolkit::Part\n";
    let main = root.join("main.rb");
    std::fs::write(&main, source).unwrap();
    let filename = main.to_str().unwrap().trim_start_matches('/');
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(root.to_str().unwrap().trim_start_matches('/'));
    editor.server().replace_configuration(config.clone());
    editor.open(filename, source).await;

    let ctx = editor.server().load_context_for_project(&root);
    IndexingCoordinator::new(root, config)
        .run_complete_indexing(&ctx)
        .await
        .unwrap();

    let unresolved = editor
        .published_diagnostics(filename)
        .into_iter()
        .filter(|diagnostic| {
            matches!(
                &diagnostic.code,
                Some(NumberOrString::String(code))
                    if code == "unresolved-require" || code == "unresolved-constant"
            )
        })
        .map(|diagnostic| diagnostic.range.start.line)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        unresolved,
        [2].into_iter().collect(),
        "shipped stdlib features resolve with their declarations; a path outside the load path does not"
    );
    for (line, target) in [(0, &feature), (1, &nested)] {
        let locations = editor.goto_def_at(filename, line, 12).await;
        assert_eq!(
            locations.len(),
            1,
            "one deterministic require target: {locations:?}"
        );
        assert_eq!(locations[0].uri.to_file_path().unwrap(), *target);
    }
}

#[tokio::test]
async fn goto_require_relative_sibling() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/app/foo.rb", "# foo target\n").await;
    editor
        .open("project/app/main.rb", "require_relative \"./foo\"\n")
        .await;

    // Cursor on the string contents: require_relative "./foo"
    //                                 01234567890123456789012
    let locs = editor.goto_def_at("project/app/main.rb", 0, 20).await;
    assert_hits_file(&locs, "project/app/foo.rb");
}

#[tokio::test]
async fn goto_require_selects_entire_target_file() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    let target = "# line one\nclass Foo\nend\n";
    editor.open("project/lib/foo.rb", target).await;
    editor.open("project/main.rb", "require \"foo\"\n").await;

    let locs = editor.goto_def_at("project/main.rb", 0, 10).await;
    let hit = locs
        .iter()
        .find(|l| l.uri == filename_to_uri("project/lib/foo.rb"))
        .expect("expected lib/foo.rb location");
    assert_eq!(hit.range.start, Position::new(0, 0));
    assert_eq!(hit.range.end, Position::new(3, 0));
}

#[tokio::test]
async fn goto_require_origin_covers_string_contents_only() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/lib/platform/helpers/json.rb", "# json helpers\n")
        .await;
    // require 'platform/helpers/json'
    // 0123456789012345678901234567890
    editor
        .open("project/main.rb", "require 'platform/helpers/json'\n")
        .await;

    // Cursor on the final path segment "json"
    let links = editor.goto_def_links_at("project/main.rb", 0, 26).await;
    assert_eq!(links.len(), 1);
    let origin = links[0]
        .origin_selection_range
        .expect("require definition must advertise the string-content origin");
    assert_eq!(origin.start, Position::new(0, 9));
    assert_eq!(origin.end, Position::new(0, 30));
}

#[tokio::test]
async fn hover_require_range_covers_string_contents_only() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/lib/platform/helpers/json.rb", "# json helpers\n")
        .await;
    editor
        .open("project/main.rb", "require 'platform/helpers/json'\n")
        .await;

    let hover = editor
        .hover_at("project/main.rb", 0, 26)
        .await
        .expect("require hover");
    let range = hover
        .range
        .expect("require hover must set the content range");
    assert_eq!(range.start, Position::new(0, 9));
    assert_eq!(range.end, Position::new(0, 30));
}

#[tokio::test]
async fn goto_require_uses_workspace_dependency_require_roots() {
    let gem = tempfile::tempdir().unwrap();
    let gem_lib = gem.path().join("lib");
    let target = gem_lib.join("platform/helpers/json.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# gem json\n").unwrap();

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .workspace_for("project/main.rb")
        .expect("project workspace")
        .handle()
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .open("project/main.rb", "require 'platform/helpers/json'\n")
        .await;

    let locs = editor.goto_def_at("project/main.rb", 0, 26).await;
    assert!(
        locs.iter()
            .any(|location| { location.uri.to_file_path().ok().as_ref() == Some(&target) }),
        "require must open the gem feature file under its require_paths, got {locs:?}"
    );
}

#[tokio::test]
async fn unresolved_require_clears_after_dependency_roots_without_edit() {
    let gem = tempfile::tempdir().unwrap();
    let gem_lib = gem.path().join("lib");
    let target = gem_lib.join("platform/helpers/json.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# gem json\n").unwrap();

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "require 'platform/helpers/json'\n")
        .await;

    let published_before = editor.published_diagnostics("project/main.rb");
    assert!(
        published_before.iter().any(|diag| {
            matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "before gem roots exist the open file must publish unresolved-require, got {published_before:?}"
    );

    let workspace = editor
        .workspace_for("project/main.rb")
        .expect("project workspace");
    workspace
        .handle()
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .server()
        .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
        .await;

    let published_after = editor.published_diagnostics("project/main.rb");
    assert!(
        published_after.iter().all(|diag| {
            !matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "dependency-root refresh must clear published unresolved-require without an edit, got {published_after:?}"
    );

    let locs = editor.goto_def_at("project/main.rb", 0, 26).await;
    assert!(
        locs.iter()
            .any(|location| { location.uri.to_file_path().ok().as_ref() == Some(&target) }),
        "require must resolve after dependency roots are published, got {locs:?}"
    );
}

#[tokio::test]
async fn unresolved_require_stays_after_refresh_when_still_missing() {
    let gem = tempfile::tempdir().unwrap();
    let gem_lib = gem.path().join("lib");
    std::fs::create_dir_all(&gem_lib).unwrap();

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "require 'still_missing'\n")
        .await;

    let workspace = editor
        .workspace_for("project/main.rb")
        .expect("project workspace");
    workspace
        .handle()
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .server()
        .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
        .await;

    let published = editor.published_diagnostics("project/main.rb");
    assert!(
        published.iter().any(|diag| {
            matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "a still-missing require must stay unresolved after dependency-root refresh, got {published:?}"
    );
}

#[tokio::test]
async fn unresolved_require_refresh_clears_only_resolved_requires_in_one_file() {
    let gem = tempfile::tempdir().unwrap();
    let gem_lib = gem.path().join("lib");
    let target = gem_lib.join("found.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# found\n").unwrap();

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open(
            "project/main.rb",
            "require 'found'\nrequire 'still_missing'\n",
        )
        .await;

    let workspace = editor
        .workspace_for("project/main.rb")
        .expect("project workspace");
    workspace
        .handle()
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .server()
        .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
        .await;

    let published = editor.published_diagnostics("project/main.rb");
    assert!(
        published
            .iter()
            .all(|diag| { !matches!(&diag.message, message if message.contains("found")) }),
        "the require that gained a gem root must clear, got {published:?}"
    );
    assert!(
        published.iter().any(|diag| {
            matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
                && diag.message.contains("still_missing")
        }),
        "the still-missing require must stay, got {published:?}"
    );
}

#[tokio::test]
async fn unresolved_require_clears_on_closed_file_after_dependency_roots() {
    let gem = tempfile::tempdir().unwrap();
    let gem_lib = gem.path().join("lib");
    let target = gem_lib.join("platform/helpers/json.rb");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "# gem json\n").unwrap();

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/app.rb", "require 'platform/helpers/json'\n")
        .await;

    let published_before = editor.published_diagnostics("project/app.rb");
    assert!(
        published_before.iter().any(|diag| {
            matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "closed-file refresh fixture must start unresolved, got {published_before:?}"
    );

    editor.close("project/app.rb").await;
    let workspace = editor
        .workspace_for("project/app.rb")
        .expect("project workspace");
    workspace
        .handle()
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .server()
        .refresh_unresolved_require_diagnostics_for_workspace(&workspace)
        .await;

    editor
        .open("project/app.rb", "require 'platform/helpers/json'\n")
        .await;
    let published_after = editor.published_diagnostics("project/app.rb");
    assert!(
        published_after.iter().all(|diag| {
            !matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "closed-file stored-fact refresh must clear unresolved-require without a reparse, got {published_after:?}"
    );
}

#[tokio::test]
async fn goto_require_lib_file() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/lib/foo.rb", "# lib foo\n").await;
    editor.open("project/main.rb", "require \"foo\"\n").await;

    // require "foo" — character inside foo
    let locs = editor.goto_def_at("project/main.rb", 0, 10).await;
    assert_hits_file(&locs, "project/lib/foo.rb");
}

#[tokio::test]
async fn goto_require_custom_load_path_wins_before_lib() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.server().update_configuration(|config| {
        config.indexing.load_paths = LoadPathsConfig {
            default: Vec::new(),
            projects: vec![ProjectLoadPaths {
                root: crate::test::harness::fixture_path("/project")
                    .to_string_lossy()
                    .into_owned(),
                paths: vec!["custom".to_string()],
            }],
        }
    });
    editor.open("project/custom/foo.rb", "# custom foo\n").await;
    editor.open("project/lib/foo.rb", "# lib foo\n").await;
    editor.open("project/main.rb", "require \"foo\"\n").await;

    let locs = editor.goto_def_at("project/main.rb", 0, 10).await;
    assert_hits_file(&locs, "project/custom/foo.rb");
}

#[tokio::test]
async fn goto_require_load_paths_are_isolated_per_project() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("server");
    editor.add_workspace("admin");
    editor.server().update_configuration(|config| {
        config.indexing.load_paths = LoadPathsConfig {
            default: Vec::new(),
            projects: vec![
                ProjectLoadPaths {
                    root: crate::test::harness::fixture_path("/server")
                        .to_string_lossy()
                        .into_owned(),
                    paths: vec!["custom".to_string()],
                },
                ProjectLoadPaths {
                    root: crate::test::harness::fixture_path("/admin")
                        .to_string_lossy()
                        .into_owned(),
                    paths: vec!["other".to_string()],
                },
            ],
        }
    });

    editor
        .open("server/custom/foo.rb", "# server custom\n")
        .await;
    editor.open("server/lib/foo.rb", "# server lib\n").await;
    editor.open("server/main.rb", "require \"foo\"\n").await;

    editor.open("admin/other/foo.rb", "# admin other\n").await;
    editor.open("admin/lib/foo.rb", "# admin lib\n").await;
    editor.open("admin/main.rb", "require \"foo\"\n").await;

    assert_hits_file(
        &editor.goto_def_at("server/main.rb", 0, 10).await,
        "server/custom/foo.rb",
    );
    assert_hits_file(
        &editor.goto_def_at("admin/main.rb", 0, 10).await,
        "admin/other/foo.rb",
    );
}

#[tokio::test]
async fn goto_require_falls_back_to_workspace_default_load_paths() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.server().update_configuration(|config| {
        config.indexing.load_paths = LoadPathsConfig {
            default: vec!["shared".to_string()],
            projects: Vec::new(),
        }
    });
    editor.open("project/shared/foo.rb", "# shared foo\n").await;
    editor.open("project/lib/foo.rb", "# lib foo\n").await;
    editor.open("project/main.rb", "require \"foo\"\n").await;

    let locs = editor.goto_def_at("project/main.rb", 0, 10).await;
    assert_hits_file(&locs, "project/shared/foo.rb");
}

#[tokio::test]
async fn goto_require_missing_path_returns_none() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "require \"missing\"\n")
        .await;

    let locs = editor.goto_def_at("project/main.rb", 0, 12).await;
    assert!(
        locs.is_empty(),
        "missing require target must not invent a location, got {locs:?}"
    );
}

#[tokio::test]
async fn unresolved_require_reports_diagnostic() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "require \"missing\"\n")
        .await;

    let diag = editor
        .assert_error_code("project/main.rb", "unresolved-require")
        .await;
    assert!(
        matches!(&diag.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
    );
    assert!(diag.message.contains("missing"));
    assert_eq!(diag.range.start, Position::new(0, 9));
    assert_eq!(diag.range.end, Position::new(0, 16));
}

#[tokio::test]
async fn unresolved_require_relative_reports_diagnostic() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/app/main.rb", "require_relative \"./missing\"\n")
        .await;

    editor
        .assert_error_code("project/app/main.rb", "unresolved-require")
        .await;
}

#[tokio::test]
async fn resolved_require_has_no_unresolved_require_diagnostic() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/lib/foo.rb", "# foo\n").await;
    editor.open("project/main.rb", "require \"foo\"\n").await;

    let diags = editor.diagnostics("project/main.rb").await;
    assert!(
        diags.iter().all(|d| {
            !matches!(&d.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "resolved require must stay silent, got {diags:?}"
    );
}

#[tokio::test]
async fn interpolated_require_has_no_unresolved_require_diagnostic() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/main.rb", "require \"a#{b}\"\n").await;

    let diags = editor.diagnostics("project/main.rb").await;
    assert!(
        diags.iter().all(|d| {
            !matches!(&d.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "interpolated require must fail closed without a diagnostic, got {diags:?}"
    );
}

#[tokio::test]
async fn autoload_is_not_diagnosed_as_unresolved_require() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "autoload :Foo, \"missing\"\n")
        .await;

    let diags = editor.diagnostics("project/main.rb").await;
    assert!(
        diags.iter().all(|d| {
            !matches!(&d.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
        }),
        "autoload stays out of v1 require diagnostics, got {diags:?}"
    );
}

#[tokio::test]
async fn goto_require_interpolated_string_returns_none() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/main.rb", "require \"a#{b}\"\n").await;

    let locs = editor.goto_def_at("project/main.rb", 0, 10).await;
    assert!(
        locs.is_empty(),
        "interpolated require must fail closed, got {locs:?}"
    );
}

#[tokio::test]
async fn goto_class_identifier_still_works() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/main.rb", "class Foo\nend\n\nFoo.new\n")
        .await;

    // Cursor on Foo in Foo.new (line 3)
    let locs = editor.goto_def_at("project/main.rb", 3, 0).await;
    assert!(!locs.is_empty(), "class goto must remain available");
}

/// A gemspec's literal `require_paths` put extra project folders on the load
/// path, so vendored jars and Ruby files there resolve like `lib`.
#[tokio::test]
async fn gemspec_require_paths_resolve_project_requires() {
    use crate::lsp::lifecycle::indexing::init_workspace_for_run;
    use std::time::Duration;

    let fixture = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(fixture.path()).unwrap().join("project");
    let jar = root.join("lib/jars/org/example/widget/1.0/widget-1.0.jar");
    std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
    std::fs::write(&jar, b"PK\x03\x04").unwrap();
    std::fs::write(
        root.join("example.gemspec"),
        "Gem::Specification.new do |spec|\n  spec.name = 'example'\n  spec.require_paths = ['lib'.freeze, 'lib/jars'.freeze]\nend\n",
    )
    .unwrap();
    let jars_source = "require 'org/example/widget/1.0/widget-1.0.jar'\nrequire 'org/example/absent/1.0/absent-1.0.jar'\n";
    let jars_file = root.join("lib/jars/example_jars.rb");
    std::fs::write(&jars_file, jars_source).unwrap();
    let main_source = "require 'example_jars'\n";
    let main_file = root.join("lib/example.rb");
    std::fs::write(&main_file, main_source).unwrap();

    let root_uri = Url::from_directory_path(&root).unwrap();
    let mut editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let workspace = server.add_workspace(root_uri.clone());
    tokio::time::timeout(
        Duration::from_secs(30),
        init_workspace_for_run(&server, root_uri, workspace.begin_indexing_run()),
    )
    .await
    .expect("cold indexing must finish")
    .expect("cold indexing must succeed");

    let jars_name = jars_file.to_str().unwrap().trim_start_matches('/');
    let main_name = main_file.to_str().unwrap().trim_start_matches('/');
    editor.open(jars_name, jars_source).await;
    editor.open(main_name, main_source).await;
    let unresolved = |diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>| {
        diagnostics
            .into_iter()
            .filter(|diagnostic| {
                matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-require")
            })
            .map(|diagnostic| diagnostic.range.start.line)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        unresolved(editor.diagnostics(jars_name).await),
        vec![1],
        "a jar under a declared require path resolves; a missing jar stays reported"
    );
    assert_eq!(
        unresolved(editor.diagnostics(main_name).await),
        Vec::<u32>::new(),
        "a Ruby file under a declared require path resolves"
    );
    let locations = editor.goto_def_at(jars_name, 0, 12).await;
    assert_eq!(
        locations
            .iter()
            .map(|location| location.uri.to_file_path().unwrap())
            .collect::<Vec<_>>(),
        vec![jar],
        "the jar require navigates to the vendored jar"
    );
}
