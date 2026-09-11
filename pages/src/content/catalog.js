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
    label: "Navigate",
    overview: "editor",
    summary:
      "Jump to definitions, find references, and rename symbols across files.",
    items: ["navigate/definition", "navigate/rename", "navigate/symbols"],
  },
  {
    label: "Write and edit",
    overview: "editor",
    summary:
      "Complete code, check method arguments, and read information on hover.",
    items: ["editing/completion", "types/hints"],
  },
  {
    label: "Diagnose and fix",
    overview: "editor",
    summary:
      "Find errors while editing, apply quick fixes, and format your code.",
    items: ["diagnostics/live", "diagnostics/fixes"],
  },
  {
    label: "Type inference",
    summary:
      "Explore inferred types, collection elements, and the contents of Hashes.",
    items: [
      "types/propagation",
      "types/hash-shapes",
      "types/signatures",
      "types/unknown",
    ],
  },
  {
    label: "Projects and runtimes",
    overview: "projects",
    summary:
      "Browse indexed code and manage Ruby runtimes across your projects.",
    items: ["projects/indexing", "projects/isolation", "projects/jruby"],
  },
  {
    label: "Frameworks and templates",
    overview: "projects",
    summary:
      "Edit ERB templates, explore Rails views, and run individual tests.",
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
export const TYPE_FEATURES = ["types/propagation", "types/hash-shapes"];
