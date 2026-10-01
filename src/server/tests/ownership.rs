use crate::loader::context::{CORE_ENGINE_CACHE_MAX_ENTRIES, CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES};
use crate::utils::single_flight::SingleFlightStat;

use crate::server::RubyLanguageServer;
use ruby_analysis::engine::AnalysisEngine;
use std::sync::Arc;

#[test]
fn server_ownership_clones_share_document_locks_and_isolate_project_engines() {
    let server = RubyLanguageServer::default();
    let clone = server.clone();
    let uri = crate::test::harness::fixture_uri("/ownership/entry.rb");
    let other_uri = crate::test::harness::fixture_uri("/ownership/other.rb");
    let lock = server.document_semantic_lock(&uri);
    assert!(Arc::ptr_eq(&lock, &clone.document_semantic_lock(&uri)));
    assert!(!Arc::ptr_eq(
        &lock,
        &clone.document_semantic_lock(&other_uri)
    ));

    let first = server.add_workspace(crate::test::harness::fixture_uri("/ownership/"));
    let second = clone.add_workspace(crate::test::harness::fixture_uri("/neighbor/"));
    assert_eq!(server.list_workspaces().len(), 2);
    assert!(Arc::ptr_eq(
        &first.analysis_engine,
        &clone.analysis_engine_for_uri(&uri)
    ));
    assert!(!Arc::ptr_eq(
        &first.analysis_engine,
        &second.analysis_engine
    ));
    let orphan = crate::test::harness::fixture_uri("/loose.rb");
    assert!(Arc::ptr_eq(
        &server.analysis_engine_for_uri(&orphan),
        &clone.analysis_engine_for_uri(&orphan)
    ));
    assert!(!Arc::ptr_eq(
        &first.analysis_engine,
        &clone.analysis_engine_for_uri(&orphan)
    ));
    clone.remove_workspace(&first.root_uri);
    assert!(Arc::ptr_eq(
        &server.analysis_engine_for_uri(&uri),
        &server.analysis_engine_for_uri(&orphan)
    ));
}

#[tokio::test]
async fn server_ownership_client_construction_defers_extension_discovery() {
    let (service, _socket) = tower_lsp::LspService::new(|client| {
        RubyLanguageServer::new(client).expect("construct client server")
    });
    assert!(service.inner().client.is_some());
    assert!(service.inner().extension_status_reports().is_empty());
    assert!(service.inner().list_workspaces().is_empty());
}

#[tokio::test]
async fn core_engine_template_retention_is_bounded_by_entries_and_estimated_heap() {
    let server = RubyLanguageServer::default();
    for index in 0..(CORE_ENGINE_CACHE_MAX_ENTRIES + 2) {
        server
            .products
            .core_templates()
            .get_or_try_init(format!("core-{index}"), || async {
                Ok(ruby_analysis::engine::AnalysisEngine::new())
            })
            .await
            .unwrap();
    }

    let products = server.runtime_product_snapshot();
    assert!(
        products.core_templates.get(SingleFlightStat::Entries)
            <= ruby_analysis::stats::count(CORE_ENGINE_CACHE_MAX_ENTRIES),
        "completed core templates must evict to the server-owned entry bound"
    );
    assert!(
        products
            .core_templates
            .get(SingleFlightStat::RetainedWeightBytes)
            <= CORE_ENGINE_CACHE_MAX_WEIGHT_BYTES,
        "completed core templates must remain within the server-owned estimated-heap bound"
    );
}

#[tokio::test]
async fn server_ownership_clones_reuse_products_with_one_ordinary_cache_root() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("cache");
    let server = RubyLanguageServer::with_user_cache_root(root.clone()).unwrap();
    let clone = server.clone();
    assert_eq!(clone.products.cache_root(), root);
    let first = server
        .products
        .core_templates()
        .get_or_try_init("shared-core".to_string(), || async {
            Ok(AnalysisEngine::new())
        })
        .await
        .unwrap();
    let second = clone
        .products
        .core_templates()
        .get_or_try_init("shared-core".to_string(), || async {
            Err("a server clone must reuse the existing core product".to_string())
        })
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let products = server.runtime_product_snapshot();
    assert_eq!(products.core_templates.get(SingleFlightStat::Lookups), 2);
    assert_eq!(products.core_templates.get(SingleFlightStat::Producers), 1);
    assert_eq!(products.core_templates.get(SingleFlightStat::Hits), 1);
    assert_eq!(clone.runtime_product_snapshot(), products);
}

#[tokio::test]
async fn server_ownership_fake_editor_receives_real_diagnostic_messages_and_clears() {
    let mut editor = crate::test::harness::FakeEditor::new().await;
    assert!(editor.server().client.is_some());
    editor
        .open("delivery.rb", "value = nil\nvalue.upcase\n")
        .await;
    let diagnostics = editor.diagnostics("delivery.rb").await;
    assert!(diagnostics.iter().any(|item| item.code
        == Some(tower_lsp::lsp_types::NumberOrString::String(
            "nil-call".to_string()
        ))));
    let count = editor.delivered_diagnostic_notifications();
    assert!(count > 0);
    editor
        .set("delivery.rb", "value = \"ready\"\nvalue.upcase\n")
        .await;
    assert!(editor.diagnostics("delivery.rb").await.is_empty());
    assert!(editor.delivered_diagnostic_notifications() > count);
}
