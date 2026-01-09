# Query Engine Requirements

## Introduction

This specification defines a unified query engine layer (`IndexQuery`) for the Ruby Fast LSP, consolidating all business logic for querying the `RubyIndex`. The goal is to create a clean 3-layer architecture (Data → Service → API) that reduces code duplication, improves maintainability, and provides a single source of truth for query logic.

## Requirements

### Requirement 1: Unified Definition Query

**User Story:** As an LSP capability, I want a single API to find definitions so that go-to-definition logic isn't duplicated across multiple files.

#### Acceptance Criteria

1. WHEN querying for a definition at a position THEN the query SHALL handle all identifier types (constants, methods, variables)
2. WHEN querying for method definitions THEN the query SHALL search the ancestor chain
3. WHEN receiver type is known THEN the query SHALL resolve methods on that type
4. WHEN YARD type references are present THEN the query SHALL resolve them to definitions

### Requirement 2: Unified Reference Query

**User Story:** As an LSP capability, I want a single API to find references so that reference logic is consistent and mixin-aware.

#### Acceptance Criteria

1. WHEN querying for references by FQN THEN the query SHALL return all usages
2. WHEN querying method references THEN the query SHALL be mixin-aware
3. WHEN querying method references THEN the query SHALL search ancestor chains
4. WHEN querying local variable references THEN the query SHALL respect scope boundaries

### Requirement 3: Unified Type Query

**User Story:** As an LSP capability, I want a single API for type inference so that hover, inlay hints, and completion use consistent logic.

#### Acceptance Criteria

1. WHEN querying type at a position THEN the query SHALL check methods, then variables
2. WHEN querying method return types THEN the query SHALL resolve through ancestor chain
3. WHEN querying local variable types THEN the query SHALL check parameters first
4. WHEN type is not found THEN the query SHALL return Unknown gracefully

### Requirement 4: Unified Completion Query

**User Story:** As an LSP capability, I want a single API for completions so that method, constant, and variable completions are consistent.

#### Acceptance Criteria

1. WHEN getting method completions THEN the query SHALL filter by receiver type
2. WHEN getting constant completions THEN the query SHALL respect scope resolution
3. WHEN getting completions THEN the query SHALL deduplicate results
4. WHEN prefix tree is used THEN the query SHALL use it for fast lookups

### Requirement 5: Unified Diagnostics Query

**User Story:** As an LSP capability, I want a single API for diagnostics that combines syntax, YARD, and unresolved entry errors.

#### Acceptance Criteria

1. WHEN generating diagnostics THEN the query SHALL combine all error sources
2. WHEN checking YARD docs THEN the query SHALL validate type references
3. WHEN checking constants THEN the query SHALL report unresolved references
4. WHEN checking methods THEN the query SHALL report unresolved calls

### Requirement 6: Clean Layer Separation

**User Story:** As a developer, I want clear separation between data (index), service (query), and API (server) layers.

#### Acceptance Criteria

1. WHEN accessing RubyIndex THEN only query layer SHALL access it directly
2. WHEN server.rs handles requests THEN it SHALL delegate to query layer
3. WHEN adding new features THEN changes SHALL stay within query layer
4. WHEN capabilities/ exists THEN it SHALL only contain AST-only logic

## Non-Functional Requirements

### Performance Requirements
- Query methods SHALL not be slower than current direct index access
- Caching SHALL be used for expensive operations (ancestor chains)
- No additional memory overhead beyond existing structures

### Maintainability Requirements
- All index-heavy logic SHALL be in one location (src/query/)
- New queries SHALL follow established patterns
- Query methods SHALL be well-documented

### Compatibility Requirements
- Existing capability tests SHALL continue to pass
- Server.rs handler signatures SHALL remain compatible
- RubyIndex public API SHALL remain stable during migration

## Success Criteria

1. **Consolidation**: All duplicated query logic moved to `src/query/`
2. **Simplification**: `capabilities/` reduced to AST-only handlers
3. **Maintainability**: Adding a new query requires changes to one module
4. **No Regressions**: All existing tests pass
5. **Documentation**: Query API is well-documented
