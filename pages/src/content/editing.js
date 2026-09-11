export default [
  {
    id: "editing/completion",
    title: "Completion and signatures",
    summary:
      "Discover methods on a known receiver and see the arguments a call expects.",
    demo: "editing/completion",
    steps: [
      "Type notebook.ti in the example below.",
      "Choose title from the language-server completion list.",
      "Use Show Hover to inspect its inferred String result; trigger signature help inside a call with parameters.",
    ],
    code: 'class Notebook\n  def title\n    "Field guide"\n  end\nend\n\nnotebook = Notebook.new\nnotebook.title',
    sections: [
      {
        title: "Types guide the candidates",
        body: "Completion uses known receiver evidence, including supported inferred values and signatures. Signature help understands user-defined parameters, nested calls, keywords, rest arguments, and supported RBS overloads.",
      },
      {
        title: "Editing assistance",
        body: "The editor can also show snippets and on-type formatting. Their availability depends on the client and its settings. The demo disables AI, word-based suggestions, and snippets to isolate the language-server result.",
      },
    ],
    limits:
      "An unresolved receiver may not have a precise completion set. Dynamic dispatch and unsupported callable forms can remain Unknown.",
    contract: "docs/usage.md",
    related: ["types/propagation", "types/signatures"],
  },
];
