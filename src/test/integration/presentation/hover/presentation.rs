//! Hover headers identify the binding without replacing its type evidence.

use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{HoverContents, MarkupKind};

async fn hover_on_last(editor: &FakeEditor, source: &str, name: &str) -> String {
    let offset = source
        .rfind(name)
        .expect("fixture contains the hovered name");
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count() as u32;
    let hover = editor
        .hover_at("binding_hover.rb", line, character)
        .await
        .expect("binding has a hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("binding hover must use Markdown to preserve its code header");
    };
    assert_eq!(markup.kind, MarkupKind::Markdown);
    markup.value
}

#[tokio::test]
async fn binding_hover_headers_show_name_type_and_kind() {
    let source = r#"module Sample
  LIMIT = 3
  class Gauge
    def read
      count = 7
      @title = "ready"
      @@enabled = true
      $ratio = 1.5
      count
      @title
      @@enabled
      $ratio
      Sample::LIMIT
    end
  end
end
"#;
    let mut editor = FakeEditor::new().await;
    editor.open("binding_hover.rb", source).await;
    for (name, expected) in [
        ("count", "count: Integer # local variable"),
        ("@title", "@title: String # instance variable"),
        ("@@enabled", "@@enabled: TrueClass # class variable"),
        ("$ratio", "$ratio: Float # global variable"),
        ("LIMIT", "Sample::LIMIT: Integer # constant"),
    ] {
        assert_eq!(
            hover_on_last(&editor, source, name).await,
            format!("```ruby\n{expected}\n```")
        );
    }
}

#[tokio::test]
async fn binding_hover_unknown_reason_survives_edits_and_reopen() {
    let known = "def read(client)\n  result = 7\n  result\nend\n";
    let unknown = known.replace("result = 7", "result = client.decode");
    let mut editor = FakeEditor::new().await;
    editor.open("binding_hover.rb", known).await;
    assert_eq!(
        hover_on_last(&editor, known, "result").await,
        "```ruby\nresult: Integer # local variable\n```"
    );

    editor.set("binding_hover.rb", &unknown).await;
    for reopened in [false, true] {
        if reopened {
            editor.close("binding_hover.rb").await;
            editor.open("binding_hover.rb", &unknown).await;
        }
        assert_eq!(hover_on_last(&editor, &unknown, "result").await,
            "```ruby\nresult: ? # local variable\n```\n\nUnknown[unresolved_assignment_value]: the reaching assignment value does not have a proven type");
    }

    editor.set("binding_hover.rb", known).await;
    assert_eq!(
        hover_on_last(&editor, known, "result").await,
        "```ruby\nresult: Integer # local variable\n```"
    );
}

#[tokio::test]
async fn binding_hover_retains_shape_details_and_unknown_identity() {
    let source = r#"def read
  payload = { label: "ready", rank: 1 }
  payload
  MissingValue
end
"#;
    let mut editor = FakeEditor::new().await;
    editor.open("binding_hover.rb", source).await;
    assert_eq!(
        hover_on_last(&editor, source, "payload").await,
        "```ruby\npayload: { label: String, rank: Integer } # local variable\n```"
    );
    assert_eq!(
        hover_on_last(&editor, source, "MissingValue").await,
        "```ruby\nMissingValue: ? # constant\n```"
    );
}
