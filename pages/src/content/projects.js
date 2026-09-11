export default [
  {
    id: "projects/indexing",
    title: "Background indexing",
    summary:
      "Browse project code and dependencies while indexing runs in the background.",
    demo: "projects/indexing",
    steps: [
      "Open a Ruby project folder.",
      "Check the Ruby status item for indexing progress.",
      "Open the status view for details, or start exploring the indexed code.",
    ],
    sections: [
      {
        title: "Explore more of your project",
        body: "Indexing discovers classes, modules, methods, and dependencies so you can navigate beyond the files you have open.",
      },
      {
        title: "Keep working",
        body: "Continue editing while the rest of your project is indexed. Use the status view to see progress and which projects are ready.",
      },
    ],
    limits:
      "Initial indexing time varies with project size, dependencies, and your machine. Some results may be unavailable until the relevant files have been indexed.",
    contract: "docs/usage.md",
    related: ["projects/isolation", "projects/jruby"],
    demoTitle: "Inspect project indexing status",
  },
  {
    id: "projects/isolation",
    title: "Multiple projects",
    summary:
      "Work across Ruby projects with their own dependencies and runtime settings.",
    demo: "projects/isolation",
    steps: [
      "Open a workspace containing your Ruby projects.",
      "Use the indexing status view to inspect the detected projects.",
      "Switch between projects and select the runtime each one needs.",
    ],
    sections: [
      {
        title: "Keep project context",
        body: "Navigation, completion, and diagnostics use the dependencies and runtime selected for the file’s project. This lets you work on different applications in the same workspace.",
      },
      {
        title: "Select a runtime",
        body: "Run Ruby Fast LSP: Select Runtime for the active project. Automatic selection uses the project’s runtime markers; you can also choose an installed runtime.",
      },
    ],
    limits:
      "Project detection uses the workspace layout and Gemfiles. Check the status view if a file is being associated with an unexpected project.",
    contract: "docs/usage.md",
    related: ["projects/indexing", "projects/jruby"],
  },
  {
    id: "projects/jruby",
    title: "JRuby and Java",
    summary: "Explore Java classes and methods from your JRuby code.",
    demo: "projects/jruby",
    steps: [
      "Select the JRuby runtime for your project.",
      "Open Ruby code that imports a Java class.",
      "Hover or use Go to Definition on a Java reference.",
    ],
    code: 'java_import "java.util.ArrayList"\n\nitems = ArrayList.new\nitems.add("Field guide")',
    requirements:
      "Install a compatible JRuby runtime and JDK, and make your project’s Java dependencies available.",
    limits:
      "Java navigation depends on the available classpath and supported Java integration forms. This feature requires a JRuby project.",
    contract: "docs/usage.md",
    related: ["projects/isolation"],
  },
];
