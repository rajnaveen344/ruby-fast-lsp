# C4 Architecture Diagrams Skill

Use this skill when creating, updating, or maintaining C4 architecture diagrams for the Ruby Fast LSP project. Provides guidance on proper layering (L1-L4), file organization, LikeC4 syntax, and validation. Triggers: c4 diagrams, architecture visualization, documentation, system design, component diagrams, dynamic views.

---

## C4 Model Layers

The C4 model provides a hierarchical approach to architecture documentation:

```
Level 1: System Context
├─ Shows the big picture
├─ Actors and external systems
└─ High-level interactions

Level 2: Container Diagram
├─ Major building blocks
├─ Technology choices
└─ Container relationships

Level 3: Component Diagram
├─ Internal structure of containers
├─ Component responsibilities
└─ Component interactions

Level 4: Code/Dynamic Views
├─ Implementation details
├─ Sequence flows
└─ State machines
```

### Layer Distribution for Ruby Fast LSP

**Levels 1-2: High-Level Architecture**
- System context (developers, IDE, LSP, external systems)
- Major containers (Server, Index, Docs, Query Layer)
- Technology stack overview
- Primary data flows

**Levels 3-4: In-Depth Visualizations**
- Component breakdowns (handlers, capabilities, indexer internals)
- Dynamic views (request flows, indexing lifecycle, notification handling)
- Detailed interactions between components
- Implementation-specific patterns

---

## C4 Diagram File Organization

All C4 diagrams live in `.ai/diagrams/` and follow a structured organization pattern.

### Directory Structure

```
.ai/diagrams/
├── README.md              # Overview and usage instructions
├── model.c4              # Base model (L1-L2): Specification, actors, containers
├── server.c4             # Server components (L3): Handlers, capabilities, query
├── indexing.c4           # Indexing lifecycle (L3): Indexer, analyzer, visitors
├── requests.c4           # Request flow (L4): Dynamic views for LSP requests
├── notifications.c4      # Notification flow (L4): Dynamic views for LSP notifications
└── [feature].c4          # Feature-specific views (L3-L4)
```

### File Naming Convention

- `model.c4` - Always the base file defining specification and L1-L2
- `[domain].c4` - Named after domain/subsystem (e.g., `indexing.c4`, `inferrer.c4`)
- Use snake_case for multi-word files (e.g., `inlay_hints.c4`)

### File Organization Rules

1. **model.c4** (Base Model)
   - Define specification styles (person, softwareSystem, container, component)
   - Define all actors and external systems
   - Define main software system and its containers
   - L1 and L2 views only

2. **[domain].c4** (Domain Extensions)
   - Extend existing containers with components
   - Define domain-specific relationships
   - Create L3 component views and L4 dynamic views
   - Keep focused on one domain/subsystem

3. **README.md** (Documentation)
   - Table of all C4 files with descriptions
   - C4 level explanations
   - Key architectural concepts
   - Viewing instructions

---

## LikeC4 Syntax Guide

### Model Structure

```c4
// Define specification (only in model.c4)
specification {
    element person { style { shape person color red } }
    element softwareSystem { style { color blue } }
    element container { style { color indigo } }
    element component { style { color slate } }
}

// Define model elements
model {
    // Actors
    developer = person 'Ruby Developer' {
        description 'Uses IDE to write Ruby code.'
    }

    // External systems
    ide = softwareSystem 'IDE' {
        description 'VS Code, Zed, etc.'
    }

    // Your system with containers
    lsp = softwareSystem 'Ruby Fast LSP' {
        server = container 'LSP Server' {
            technology 'Rust / tower-lsp'
            description 'Handles LSP protocol.'
        }
    }

    // Extend existing elements (in other files)
    extend lsp.server {
        handlers = component 'Handlers' {
            technology 'handlers/'
            description 'Routes LSP requests.'
        }
    }

    // Relationships
    ide -> lsp.server 'LSP requests'
    lsp.server.handlers -> lsp.capabilities 'Delegates to'
}
```

### View Types

**1. System Context View (L1)**
```c4
views {
    view context of lsp {
        title 'System Context'
        description 'Shows LSP and external dependencies.'
        include developer, ide, lsp, prism, filesystem
        autoLayout TopBottom
    }
}
```

**2. Container View (L2)**
```c4
views {
    view containers of lsp {
        title 'Containers'
        description 'Major containers within LSP.'
        include *  // All containers
        autoLayout TopBottom
    }
}
```

**3. Component View (L3)**
```c4
views {
    view components of lsp.server {
        title 'Server Components'
        description 'Internal components of LSP Server.'
        include *
        include lsp.capabilities  // Related containers
        autoLayout LeftRight
    }
}
```

**4. Dynamic View (L4)**
```c4
views {
    dynamic view requestFlow {
        title 'LSP Request Flow'
        description 'Shows step-by-step request handling.'

        ide -> lsp.server 'textDocument/definition'
        lsp.server -> lsp.server.handlers 'Route request'
        lsp.server.handlers -> lsp.capabilities.definitions 'handle_definition()'
        lsp.capabilities.definitions -> lsp.query 'find_definitions()'
        lsp.query -> lsp.index 'Lookup symbol'
        lsp.index -> lsp.query 'Location'
        lsp.query -> lsp.capabilities.definitions 'Location'
        lsp.capabilities.definitions -> lsp.server 'Response'
        lsp.server -> ide 'Location response'

        autoLayout TopBottom
    }
}
```

---

## Common Patterns

### Component Hierarchy

**Rule**: Only containers can have components, components cannot have sub-components.

```c4
// ✅ CORRECT: Components inside containers
extend lsp.query {
    definitionQuery = component 'Definition Query' { ... }
    hoverQuery = component 'Hover Query' { ... }
}

// ❌ WRONG: Components inside components
extend lsp.query.definitionQuery {
    helper = component 'Helper' { ... }  // Invalid!
}
```

If you need to show internal structure of a component, use:
1. Comments to document sub-modules
2. Dynamic views to show internal flow
3. Detailed description field

### Relationship Patterns

**Model Block Relationships** (static structure):
```c4
model {
    // Simple relationships
    ide -> lsp.server 'LSP protocol'

    // Component-to-component
    lsp.server.handlers -> lsp.capabilities.definitions 'Delegates'

    // Container-to-container
    lsp.query -> lsp.index 'Queries'
}
```

**Dynamic View Relationships** (execution flow):
```c4
dynamic view flow {
    // Step-by-step interactions
    ide -> lsp.server 'Request'
    lsp.server -> lsp.capabilities 'Delegate'
    lsp.capabilities -> lsp.query 'Query'
    lsp.query -> lsp.index 'Lookup'
    // Return path
    lsp.index -> lsp.query 'Result'
    lsp.query -> lsp.capabilities 'Data'
}
```

---

## Creating New Diagrams

### Step 1: Determine Layer

Ask yourself:
- **Is this showing system boundaries and external dependencies?** → L1 (System Context)
- **Is this showing major building blocks and technology?** → L2 (Container)
- **Is this showing internal components of a container?** → L3 (Component)
- **Is this showing execution flow or detailed interactions?** → L4 (Dynamic View)

### Step 2: Choose File

- L1-L2: Add to or modify `model.c4`
- L3-L4: Create or update domain-specific file (e.g., `indexing.c4`, `requests.c4`)

### Step 3: Define Elements

```c4
model {
    // Only if creating new containers/components
    extend lsp.myContainer {
        myComponent = component 'My Component' {
            technology 'path/to/code.rs'
            description 'Brief description of responsibility.'
        }
    }

    // Define relationships
    lsp.myContainer.myComponent -> lsp.otherContainer 'Interaction'
}
```

### Step 4: Create Views

```c4
views {
    // For L3: Component view
    view components of lsp.myContainer {
        title 'My Container Components'
        description 'Shows internal structure.'
        include *
        include lsp.relatedContainer
        autoLayout LeftRight
    }

    // For L4: Dynamic view
    dynamic view myFlow {
        title 'My Feature Flow'
        description 'Shows execution sequence.'
        // Step-by-step interactions
        autoLayout TopBottom
    }
}
```

### Step 5: Update README

Add entry to the file structure table:
```markdown
| `my_feature.c4` | **My feature.** Description of what this diagram shows. |
```

Add architecture notes if needed in the relevant section.

### Step 6: Validate

Always validate after creating or modifying diagrams:
```bash
npx likec4 validate .ai/diagrams
```

---

## Validation Workflow

### Automatic Validation

Run validation before committing:
```bash
npx likec4 validate .ai/diagrams
```

Expected output (success):
```
version 1.x.x
layout wasm
workspace: file:///path/to/.ai/diagrams
workspace: found N source files
```

Expected output (failure):
```
Invalid /path/to/file.c4
    Line X: Error description
    Line Y: Error description
```

### Common Validation Errors

**1. Duplicate Element Names**
```
Line X: Duplicate element name 'analyzer'
```
**Fix**: Remove duplicate definition, use `extend` in secondary files

**2. Invalid Parent-Child Relationship**
```
Line X: Invalid parent-child relationship
```
**Fix**: Only nest components in containers, not in other components

**3. Could Not Resolve Reference**
```
Line X: Could not resolve 'lsp.foo.bar'
```
**Fix**: Ensure referenced element is defined in model or imported files

**4. Missing Element**
```
Line X: Element 'lsp.server.foo' not found
```
**Fix**: Define the element in a model block before referencing

### Validation Checklist

Before finalizing diagrams:
- [ ] Run `npx likec4 validate .ai/diagrams`
- [ ] No validation errors
- [ ] No duplicate elements across files
- [ ] All relationships point to defined elements
- [ ] Components only nested in containers
- [ ] Proper `extend` usage for cross-file definitions

---

## Best Practices

### 1. Keep Model Files Focused

Each domain file should focus on one subsystem:
- ✅ `indexing.c4` - All indexing-related components and flows
- ✅ `requests.c4` - All LSP request flows
- ❌ `misc.c4` - Catch-all for unrelated components

### 2. Use Extend, Not Duplicate

```c4
// model.c4
lsp = softwareSystem 'Ruby Fast LSP' {
    server = container 'LSP Server' { ... }
}

// server.c4 - ✅ CORRECT
extend lsp.server {
    handlers = component 'Handlers' { ... }
}

// server.c4 - ❌ WRONG
lsp = softwareSystem 'Ruby Fast LSP' {
    server = container 'LSP Server' { ... }  // Duplicate!
}
```

### 3. Technology Field for Implementation Mapping

Always link to actual code:
```c4
definitionQuery = component 'Definition Query' {
    technology 'query/definition.rs'  // Actual file path
    description 'Finds where symbols are defined.'
}
```

### 4. Meaningful Descriptions

```c4
// ✅ GOOD: Explains what and why
description 'Visitor that traverses AST and collects nodes relevant for hints. Does NOT generate hints.'

// ❌ BAD: Just repeats the name
description 'Inlay node collector.'
```

### 5. Auto-Layout Direction

Choose based on natural flow:
- `TopBottom` - For sequential processes (request flows, lifecycles)
- `LeftRight` - For layered architectures (handlers → capabilities → query)
- `BottomTop` - For reverse flows (responses)
- `RightLeft` - Rarely used

### 6. Comments for Internal Structure

When components have complex internals that can't be nested:
```c4
extend lsp.query.inlayHintsQuery {
    // Note: The inlayHintsQuery component contains:
    // - InlayNodeCollector (collector.rs): Visitor that collects AST nodes
    // - InlayNode types (nodes.rs): Data structures for collected nodes
    // - Hint Generators (generators.rs): Convert nodes to hints
    // - HintContext: Provides access to Index and type inference
}
```

### 7. Dynamic Views for Complexity

Use dynamic views to show what can't be shown in static structure:
- Request/response flows
- Multi-step processes
- Conditional logic paths
- Temporal sequences

---

## Viewing Diagrams

### VS Code Extension

Install the [LikeC4 VS Code extension](https://marketplace.visualstudio.com/items?itemName=likec4.likec4):
- Auto-preview on save
- Interactive diagram navigation
- Export to PNG/SVG

### Local Server

Run local preview server:
```bash
npx likec4 serve .ai/diagrams
```

Opens in browser with:
- Interactive exploration
- Zoom and pan
- View switching
- Export options

### Export to Images

```bash
# Export all views
npx likec4 export png .ai/diagrams -o ./diagrams-export/

# Export specific view
npx likec4 export png .ai/diagrams --view systemContext -o ./context.png
```

---

## Maintenance Guidelines

### When to Update Diagrams

Update C4 diagrams when:
- Adding new LSP capabilities
- Refactoring major components
- Changing container responsibilities
- Modifying request/notification flows
- Introducing new architectural patterns

### Review Checklist

When reviewing diagram changes:
- [ ] Correct layer (L1/L2 for high-level, L3/L4 for details)
- [ ] Proper file organization
- [ ] No duplicate elements
- [ ] Relationships are accurate
- [ ] Technology fields updated
- [ ] README.md updated
- [ ] Validation passes
- [ ] Diagrams render correctly

### Keeping Diagrams in Sync

Diagrams should be **living documentation**:
1. Update diagrams in the same PR as code changes
2. Treat diagram validation failures as build failures
3. Review diagrams during architecture reviews
4. Periodically audit diagrams against actual code structure

---

## Examples

### Example 1: Adding a New Capability

When adding `textDocument/codeActions`:

1. **Update server.c4** (L3 - Components)
```c4
extend lsp.capabilities {
    codeActionsCap = component 'Code Actions' {
        technology 'capabilities/code_actions.rs'
        description 'Provides quick fixes and refactorings.'
    }
}

lsp.server.requestHandlers -> lsp.capabilities.codeActionsCap 'Code Actions'
lsp.capabilities.codeActionsCap -> lsp.query 'find_fixable_issues()'
```

2. **Create code_actions.c4** (L4 - Dynamic Views)
```c4
views {
    dynamic view codeActionsFlow {
        title 'Code Actions Request Flow'
        description 'Shows how code actions are generated.'

        ide -> lsp.server 'textDocument/codeActions'
        lsp.server -> lsp.capabilities.codeActionsCap 'handle_code_actions()'
        lsp.capabilities.codeActionsCap -> lsp.query 'find_diagnostics()'
        lsp.query -> lsp.index 'Analyze symbols'
        lsp.index -> lsp.query 'Issues found'
        lsp.query -> lsp.capabilities.codeActionsCap 'Diagnostic[]'
        lsp.capabilities.codeActionsCap -> lsp.server 'CodeAction[]'
        lsp.server -> ide 'Quick fixes'

        autoLayout TopBottom
    }
}
```

3. **Update README.md**
```markdown
| `code_actions.c4` | **Code actions flow.** Dynamic view showing quick fix generation. |
```

4. **Validate**
```bash
npx likec4 validate .ai/diagrams
```

### Example 2: Documenting Existing Complex Flow

For the inlay hints feature:

1. **Don't create sub-components** (InlayNodeCollector, HintGenerators are too granular)
2. **Use comments** to document internal structure
3. **Use dynamic views** to show the flow

```c4
// inlay_hints.c4
model {
    // No additional model elements - inlayHintsQuery already exists
}

views {
    dynamic view inlayHintsFlow {
        title 'Inlay Hints Request Flow'
        // Show the high-level flow
    }

    dynamic view inlayHintsArchitecture {
        title 'Inlay Hints Internal Architecture'
        // Show internal steps with comments documenting:
        // - Collector phase
        // - Node types collected
        // - Generator functions
    }
}
```

---

## Troubleshooting

### Diagrams Not Rendering

**Problem**: Empty or broken diagrams
**Solution**:
- Check validation output for errors
- Ensure all referenced elements are defined
- Verify include statements in views

### Slow Validation

**Problem**: Validation takes too long
**Solution**:
- Keep files focused and small
- Avoid deeply nested structures
- Use comments instead of over-modeling

### Elements Not Appearing in Views

**Problem**: Defined elements don't show in views
**Solution**:
- Check `include` statements in view
- Verify element is defined in model block
- Ensure proper nesting (components in containers only)

---

## Quick Reference

### File Template

```c4
// =============================================================================
// Ruby Fast LSP - [Feature Name] ([Level])
// =============================================================================
// Brief description of what this file documents.
// =============================================================================

model {
    // Extend existing elements
    extend lsp.containerName {
        myComponent = component 'Component Name' {
            technology 'path/to/code.rs'
            description 'Component responsibility.'
        }
    }

    // Define relationships
    lsp.containerName.myComponent -> lsp.otherContainer 'Interaction'
}

// =============================================================================
// VIEWS
// =============================================================================

views {
    // Component view
    view components of lsp.containerName {
        title 'Container Components'
        description 'Shows internal structure.'
        include *
        autoLayout LeftRight
    }

    // Dynamic view
    dynamic view myFlow {
        title 'My Feature Flow'
        description 'Shows execution sequence.'
        // Steps here
        autoLayout TopBottom
    }
}
```

### Validation Command

```bash
# Validate all diagrams
npx likec4 validate .ai/diagrams

# View diagrams locally
npx likec4 serve .ai/diagrams

# Export diagrams
npx likec4 export png .ai/diagrams -o ./output/
```

### Common Element Types

```c4
person          // External users/actors
softwareSystem  // External systems or your main system
container       // Major deployable units
component       // Internal building blocks
```

### Relationship Syntax

```c4
source -> target 'Label'
source -> target.component 'Label'
container.component1 -> container.component2 'Label'
```

---

## Summary

- **L1-L2**: High-level architecture in `model.c4`
- **L3-L4**: Detailed components and flows in domain files
- **Always validate**: `npx likec4 validate .ai/diagrams`
- **Keep focused**: One domain per file
- **Use extend**: Don't duplicate definitions
- **Living documentation**: Update with code changes
