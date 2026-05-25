use crate::test::harness::check;
use crate::test::harness::FakeEditor;

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
