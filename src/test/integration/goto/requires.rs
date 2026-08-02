//! Goto definition and unresolved diagnostics for `require` / `require_relative`.

use crate::config::{LoadPathsConfig, ProjectLoadPaths};
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{Location, NumberOrString, Position, Url};

fn filename_to_uri(name: &str) -> Url {
    Url::parse(&format!("file:///{name}")).unwrap()
}

fn assert_hits_file(locs: &[Location], expected_filename: &str) {
    let expected = filename_to_uri(expected_filename);
    assert!(!locs.is_empty(), "expected ≥1 location, got none");
    assert!(
        locs.iter().any(|l| l.uri == expected),
        "expected a location in {expected_filename}, got {locs:?}"
    );
}

#[tokio::test]
async fn goto_require_relative_sibling() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor
        .open("project/app/foo.rb", "# foo target\n")
        .await;
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
        .open(
            "project/lib/platform/helpers/json.rb",
            "# json helpers\n",
        )
        .await;
    // require 'platform/helpers/json'
    // 0123456789012345678901234567890
    editor
        .open(
            "project/main.rb",
            "require 'platform/helpers/json'\n",
        )
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
        .open(
            "project/lib/platform/helpers/json.rb",
            "# json helpers\n",
        )
        .await;
    editor
        .open(
            "project/main.rb",
            "require 'platform/helpers/json'\n",
        )
        .await;

    let hover = editor
        .hover_at("project/main.rb", 0, 26)
        .await
        .expect("require hover");
    let range = hover.range.expect("require hover must set the content range");
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
        .set_dependency_require_paths(vec![gem_lib]);
    editor
        .open(
            "project/main.rb",
            "require 'platform/helpers/json'\n",
        )
        .await;

    let locs = editor.goto_def_at("project/main.rb", 0, 26).await;
    assert!(
        locs.iter().any(|location| {
            location
                .uri
                .to_file_path()
                .ok()
                .as_ref()
                == Some(&target)
        }),
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
        .open(
            "project/main.rb",
            "require 'platform/helpers/json'\n",
        )
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
    workspace.set_dependency_require_paths(vec![gem_lib]);
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
        locs.iter().any(|location| {
            location.uri.to_file_path().ok().as_ref() == Some(&target)
        }),
        "require must resolve after dependency roots are published, got {locs:?}"
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
    editor.server().config.lock().indexing.load_paths = LoadPathsConfig {
        default: Vec::new(),
        projects: vec![ProjectLoadPaths {
            root: "/project".to_string(),
            paths: vec!["custom".to_string()],
        }],
    };
    editor
        .open("project/custom/foo.rb", "# custom foo\n")
        .await;
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
    editor.server().config.lock().indexing.load_paths = LoadPathsConfig {
        default: Vec::new(),
        projects: vec![
            ProjectLoadPaths {
                root: "/server".to_string(),
                paths: vec!["custom".to_string()],
            },
            ProjectLoadPaths {
                root: "/admin".to_string(),
                paths: vec!["other".to_string()],
            },
        ],
    };

    editor
        .open("server/custom/foo.rb", "# server custom\n")
        .await;
    editor.open("server/lib/foo.rb", "# server lib\n").await;
    editor.open("server/main.rb", "require \"foo\"\n").await;

    editor
        .open("admin/other/foo.rb", "# admin other\n")
        .await;
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
    editor.server().config.lock().indexing.load_paths = LoadPathsConfig {
        default: vec!["shared".to_string()],
        projects: Vec::new(),
    };
    editor
        .open("project/shared/foo.rb", "# shared foo\n")
        .await;
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
    editor
        .open("project/main.rb", "require \"a#{b}\"\n")
        .await;

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
    editor
        .open("project/main.rb", "require \"a#{b}\"\n")
        .await;

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
        .open(
            "project/main.rb",
            "class Foo\nend\n\nFoo.new\n",
        )
        .await;

    // Cursor on Foo in Foo.new (line 3)
    let locs = editor.goto_def_at("project/main.rb", 3, 0).await;
    assert!(!locs.is_empty(), "class goto must remain available");
}
