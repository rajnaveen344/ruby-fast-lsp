export default [
  {
    id: "frameworks/templates",
    title: "ERB and Rails views",
    summary:
      "Navigate Ruby inside templates and use complementary HTML editing support.",
    demo: "frameworks/templates",
    steps: [
      "Open an ERB template containing a small Ruby binding and expression.",
      "Inspect Ruby hover and navigation inside the tags.",
      "Use HTML completion in the host markup; in a supported Rails project, follow an Open View lens from a controller action.",
    ],
    code: '<% title = "Trail notes" %>\n\n<h1><%= title %></h1>\n\n<p>A place for observations.</p>',
    sections: [
      {
        title: "Two languages, original positions",
        body: "Ruby analysis preserves source offsets inside ERB. The VS Code adapter supplies HTML completion, hover, symbols, folding, selection, and highlights for the host content.",
      },
      {
        title: "Rails and other framework DSLs",
        body: "Bundled extensions activate from supported locked dependencies. Rails contributes supported DSL facts and conventional Open View targets. Sinatra models supported route/helper contexts, and Cucumber models supported step/World contexts.",
      },
    ],
    limits:
      "Whole-document Ruby formatting and linting do not edit templates. Host-language diagnostics and arbitrary mixed-language edits are not delegated. Framework support covers documented forms, not all runtime metaprogramming. Cross-tag local hover can currently report Unknown even when the declaration has a concrete hint; navigation in the recorded example still reaches the binding.",
    contract: "docs/usage.md",
    related: ["frameworks/tests", "extensions"],
    demoTitle: "Go to a Ruby binding inside ERB",
    lang: "erb",
  },
  {
    id: "frameworks/tests",
    title: "Run focused tests",
    summary: "Run an RSpec or Minitest example directly from its declaration.",
    demo: "frameworks/tests",
    steps: [
      "Open a supported test file in a project with a complete lockfile.",
      "Click the Run lens for one example or test method.",
      "Inspect the actual test process result in the editor terminal.",
    ],
    code: 'require "minitest/autorun"\n\nclass NotebookTest < Minitest::Test\n  def test_title\n    notebook = { title: "Field guide" }\n    assert_equal "Field guide", notebook[:title]\n  end\nend',
    requirements:
      "The owning project needs a supported locked RSpec or Minitest version and an executable test environment. Debug additionally requires the debug gem and a compatible debugger extension.",
    limits:
      "Dynamically generated test names and unsupported DSL forms may not receive lenses. RSpec targets file:line. Minitest uses a supported Rails runner or a focused Ruby method filter. A Run lens is not evidence that the test passed; inspect the process result.",
    contract: "docs/usage.md",
    related: ["frameworks/templates", "extensions"],
  },
];
