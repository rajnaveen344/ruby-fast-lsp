# Ruby Fast LSP Architecture

This document describes the architecture of the Ruby Fast LSP server. The codebase is organized into several key components, each with a distinct responsibility.

## High-Level Architecture

The Ruby Fast LSP server follows a modular architecture with clear separation of concerns:

```
src/
├── analyzer_prism/ - Ruby AST analysis and visitors
├── capabilities/   - LSP feature implementations (AST-only logic)
├── indexer/        - Data Layer: Global symbol indexing and workspace tracking
├── query/          - Service Layer: Unified RubyIndex query engine
├── inferrer/       - Type inference and RBS integration
├── handlers/       - LSP request/notification routing
├── types/          - Core data structures (FQN, Document, Method)
├── server.rs       - LSP server coordination
└── main.rs         - Application entry point
tests/
└── fixtures/       - Ruby files to be tested by the LSP
```

### Core Philosophy

1. **Separation of Concerns**: Each module has a clear, focused responsibility
2. **Loose Coupling**: Components interact through well-defined interfaces
3. **Single Responsibility**: Each file handles one aspect of the system

## Component Responsibilities

### 1. Indexer (`src/indexer/`)

The Indexer is responsible for building and maintaining an index of Ruby symbols across the workspace.

- **Primary Responsibility**: Track the location of all symbols in the workspace
- **Secondary Responsibility**: Handle file events (open, change, close)

#### Key Files:

- `entry.rs`: Defines the structure for index entries (classes, methods, etc.)
- `index.rs`: Core index data structure that maps symbols to their locations
- `traverser.rs`: Traverses Ruby AST to build the index
- `events.rs`: Handles workspace indexing and file change events

#### Design Decisions:

- The indexer maintains maps of symbols to their locations for efficient lookup
- The indexer does not perform code analysis - it just stores locations
- File operations are separated from the core indexing logic

### 2. Analyzer (`src/analyzer_prism/`)

The Analyzer is responsible for understanding Ruby code structure using the Prism parser.

- **Primary Responsibility**: Provide AST visitors for different analysis tasks (indexing, references, symbols)
- **Secondary Responsibility**: Extract semantic information from Ruby source code

#### Key Files:

- `mod.rs`: Central module file and Identifier resolution
- `scope_tracker.rs`: Tracks current namespace and scope during traversal
- `visitors/`: A collection of specialized visitors for different LSP features

#### Design Decisions:

- Uses the Visitor pattern for efficient AST traversal
- Separates analysis logic from feature implementation (Capabilities)
- Stateless analysis: processes one document at a time

### 3. Capabilities (`src/capabilities/`)

Capabilities implement specific LSP features by coordinating between the Analyzer and the Query Engine.

- **Primary Responsibility**: Implement LSP feature endpoints and handle AST-only logic
- **Secondary Responsibility**: Convert between LSP types and internal types

#### Key Files:

- `definition.rs`: Go-to-definition entry point
- `references.rs`: Find-references entry point
- `hover.rs`: Hover information entry point
- `completion/`: Code completion coordination
- `semantic_tokens.rs`: Syntax highlighting functionality
- `type_hierarchy.rs`: Superclass/Subclass navigation
- `inlay_hints.rs`: Inline type and parameter hints coordination

#### Design Decisions:

- Each capability is self-contained in its own module
- Capabilities focus on AST traversal and local scope analysis
- For index-heavy queries, capabilities delegate to the **Query Engine**
- Capabilities handle LSP-specific concerns (request/validation/shaping)

### 4. Query Engine (`src/query/`)

The Query Engine provides a unified service layer for querying the `RubyIndex`.

- **Primary Responsibility**: Consolidate business logic for index-based queries
- **Secondary Responsibility**: Provide composable helpers for complex resolution (e.g., method return types)

#### Key Files:

- `mod.rs`: Defines `IndexQuery` struct and entry points
- `definition.rs`: Unified definition lookups
- `references.rs`: Unified reference lookups
- `hover.rs`: Type and documentation lookups
- `types.rs`: Type inference helpers
- `method.rs`: Method resolution and dispatch logic
- `inlay_hints.rs`: Unified inlay hints and on-demand inference logic

#### Design Decisions:

- Consolidates all "index-aware" logic into one place
- Provides a stable API for capabilities to query project-wide information
- Enables complex "chained" queries through composable helpers

### 5. Server (`src/server.rs`)

The Server coordinates between LSP clients and the internal components.

- **Primary Responsibility**: Route LSP requests to appropriate components
- **Secondary Responsibility**: Manage server state (document cache, etc.)

#### Design Decisions:

- The server is the only component aware of the LSP protocol details
- The server delegates actual implementation to capability modules
- The server maintains minimal state (mostly for coordination)

### 5. Inferrer (`src/inferrer/`)

The Inferrer handles type analysis and integration with RBS type signatures.

- **Primary Responsibility**: Infer types for Ruby expressions
- **Secondary Responsibility**: Load and query RBS type information

### 6. Handlers (`src/handlers/`)

Handlers manage the routing of LSP requests and notifications.

- **Primary Responsibility**: Receive requests from the server and route them to capabilities
- **Secondary Responsibility**: Handle document lifecycle notifications (open, change, save)

### 7. Types (`src/types/`)

Centrally defined types used throughout the system.

- **Key Types**: `FullyQualifiedName`, `RubyDocument`, `RubyMethod`, `RubyVersion`

## Key Workflows

### 1. Workspace Indexing

1. Client connects to the LSP server
2. Server initializes and receives workspace information
3. Server asks the indexer to index all Ruby files in the workspace
4. Indexer finds all Ruby files and processes each one:
   - Parse the file using Ruby Prism
   - Traverse the AST to find symbols
   - Add symbols to the index

### 2. Go to Definition

1. Client sends a "go to definition" request with a position
2. Server delegates to the definition capability (`src/capabilities/definition.rs`)
3. Definition capability:
   - Uses the analyzer to identify the identifier and local scope at the position
   - If not a local variable, delegates to the **Query Engine** (`src/query/definition.rs`)
4. Query Engine:
   - Uses `IndexQuery` to perform project-wide lookups in `RubyIndex` (handling inheritance, mixins, etc.)
   - Returns resolved locations
5. Capability returns the location(s) to the client

### 3. File Change Handling

1. Client edits a file and sends a "did change" notification
2. Server receives the notification and:
   - Updates its document cache
   - Asks the indexer to reindex the file
3. Indexer:
   - Removes old entries for the file
   - Parses and reindexes the updated content

## Component Interactions

### 3-Layer Architecture

The Ruby Fast LSP follows a clear 3-layer architecture:

1. **API Layer** (`server.rs`, `handlers/`): Handles LSP protocol, request validation, and routing.
2. **Service Layer** (`src/query/`, `src/capabilities/`): Implements business logic for LSP features. `IndexQuery` acts as the primary service interface for data lookups.
3. **Data Layer** (`src/indexer/`): Manages the in-memory `RubyIndex`, providing raw access to symbol data.

### Analyzer, Query Engine, and Indexer Relationship

The separation between these components is crucial:

- **Analyzer**: Focuses on "what is this piece of code?" (local context)
- **Query Engine**: Focuses on "where is this in the project and how does it relate to other code?" (global context)
- **Indexer**: Focuses on storing and retrieving raw symbol data efficiently.

This separation allows:

1. Independent evolution of each component
2. Clearer testing boundaries
3. Better caching strategies (indexer can be persistent, analyzer is on-demand)

### Capability and Query Engine Relationship

Capabilities use the Query Engine as their primary data service:

1. Capabilities handle the AST traversal and identifying _what_ the user is interacting with.
2. They call the Query Engine to resolve _where_ that thing is defined or referenced across the workspace.
3. They translate the results back into LSP-specific formats.

## Future Extensions

The modular architecture facilitates extending the server with new capabilities:

1. Add a new capability module in `src/capabilities/`
2. Use existing services (Analyzer, Indexer) as needed
3. Wire it up in the server implementation
4. Update server capabilities in the initialize method

## Performance Considerations

- The Indexer builds an in-memory index for fast lookups
- Document changes trigger targeted reindexing
- Analysis is performed on-demand rather than eagerly
