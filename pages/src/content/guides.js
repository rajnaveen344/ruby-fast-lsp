export default [
  {
    id: "install",
    title: "Install and start",
    summary:
      "Open your Ruby project with a language server that follows its code and types.",
    steps: [
      "Install Ruby Fast LSP from the VS Code Marketplace or Open VSX.",
      "Open your Ruby project folder and wait for its indexing status.",
      "Open a Ruby file, inspect an inferred type, and try Go to Definition.",
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
      "The npm server and the server bundled inside the VS Code extension are separate installations. Avoid enabling overlapping Ruby providers while evaluating a feature.",
    contract: "docs/usage.md",
    related: ["types/hash-shapes", "projects/indexing"],
  },
  {
    id: "support",
    title: "Support and limits",
    summary:
      "Check the active project, inspect the evidence, and share a small reproducible example.",
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
        body: "Include editor/extension/server versions, OS, Ruby runtime, project layout, whether indexing finished, and a small generic reproduction. Share expected and actual results. Private source and business examples are unnecessary.",
        links: [
          {
            label: "Report a bug",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/issues/new/choose",
          },
        ],
      },
      {
        title: "Understand the boundary",
        body: "Ruby reflection, eval, unconstrained method_missing, unsupported mutation, and exceeded inference bounds can remain Unknown. A missing runtime still retains conservative core declarations, but runtime-dependent modules may be unavailable.",
      },
      {
        title: "Contribute",
        body: "Correctness checks, lifecycle simulation, and release validation cover different contracts. A feature demonstration shows one editor workflow; it is not a replacement for regression coverage.",
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
    summary:
      "Add framework knowledge through validated extension facts and editor responses.",
    sections: [
      {
        title: "Bundled integrations",
        body: "Rails, RSpec, Minitest, Sinatra, and Cucumber integrations activate against their declared locked dependency ranges. Each manifest describes its supported capabilities.",
      },
      {
        title: "Project-local extensions",
        body: "Trusted projects may provide manifest packages under .ruby-fast-lsp/extensions/*/extension.toml or ruby_fast_lsp/**/extension.toml. Compatibility, checksums, permissions, and resource limits are validated before execution.",
      },
      {
        title: "Author an extension",
        body: "Use the existing guest SDK and framework examples. Extensions contribute facts through the ordinary file lifecycle; they do not own a second semantic database.",
        links: [
          {
            label: "Extension authoring guide",
            href: "https://github.com/rajnaveen344/ruby-fast-lsp/blob/main/extensions/README.md",
          },
        ],
      },
    ],
    limits:
      "Untrusted workspaces do not run project-local Wasm. Manifest activation ranges describe supported inputs, not an exhaustive compatibility certification.",
    contract: "extensions/README.md",
    related: ["frameworks/templates", "frameworks/tests"],
  },
];
