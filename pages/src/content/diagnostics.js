export default [
  {
    id: "diagnostics/live",
    title: "Live diagnostics",
    summary: "Find syntax errors and code problems while you work.",
    demo: "diagnostics/live",
    steps: [
      "Open a Ruby file and look for diagnostic underlines.",
      "Hover an underline or open the Problems panel to read the message.",
      "Edit the code and review the updated results.",
    ],
    code: 'class Notebook\n  def title\n    "Field guide"\n  end\nend\n\nNotebook.new.titel',
    sections: [
      {
        title: "Feedback as you edit",
        body: "Diagnostics update as you change an open file, helping you catch problems before running your code.",
      },
      {
        title: "Understand the problem",
        body: "Checks include syntax errors, unresolved constants, missing methods, and incompatible method arguments. Messages appear alongside your code and in the Problems panel.",
      },
    ],
    limits:
      "Available checks depend on what the server knows about your code. Dynamic Ruby behavior can limit the results. Enable RuboCop or Standard for additional linting rules.",
    contract: "docs/usage.md",
    related: ["types/unknown", "diagnostics/fixes"],
  },
  {
    id: "diagnostics/fixes",
    title: "Safe fixes and formatting",
    summary:
      "Apply quick fixes and keep Ruby code formatted with RuboCop or Standard.",
    demo: "diagnostics/fixes",
    steps: [
      "Run Ruby Fast LSP: Select Linter and choose your project’s tool.",
      "Open or save a Ruby file, then review any available quick fixes.",
      "Run Ruby Fast LSP: Select Formatter, then use Format Document.",
    ],
    code: '# frozen_string_literal: true\n\nlabel = "Trail notes"\nputs( label )',
    requirements:
      "Install RuboCop or Standard in your project. Choose the linter and formatter with the extension’s commands; you can select them independently.",
    limits:
      "Linting runs when a Ruby file is opened or saved. Quick fixes use the tool’s safe corrections. These integrations apply to Ruby files; ERB templates are not formatted or linted by them.",
    contract: "docs/usage.md",
    related: ["diagnostics/live"],
    demoTitle: "Safe whitespace formatting with RuboCop",
  },
];
