export default [
  {
    id: "types/hash-shapes",
    title: "Hash shapes",
    summary:
      "See the fields inside a Hash, while keeping your code readable with compact type hints.",
    demo: "types/hash-shapes",
    steps: [
      "Open the example and inspect the compact Hash hint.",
      "Hover the inlay hint to read the formatted shape.",
      "Read a known key and inspect the resulting type.",
    ],
    code: "measurements = { width: 12.5, height: 8.0, depth: 3.25 }\n\nputs measurements[:width]\n\ndepth = measurements[:depth]\nputs depth",
    sections: [
      {
        title: "Compact outside, detailed inside",
        body: "Inlay hints show a generic summary such as Hash<Symbol, Float>. Hover preserves the full structural shape, with one field per line and nested indentation. Compatible shape alternatives can share a compact label without changing the underlying inferred union.",
      },
      {
        title: "Follow known changes",
        body: "Supported literal-key writes, aliases, merge operations, and collection blocks update the same shape evidence. Key completion, chained calls, and diagnostics consume that result.",
      },
    ],
    limits:
      "Shapes describe Hash-backed values with proven fields. Unknown calls, unsupported mutation, and exceeded bounds invalidate evidence. Symbol and String keys remain distinct. Up to 32 fields, eight nested levels, and eight correlated variants are supported.",
    contract: "docs/features/structural-hash-shapes.md",
    related: ["types/propagation", "types/unknown"],
  },
  {
    id: "types/propagation",
    title: "Type propagation",
    summary:
      "Follow values through constants, collection blocks, and supported method results.",
    demo: "types/propagation",
    steps: [
      "Declare a frozen collection of symbols.",
      "Read it inside a map block and inspect the block element.",
      "Convert the elements to strings and inspect the resulting collection.",
    ],
    code: "module Vocabulary\n  KEYS = [:title, :author].freeze\nend\n\nnames = Vocabulary::KEYS.map do |key|\n  key.to_s\nend\n\nputs names",
    sections: [
      {
        title: "Evidence travels with the value",
        body: "A value constant is not automatically a class object. Known element types flow into supported collection blocks; the block result determines the collection result. Cross-file declarations enter the same analysis.",
      },
      {
        title: "Beyond map",
        body: "Core RBS signatures support collect, filter_map, select/filter/reject, each, and each_with_object. Supported lambdas, procs, yield wrappers, and block forwarding use the same bounded inference path.",
      },
    ],
    limits:
      "The dependent result remains Unknown when required inputs, overloads, or callable bodies cannot be proven. Arbitrary metaprogramming and all possible yielding methods are not supported.",
    contract: "docs/features/higher-order-call-inference.md",
    related: ["types/hash-shapes", "types/signatures"],
  },
  {
    id: "types/hints",
    title: "Hover and clickable types",
    summary:
      "Read inferred types next to your code and follow a type to its declaration.",
    demo: "types/hints",
    steps: [
      "Open the example with inlay hints enabled.",
      "Inspect the variable and method return hints.",
      "Follow a linked type in a hint, or Show Hover on the variable for its name, type, and binding kind.",
    ],
    code: 'class Journal\n  def title\n    "Trail notes"\n  end\nend\n\nentry = Journal.new\nentry.title',
    sections: [
      {
        title: "Hints that fit",
        body: "Long names are shortened in labels, while tooltips retain full type information. Hash hints keep useful generic key/value types; the formatted structure stays in the tooltip.",
      },
      {
        title: "Navigate the type",
        body: "Resolvable type labels can link to their declaration. In VS Code, use the modifier-click behavior displayed by the editor. Ordinary Go to Definition on a local binding still goes to its assignment.",
      },
    ],
    requirements:
      "Enable Editor: Inlay Hints in VS Code. Named types need an indexed declaration to provide a navigation target.",
    limits:
      "An Unknown type cannot supply a concrete type target. Compact labels summarize presentation; they do not remove real union alternatives from analysis.",
    contract: "docs/usage.md",
    related: ["types/hash-shapes", "navigate/definition"],
  },
  {
    id: "types/unknown",
    title: "Explained Unknowns",
    summary:
      "Understand when the server has enough evidence for a type—and when it does not.",
    demo: "types/unknown",
    steps: [
      "Inspect a known Hash value.",
      "Pass the Hash to an unresolved call that may change it.",
      "Hover a subsequent read and inspect the Unknown result and explanation.",
    ],
    code: 'record = { title: "Field guide" }\n\nhand_off(record)\n\ntitle = record[:title]\nputs title',
    intro:
      "An Unknown result is useful information. It tells you where the analysis stopped proving a type instead of silently treating an earlier guess as current.",
    limits:
      "An unresolved call may retain or mutate the Hash. The server invalidates the shape and tracked aliases. Not every uncertain expression has a specific explanation; signatures can supply missing contracts, but cannot prove arbitrary runtime behavior.",
    contract: "docs/features/structural-hash-shapes.md",
    related: ["types/signatures", "diagnostics/live"],
  },
  {
    id: "types/signatures",
    title: "YARD and RBS",
    summary:
      "Supplement inferred Ruby behavior with explicit contracts from documentation and signature files.",
    steps: [
      "Add a YARD return or parameter annotation to a method.",
      "For project signatures, place RBS declarations under sig/.",
      "Inspect hover and signature help at a call.",
    ],
    code: '# @param name [String]\n# @return [String]\ndef greeting(name)\n  "Hello, #{name}"\nend',
    sections: [
      {
        title: "Project RBS",
        body: "Project sig/**/*.rbs files supply ordinary signature facts. Ruby implementations take navigation precedence when both exist. Supported RBS records use the same Hash-shape model.",
        lang: "rbs",
        code: "class Notebook\n  def title: () -> String\nend",
      },
    ],
    limits:
      "Signature support is bounded. Incomplete or conflicting evidence can remain Unknown; an annotation is not a guarantee that runtime code obeys it. Signature sources are navigation inputs and are not editable through project rename.",
    contract: "docs/usage.md",
    related: ["types/propagation", "editing/completion"],
  },
];
