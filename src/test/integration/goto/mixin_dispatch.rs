//! Calls inside a mixin dispatch on the including object, including its overrides.

use crate::test::harness::{check_multi_file, FakeEditor};
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, RubyMethod, RubyType};
use tower_lsp::lsp_types::{Location, Url};

const FEATURE: &str = "module Feature
  include Defaults
  def render
    registry
  end
end
";
const DEFAULTS: &str = "module Defaults
  protected
  def registry
    :default
  end
end
";
const HOST: &str = "class Host
  include Feature
  def registry
    :custom
  end
end
";

fn assert_targets(locations: &[Location], expected: &[(&str, u32)]) {
    let mut actual = locations
        .iter()
        .map(|location| (location.uri.clone(), location.range.start.line))
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = expected
        .iter()
        .map(|(file, line)| (Url::parse(&format!("file:///{file}")).unwrap(), *line))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        actual, expected,
        "navigation must follow each known receiver's Ruby lookup order"
    );
}

#[tokio::test]
async fn mixin_dispatch_follows_host_override_after_edits() {
    let mut editor = FakeEditor::new().await;
    editor.open("defaults.rb", DEFAULTS).await;
    editor.open("feature.rb", FEATURE).await;
    editor.open("host.rb", HOST).await;

    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("host.rb", 2)],
    );
    assert!(
        editor
            .references_at("host.rb", 2, 7)
            .await
            .iter()
            .any(|location| {
                location.uri == Url::parse("file:///feature.rb").unwrap()
                    && location.range.start.line == 3
            }),
        "references must agree with the effective override"
    );

    editor
        .set("host.rb", "class Host\n  include Feature\nend\n")
        .await;
    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("defaults.rb", 2)],
    );

    editor.set("host.rb", HOST).await;
    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("host.rb", 2)],
    );
}

#[tokio::test]
async fn mixin_dispatch_type_uses_the_override() {
    check_multi_file(&[
        (
            "defaults.rb",
            "module Defaults\n  def registry\n    7\n  end\nend\n",
        ),
        (
            "feature.rb",
            r#"module Feature
  include Defaults
  def rend<type label="String" kind="return">er
    registry
  end
end
"#,
        ),
        (
            "host.rb",
            "class Host\n  include Feature\n  def registry\n    \"custom\"\n  end\nend\n",
        ),
    ])
    .await;
}

#[tokio::test]
async fn mixin_dispatch_unknown_override_does_not_reuse_default_type() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "defaults.rb",
            "module Defaults\n  def registry\n    \"default\"\n  end\nend\n",
        )
        .await;
    editor.open("feature.rb", FEATURE).await;
    editor
        .open("host.rb", "class Host\n  include Feature\nend\n")
        .await;
    editor
        .open("other.rb", "class Other\n  include Feature\nend\n")
        .await;

    // Observe the engine's public return query after real editor operations.
    // Both receivers initially reach the same declaration; that is not a cycle.
    let return_type = |editor: &FakeEditor| {
        let engine = editor
            .server()
            .analysis_engine_for_uri(&Url::parse("file:///feature.rb").unwrap());
        let engine = engine.read();
        engine.query().method_return_type_for_receiver(
            &FullyQualifiedName::namespace(vec![RubyConstant::new("Feature").unwrap()]),
            &RubyMethod::new("registry").unwrap(),
        )
    };
    assert_eq!(return_type(&editor), Some(RubyType::string()));

    editor
        .set(
            "host.rb",
            "class Host\n  include Feature\n  def registry\n    unknown_value\n  end\nend\n",
        )
        .await;
    assert_eq!(
        return_type(&editor),
        None,
        "an unknown override must block a partial return type from another receiver"
    );

    editor
        .set("host.rb", "class Host\n  include Feature\nend\n")
        .await;
    assert_eq!(return_type(&editor), Some(RubyType::string()));
}

#[tokio::test]
async fn mixin_dispatch_retains_distinct_receivers_without_unrelated_methods() {
    let mut editor = FakeEditor::new().await;
    editor.open("defaults.rb", DEFAULTS).await;
    editor.open("feature.rb", FEATURE).await;
    editor.open("host.rb", HOST).await;
    editor
        .open("other.rb", "class Other\n  include Feature\nend\n")
        .await;
    editor
        .open(
            "unrelated.rb",
            "class Unrelated\n  def registry\n  end\nend\n",
        )
        .await;

    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("defaults.rb", 2), ("host.rb", 2)],
    );
}

#[tokio::test]
async fn mixin_dispatch_preserves_prepend_priority_over_host_override() {
    let mut editor = FakeEditor::new().await;
    editor.open("defaults.rb", DEFAULTS).await;
    editor.open("feature.rb", FEATURE).await;
    editor
        .open(
            "host.rb",
            HOST.replace("include Feature", "prepend Feature").as_str(),
        )
        .await;

    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("defaults.rb", 2)],
    );
}

#[tokio::test]
async fn mixin_dispatch_reflection_uses_the_named_module() {
    let mut editor = FakeEditor::new().await;
    editor.open("defaults.rb", DEFAULTS).await;
    editor.open("feature.rb", FEATURE).await;
    editor.open("host.rb", HOST).await;
    editor
        .open("reflection.rb", "Feature.instance_method(:registry)\n")
        .await;

    assert_targets(
        &editor.goto_def_at("reflection.rb", 0, 26).await,
        &[("defaults.rb", 2)],
    );
    editor.set("defaults.rb", "module Defaults\nend\n").await;
    assert!(
        editor.goto_def_at("reflection.rb", 0, 26).await.is_empty(),
        "reflection must not borrow a method that exists only on an includer"
    );
    assert_targets(
        &editor.goto_def_at("feature.rb", 3, 5).await,
        &[("host.rb", 2)],
    );
    editor.set("defaults.rb", DEFAULTS).await;
    assert_targets(
        &editor.goto_def_at("reflection.rb", 0, 26).await,
        &[("defaults.rb", 2)],
    );
}
