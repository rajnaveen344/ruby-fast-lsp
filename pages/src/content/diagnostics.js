export default [
  {
    id: "diagnostics/live",
    title: "Live diagnostics",
    summary:
      "See supported syntax and semantic problems as you edit, then watch them clear when fixed.",
    demo: "diagnostics/live",
    steps: [
      "Introduce a misspelled method on a receiver with a known type.",
      "Inspect the underline or Problems panel.",
      "Correct the method name and verify that the diagnostic clears.",
    ],
    code: 'class Notebook\n  def title\n    "Field guide"\n  end\nend\n\nNotebook.new.titel',
    sections: [
      {
        title: "Current source, current results",
        body: "Open buffers are authoritative. Syntax and semantic diagnostics refresh after edits. Cold indexing keeps facts for project files while publishing diagnostics only for open documents.",
      },
      {
        title: "Semantic checks",
        body: "Supported checks include unresolved constants, missing methods on complete lookup chains, and incompatible argument counts/shapes. Unresolved ancestry suppresses unsupported missing-method claims.",
      },
    ],
    limits:
      "This is not a proof that arbitrary Ruby code is correct. Unknown evidence, dynamic behavior, and incomplete dependencies limit what can be diagnosed. Linter diagnostics are a separate opt-in integration.",
    contract: "docs/usage.md",
    related: ["types/unknown", "diagnostics/fixes"],
  },
  {
    id: "diagnostics/fixes",
    title: "Safe fixes and formatting",
    summary:
      "Use RuboCop or Standard against your current Ruby buffer, with safe corrections returned as editor edits.",
    demo: "diagnostics/fixes",
    steps: [
      "Run Ruby Fast LSP: Select Linter and choose a tool available to your project.",
      "Open or save a Ruby file and apply an offered safe quick fix.",
      "Select a formatter independently, then run Format Document.",
    ],
    code: '# frozen_string_literal: true\n\nlabel = "Trail notes"\nputs( label )',
    requirements:
      "Install and configure RuboCop or Standard for the owning project. Linter and formatter selections are independent editor commands; they are not public settings.json options.",
    limits:
      "Linting runs on open/save, outside the typing path. Only supported safe corrections become edits. Failed, timed-out, invalid, or unsafe empty output produces no edit. Ruby linters/formatters do not edit ERB templates.",
    contract: "docs/usage.md",
    related: ["diagnostics/live"],
    demoTitle: "Safe whitespace formatting with RuboCop",
  },
];
