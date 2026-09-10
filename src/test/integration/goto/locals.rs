use crate::test::harness::check;
use crate::test::harness::FakeEditor;

#[tokio::test]
async fn goto_unknown_local_in_rescue_interpolation() {
    check(
        r##"
def read_packet(client)
  client.read
rescue IOError => failure
  warn failure.message
rescue StandardError => failure
  <def>payload</def> = client.decode(failure.message)
  if payload["retry"]
    raise "retry required"
  else
    notes = <<~TEXT
      Please retry later.
    TEXT
    raise "Invalid packet: #{pay$0<hover label="Unknown[unresolved_assignment_value]">load}. #{notes}"
  end
end
"##,
    )
    .await;
}

#[tokio::test]
async fn goto_unknown_local_in_ordinary_read() {
    check(
        r#"
def read_packet(client)
  <def>payload</def> = client.decode
  puts pay$0load
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_unknown_local_survives_edits_and_reopen() {
    let mut editor = FakeEditor::new().await;
    let known = r##"class Packet
  def label
    "ready"
  end
end
def read_packet(client)
  payload = Packet.new
  payload.label
  "Result: #{payload}"
end
"##;
    let unknown = known.replace("Packet.new", "client.decode");
    let file = "packet.rb";

    editor.open(file, known).await;
    assert_eq!(editor.goto_def_at(file, 7, 12).await.len(), 1);

    editor.set(file, &unknown).await;
    for reopened in [false, true] {
        if reopened {
            editor.close(file).await;
            editor.open(file, &unknown).await;
        }
        for (line, character) in [(7, 5), (8, 15)] {
            let definitions = editor.goto_def_at(file, line, character).await;
            assert_eq!(
                definitions.len(),
                1,
                "an unknown value must retain its local binding after edits/reopen"
            );
            assert_eq!(definitions[0].range.start.line, 6);
            assert_eq!(definitions[0].range.start.character, 2);
            assert_eq!(definitions[0].range.end.line, 6);
            assert_eq!(definitions[0].range.end.character, 9);
        }
        assert!(
            editor.goto_def_at(file, 7, 12).await.is_empty(),
            "an unknown receiver must not navigate to the previous value's method"
        );
    }

    editor.set(file, known).await;
    assert_eq!(editor.goto_def_at(file, 7, 12).await.len(), 1);
}

#[tokio::test]
async fn goto_pattern_capture_definition() {
    check(
        r#"
case {user: "Ada"}
in {user: <def>user</def>}
  puts user$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn local_definition_survives_reopen_without_reindex() {
    let mut editor = FakeEditor::new().await;
    let content = "def work\n  user = 1\n  puts user\nend\n";

    editor.open("locals_reopen.rb", content).await;
    editor.close("locals_reopen.rb").await;
    editor.open("locals_reopen.rb", content).await;

    let locations = editor.goto_def_at("locals_reopen.rb", 2, 7).await;
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].range.start.line, 1);
    assert_eq!(locations[0].range.start.character, 2);
    assert_eq!(locations[0].range.end.line, 1);
    assert_eq!(locations[0].range.end.character, 6);
}
