use crate::test::harness::FakeEditor;

#[tokio::test]
async fn erb_ruby_regions_use_template_utf16_positions_and_reindex() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "app/models/user.rb",
            "class User\n  def self.name\n    \"Ada\"\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "app/views/users/show.html.erb",
            "<p>😀</p><%= User.name %>\n",
        )
        .await;

    let user = editor
        .goto_def_at("app/views/users/show.html.erb", 0, 14)
        .await;
    assert_eq!(
        user.len(),
        1,
        "ERB constant must resolve at its template position"
    );
    assert!(user[0].uri.path().ends_with("/app/models/user.rb"));
    assert_eq!(user[0].range.start.line, 0);

    let name = editor
        .goto_def_at("app/views/users/show.html.erb", 0, 19)
        .await;
    assert_eq!(
        name.len(),
        1,
        "ERB method must resolve through ordinary inference"
    );
    assert_eq!(name[0].range.start.line, 1);
    let hover = editor
        .hover_at("app/views/users/show.html.erb", 0, 19)
        .await;
    assert!(
        hover
            .as_ref()
            .is_some_and(|hover| format!("{:?}", hover.contents).contains("String")),
        "ERB method hover must use ordinary return inference, got {hover:?}"
    );
    let references = editor.references_at("app/models/user.rb", 0, 7).await;
    assert!(
        references.iter().any(|location| {
            location.uri.path().ends_with("/app/views/users/show.html.erb")
                && location.range.start.character == 13
        }),
        "ERB constant must enter ordinary engine references at the template range, got {references:?}"
    );
    let host_selection = editor
        .selection_ranges(
            "app/views/users/show.html.erb",
            &[tower_lsp::lsp_types::Position::new(0, 1)],
        )
        .await;
    assert_eq!(host_selection[0].range.start, host_selection[0].range.end);
    assert!(
        editor
            .goto_def_at("app/views/users/show.html.erb", 0, 1)
            .await
            .is_empty(),
        "host-language text must never become a Ruby target"
    );
    let diagnostics = editor.diagnostics("app/views/users/show.html.erb").await;
    assert!(
        diagnostics.is_empty(),
        "HTML host text must not create Ruby syntax or semantic diagnostics, got {diagnostics:?}"
    );

    editor
        .set(
            "app/views/users/show.html.erb",
            "<p>😀</p><%= \"plain\" %>\n",
        )
        .await;
    assert!(
        editor
            .references_at("app/models/user.rb", 0, 7)
            .await
            .iter()
            .all(|location| !location
                .uri
                .path()
                .ends_with("/app/views/users/show.html.erb")),
        "removing ERB Ruby code must remove stale engine references"
    );
}

#[tokio::test]
async fn erb_completion_uses_only_the_ruby_region() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "app/models/user.rb",
            "class User\n  def self.display_name\n    \"Ada\"\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "app/views/users/show.html.erb",
            "<div>User.</div>\n<%= User. %>\n",
        )
        .await;

    let items = editor
        .complete_with_trigger("app/views/users/show.html.erb", 1, 9, ".")
        .await;
    assert!(
        items.iter().any(|item| item.label == "display_name"),
        "ERB Ruby completion must use engine methods, got {items:?}"
    );
    assert!(
        editor
            .complete_with_trigger("app/views/users/show.html.erb", 0, 10, ".")
            .await
            .is_empty(),
        "HTML host text must not receive Ruby completions"
    );
}

#[tokio::test]
async fn erb_local_variable_references_share_the_template_coordinate_space() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "app/views/users/show.html.erb",
            "<% greeting = \"hello\" %>\n<p><%= greeting %></p>\n",
        )
        .await;

    let references = editor
        .references_at("app/views/users/show.html.erb", 0, 4)
        .await;
    assert_eq!(
        references.len(),
        2,
        "the assignment and later ERB expression must share one Ruby local scope: {references:?}"
    );
    assert_eq!(references[0].range.start.character, 3);
    assert_eq!(references[1].range.start.line, 1);
    assert_eq!(references[1].range.start.character, 7);
}

#[tokio::test]
async fn erb_code_lenses_parse_only_embedded_ruby() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "app/models/user.rb",
            "class User\n  include ViewConcern\nend\n",
        )
        .await;
    editor
        .open(
            "app/views/shared/_concern.html.erb",
            "<section>host text</section>\n<% module ViewConcern %>\n<% end %>\n",
        )
        .await;

    let lenses = editor.code_lens("app/views/shared/_concern.html.erb").await;
    assert!(
        lenses.iter().any(|lens| {
            lens.command
                .as_ref()
                .is_some_and(|command| command.title == "1 include")
        }),
        "module lenses must use the same mapped Ruby source as indexing: {lenses:?}"
    );
}
