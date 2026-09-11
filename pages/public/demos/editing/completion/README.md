# Local variables and method completion

[GIF](demo.gif) · [MP4](demo.mp4) · [Poster](poster.png)

This loop shows real VS Code results from Ruby Fast LSP 0.3.0 in an extension
development host, with AI features, inline suggestions, word suggestions, and
snippets disabled in the demo workspace.

## What happens

1. A small `Notebook` class is already on screen. Its `title` method returns a
   String; the editor shows the inferred return and local-variable types.
2. Type `not`. Automatic completion offers the local variable `notebook`.
3. Select `notebook` with the mouse, then type `.ti`. The language server offers
   `title` with a String result.
4. Select `title` with the mouse. The call becomes `notebook.title` and the
   incomplete-call warning clears.
5. Show Hover on the call to display its inferred String type.

Ordinary keystrokes play 60–100 ms apart. Longer pauses give time to read the
completion and hover. The clip consists of actual captured editor frames with
edited timing, so it does not demonstrate request latency or continuous mouse
motion.

Source: [generic fixture](../../../../demos/fixtures/notebook.rb).
Setup: [demo settings](../../../../demos/vscode-settings.json).
Provenance: [capture metadata](metadata.json).
