export default [
  {
    id: "frameworks/templates",
    title: "ERB and Rails views",
    summary:
      "Work with Ruby and HTML in ERB templates and navigate Rails views.",
    demo: "frameworks/templates",
    steps: [
      "Open an ERB template.",
      "Hover Ruby expressions or use Go to Definition inside Ruby tags.",
      "Use HTML completion for markup and Open View links in supported Rails controllers.",
    ],
    code: '<% title = "Trail notes" %>\n\n<h1><%= title %></h1>\n\n<p>A place for observations.</p>',
    sections: [
      {
        title: "Edit Ruby and HTML together",
        body: "Use Ruby hover and navigation inside ERB tags. In VS Code, HTML completion, hover, symbols, and folding help you work with the surrounding markup.",
      },
      {
        title: "Find a Rails view",
        body: "Open View links on supported controller actions take you to the corresponding template, making it easier to move between an action and its view.",
      },
    ],
    limits:
      "Ruby formatting and linting are not available for whole ERB templates. Type information for values shared across Ruby tags may be incomplete. Rails navigation follows supported view conventions.",
    contract: "docs/usage.md",
    related: ["frameworks/tests", "extensions"],
    demoTitle: "Navigate Ruby inside an ERB template",
    lang: "erb",
  },
  {
    id: "frameworks/tests",
    title: "Run focused tests",
    summary:
      "Run individual RSpec examples and Minitest tests from your editor.",
    demo: "frameworks/tests",
    steps: [
      "Open a test file in your project.",
      "Click Run above an example or test method.",
      "Read the test output in the editor terminal.",
    ],
    code: 'require "minitest/autorun"\n\nclass NotebookTest < Minitest::Test\n  def test_title\n    notebook = { title: "Field guide" }\n    assert_equal "Field guide", notebook[:title]\n  end\nend',
    requirements:
      "Install a supported RSpec or Minitest version and keep your project’s lockfile up to date. Debugging also needs the debug gem and a compatible debugger extension.",
    limits:
      "Run links are available for supported test declarations. Tests generated dynamically may need to be run from the terminal.",
    contract: "docs/usage.md",
    related: ["frameworks/templates", "extensions"],
  },
];
