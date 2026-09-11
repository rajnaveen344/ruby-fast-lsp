export default [
  {
    id: "navigate/definition",
    title: "Definitions and references",
    summary: "Jump to definitions and find references throughout your project.",
    demo: "navigate/definition",
    steps: [
      "Place the cursor on a method, variable, or constant.",
      "Use Go to Definition (F12) or modifier-click to open its declaration.",
      "Use Find All References to see where it is used.",
    ],
    code: "class Compass\n  def heading\n    :north\n  end\nend\n\ncompass = Compass.new\ncompass.heading",
    sections: [
      {
        title: "Find your way through the code",
        body: "Open the declaration of a method, class, constant, or variable directly from its use. Follow references to understand how a piece of code fits into the rest of your project.",
      },
      {
        title: "Explore related code",
        body: "Go to Implementation and type hierarchy help you explore inheritance. Call hierarchy shows which methods call a method and which methods it calls.",
      },
      {
        title: "Choose a destination",
        body: "When more than one definition is possible, your editor shows the available destinations so you can choose which to open.",
      },
    ],
    limits:
      "Navigation depends on the code and dependencies available to the server. Dynamically defined methods may have several possible destinations or none.",
    contract: "docs/features/definition-navigation.md",
    related: ["navigate/rename", "navigate/symbols"],
  },
  {
    id: "navigate/rename",
    title: "Rename across files",
    summary: "Rename a symbol and update its references across your project.",
    demo: "navigate/rename",
    steps: [
      "Place the cursor on the symbol you want to rename.",
      "Run Rename Symbol (F2) and enter the new name.",
      "Review the proposed changes, then apply them.",
    ],
    code: "class DirectionFinder\n  def heading\n    :north\n  end\nend\n\nDirectionFinder.new.heading",
    limits:
      "Some symbols cannot be renamed automatically, including ambiguous references, naming conflicts, and declarations in external or generated files.",
    contract: "docs/features/definition-navigation.md",
    related: ["navigate/definition"],
    sections: [
      {
        title: "Review changes together",
        body: "Rename updates the declaration and the references the server can identify. Use your editor’s rename preview to review the affected files before applying the changes.",
      },
    ],
  },
  {
    id: "navigate/symbols",
    title: "Symbols and code structure",
    summary:
      "Find classes, modules, and methods without remembering their file names.",
    steps: [
      "Use Go to Symbol in Editor for declarations in the current file.",
      "Use Go to Symbol in Workspace for project-wide search.",
      "Open the Ruby Index view to explore classes, modules, and their members.",
    ],
    sections: [
      {
        title: "Search your project",
        body: "Search symbols in the current file or across your workspace. The Ruby Index view lets you browse classes, modules, and their members.",
      },
      {
        title: "Read the structure",
        body: "Fold methods and classes to focus on the surrounding code. Expand Selection selects larger parts of an expression, while semantic highlighting helps distinguish different kinds of symbols.",
      },
    ],
    limits:
      "Available commands and shortcuts vary by editor. Search results expand as your project is indexed.",
    contract: "docs/usage.md",
    related: ["navigate/definition", "projects/indexing"],
  },
];
