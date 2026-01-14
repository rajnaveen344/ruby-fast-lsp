# Architecture Skill

Use this skill when designing new features, understanding module responsibilities, or making structural changes to the Ruby Fast LSP project. Provides guidance on the 3-layer architecture, module boundaries, and dependency rules. Triggers: architecture, design, module structure, dependencies, layers, new feature, refactoring structure.

---

## 3-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  LAYER 1: API Layer                                         │
│  server.rs + handlers/                                       │
│  - LSP protocol handling                                     │
│  - Request/response routing                                  │
│  - Thin: delegates to capabilities                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  LAYER 2: Service Layer                                      │
│  query/ + capabilities/                                      │
│  - Business logic                                            │
│  - Feature implementations                                   │
│  - IndexQuery as unified interface                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  LAYER 3: Data Layer                                         │
│  indexer/ + types/                                           │
│  - Symbol storage (RubyIndex)                                │
│  - Core types (FQN, RubyType)                                │
│  - Two-phase indexing                                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  FOUNDATION: Analysis Layer                                  │
│  analyzer_prism/ + inferrer/                                 │
│  - AST traversal                                             │
│  - Type inference                                            │
│  - CFG-based analysis                                        │
└─────────────────────────────────────────────────────────────┘
```

---

## Dependency Rules

### Allowed Dependencies (Top to Bottom Only)

```
handlers/ ──────► capabilities/
                      │
                      ▼
                  query/
                      │
         ┌───────────┼───────────┐
         ▼           ▼           ▼
    indexer/   analyzer_prism/  inferrer/
         │           │           │
         └───────────┼───────────┘
                     ▼
                  types/
```

### Forbidden Dependencies

- **No upward dependencies**: indexer/ cannot depend on capabilities/
- **No circular dependencies**: if A depends on B, B cannot depend on A
- **No cross-layer skipping**: handlers/ should not directly use indexer/

---

## Module Responsibilities

### handlers/ (API Layer)

**Purpose**: Route LSP requests/notifications to appropriate handlers.

**Files**:

- `request.rs` - Handle LSP requests (goto, hover, completion, etc.)
- `notification.rs` - Handle LSP notifications (didOpen, didChange, etc.)

**Rules**:

- Maximum 10 lines per handler function
- Delegate immediately to capabilities/
- Only do parameter extraction and response formatting

```rust
// GOOD: Thin handler
pub async fn handle_goto_definition(
    server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    definition::goto_definition(server, &uri, position).await
}
```

### capabilities/ (Service Layer)

**Purpose**: Implement LSP features using query layer.

**Modules**:

- `completion/` - Code completion
- `definition/` - Go-to-definition
- `hover/` - Hover information
- `references/` - Find references
- `inlay_hints/` - Type hints
- `code_lens/` - Code lenses
- `diagnostics/` - Error reporting

**Rules**:

- Each capability should be ~50-200 lines
- Use IndexQuery for all index access
- No direct index manipulation

```rust
// GOOD: Uses IndexQuery
pub async fn goto_definition(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let query = IndexQuery::new(&server.index, uri);
    query.find_definitions_at_position(position)
}
```

### query/ (Service Layer)

**Purpose**: Unified interface for all index queries. Single point of access.

**Key Type**: `IndexQuery`

```rust
pub struct IndexQuery<'a> {
    index: &'a RubyIndex,
    document: Option<&'a RubyDocument>,
}

impl IndexQuery<'_> {
    // Navigation
    pub fn find_definitions_at_position(&self, pos: Position) -> Vec<Location>;
    pub fn find_references_at_position(&self, pos: Position) -> Vec<Location>;

    // Type information
    pub fn get_hover_at_position(&self, pos: Position) -> Option<Hover>;
    pub fn resolve_type_at_position(&self, pos: Position) -> Option<RubyType>;

    // Completion
    pub fn get_completions_at_position(&self, pos: Position) -> Vec<CompletionItem>;
}
```

**Rules**:

- All business logic lives here
- Consolidate scattered query patterns
- Position-based API (elegant abstraction)

### indexer/ (Data Layer)

**Purpose**: Symbol storage and retrieval.

**Key Components**:

- `RubyIndex` - Central in-memory symbol store
- `FileProcessor` - Process single files
- `Coordinator` - Orchestrate workspace indexing

**Two-Phase Indexing Protocol**:

```
Phase 1: Build Definitions
┌─────────────────────────────────────┐
│ For each file:                      │
│   1. Parse with Prism               │
│   2. Extract class/module/method    │
│   3. Store definitions in index     │
└─────────────────────────────────────┘
                 │
                 ▼
Phase 2: Resolve References
┌─────────────────────────────────────┐
│ For each file:                      │
│   1. Resolve constant references    │
│   2. Build inheritance graph        │
│   3. Link method calls to defs      │
└─────────────────────────────────────┘
```

**Rules**:

- No LSP-specific types (use core types only)
- Support incremental updates
- Thread-safe access via RwLock

### analyzer_prism/ (Foundation)

**Purpose**: Ruby code analysis via Prism parser.

**Key Types**:

- `RubyPrismAnalyzer` - Main analyzer
- `Identifier` - Parsed identifier with position
- `MethodReceiver` - Receiver type for method calls

**Rules**:

- Visitor pattern for AST traversal
- Return rich types, not raw AST nodes
- Position tracking for all identifiers

### inferrer/ (Foundation)

**Purpose**: Type inference engine.

**Key Components**:

- `RubyType` - Type representation
- `TypeQuery` - Query types at positions
- `cfg/` - Control flow graph for type narrowing

**Rules**:

- Support YARD annotations
- Support RBS type definitions
- Handle union types gracefully

### types/ (Foundation)

**Purpose**: Core type definitions used everywhere.

**Key Types**:

- `FullyQualifiedName` (FQN) - Validated symbol names
- `RubyConstant` - Validated constant names
- `RubyMethod` - Validated method names
- `Location`, `Position`, `Range` - LSP primitives

**Rules**:

- Validate at construction time
- Immutable after creation
- Use `Ustr` for interned strings

---

## Adding a New LSP Capability

### Step 1: Create capability module

```
src/capabilities/my_feature/
├── mod.rs          # Main entry point
└── helpers.rs      # Optional helpers
```

### Step 2: Implement using IndexQuery

```rust
// src/capabilities/my_feature/mod.rs
use crate::query::IndexQuery;

pub async fn handle_my_feature(
    server: &RubyLanguageServer,
    params: MyFeatureParams,
) -> Option<MyFeatureResponse> {
    let query = IndexQuery::new(&server.index, &params.uri);

    // Use query methods
    let result = query.find_something(params.position)?;

    Some(result.into())
}
```

### Step 3: Add handler routing

```rust
// src/handlers/request.rs
MyFeatureRequest::METHOD => {
    let params: MyFeatureParams = serde_json::from_value(params)?;
    let result = my_feature::handle_my_feature(&server, params).await;
    Ok(serde_json::to_value(result)?)
}
```

### Step 4: Register capability

```rust
// src/server.rs - in initialize()
capabilities.my_feature_provider = Some(MyFeatureOptions::default());
```

---

## Adding to IndexQuery

When capabilities need new query patterns, add to IndexQuery:

````rust
// src/query/mod.rs
impl IndexQuery<'_> {
    /// Find all symbols matching the given pattern.
    ///
    /// # Example
    /// ```
    /// let symbols = query.find_symbols_matching("User*");
    /// ```
    pub fn find_symbols_matching(&self, pattern: &str) -> Vec<Symbol> {
        // Implementation using self.index
    }
}
````

---

## File Size Guidelines

| Module           | Target    | Max       | Current Status         |
| ---------------- | --------- | --------- | ---------------------- |
| Handler files    | 50 lines  | 100 lines | OK                     |
| Capability entry | 100 lines | 300 lines | Some exceed            |
| Query methods    | 50 lines  | 100 lines | OK                     |
| Indexer files    | 200 lines | 500 lines | coordinator.rs exceeds |
| Analyzer         | 500 lines | 800 lines | mod.rs exceeds (2420)  |

---

## Architectural Decisions Record

When making structural changes, document:

1. **Context**: What problem are we solving?
2. **Decision**: What approach did we choose?
3. **Consequences**: What are the tradeoffs?

Example:

```markdown
## ADR-001: Unified Query Layer

### Context

Query logic was scattered across capabilities, leading to duplication.

### Decision

Create IndexQuery as single entry point for all index queries.

### Consequences

- (+) Consistent API across features
- (+) Easier to add new query patterns
- (-) One more layer of indirection
```
