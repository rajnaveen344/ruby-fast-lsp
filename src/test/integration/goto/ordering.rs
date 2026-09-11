//! Ruby lookup precedence ranks definitions; source order only breaks ties.

use crate::query::EngineQuery;
use crate::test::harness::FakeEditor;
use ruby_analysis::core::{NamespaceKind, RubyConstant, RubyMethod};
use ruby_analysis::indexer::MethodReceiver;
use tower_lsp::lsp_types::{Location, Position, Url};

fn destinations(locations: Vec<Location>) -> Vec<(String, u32, u32)> {
    locations
        .into_iter()
        .map(|location| {
            (
                location.uri.path().to_owned(),
                location.range.start.line,
                location.range.start.character,
            )
        })
        .collect()
}

fn files(locations: Vec<Location>) -> Vec<String> {
    locations
        .into_iter()
        .map(|location| location.uri.path().to_owned())
        .collect()
}

#[tokio::test]
async fn semantic_definition_order_handles_union_receivers_and_singleton_methods() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "a_parent.rb",
            "class Parent\n  def label; end\n  def self.label; end\nend\n",
        )
        .await;
    editor
        .open(
            "z_child.rb",
            "class Child < Parent\n  def label; end\n  def self.label; end\nend\n",
        )
        .await;
    editor.open("use.rb", "def instance(flag)\n  receiver = flag ? Parent.new : Child.new\n  receiver.label\nend\ndef singleton(flag)\n  receiver = flag ? Parent : Child\n  receiver.label\nend\n").await;
    for (line, definition_line) in [(2, 1), (6, 2)] {
        let locations = editor.goto_def_at("use.rb", line, 12).await;
        assert_eq!(
            destinations(locations),
            vec![
                ("/z_child.rb".to_owned(), definition_line, 2),
                ("/a_parent.rb".to_owned(), definition_line, 2)
            ],
            "union navigation must rank the right namespace kind without losing either receiver"
        );
    }
}

#[tokio::test]
async fn semantic_definition_order_does_not_rank_through_missing_dependencies() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "feature.rb",
            "module Feature\n  def run\n    label\n  end\nend\n",
        )
        .await;
    editor
        .open("a_left.rb", "module Left\n  def label; end\nend\n")
        .await;
    editor
        .open("z_right.rb", "module Right\n  def label; end\nend\n")
        .await;
    editor.open("hosts.rb", "class LeftHost\n  include Feature, Left\nend\nclass RightHost\n  include Feature, Right\nend\nclass Pair\n  include Feature\n  include Left\n  include MissingLayer\n  include Right\nend\n").await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/a_left.rb", "/z_right.rb"],
        "partial lookup evidence cannot establish semantic priority"
    );
    editor
        .open("missing.rb", "module MissingLayer; end\n")
        .await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/z_right.rb", "/a_left.rb"],
        "new dependency facts should establish priority through the normal lifecycle"
    );
}

#[tokio::test]
async fn semantic_definition_order_prefers_overrides_and_tracks_edits() {
    let feature = "module Feature\n  def run\n    label\n  end\nend\n";
    let parent = "class Parent\n  include Feature\n  def label; :parent; end\nend\n";
    let child = "class Child < Parent\n  include Feature\n  def label\n    super\n  end\nend\n";
    for order in [[0, 1], [1, 0]] {
        let mut editor = FakeEditor::new().await;
        editor.open("feature.rb", feature).await;
        let sources = [("a_parent.rb", parent), ("z_child.rb", child)];
        for index in order {
            editor.open(sources[index].0, sources[index].1).await;
        }
        assert_eq!(
            files(editor.goto_def_at("feature.rb", 2, 5).await),
            ["/z_child.rb", "/a_parent.rb"]
        );
        let engine = editor
            .server()
            .analysis_engine_for_uri(&Url::parse("file:///feature.rb").unwrap());
        assert_eq!(
            files(
                EngineQuery::with_engine(engine)
                    .find_method_definitions(
                        &MethodReceiver::None,
                        &RubyMethod::new("label").unwrap(),
                        &[RubyConstant::new("Feature").unwrap()],
                        NamespaceKind::Instance,
                        Position::new(0, 0),
                    )
                    .unwrap()
            ),
            ["/z_child.rb", "/a_parent.rb"],
            "fallback navigation must preserve the same semantic order"
        );
        assert_eq!(
            files(editor.goto_def_at("z_child.rb", 3, 5).await),
            ["/a_parent.rb"]
        );
        editor.open("use.rb", "Child.new.label\n").await;
        assert_eq!(
            files(editor.goto_def_at("use.rb", 0, 12).await),
            ["/z_child.rb"]
        );

        editor
            .set(
                "z_child.rb",
                "class Child < Parent\n  include Feature\nend\n",
            )
            .await;
        assert_eq!(
            files(editor.goto_def_at("feature.rb", 2, 5).await),
            ["/a_parent.rb"]
        );
        editor.set("z_child.rb", child).await;
        assert_eq!(
            files(editor.goto_def_at("feature.rb", 2, 5).await),
            ["/z_child.rb", "/a_parent.rb"]
        );
    }
}

#[tokio::test]
async fn semantic_definition_order_uses_the_receivers_prepend_chain() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "feature.rb",
            "module Feature\n  def run\n    label\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "a_parent.rb",
            "class Parent\n  include Feature\n  def label; end\nend\n",
        )
        .await;
    editor
        .open("z_wrapper.rb", "module Wrapper\n  def label; end\nend\n")
        .await;
    editor
        .open(
            "decorated.rb",
            "class Decorated < Parent\n  include Feature\n  prepend Wrapper\nend\n",
        )
        .await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/z_wrapper.rb", "/a_parent.rb"]
    );
    editor
        .open("reflection.rb", "Parent.instance_method(:label)\n")
        .await;
    assert_eq!(
        files(editor.goto_def_at("reflection.rb", 0, 25).await),
        ["/a_parent.rb"]
    );
}

#[tokio::test]
async fn semantic_definition_order_preserves_conflicting_receiver_branches() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "feature.rb",
            "module Feature\n  def run\n    label\n  end\nend\n",
        )
        .await;
    editor
        .open("a_left.rb", "module Left\n  def label; end\nend\n")
        .await;
    editor
        .open("z_right.rb", "module Right\n  def label; end\nend\n")
        .await;
    editor.open("hosts.rb", "class LeftHost\n  include Feature, Left\nend\nclass RightHost\n  include Feature, Right\nend\nclass Pair\n  include Feature\n  include Left\n  include Right\nend\n").await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/z_right.rb", "/a_left.rb"]
    );

    // This receiver is outside the call's possible hosts and cannot change ranking.
    editor
        .open(
            "unrelated.rb",
            "class Unrelated\n  include Right\n  include Left\nend\n",
        )
        .await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/z_right.rb", "/a_left.rb"]
    );
    editor
        .set(
            "unrelated.rb",
            "class Unrelated\n  include Feature\n  include Right\n  include Left\nend\n",
        )
        .await;
    assert_eq!(
        files(editor.goto_def_at("feature.rb", 2, 5).await),
        ["/a_left.rb", "/z_right.rb"],
        "opposing participating chains are tied, retaining both valid targets"
    );
}

#[tokio::test]
async fn definition_order_is_independent_of_open_order_and_edits() {
    let feature = "module Feature\n  def run\n    label\n  end\nend\n";
    let early_path = "class Zebra\n  include Feature\n  def label\n    :first\n  end\nend\n";
    let later_path = "class Alpha\n  include Feature\n  def label\n    :last\n  end\nend\n";
    let files = [("z_host.rb", later_path), ("a_host.rb", early_path)];
    let expected = vec![
        ("/a_host.rb".to_owned(), 2, 2),
        ("/z_host.rb".to_owned(), 2, 2),
    ];

    for order in [[0, 1], [1, 0]] {
        let mut editor = FakeEditor::new().await;
        editor.open("feature.rb", feature).await;
        for index in order {
            editor.open(files[index].0, files[index].1).await;
        }
        assert_eq!(
            destinations(editor.goto_def_at("feature.rb", 2, 5).await),
            expected,
            "definition order must follow source paths, independently of file IDs and receiver names"
        );

        editor.set("a_host.rb", &format!("\n{early_path}")).await;
        assert_eq!(
            destinations(editor.goto_def_at("feature.rb", 2, 5).await),
            vec![
                ("/a_host.rb".to_owned(), 3, 2),
                ("/z_host.rb".to_owned(), 2, 2)
            ]
        );
        editor.set("a_host.rb", early_path).await;
        assert_eq!(
            destinations(editor.goto_def_at("feature.rb", 2, 5).await),
            expected
        );
    }
}

#[tokio::test]
async fn definition_order_covers_reopened_constants_and_yard_types() {
    let files = [
        ("z_record.rb", "class Record\nend\n"),
        (
            "a_record.rb",
            "class Record; end; class Record; end\n\nclass Record\nend\n",
        ),
    ];
    let expected = vec![
        ("/a_record.rb".to_owned(), 0, 0),
        ("/a_record.rb".to_owned(), 0, 19),
        ("/a_record.rb".to_owned(), 2, 0),
        ("/z_record.rb".to_owned(), 0, 0),
    ];
    for order in [[0, 1], [1, 0]] {
        let mut editor = FakeEditor::new().await;
        for index in order {
            editor.open(files[index].0, files[index].1).await;
        }
        editor
            .open("use.rb", "Record\n# @return [Record]\ndef build\nend\n")
            .await;
        for (line, column) in [(0, 2), (1, 13)] {
            assert_eq!(
                destinations(editor.goto_def_at("use.rb", line, column).await),
                expected,
                "ordinary and documentation navigation must use path, line, and column order"
            );
        }
    }
}

#[tokio::test]
async fn definition_order_covers_method_lookup_without_document_facts() {
    let mut editor = FakeEditor::new().await;
    editor.open("feature.rb", "module Feature\nend\n").await;
    editor
        .open(
            "z_host.rb",
            "class Alpha\n  include Feature\n  def label; end\nend\n",
        )
        .await;
    editor
        .open(
            "a_host.rb",
            "class Zebra\n  include Feature\n  def label; end\nend\n",
        )
        .await;

    let engine = editor
        .server()
        .analysis_engine_for_uri(&Url::parse("file:///feature.rb").unwrap());
    let locations = EngineQuery::with_engine(engine)
        .find_method_definitions(
            &MethodReceiver::None,
            &RubyMethod::new("label").unwrap(),
            &[RubyConstant::new("Feature").unwrap()],
            NamespaceKind::Instance,
            Position::new(0, 0),
        )
        .expect("known module receivers must retain their definition targets");
    assert_eq!(
        destinations(locations),
        vec![
            ("/a_host.rb".to_owned(), 2, 2),
            ("/z_host.rb".to_owned(), 2, 2)
        ],
        "fallback lookup must order the combined destinations, independently of receiver names"
    );
}
