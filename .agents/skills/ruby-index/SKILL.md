---
name: ruby-index
description: "Debug the Ruby Index tree view, inheritance graph, exported JSON, jq workflows, mixin resolution, included_by, and MRO."
---

# Ruby Index Debugging Skill

Use this skill when debugging the Ruby Index tree view, understanding the inheritance graph, or analyzing the exported JSON with jq. Triggers: ruby index, namespace tree, inheritance graph, export graph, jq debugging, mixin resolution, included_by, MRO.

---

## Overview

The **Ruby Index** is a VS Code tree view that visualizes the project's class/module hierarchy. It shows:

- Classes and modules with their FQNs
- Inheritance relationships (superclass)
- Mixins (includes, prepends, extends)
- Singleton classes (for class-level methods)
- Reverse relationships (which classes include a module)

The data comes from the LSP server's inheritance graph, which can be exported as JSON for debugging.

---

## Tree View Structure

### Node Types

| Node Type           | Icon                                    | Description                             |
| ------------------- | --------------------------------------- | --------------------------------------- |
| `namespace`         | `$(symbol-class)` / `$(symbol-module)`  | Class or module definition              |
| `singleton`         | `$(symbol-class)`                       | Singleton class (class methods context) |
| `mixinSection`      | `$(plug)` / `$(pinned)` / `$(arrow-up)` | Group of mixins                         |
| `mixin`             | `$(symbol-interface)`                   | Individual include/prepend/extend       |
| `includedBySection` | `$(references)`                         | Classes that include this module        |
| `includer`          | `$(symbol-class)`                       | A class that includes the module        |

### Tree Hierarchy Example

```
Foo (Class)
├── Superclass (1)
│   └── Bar
├── Includes (2)
│   ├── Enumerable
│   └── Comparable
├── Singleton
│   └── Includes (1)
│       └── ClassMethods
├── Included By Classes (3)
│   ├── SubFoo
│   ├── AnotherFoo
│   └── ThirdFoo
└── NestedClass (Class)
    └── ...
```

---

## VS Code Commands

### Tree View Actions

| Command             | Shortcut            | Description                            |
| ------------------- | ------------------- | -------------------------------------- |
| `rubyIndex.search`  | `Cmd+Shift+R`       | Search and reveal class/module in tree |
| `rubyIndex.refresh` | Click refresh icon  | Refresh the tree view                  |
| `rubyIndex.export`  | Click download icon | Export inheritance graph as JSON       |

### Search Behavior

The search command (`Cmd+Shift+R`) allows fuzzy search on both:

- **Name**: `Baz` matches `Foo::Bar::Baz`
- **FQN**: `Bar::Baz` matches `Foo::Bar::Baz`

Selection reveals and expands the item in the tree.

---

## Exporting the Inheritance Graph

Click the download icon in the Ruby Index view title bar, or run:

- Command Palette → "Export Index as JSON"

This exports the full inheritance graph with all relationships.

### Export JSON Structure

```json
{
  "node_count": 150,
  "nodes": {
    "MyClass": {
      "kind": "Class",
      "superclass": "BaseClass",
      "includes": ["Enumerable", "Comparable"],
      "prepends": ["PrependedModule"],
      "included_by": [],
      "prepended_by": [],
      "children": ["SubClass"],
      "included_by_classes": [],
      "mro": [
        "MyClass",
        "PrependedModule",
        "Enumerable",
        "Comparable",
        "BaseClass",
        "Object"
      ]
    },
    "MyModule": {
      "kind": "Module",
      "superclass": null,
      "includes": ["AnotherModule"],
      "prepends": [],
      "included_by": ["ClassA", "ModuleB"],
      "prepended_by": [],
      "children": [],
      "included_by_classes": ["ClassA", "ClassC"],
      "mro": ["MyModule", "AnotherModule"]
    },
    "#<Class:MyClass>": {
      "kind": "Class",
      "superclass": "#<Class:BaseClass>",
      "includes": ["ClassMethods"],
      "prepends": [],
      "included_by": [],
      "prepended_by": [],
      "children": [],
      "included_by_classes": [],
      "mro": ["#<Class:MyClass>", "ClassMethods", "#<Class:BaseClass>"]
    }
  }
}
```

### Field Definitions

| Field                 | Description                                             |
| --------------------- | ------------------------------------------------------- |
| `kind`                | "Class" or "Module"                                     |
| `superclass`          | Parent class FQN (classes only)                         |
| `includes`            | Modules included via `include`                          |
| `prepends`            | Modules prepended via `prepend`                         |
| `included_by`         | Direct includers (classes/modules that `include` this)  |
| `prepended_by`        | Direct prependers (classes/modules that `prepend` this) |
| `children`            | Direct subclasses                                       |
| `included_by_classes` | All classes that transitively include this module       |
| `mro`                 | Method Resolution Order (lookup chain)                  |

### Singleton Classes

Singleton classes (for class methods) are represented as `#<Class:ClassName>`:

- `MyClass` → instance context
- `#<Class:MyClass>` → class method context (singleton)

---

## Debugging with jq

Save the exported JSON to a file, then use these jq queries:

### Basic Queries

```bash
# Count total nodes
jq '.node_count' graph.json

# List all class names
jq '.nodes | keys[]' graph.json

# List only classes (not modules)
jq '.nodes | to_entries | map(select(.value.kind == "Class")) | .[].key' graph.json

# List only modules
jq '.nodes | to_entries | map(select(.value.kind == "Module")) | .[].key' graph.json

# Exclude singleton classes from output
jq '.nodes | to_entries | map(select(.key | startswith("#<Class:") | not)) | .[].key' graph.json
```

### Lookup Specific Class/Module

```bash
# Get full info for a class
jq '.nodes["MyClass"]' graph.json

# Get superclass of a class
jq '.nodes["MyClass"].superclass' graph.json

# Get all includes for a class
jq '.nodes["MyClass"].includes' graph.json

# Get MRO (Method Resolution Order)
jq '.nodes["MyClass"].mro' graph.json
```

### Inheritance Analysis

```bash
# Find all subclasses of a class
jq '.nodes["BaseClass"].children' graph.json

# Find all classes that inherit from BaseClass (recursively)
jq --arg base "BaseClass" '
  .nodes | to_entries | map(select(.value.superclass == $base)) | .[].key
' graph.json

# Find all classes without a superclass (root classes)
jq '.nodes | to_entries | map(select(.value.kind == "Class" and .value.superclass == null)) | .[].key' graph.json

# Find classes with deep inheritance (MRO > 5)
jq '.nodes | to_entries | map(select(.value.mro | length > 5)) | .[] | {name: .key, depth: (.value.mro | length)}' graph.json
```

### Mixin Analysis

```bash
# Find all classes/modules that include a specific module
jq '.nodes["Enumerable"].included_by' graph.json

# Find all CLASSES that include a module (transitively)
jq '.nodes["Enumerable"].included_by_classes' graph.json

# Find modules that are never included
jq '.nodes | to_entries | map(select(.value.kind == "Module" and (.value.included_by | length == 0))) | .[].key' graph.json

# Find classes with the most includes
jq '.nodes | to_entries | map(select(.value.kind == "Class")) | sort_by(.value.includes | length) | reverse | .[:10] | .[] | {name: .key, includes: (.value.includes | length)}' graph.json

# Find modules included by the most classes
jq '.nodes | to_entries | map(select(.value.kind == "Module")) | sort_by(.value.included_by_classes | length) | reverse | .[:10] | .[] | {name: .key, included_by: (.value.included_by_classes | length)}' graph.json
```

### Prepend Analysis

```bash
# Find all classes/modules using prepend
jq '.nodes | to_entries | map(select(.value.prepends | length > 0)) | .[] | {name: .key, prepends: .value.prepends}' graph.json

# Find modules that are prepended (not just included)
jq '.nodes | to_entries | map(select(.value.prepended_by | length > 0)) | .[].key' graph.json
```

### Singleton/Class Method Analysis

```bash
# List all singleton classes
jq '.nodes | keys | map(select(startswith("#<Class:")))' graph.json

# Find classes with extends (singleton includes)
jq '.nodes | to_entries | map(select(.key | startswith("#<Class:")) and (.value.includes | length > 0)) | .[] | {class: .key, extends: .value.includes}' graph.json

# Compare instance includes vs class extends for a class
jq '{
  instance_includes: .nodes["MyClass"].includes,
  class_extends: .nodes["#<Class:MyClass>"].includes
}' graph.json
```

### MRO (Method Resolution Order) Analysis

```bash
# Get MRO for a class
jq '.nodes["MyClass"].mro' graph.json

# Find classes where a specific module appears in MRO
jq --arg mod "Enumerable" '
  .nodes | to_entries | map(select(.value.mro | index($mod))) | .[].key
' graph.json

# Find the position of a module in a class's MRO
jq --arg mod "Enumerable" '
  .nodes["MyClass"].mro | to_entries | map(select(.value == $mod)) | .[0].key
' graph.json

# Compare MRO of two classes
jq '{
  class_a: .nodes["ClassA"].mro,
  class_b: .nodes["ClassB"].mro
}' graph.json
```

### Debugging Common Issues

```bash
# Find circular includes (module A includes B, B includes A)
jq '
  .nodes | to_entries | map(
    select(.value.kind == "Module") |
    . as $mod |
    .value.includes | map(
      . as $inc |
      if $mod.key == ($inc | tostring) then empty
      else
        select(
          ($mod.key | tostring) as $modkey |
          .nodes[$inc].includes | index($modkey)
        ) | {a: $mod.key, b: $inc}
      end
    )
  ) | flatten
' graph.json

# Find unresolved mixins (empty string or null)
jq '.nodes | to_entries | map(select(.value.includes | any(. == "" or . == null))) | .[].key' graph.json

# Find classes with duplicate includes
jq '.nodes | to_entries | map(select(.value.includes | group_by(.) | any(length > 1))) | .[].key' graph.json

# Verify MRO starts with self
jq '.nodes | to_entries | map(select(.value.mro[0] != .key)) | .[].key' graph.json
```

### Export to CSV for Analysis

```bash
# Export class hierarchy as CSV
jq -r '.nodes | to_entries | map(select(.value.kind == "Class")) | .[] | [.key, (.value.superclass // "nil"), (.value.includes | join(";"))] | @csv' graph.json > classes.csv

# Export module usage as CSV
jq -r '.nodes | to_entries | map(select(.value.kind == "Module")) | .[] | [.key, (.value.included_by | length), (.value.included_by_classes | length)] | @csv' graph.json > modules.csv
```

---

## Troubleshooting

### Tree View Issues

**Tree is empty:**

- Wait for indexing to complete (check status bar)
- Run `rubyIndex.refresh` command
- Check Output panel for errors

**Class not appearing:**

- Ensure file is saved
- Check if it's a project file (not gem/stdlib)
- Verify syntax is valid Ruby

**Wrong superclass/mixins:**

- Export graph and verify with jq
- Check for reopened class definitions
- Verify constant resolution (absolute vs relative)

### Graph Export Issues

**Missing nodes:**

- Only project files are included (not gems/stdlib)
- Singleton classes only appear if they have mixins

**Wrong included_by_classes:**

- This field traverses through modules to find ultimate class consumers
- Check `included_by` for direct relationships

**Empty MRO:**

- Node might not be in the graph yet
- Check if class/module is fully defined

---

## Architecture Reference

### Data Flow

```
VS Code Extension                    LSP Server
─────────────────                    ──────────
RubyIndexProvider
    │
    ├─► ruby/namespaceTree ────────► namespace_tree.rs
    │   (tree view data)                 │
    │                                    ├─► RubyIndex
    │                                    └─► Graph
    │
    └─► ruby/exportGraph ──────────► debug.rs
        (full graph JSON)                │
                                         ├─► Graph.get_node()
                                         └─► Graph.method_lookup_chain()
```

### Key Files

| File                                 | Purpose                          |
| ------------------------------------ | -------------------------------- |
| `vsix/extension.js`                  | Tree view provider, commands     |
| `src/capabilities/namespace_tree.rs` | Tree data generation             |
| `src/capabilities/debug.rs`          | Graph export, debug commands     |
| `src/indexer/graph.rs`               | Inheritance graph data structure |
| `src/indexer/index.rs`               | Symbol index storage             |

---

## Quick Reference

### Common jq Patterns

```bash
# Filter by kind
jq '.nodes | to_entries | map(select(.value.kind == "Class"))'

# Search by name pattern
jq '.nodes | to_entries | map(select(.key | test("Controller$")))'

# Get specific field for all
jq '.nodes | to_entries | map({name: .key, field: .value.superclass})'

# Count by condition
jq '[.nodes | to_entries | map(select(.value.includes | length > 0))] | length'

# Sort by field length
jq '.nodes | to_entries | sort_by(.value.mro | length) | reverse'
```

### VS Code Shortcuts

| Action            | Shortcut                       |
| ----------------- | ------------------------------ |
| Search Ruby Index | `Cmd+Shift+R`                  |
| Refresh tree      | Click `$(refresh)` icon        |
| Export graph      | Click `$(cloud-download)` icon |
