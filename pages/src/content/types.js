export default [
  {
    id: "types/hash-shapes",
    title: "Hash shapes",
    summary: "Explore the keys and value types inside your Hashes.",
    demo: "types/hash-shapes",
    steps: [
      "Create a Hash and inspect its type hint.",
      "Hover the hint to see its keys and value types.",
      "Access a key and hover the result to inspect its type.",
    ],
    code: "measurements = { width: 12.5, height: 8.0, depth: 3.25 }\n\nputs measurements[:width]\n\ndepth = measurements[:depth]\nputs depth",
    sections: [
      {
        title: "See the contents at a glance",
        body: "A Hash shape describes its known keys and the type of each value. This helps you understand structured data and explore nested values from the editor.",
      },
      {
        title: "Choose the level of detail",
        body: "Inline hints show a compact summary such as Hash<Symbol, Float>. Hover for a formatted list of fields, including nested Hashes.",
      },
    ],
    limits:
      "Shape information is available when the server can follow the Hash’s contents. Complex transformations or unknown method calls may leave some types unavailable.",
    contract: "docs/features/structural-hash-shapes.md",
    related: ["types/propagation", "types/unknown"],
  },
  {
    id: "types/propagation",
    title: "Type inference",
    summary:
      "Discover types from your Ruby code, including method results and collection elements.",
    demo: "types/propagation",
    steps: [
      "Open Ruby code that assigns a value or calls a method.",
      "Inspect the inferred type in a hint or hover.",
      "Follow the value through a collection operation to see the resulting type.",
    ],
    code: "module Vocabulary\n  KEYS = [:title, :author].freeze\nend\n\nnames = Vocabulary::KEYS.map do |key|\n  key.to_s\nend\n\nputs names",
    sections: [
      {
        title: "Understand values without extra annotations",
        body: "The server uses assignments, constants, and method bodies to infer types. These types help provide completion, hover information, and diagnostics throughout your code.",
      },
      {
        title: "Work with collections",
        body: "Inspect the element types of arrays and the values used inside collection blocks. Operations such as map can produce a new collection type based on the block’s result.",
      },
    ],
    limits:
      "Type inference covers supported Ruby expressions and method signatures. Highly dynamic code or missing information can leave a type unknown.",
    contract: "docs/features/higher-order-call-inference.md",
    related: ["types/hash-shapes", "types/signatures"],
  },
  {
    id: "types/hints",
    title: "Hover and type hints",
    summary:
      "Read information about your code on hover and see types beside variables and methods.",
    demo: "types/hints",
    steps: [
      "Hover a method, variable, or constant to inspect it.",
      "Enable inlay hints to see type information alongside your code.",
      "Modifier-click a linked type in a hint to open its declaration.",
    ],
    code: 'class Journal\n  def title\n    "Trail notes"\n  end\nend\n\nentry = Journal.new\nentry.title',
    sections: [
      {
        title: "Read code in context",
        body: "Hover shows information about the symbol under your cursor, such as its type or method signature. Variable hovers include the name and kind of variable.",
      },
      {
        title: "Keep useful types in view",
        body: "Inlay hints show variable and return types without adding annotations to your source. Hover a hint for the full type name or more detail about a structured value.",
      },
      {
        title: "Explore a type",
        body: "Linked type names in hints take you to their declaration. Follow your editor’s modifier-click shortcut to open them.",
      },
    ],
    requirements:
      "Enable Editor: Inlay Hints in VS Code to display inline types.",
    limits:
      "Type information depends on the code available to the server. A type name is clickable when its declaration can be found.",
    contract: "docs/usage.md",
    related: ["types/hash-shapes", "navigate/definition"],
  },
  {
    id: "types/unknown",
    title: "Understanding unknown types",
    summary:
      "Understand why a type may be unavailable and where to look for more information.",
    demo: "types/unknown",
    steps: [
      "Find a value whose type is shown as ?.",
      "Hover it to see the available type information and explanation.",
      "Check the surrounding code and any relevant method signatures.",
    ],
    code: 'record = { title: "Field guide" }\n\nhand_off(record)\n\ntitle = record[:title]\nputs title',
    intro:
      "An Unknown type means the server does not have enough information to determine the value’s type. You can still explore the code and use other editor features.",
    limits:
      "Not every unknown type has a detailed explanation. Some Ruby behavior can only be determined at runtime.",
    contract: "docs/features/structural-hash-shapes.md",
    related: ["types/signatures", "diagnostics/live"],
    sections: [
      {
        title: "Add context when needed",
        body: "Check that dependencies are available and indexing has completed. YARD annotations and RBS signatures can provide type information for methods the server cannot infer on its own.",
      },
    ],
  },
  {
    id: "types/signatures",
    title: "YARD and RBS",
    summary:
      "Describe method parameters and return types with YARD annotations or RBS files.",
    steps: [
      "Add a YARD return or parameter annotation to a method.",
      "For project signatures, place RBS declarations under sig/.",
      "Inspect hover and signature help at a call.",
    ],
    code: '# @param name [String]\n# @return [String]\ndef greeting(name)\n  "Hello, #{name}"\nend',
    sections: [
      {
        title: "Keep signatures alongside your project",
        body: "Use YARD comments in Ruby files or place RBS declarations under sig/. The server uses supported signatures to provide type information in hover, completion, and signature help.",
        lang: "rbs",
        code: "class Notebook\n  def title: () -> String\nend",
      },
    ],
    limits:
      "Not every YARD or RBS form is supported. Keep signatures in sync with the Ruby code they describe.",
    contract: "docs/usage.md",
    related: ["types/propagation", "editing/completion"],
  },
];
