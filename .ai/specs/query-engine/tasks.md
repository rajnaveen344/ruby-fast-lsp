# Query Engine Tasks

## Task Status Overview

### ✅ Completed Tasks
- [x] Requirements analysis
- [x] Architecture design
- [x] Capability categorization (index-heavy vs AST-only)

### 🔄 In Progress Tasks
None currently.

### 📋 Pending Tasks

## Phase 1: Create Query Module (Non-breaking)

### 1.1 Foundation
- [ ] **Create `src/query/mod.rs`**
  - Create `IndexQuery` struct
  - Add factory methods (`new`, `for_file`)
  - Set up module re-exports
  - **Estimated**: 30 min

### 1.2 Definition Queries
- [ ] **Create `src/query/definition.rs`**
  - Move logic from `capabilities/definitions/mod.rs`
  - Move method resolution from `definitions/method.rs`
  - Move constant resolution from `definitions/constant.rs`
  - Move variable resolution from `definitions/variable.rs`
  - **Estimated**: 2 hours

### 1.3 Reference Queries  
- [ ] **Create `src/query/references.rs`**
  - Move logic from `capabilities/references.rs`
  - Consolidate mixin-aware method reference finding
  - Consolidate ancestor chain searching
  - **Estimated**: 2 hours

### 1.4 Type Queries
- [ ] **Create `src/query/types.rs`**
  - Merge existing `inferrer/query.rs` (TypeQuery)
  - Move type hints generation
  - Move local variable type inference
  - **Estimated**: 1.5 hours

### 1.5 Method Resolution
- [ ] **Create `src/query/method.rs`**
  - Move logic from `inferrer/method/resolver.rs`
  - Consolidate method return type resolution
  - Consolidate RBS fallback handling
  - **Estimated**: 1.5 hours

### 1.6 Hover Queries
- [ ] **Create `src/query/hover.rs`**
  - Move logic from `capabilities/hover.rs`
  - Integrate with type queries
  - **Estimated**: 1.5 hours

### 1.7 Completion Queries
- [ ] **Create `src/query/completion.rs`**
  - Move method completion logic
  - Move constant completion with scope resolution
  - **Estimated**: 2 hours

### 1.8 Diagnostics Queries
- [ ] **Create `src/query/diagnostics.rs`**
  - Move logic from `capabilities/diagnostics.rs`
  - Combine syntax, YARD, and unresolved diagnostics
  - **Estimated**: 1.5 hours

## Phase 2: Update Handlers

### 2.1 Update server.rs
- [ ] **Update definition handler**
  - Replace capabilities call with IndexQuery call
  - **Estimated**: 30 min

- [ ] **Update references handler**
  - Replace capabilities call with IndexQuery call
  - **Estimated**: 30 min

- [ ] **Update hover handler**
  - Replace capabilities call with IndexQuery call
  - **Estimated**: 30 min

- [ ] **Update completion handler**
  - Replace capabilities call with IndexQuery call
  - **Estimated**: 30 min

- [ ] **Update other index-heavy handlers**
  - inlay hints, diagnostics, type hierarchy, workspace symbols
  - **Estimated**: 1 hour

## Phase 3: Cleanup

### 3.1 Remove Old Code
- [ ] **Delete moved capability files**
  - Delete `capabilities/definitions/` directory
  - Delete `capabilities/references.rs`
  - Delete `capabilities/hover.rs`
  - Delete `capabilities/diagnostics.rs`
  - Keep AST-only: folding, semantic_tokens, document_symbols, formatting
  - **Estimated**: 30 min

- [ ] **Clean up inferrer/query.rs**
  - Remove TypeQuery (now in query/types.rs)
  - Keep or move remaining logic
  - **Estimated**: 30 min

### 3.2 Documentation
- [ ] **Update steering docs**
  - Update `.ai/steering/structure.md`
  - Update `src/ARCHITECTURE.md`
  - **Estimated**: 30 min

## Verification

### After Each Phase
- [ ] Run `cargo test` - all tests pass
- [ ] Run `cargo build` - no warnings
- [ ] Manual test with lsp-repl

### Integration Tests
- [ ] Go-to-definition works
- [ ] Find references works
- [ ] Hover shows types
- [ ] Completion suggests items
- [ ] Diagnostics appear

## Timeline Estimate

| Phase | Estimated Time |
|-------|----------------|
| Phase 1 (Query Module) | ~12 hours |
| Phase 2 (Update Handlers) | ~3 hours |
| Phase 3 (Cleanup) | ~1.5 hours |
| **Total** | ~16.5 hours |

## Success Criteria

- All index-heavy logic in `src/query/`
- `capabilities/` contains only 4 AST-only files
- All existing tests pass
- No performance regression
