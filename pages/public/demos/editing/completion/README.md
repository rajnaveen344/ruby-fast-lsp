# Completion and inferred return type

[GIF](demo.gif) · [MP4](demo.mp4) · [Poster](poster.png)

This short pacing sample shows real VS Code results from Ruby Fast LSP 0.3.0,
with built-in AI features and inline/next-edit suggestions disabled in the demo
workspace. It is not a comparison against other language servers.

## What happens

1. A small `Notebook` class is already on screen. Its `title` method returns a
   String; the editor shows the inferred return and local-variable types.
2. Type `notebook.ti`. The language server offers `title` with a String result.
3. Select `title` with the mouse. The call becomes `notebook.title` and the
   incomplete-call warning clears.
4. Show Hover on the call to display its inferred String type.

Ordinary keystrokes play 60–100 ms apart. Longer pauses give time to read the
completion and hover. The clip consists of actual captured editor frames with
edited timing, so it does not demonstrate request latency or continuous mouse
motion.

Source: [generic fixture](../../../../demos/fixtures/notebook.rb).
Setup: [demo settings](../../../../demos/vscode-settings.json).
Provenance: [capture metadata](metadata.json).
