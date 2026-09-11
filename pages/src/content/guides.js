export default [
  {
    id: "install",
    title: "Install and start",
    summary:
      "Set up Ruby Fast LSP and start using navigation, completion, and diagnostics.",
    steps: [
      "Install Ruby Fast LSP from the VS Code Marketplace or Open VSX.",
      "Open your Ruby project folder and check its indexing status.",
      "Open a Ruby file, try Go to Definition, and start typing a method call to see completions.",
    ],
    sections: [
      {
        title: "VS Code and compatible editors",
        links: [
          {
            label: "Install from VS Code Marketplace",
            href: "https://marketplace.visualstudio.com/items?itemName=naveenraj.ruby-fast-lsp",
          },
          {
            label: "Install from Open VSX",
            href: "https://open-vsx.org/extension/naveenraj/ruby-fast-lsp",
          },
        ],
      },
      {
        title: "Other LSP clients",
        body: "Install the server and configure your client to start it with --stdio and language ID ruby. Follow your client’s supported configuration format.",
        lang: "sh",
        code: "npm install -g @ruby-fast/lsp\nruby-fast-lsp --stdio",
      },
      {
        title: "Build from source",
        lang: "sh",
        code: "cargo build --locked --release --bin ruby-fast-lsp\n./target/release/ruby-fast-lsp --stdio",
      },
    ],
    limits:
      "The VS Code extension includes its own server; other LSP clients use a separate installation. Choose one Ruby language server in your editor to avoid duplicate results.",
    contract: "docs/usage.md",
    related: ["navigate/definition", "editing/completion", "projects/indexing"],
  },
  {
    id: "support",
    title: "Support and limits",
    summary:
      "Check your setup, understand current limitations, and get help with a problem.",
    sections: [
      {
        title: "Start with the current project",
        items: [
          "Run Ruby Fast LSP: Show Runtime Status to inspect runtime selection.",
          "Run Ruby Fast LSP: Show Project Indexing Status to inspect readiness.",
          "Check the Ruby Fast LSP output channel. Temporarily enable debug logs only when needed.",
        ],
      },
      {
        title: "Report a useful issue",
        body: "Include your editor and extension versions, operating system, Ruby version, and a small example that reproduces the problem. Describe what you expected and what happened, and remove any private information.",
        links: [
          {
            label: "Report a bug",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/issues/new/choose",
          },
        ],
      },
      {
        title: "Current limitations",
        body: "Highly dynamic Ruby code may have incomplete navigation or type information. If a required runtime or dependency is unavailable, features that depend on it may be limited.",
      },
      {
        title: "Contribute",
        body: "Help improve Ruby Fast LSP through bug reports, documentation, or code contributions. The contributor guide explains the project structure and how to run its checks.",
        links: [
          {
            label: "Contributor guide",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/blob/main/AGENTS.md",
          },
          {
            label: "Testing and simulation",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/blob/main/docs/development/simulation.md",
          },
          {
            label: "Performance measurements",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/blob/main/docs/development/performance.md",
          },
        ],
      },
    ],
    contract: "docs/usage.md",
    related: ["types/unknown"],
  },
  {
    id: "extensions",
    title: "Framework extensions",
    summary: "Add editor support for Ruby frameworks and their conventions.",
    sections: [
      {
        title: "Bundled integrations",
        body: "Integrations for Rails, RSpec, Minitest, Sinatra, and Cucumber add support for framework conventions. They activate when compatible dependencies are found in your project’s lockfile.",
      },
      {
        title: "Project extensions",
        body: "Add a custom extension when your project needs support for its own conventions. Project extensions run in trusted workspaces.",
      },
      {
        title: "Create an extension",
        body: "The extension guide includes the SDK, examples, and instructions for packaging a project integration.",
        links: [
          {
            label: "Extension authoring guide",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/blob/main/extensions/README.md",
          },
        ],
      },
    ],
    limits:
      "Framework features depend on the versions and conventions supported by each integration. See the extension guide for compatibility details.",
    contract: "extensions/README.md",
    related: ["frameworks/templates", "frameworks/tests"],
  },
];
