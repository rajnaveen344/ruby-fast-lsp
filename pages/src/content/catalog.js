import types from "./types.js";
import navigate from "./navigate.js";
import editing from "./editing.js";
import diagnostics from "./diagnostics.js";
import projects from "./projects.js";
import frameworks from "./frameworks.js";
import guides from "./guides.js";

export const REPOSITORY = "https://github.com/rajnaveen344/ruby-fast-lsp";
export const GROUPS = [
  {
    label: "Get started",
    items: ["install"],
  },
  {
    label: "Understand types",
    feature: true,
    summary: "Follow evidence from Ruby values to useful editor types.",
    items: [
      "types/hash-shapes",
      "types/propagation",
      "types/hints",
      "types/unknown",
      "types/signatures",
    ],
  },
  {
    label: "Navigate",
    feature: true,
    summary: "Find the declaration, trace its uses, and change names safely.",
    items: ["navigate/definition", "navigate/rename", "navigate/symbols"],
  },
  {
    label: "Write and edit",
    feature: true,
    summary: "Discover receiver methods and fill in call arguments.",
    items: ["editing/completion"],
  },
  {
    label: "Diagnose and fix",
    feature: true,
    summary: "Read current diagnostics and apply supported safe edits.",
    items: ["diagnostics/live", "diagnostics/fixes"],
  },
  {
    label: "Projects and runtimes",
    feature: true,
    summary: "Keep project facts, dependencies, and runtimes separate.",
    items: ["projects/indexing", "projects/isolation", "projects/jruby"],
  },
  {
    label: "Frameworks and templates",
    feature: true,
    summary: "Work inside templates and run tests where they are declared.",
    items: ["frameworks/templates", "frameworks/tests"],
  },
  {
    label: "Reference",
    items: ["extensions", "support"],
  },
];
export const PAGES = Object.fromEntries(
  [
    ...types,
    ...navigate,
    ...editing,
    ...diagnostics,
    ...projects,
    ...frameworks,
    ...guides,
  ].map((page) => [
    page.id,
    {
      ...page,
      group: GROUPS.find((group) => group.items.includes(page.id)).label,
    },
  ]),
);
export const STORIES = [
  "types/propagation",
  "types/hash-shapes",
  "types/hints",
  "types/unknown",
];
