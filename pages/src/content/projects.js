export default [
  {
    id: "projects/indexing",
    title: "Background indexing",
    summary:
      "Open a Ruby workspace and keep track of what is ready to navigate.",
    demo: "projects/indexing",
    steps: [
      "Open a project folder and watch the Ruby status item.",
      "Click the indexing status to inspect the project details.",
      "Open a file and begin navigating once its declarations are available.",
    ],
    sections: [
      {
        title: "Files and dependencies",
        body: "Indexing discovers project sources, core declarations, and selected dependencies. A standalone folder without a Gemfile receives core analysis without searching unrelated globally installed gems.",
      },
      {
        title: "Keep editing",
        body: "Open buffers take precedence over disk. Delayed background work cannot overwrite a newer accepted interactive revision. The status view distinguishes progress for discovered projects.",
      },
    ],
    limits:
      "Readiness and resource use depend on the corpus, runtime, dependencies, and machine. Edited demo timings are not indexing benchmarks.",
    contract: "docs/usage.md",
    related: ["projects/isolation", "projects/jruby"],
    demoTitle: "Inspect project indexing status",
  },
  {
    id: "projects/isolation",
    title: "Isolated projects",
    summary:
      "Work with several Ruby projects without mixing their declarations, dependencies, or runtimes.",
    demo: "projects/isolation",
    steps: [
      "Open an umbrella folder containing two nearest project Gemfiles.",
      "Inspect the independently discovered projects in indexing status.",
      "Switch between source files and inspect a same-named constant with different definitions.",
    ],
    sections: [
      {
        title: "A folder can contain several projects",
        body: "A root Gemfile owns its folder. Without one, nearest nested Gemfiles define isolated project roots. Requests route to the deepest owning root.",
      },
      {
        title: "Select the right runtime",
        body: "Run Ruby Fast LSP: Select Runtime for the active project. Auto follows exact project markers. Saving a runtime to .ruby-version is a separate confirmed project write; the default selection stays in private editor state.",
      },
    ],
    limits:
      "Each project has its own semantic engine. Immutable dependency products may be shared, but source identity and query ownership stay separate. Ambiguous external files do not borrow an arbitrary project context.",
    contract: "docs/usage.md",
    related: ["projects/indexing", "projects/jruby"],
  },
  {
    id: "projects/jruby",
    title: "JRuby and Java",
    summary:
      "Navigate supported Java types and methods from a project using JRuby.",
    demo: "projects/jruby",
    steps: [
      "Select the exact installed JRuby runtime for the demo project.",
      "Open a Ruby file importing a supported Java class with java_import.",
      "Hover or navigate a Java proxy reference after its classpath is indexed.",
    ],
    code: 'java_import "java.util.ArrayList"\n\nitems = ArrayList.new\nitems.add("Field guide")',
    requirements:
      "A compatible JRuby runtime and JDK must be available. Java classpath inputs belong to the selected project. Runtime detection does not certify every Ruby/JDK combination.",
    limits:
      "Static classfile facts support navigation; they do not execute application artifacts. Implementation navigation may use bounded decompilation and depends on available inputs. MRI projects do not gain Java semantics just from a similar-looking constant.",
    contract: "docs/usage.md",
    related: ["projects/isolation"],
  },
];
