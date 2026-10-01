//! Inlay navigation uses real type identities, including abbreviated names.

use crate::test::harness::{get_hint_label, get_hint_tooltip, FakeEditor};
use tower_lsp::lsp_types::{InlayHint, InlayHintLabel, InlayHintLabelPart, Position};

fn parts(hint: &InlayHint) -> &[InlayHintLabelPart] {
    let InlayHintLabel::LabelParts(parts) = &hint.label else {
        panic!("type hints must carry independently navigable label parts");
    };
    parts
}

#[tokio::test]
async fn compact_inlay_navigation_links_hash_keys_and_values() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "types.rb",
            "class Hash; end\nclass Symbol; end\nclass Float; end\n",
        )
        .await;
    editor
        .open("values.rb", "rates = { north: 1.5, south: 2.5 }\n")
        .await;
    let hints = editor.inlay_hints("values.rb").await;
    let hint = hints
        .iter()
        .find(|hint| hint.position == Position::new(0, 5))
        .unwrap();
    assert_eq!(get_hint_label(hint), ": Hash<Symbol, Float>");
    assert_eq!(
        get_hint_tooltip(hint),
        Some("```ruby\n{\n  north: Float,\n  south: Float\n}\n```")
    );
    for (name, line) in [("Hash", 0), ("Symbol", 1), ("Float", 2)] {
        let part = parts(hint).iter().find(|part| part.value == name).unwrap();
        let location = part
            .location
            .as_ref()
            .expect("proven type must have a definition link");
        assert!(location.uri.path().ends_with("/types.rb"));
        assert_eq!(location.range.start.line, line);
        assert_eq!(location.range.start.character, 6);
        assert_eq!(location.range.end.character, 6 + name.len() as u32);
        assert!(
            !editor.goto_def_at("types.rb", line, 6).await.is_empty(),
            "a client must be able to resolve the label-part location as a definition request"
        );
    }
    assert!(parts(hint)
        .iter()
        .filter(|part| [": ", "<", ", ", ">"].contains(&part.value.as_str()))
        .all(|part| part.location.is_none()));
}

#[tokio::test]
async fn compact_inlay_navigation_retains_abbreviated_identity_after_provider_edit() {
    let mut editor = FakeEditor::new().await;
    let provider =
        "note = \"🙂\"; module Workshop::Runtime::Services::Reports; class Entry; end; end\n";
    editor.open("reports.rb", provider).await;
    editor
        .open("unrelated.rb", "module Library; class Entry; end; end\n")
        .await;
    editor
        .open(
            "consumer.rb",
            "item = Workshop::Runtime::Services::Reports::Entry.new\n",
        )
        .await;
    for (prefix, expected_line) in [("", 0), ("# moved\n\n", 2)] {
        editor
            .set("reports.rb", &format!("{prefix}{provider}"))
            .await;
        let hints = editor.inlay_hints("consumer.rb").await;
        let hint = hints
            .iter()
            .find(|hint| hint.position == Position::new(0, 4))
            .unwrap();
        assert_eq!(get_hint_label(hint), ": …::Entry");
        let part = parts(hint)
            .iter()
            .find(|part| part.value == "…::Entry")
            .unwrap();
        let location = part
            .location
            .as_ref()
            .expect("abbreviated name must retain its exact target");
        assert!(location.uri.path().ends_with("/reports.rb"));
        assert_eq!(location.range.start.line, expected_line);
        assert_eq!(
            location.range.start.character,
            provider[..provider.find("Entry").unwrap()]
                .encode_utf16()
                .count() as u32
        );
    }
    editor.set("reports.rb", "# removed\n").await;
    assert!(editor
        .inlay_hints("consumer.rb")
        .await
        .iter()
        .flat_map(parts)
        .all(|part| part.location.is_none()));
}

#[tokio::test]
async fn compact_inlay_navigation_retains_external_project_context() {
    use crate::indexer::file_processor::FileProcessor;
    use ruby_analysis::core::SourceKind;

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("workspace_a");
    editor.add_workspace("workspace_b");
    let processor =
        FileProcessor::with_extension_registry(editor.server().extensions.registry().clone());
    let entry_uri = crate::test::harness::fixture_uri("/external/catalog/lib/entry.rb");
    let source = "module Catalog\n  class Entry\n    Inner\n  end\nend\n";
    for project in ["workspace_a", "workspace_b"] {
        let workspace = editor.workspace_for(&format!("{project}/app.rb")).unwrap();
        for (uri, content) in [
            (entry_uri.clone(), source),
            (
                crate::test::harness::fixture_uri(format!("/external/catalog/lib/{project}.rb")),
                "module Catalog; class Inner; end; end\n",
            ),
        ] {
            processor
                .collect_file_facts_as_deferred_resolution_in_engine(
                    &uri,
                    content,
                    workspace.analysis_engine.clone(),
                    SourceKind::Gem,
                )
                .unwrap();
        }
        workspace.analysis_engine.write().resolve();
    }
    editor
        .open("workspace_a/app.rb", "item = Catalog::Entry.new\n")
        .await;
    let hints = editor.inlay_hints("workspace_a/app.rb").await;
    let linked = hints
        .iter()
        .flat_map(parts)
        .find(|part| part.value == "Catalog::Entry")
        .unwrap();
    assert_eq!(linked.location.as_ref().unwrap().uri, entry_uri);
    editor.open("external/catalog/lib/entry.rb", source).await;
    let definitions = editor
        .goto_def_at("external/catalog/lib/entry.rb", 2, 6)
        .await;
    assert_eq!(definitions.len(), 1);
    assert!(definitions[0].uri.path().ends_with("/workspace_a.rb"));
}
