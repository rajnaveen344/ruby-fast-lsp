//! Multi-root workspace routing: project and dependency files map to the correct
//! isolated analysis engine without leaking semantic facts across roots.

use crate::test::harness::FakeEditor;
use ruby_analysis::core::SourceKind;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{PartialResultParams, WorkDoneProgressParams, WorkspaceSymbolParams};

#[tokio::test]
async fn each_workspace_gets_its_own_index() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");

    assert_eq!(editor.workspace_count(), 2);

    editor
        .open(
            "workspace_a/user.rb",
            "class User\n  def name_a; end\nend\n",
        )
        .await;
    editor
        .open(
            "workspace_b/user.rb",
            "class User\n  def name_b; end\nend\n",
        )
        .await;

    let ws_a = editor
        .workspace_for("workspace_a/user.rb")
        .expect("workspace_a should match the file URI");
    let ws_b = editor
        .workspace_for("workspace_b/user.rb")
        .expect("workspace_b should match the file URI");

    assert_ne!(
        ws_a.root_uri, ws_b.root_uri,
        "each file should land in its own workspace"
    );

    assert!(
        method_fact_in_path(editor.server(), "name_a", "workspace_a/user.rb"),
        "workspace_a file should produce its method fact"
    );
    assert!(
        method_fact_in_path(editor.server(), "name_b", "workspace_b/user.rb"),
        "workspace_b file should produce its method fact"
    );
    assert!(
        !method_fact_in_path(editor.server(), "name_a", "workspace_b/user.rb"),
        "workspace_b file must not own workspace_a method fact"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn ready_project_definition_stays_responsive_while_sibling_workers_are_saturated() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("ready");
    editor
        .open(
            "ready/service.rb",
            "class ReadyService\n  def target; end\n  def call; target; end\nend\n",
        )
        .await;
    let initial = editor.goto_def_at("ready/service.rb", 2, 14).await;
    assert_eq!(
        initial.len(),
        1,
        "the ready-project fixture must resolve before saturation"
    );

    let started = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for index in 0..2 {
        let scheduler = editor.server().indexing_scheduler.clone();
        let started = started.clone();
        workers.push(tokio::spawn(async move {
            let _permit = scheduler
                .acquire(
                    format!("/busy/project-{index}").into(),
                    crate::indexing_scheduler::IndexingPriority::Background,
                )
                .await;
            tokio::task::spawn_blocking(move || {
                started.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(150));
            })
            .await
            .expect("bounded sibling worker must complete");
        }));
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while started.load(Ordering::SeqCst) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both scheduler slots must become saturated");

    let query_started = Instant::now();
    let definitions = editor.goto_def_at("ready/service.rb", 2, 14).await;
    let query_elapsed = query_started.elapsed();
    assert_eq!(
        definitions, initial,
        "sibling indexing must not change a ready engine's semantic answer"
    );
    assert!(
        query_elapsed < Duration::from_millis(100),
        "ready-project definition was starved for {query_elapsed:?} while sibling indexing was saturated"
    );

    let hover_started = Instant::now();
    assert!(
        editor.hover_at("ready/service.rb", 2, 14).await.is_some(),
        "ready-project hover must remain available while sibling indexing is saturated"
    );
    assert!(
        hover_started.elapsed() < Duration::from_millis(100),
        "ready-project hover was starved while sibling indexing was saturated"
    );

    let edit_started = Instant::now();
    editor
        .set(
            "ready/service.rb",
            "class ReadyService\n  def target; end\n  def call; target; end\nend\n# body-only edit\n",
        )
        .await;
    let definitions_after_edit = editor.goto_def_at("ready/service.rb", 2, 14).await;
    assert_eq!(
        definitions_after_edit, initial,
        "a ready-project body edit must preserve its semantic answer during sibling indexing"
    );
    assert!(
        edit_started.elapsed() < Duration::from_millis(500),
        "ready-project edit and definition refresh exceeded the 500 ms interactive budget during sibling indexing"
    );

    for worker in workers {
        worker.await.expect("sibling indexing task must complete");
    }
}

#[tokio::test]
async fn longest_prefix_wins_for_nested_workspaces() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("apps");
    editor.add_workspace("apps/web");

    editor
        .open(
            "apps/web/controller.rb",
            "class Controller\n  def index; end\nend\n",
        )
        .await;

    let ws = editor
        .workspace_for("apps/web/controller.rb")
        .expect("nested workspace should match");
    assert!(
        ws.root_uri.as_str().ends_with("apps/web/"),
        "expected longest-prefix match `apps/web/`, got {}",
        ws.root_uri.as_str()
    );

    let outer = editor
        .server()
        .list_workspaces()
        .into_iter()
        .find(|w| w.root_uri.as_str().ends_with("apps/"))
        .expect("outer workspace should still be registered");
    assert!(
        outer.root_uri.as_str().ends_with("apps/"),
        "outer workspace should remain registered"
    );
}

#[tokio::test]
async fn semantic_facts_do_not_cross_workspace_project_boundaries() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");

    editor
        .open(
            "workspace_a/user.rb",
            "class User\n  def only_in_a; end\nend\n",
        )
        .await;
    editor
        .open(
            "workspace_b/user.rb",
            "class User\n  def call\n    only_in_a\n  end\nend\n",
        )
        .await;

    editor
        .check(
            "workspace_b/user.rb",
            "class User\n  def call\n    <warn code=\"unresolved-method\">only_in_a</warn>\n  end\nend\n",
        )
        .await;
}

#[tokio::test]
async fn workspace_symbol_search_aggregates_isolated_project_engines() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");
    editor
        .open("workspace_a/a.rb", "class AlphaService\nend\n")
        .await;
    editor
        .open("workspace_b/b.rb", "class BetaService\nend\n")
        .await;

    let symbols = crate::capabilities::workspace_symbols::handle_workspace_symbols(
        editor.server(),
        WorkspaceSymbolParams {
            query: "Service".to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await
    .unwrap();
    let names = symbols
        .into_iter()
        .map(|symbol| symbol.name)
        .collect::<Vec<_>>();

    assert_eq!(names, ["AlphaService", "BetaService"]);
}

#[tokio::test]
async fn navigation_into_external_dependency_retains_originating_project_context() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    let workspace = editor
        .workspace_for("workspace_a/app.rb")
        .expect("workspace_a must own its project files");
    let processor = crate::indexer::file_processor::FileProcessor::with_extension_registry(
        editor.server().extension_registry.clone(),
    );
    let entry_uri = tower_lsp::lsp_types::Url::parse("file:///external/demo-gem/lib/entry.rb")
        .expect("dependency URI must parse");
    let inner_uri = tower_lsp::lsp_types::Url::parse("file:///external/demo-gem/lib/inner.rb")
        .expect("dependency URI must parse");
    let entry_source = "module DemoGem\n  class Entry\n    Inner\n  end\nend\n";

    processor
        .collect_file_facts_as_deferred_resolution_in_engine(
            &entry_uri,
            entry_source,
            workspace.analysis_engine.clone(),
            SourceKind::Gem,
        )
        .expect("entry dependency facts must index");
    processor
        .collect_file_facts_as_deferred_resolution_in_engine(
            &inner_uri,
            "module DemoGem\n  class Inner\n  end\nend\n",
            workspace.analysis_engine.clone(),
            SourceKind::Gem,
        )
        .expect("inner dependency facts must index");
    workspace.analysis_engine.write().resolve();

    editor.open("workspace_a/app.rb", "DemoGem::Entry\n").await;
    let entry_definitions = editor.goto_def_at("workspace_a/app.rb", 0, 10).await;
    assert_eq!(entry_definitions.len(), 1);
    assert_eq!(entry_definitions[0].uri, entry_uri);

    editor
        .open("external/demo-gem/lib/entry.rb", entry_source)
        .await;
    let inner_definitions = editor
        .goto_def_at("external/demo-gem/lib/entry.rb", 2, 6)
        .await;

    assert_eq!(inner_definitions.len(), 1);
    assert_eq!(inner_definitions[0].uri, inner_uri);
    assert!(
        editor
            .server()
            .analysis_engine
            .read()
            .file_id(
                entry_uri
                    .to_file_path()
                    .expect("dependency URI must be a file path")
            )
            .is_none(),
        "opening a navigated dependency must not promote it into orphan project state"
    );
}

#[tokio::test]
async fn directly_opened_dependency_uses_its_unique_indexed_project_owner() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    let workspace = editor
        .workspace_for("workspace_a/app.rb")
        .expect("workspace_a must own its project files");
    let processor = crate::indexer::file_processor::FileProcessor::with_extension_registry(
        editor.server().extension_registry.clone(),
    );
    let entry_uri = tower_lsp::lsp_types::Url::parse("file:///external/unique-gem/lib/entry.rb")
        .expect("dependency URI must parse");
    let inner_uri = tower_lsp::lsp_types::Url::parse("file:///external/unique-gem/lib/inner.rb")
        .expect("dependency URI must parse");
    let entry_source = "module UniqueGem\n  class Entry\n    Inner\n  end\nend\n";

    for (uri, source) in [
        (&entry_uri, entry_source),
        (&inner_uri, "module UniqueGem\n  class Inner\n  end\nend\n"),
    ] {
        processor
            .collect_file_facts_as_deferred_resolution_in_engine(
                uri,
                source,
                workspace.analysis_engine.clone(),
                SourceKind::Gem,
            )
            .expect("dependency facts must index");
    }
    workspace.analysis_engine.write().resolve();

    editor
        .open("external/unique-gem/lib/entry.rb", entry_source)
        .await;
    let definitions = editor
        .goto_def_at("external/unique-gem/lib/entry.rb", 2, 6)
        .await;

    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].uri, inner_uri);
}

#[tokio::test]
async fn unbound_external_document_is_not_promoted_to_project_source() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    let external_uri = tower_lsp::lsp_types::Url::parse("file:///external/loose.rb")
        .expect("external URI must parse");

    editor.open("external/loose.rb", "UnknownExternal\n").await;

    let path = external_uri
        .to_file_path()
        .expect("external URI must be a file path");
    let orphan = editor.server().analysis_engine.read();
    let file_id = orphan
        .file_id(&path)
        .expect("unbound open document must retain local interactive facts");
    assert_eq!(
        orphan
            .file(file_id)
            .expect("open file metadata must exist")
            .kind,
        SourceKind::Excluded
    );
    drop(orphan);
    assert!(
        editor.published_diagnostics("external/loose.rb").is_empty(),
        "unbound external documents must not publish project diagnostics"
    );
}

#[tokio::test]
async fn closing_external_document_releases_ambiguous_project_provenance() {
    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");
    let processor = crate::indexer::file_processor::FileProcessor::with_extension_registry(
        editor.server().extension_registry.clone(),
    );
    let entry_uri = tower_lsp::lsp_types::Url::parse("file:///external/shared-gem/lib/entry.rb")
        .expect("dependency URI must parse");
    let entry_source = "module SharedGem\n  class Entry\n    Inner\n  end\nend\n";

    for (workspace_file, inner_path) in [
        ("workspace_a/app.rb", "inner_a.rb"),
        ("workspace_b/app.rb", "inner_b.rb"),
    ] {
        let workspace = editor
            .workspace_for(workspace_file)
            .expect("project file must have a workspace");
        let inner_uri = tower_lsp::lsp_types::Url::parse(&format!(
            "file:///external/shared-gem/lib/{inner_path}"
        ))
        .expect("dependency URI must parse");
        processor
            .collect_file_facts_as_deferred_resolution_in_engine(
                &entry_uri,
                entry_source,
                workspace.analysis_engine.clone(),
                SourceKind::Gem,
            )
            .expect("entry dependency facts must index");
        processor
            .collect_file_facts_as_deferred_resolution_in_engine(
                &inner_uri,
                "module SharedGem\n  class Inner\n  end\nend\n",
                workspace.analysis_engine.clone(),
                SourceKind::Gem,
            )
            .expect("inner dependency facts must index");
        workspace.analysis_engine.write().resolve();
    }

    editor
        .open("workspace_a/app.rb", "SharedGem::Entry\n")
        .await;
    assert_eq!(
        editor.goto_def_at("workspace_a/app.rb", 0, 12).await.len(),
        1
    );
    editor
        .open("external/shared-gem/lib/entry.rb", entry_source)
        .await;
    let contextual = editor
        .goto_def_at("external/shared-gem/lib/entry.rb", 2, 6)
        .await;
    assert_eq!(contextual.len(), 1);
    assert!(contextual[0].uri.path().ends_with("/inner_a.rb"));

    editor.close("external/shared-gem/lib/entry.rb").await;
    editor
        .open("external/shared-gem/lib/entry.rb", entry_source)
        .await;
    assert!(
        editor
            .goto_def_at("external/shared-gem/lib/entry.rb", 2, 6)
            .await
            .is_empty(),
        "direct reopen with two possible owners must not guess a project context"
    );
}

fn method_fact_in_path(
    server: &crate::server::RubyLanguageServer,
    method_name: &str,
    path_suffix: &str,
) -> bool {
    server
        .analysis_engines()
        .into_iter()
        .any(|analysis_engine| {
            let engine = analysis_engine.read();
            engine.all_method_facts().into_iter().any(|fact| {
                let ruby_analysis::core::FullyQualifiedName::Method(_, method) = fact.fqn else {
                    return false;
                };
                if method.as_str() != method_name {
                    return false;
                }
                engine
                    .file(fact.range.file_id)
                    .map(|file| file.path.to_string_lossy().ends_with(path_suffix))
                    .unwrap_or(false)
            })
        })
}
