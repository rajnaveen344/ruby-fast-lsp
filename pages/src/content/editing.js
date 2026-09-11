export default [
  {
    id: "editing/completion",
    title: "Completion and signatures",
    summary:
      "Complete local variables and method names, and see which arguments a method expects.",
    demo: "editing/completion",
    steps: [
      "Start typing a local variable or method name, or use Trigger Suggest.",
      "Choose a suggestion to complete the name.",
      "Use signature help while entering arguments to check the method’s parameters.",
    ],
    code: 'class Notebook\n  def title\n    "Field guide"\n  end\nend\n\nnotebook = Notebook.new\nnotebook.title',
    sections: [
      {
        title: "Find variables and methods",
        body: "Completion offers local variables in the current scope and methods available on an object. As you type, narrow the list and choose a name without leaving the editor.",
      },
      {
        title: "Check arguments as you type",
        body: "Signature help shows method parameters, including keyword and optional arguments. Hover over a method for more information about its signature and result.",
      },
      {
        title: "Use your editor’s writing tools",
        body: "Snippets and on-type formatting provide additional help while writing Ruby. Enable them through your editor’s settings.",
      },
    ],
    limits:
      "Suggestions can be less precise when an object’s type is unknown or methods are created dynamically.",
    contract: "docs/usage.md",
    related: ["types/hints", "types/propagation", "types/signatures"],
  },
];
