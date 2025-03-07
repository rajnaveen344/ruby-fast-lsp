# Ruby Fast LSP Architecture

This document describes the architecture of the Ruby Fast LSP server. The codebase is organized into several key components, each with a distinct responsibility.

## High-Level Architecture

The Ruby Fast LSP server follows a modular architecture with clear separation of concerns:

```
src/
├── analyzer/       - Code understanding and analysis
├── capabilities/   - LSP feature implementations
├── indexer/        - File tracking and indexing
├── parser/         - Ruby parsing utilities
├── server.rs       - LSP server coordination
└── main.rs         - Application entry point
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

### 2. Analyzer (`src/analyzer/`)

The Analyzer is responsible for understanding Ruby code structure and semantics.

- **Primary Responsibility**: Analyze Ruby code to understand its structure
- **Secondary Responsibility**: Extract semantic information from code

#### Key Files:

- `mod.rs`: Central module file that re-exports public components
- `core.rs`: Core analyzer implementation with basic functionality
- `identifier.rs`: Identifier resolution and fully qualified name determination
- `context.rs`: Code context detection (current class, method, etc.)
- `position.rs`: Position conversion utilities

#### Design Decisions:

- The analyzer focuses on understanding what code means, not where it's located
- The analyzer is stateless - it analyzes code on demand
- Analysis is separated from indexing to maintain separation of concerns
- Structured in smaller files with focused responsibilities

### 3. Capabilities (`src/capabilities/`)

Capabilities implement specific LSP features by combining the Analyzer and Indexer.

- **Primary Responsibility**: Implement LSP feature endpoints
- **Secondary Responsibility**: Convert between LSP types and internal types

#### Key Files:

- `definition.rs`: Go-to-definition functionality
- `references.rs`: Find-references functionality
- `semantic_tokens.rs`: Semantic highlighting functionality

#### Design Decisions:

- Each capability is self-contained in its own module
- Capabilities use the analyzer and indexer as services
- Capabilities handle LSP-specific concerns (request/response formats)

### 4. Server (`src/server.rs`)

The Server coordinates between LSP clients and the internal components.

- **Primary Responsibility**: Route LSP requests to appropriate components
- **Secondary Responsibility**: Manage server state (document cache, etc.)

#### Design Decisions:

- The server is the only component aware of the LSP protocol details
- The server delegates actual implementation to capability modules
- The server maintains minimal state (mostly for coordination)

### 5. Parser (`src/parser/`)

The Parser provides utilities for parsing Ruby code.

- **Primary Responsibility**: Interface with tree-sitter for Ruby parsing

## Key Workflows

### 1. Workspace Indexing

1. Client connects to the LSP server
2. Server initializes and receives workspace information
3. Server asks the indexer to index all Ruby files in the workspace
4. Indexer finds all Ruby files and processes each one:
   - Parse the file using tree-sitter
   - Traverse the AST to find symbols
   - Add symbols to the index

### 2. Go to Definition

1. Client sends a "go to definition" request with a position
2. Server delegates to the definition capability
3. Definition capability:
   - Uses the analyzer to identify the symbol at the position
   - Uses the indexer to find where that symbol is defined
   - Returns the location to the client

### 3. File Change Handling

1. Client edits a file and sends a "did change" notification
2. Server receives the notification and:
   - Updates its document cache
   - Asks the indexer to reindex the file
3. Indexer:
   - Removes old entries for the file
   - Parses and reindexes the updated content

## Component Interactions

### Analyzer and Indexer Relationship

The separation between the Analyzer and Indexer is crucial:

- **Analyzer**: Focuses on "what is this piece of code?"
- **Indexer**: Focuses on "where are all the definitions and references?"

This separation allows:
1. Independent evolution of each component
2. Clearer testing boundaries
3. Better caching strategies (indexer can be persistent, analyzer is on-demand)

### Capability and Service Relationship

Capabilities use the Analyzer and Indexer as services:

1. They depend on these services but don't implement their logic
2. They focus on translating between LSP requests and internal operations
3. They handle LSP-specific concerns like request validation

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
