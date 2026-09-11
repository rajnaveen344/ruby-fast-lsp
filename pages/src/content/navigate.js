export default [
  {
    id: "navigate/definition",
    title: "Definitions and references",
    summary:
      "Move from a use to its source and find where that source is used.",
    demo: "navigate/definition",
    steps: [
      "Place the cursor on a method call or local binding.",
      "Use Go to Definition (F12) or modifier-click.",
      "Use Find All References to inspect the related uses.",
    ],
    code: "class Compass\n  def heading\n    :north\n  end\nend\n\ncompass = Compass.new\ncompass.heading",
    sections: [
      {
        title: "The clicked token matters",
        body: "A local variable used inside brackets navigates to its binding, not the enclosing [] method. A binding can be navigable even when its value type is unknown.",
      },
      {
        title: "Several valid definitions",
        body: "The engine resolves identity and ranks valid targets semantically. A known receiver uses its effective implementation; ambiguous contexts can return multiple candidates. Path ordering is only a deterministic tie-breaker, not the meaning of the result.",
      },
      {
        title: "More ways to explore",
        body: "Use Go to Implementation, incoming/outgoing call hierarchy, and type hierarchy for related declarations. Document highlights show same-file occurrences of the selected symbol.",
      },
    ],
    limits:
      "Dynamic calls and incomplete lookup chains may leave several candidates or no proven target. Dependency navigation retains the owning project context.",
    contract: "docs/features/definition-navigation.md",
    related: ["navigate/rename", "navigate/symbols"],
  },
  {
    id: "navigate/rename",
    title: "Rename across files",
    summary: "Change a proven symbol and its editable references together.",
    demo: "navigate/rename",
    steps: [
      "Place the cursor on an unambiguous method or constant.",
      "Run Rename Symbol (F2), enter a valid Ruby name, and review the changes.",
      "Confirm that declarations and their resolved uses changed together.",
    ],
    code: "class DirectionFinder\n  def heading\n    :north\n  end\nend\n\nDirectionFinder.new.heading",
    limits:
      "Rename is deliberately conservative. Ambiguous targets, external/generated declarations, unsupported operator transformations, collisions, and certain coupled override families are rejected rather than producing a partial unsafe edit.",
    contract: "AGENTS.md",
    related: ["navigate/definition"],
  },
  {
    id: "navigate/symbols",
    title: "Symbols and code structure",
    summary:
      "Find declarations without remembering their file, then explore their namespace and structure.",
    steps: [
      "Use Go to Symbol in Editor for declarations in the current file.",
      "Use Go to Symbol in Workspace for project-wide search.",
      "Open the Ruby Index view to explore classes, modules, and their members.",
    ],
    sections: [
      {
        title: "Selection and folding",
        body: "Expand Selection follows nested syntax ranges. Folding ranges collapse methods, classes, and other supported regions. Semantic highlighting distinguishes symbols that plain text coloring can confuse.",
      },
      {
        title: "Project ownership",
        body: "Workspace symbol search aggregates isolated project engines. External dependencies remain navigation inputs; the project-only namespace view does not promote them into editable project sources.",
      },
    ],
    limits:
      "The exact command names and keyboard shortcuts vary by editor. Indexing must have discovered the relevant declarations.",
    contract: "docs/usage.md",
    related: ["navigate/definition", "projects/indexing"],
  },
];
