//! Incremental mixin re-resolution: mixin graph edges wire up correctly
//! regardless of the order in which files are opened / edited / closed.
//!
//! Mechanism: `RubyIndex::retry_unresolved_mixin_edges`, called after every
//! `resolve_mixins_for_uri`. Refs that fail to resolve at index time are
//! recorded as `UnresolvedMixinEdge`s and retried on each subsequent file update.

use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::Location;

// ============================================================================
// Helpers
// ============================================================================

fn filename_to_uri(name: &str) -> tower_lsp::lsp_types::Url {
    tower_lsp::lsp_types::Url::parse(&format!("file:///{name}")).unwrap()
}

fn assert_hits_file(locs: &[Location], expected_filename: &str) {
    let expected = filename_to_uri(expected_filename);
    assert!(!locs.is_empty(), "expected ≥1 location, got none");
    assert!(
        locs.iter().any(|l| l.uri == expected),
        "expected a location in {expected_filename}, got {locs:?}"
    );
}

// ============================================================================
// Include ordering
// ============================================================================

/// ClassA indexed first with `include M_A`, then M_A opens and references
/// `services` (defined on ClassA). The unresolved include edge must retry
/// and wire up so the ancestor chain reaches ClassA.
#[tokio::test]
async fn include_class_first_then_module() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// Reverse order sanity regression: M_A opens first, ClassA second. Always
/// worked via the batched resolution path; this guards against retry-machinery
/// regressions.
#[tokio::test]
async fn include_module_first_then_class() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

// ============================================================================
// Prepend / Extend / Superclass ordering
// ============================================================================

#[tokio::test]
async fn prepend_class_first_then_module() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  prepend M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// `extend M` on ClassA, module defined later. Extend is modeled as ClassA's
/// singleton including M's instance namespace. Class method lookup from within
/// the extended module should reach ClassA's class methods.
#[tokio::test]
async fn extend_class_first_then_module() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  extend M_A\n  def self.services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// `class Child < Parent` with Parent defined later.
#[tokio::test]
async fn superclass_child_first_then_parent() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "child.rb",
            "class Child < Parent\n  def use_it\n    work\n  end\nend\n",
        )
        .await;
    editor
        .open("parent.rb", "class Parent\n  def work\n  end\nend\n")
        .await;

    let locs = editor.goto_def_at("child.rb", 2, 4).await;
    assert_hits_file(&locs, "parent.rb");
}

// ============================================================================
// Path variants
// ============================================================================

/// Nested constant path: `include Outer::Inner` where `Outer::Inner` opens later.
#[tokio::test]
async fn nested_path_include() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include Outer::Inner\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "outer_inner.rb",
            "module Outer\n  module Inner\n    def foo\n      services\n    end\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("outer_inner.rb", 3, 6).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// Absolute path `include ::M_A`.
#[tokio::test]
async fn absolute_path_include() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include ::M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

// ============================================================================
// Chain resolution
// ============================================================================

/// ClassA includes M_A, M_A includes M_B. M_B is defined last. A method from
/// ClassA called inside M_B must goto ClassA via the transitive chain.
#[tokio::test]
async fn chain_resolution_late_leaf() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def deep\n  end\nend\n",
        )
        .await;
    editor
        .open("m_a.rb", "module M_A\n  include M_B\nend\n")
        .await;
    editor
        .open(
            "m_b.rb",
            "module M_B\n  def call_deep\n    deep\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_b.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

// ============================================================================
// Unresolved-edge persistence & safety
// ============================================================================

/// Typo — target never appears. Indexing must tolerate it without crashing.
#[tokio::test]
async fn typo_stays_unresolved_no_crash() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include Mispelled\n  def services\n  end\nend\n",
        )
        .await;
}

/// Editing the includer clears its stale pending edges before re-adding.
#[tokio::test]
async fn reindex_clears_stale_unresolved() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include Nope1\n  include Nope2\nend\n",
        )
        .await;
    editor
        .set("class_a.rb", "class ClassA\n  include OnlyOne\nend\n")
        .await;
    editor
        .open(
            "only.rb",
            "module OnlyOne\n  def foo\n    bar\n  end\nend\n",
        )
        .await;
    editor
        .set(
            "class_a.rb",
            "class ClassA\n  include OnlyOne\n  def bar\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("only.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// Closing a file preserves its indexed state — goto still works across
/// mixin boundary after close + reopen.
#[tokio::test]
async fn close_preserves_index_state() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;
    editor.close("m_a.rb").await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// Deleting the module definition demotes the dependent edge back to unresolved.
/// Re-adding the module re-wires the chain via the retry pass. Exercises the
/// `dependent_uris` demotion path in `remove_entries_for_uri`.
#[tokio::test]
async fn delete_then_readd_module_rewires() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;
    let m_a = "module M_A\n  def foo\n    services\n  end\nend\n";
    editor.open("m_a.rb", m_a).await;

    // Sanity: live before deletion.
    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");

    // Wipe M_A's contents (simulates removing the module definition).
    editor.set("m_a.rb", "\n").await;
    // Re-add.
    editor.set("m_a.rb", m_a).await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert_hits_file(&locs, "class_a.rb");
}

/// Unresolved refs yield empty goto, not ghost locations.
#[tokio::test]
async fn unresolved_yields_no_ghost_locations() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    missing_method\n  end\nend\n",
        )
        .await;

    let locs = editor.goto_def_at("m_a.rb", 2, 4).await;
    assert!(locs.is_empty(), "expected no definitions, got {locs:?}");
}

// ============================================================================
// Cross-feature: retry benefits hover, references, multi-includer scenarios
// ============================================================================

/// `find-references` across a mixin boundary resolves after the late-indexed
/// module opens.
#[tokio::test]
async fn references_span_late_indexed_module() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    // Position on `services` definition in ClassA.
    let refs = editor.references_at("class_a.rb", 2, 6).await;
    let m_a_uri = filename_to_uri("m_a.rb");
    assert!(
        refs.iter().any(|loc| loc.uri == m_a_uri),
        "expected a reference in m_a.rb, got {refs:?}"
    );
}

/// Hover on a method call inside a late-indexed module resolves via the
/// retried include edge.
#[tokio::test]
async fn hover_spans_late_indexed_module() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "class_a.rb",
            "class ClassA\n  include M_A\n  def services\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "m_a.rb",
            "module M_A\n  def foo\n    services\n  end\nend\n",
        )
        .await;

    let hover = editor.hover_at("m_a.rb", 2, 4).await;
    assert!(hover.is_some(), "hover should resolve to method info");
}

/// Three unrelated classes all include the same late-indexed module. Opening
/// the module triggers a single retry pass that resolves all three edges.
#[tokio::test]
async fn multiple_pending_refs_resolve_together() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("a.rb", "class A\n  include M\n  def a1\n  end\nend\n")
        .await;
    editor
        .open("b.rb", "class B\n  include M\n  def b1\n  end\nend\n")
        .await;
    editor
        .open("c.rb", "class C\n  include M\n  def c1\n  end\nend\n")
        .await;
    editor
        .open(
            "m.rb",
            "module M\n  def call_all\n    a1\n    b1\n    c1\n  end\nend\n",
        )
        .await;

    assert_hits_file(&editor.goto_def_at("m.rb", 2, 4).await, "a.rb");
    assert_hits_file(&editor.goto_def_at("m.rb", 3, 4).await, "b.rb");
    assert_hits_file(&editor.goto_def_at("m.rb", 4, 4).await, "c.rb");
}
