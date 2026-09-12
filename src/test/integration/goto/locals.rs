use crate::test::harness::check;
use crate::test::harness::FakeEditor;

#[tokio::test]
async fn goto_index_argument_uses_local_binding() {
    check(
        r#"
class Table
  def [](key)
    self
  end

  def []=(key, value)
    value
  end
end

buckets = Table.new
[:north].each do |entry|
  <def>partition</def> = { active: true }
  buckets[par$0tition][entry] = buckets[entry]
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_index_brackets_preserve_operator_navigation() {
    for (method, expressions) in [
        (
            "[](key)",
            ["table$0[key]", "table[key$0]", "table.$0[](key)"],
        ),
        (
            "[]=(key, value)",
            [
                "table$0[key] = 1",
                "table[key$0] = 1",
                "table.$0[]=(key, 1)",
            ],
        ),
    ] {
        for expression in expressions {
            check(&format!(
                "class Parent\n  <def>def {method}\n    1\n  end</def>\nend\nclass Table < Parent; end\ntable = Table.new\nkey = :north\n{expression}\n"
            ))
            .await;
        }
    }
}

#[tokio::test]
async fn goto_index_arguments_survive_edits_and_reopen() {
    use tower_lsp::lsp_types::{Location, Position, Range};

    let source = "class Table\n  def [](key); Table.new; end\n  def []=(key, value); value; end\nend\ntable = Table.new\n[:north].each do |entry|\n  partition = { active: true }\n  table[partition][entry] = table[entry]\n  table.[](partition)\n  table.[]=(partition, entry)\nend\n";
    let edited = format!(
        "# moved bindings\n{}",
        source.replace("  table", "  \"🧩\"; table")
    );
    let mut editor = FakeEditor::new().await;
    let file = "indexed_locals.rb";
    editor.open(file, source).await;

    for (phase, content) in [source, edited.as_str(), edited.as_str()]
        .into_iter()
        .enumerate()
    {
        if phase == 1 {
            editor.set(file, content).await;
        } else if phase == 2 {
            editor.close(file).await;
            editor.open(file, content).await;
        }
        for (name, declaration) in [
            ("table", "table ="),
            ("entry", "|entry|"),
            ("partition", "partition ="),
        ] {
            let (line, text) = content
                .lines()
                .enumerate()
                .find(|(_, text)| text.contains(declaration))
                .unwrap();
            let start = text[..text.find(name).unwrap()].encode_utf16().count() as u32;
            let expected = Location::new(
                crate::test::harness::fixture_uri("/indexed_locals.rb"),
                Range::new(
                    Position::new(line as u32, start),
                    Position::new(line as u32, start + name.len() as u32),
                ),
            );
            for (line, text) in content
                .lines()
                .enumerate()
                .filter(|(_, text)| text.contains("table[") || text.contains("table.[]"))
            {
                for (start, _) in text.match_indices(name) {
                    // Every character must select the binding, including the
                    // first character directly after an opening bracket.
                    for offset in 0..name.len() {
                        let character = text[..start + offset].encode_utf16().count() as u32;
                        assert_eq!(
                            editor.goto_def_at(file, line as u32, character).await,
                            vec![expected.clone()],
                            "{name} must keep its lexical binding in phase {phase} at {line}:{character}"
                        );
                    }
                }
            }
        }
    }
}

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
