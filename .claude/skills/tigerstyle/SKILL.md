# TigerStyle Code Quality Skill

Use this skill when writing new code, reviewing code, or refactoring existing code in the Ruby Fast LSP project. Enforces TigerBeetle-inspired coding standards for safety, performance, and maintainability. Triggers: writing code, code quality, style guide, best practices, coding standards.

---

## Core Philosophy

**Priority order: Safety > Performance > Developer Experience**

Simplicity and elegance require hard work and discipline. Invest upfront design effort to prevent expensive production problems.

---

## Hard Rules

### 1. Function Length Limit: 70 Lines Maximum

No function should exceed 70 lines. This prevents cognitive overload and forces proper decomposition.

**If a function exceeds 70 lines:**

1. Identify logical sections
2. Extract helper functions with clear names
3. Each helper should do one thing well

```rust
// BAD: 150-line function
fn process_document(doc: &Document) -> Result<()> {
    // ... 150 lines of mixed concerns
}

// GOOD: Decomposed
fn process_document(doc: &Document) -> Result<()> {
    let parsed = parse_content(doc)?;
    let indexed = index_symbols(&parsed)?;
    resolve_references(&indexed)?;
    Ok(())
}
```

### 2. Minimum Two Assertions Per Function

Assert both positive space (expected behavior) and negative space (invalid states).

```rust
fn find_definition(index: &Index, fqn: &Fqn) -> Option<Location> {
    assert!(!fqn.is_empty(), "FQN cannot be empty");

    let result = index.get(fqn);

    // Assert negative space
    if let Some(loc) = &result {
        assert!(loc.uri.scheme() == "file", "Only file URIs supported");
    }

    result
}
```

### 3. Explicit Control Flow Only

- No recursion (use iteration with explicit stack)
- No deeply nested conditionals (max 3 levels)
- Push `if`s up, `for`s down

```rust
// BAD: Recursion
fn traverse(node: &Node) {
    process(node);
    for child in node.children() {
        traverse(child);  // Recursive
    }
}

// GOOD: Explicit stack
fn traverse(root: &Node) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        process(node);
        stack.extend(node.children());
    }
}
```

### 4. Push Ifs Up, Fors Down

Centralize branching logic in parent functions. Keep leaf functions pure.

```rust
// BAD: Branching in leaf function
fn process_item(item: &Item, mode: Mode) {
    if mode == Mode::Fast {
        // fast path
    } else {
        // slow path
    }
}

// GOOD: Branch in parent, pure leaves
fn process_items(items: &[Item], mode: Mode) {
    match mode {
        Mode::Fast => items.iter().for_each(process_fast),
        Mode::Slow => items.iter().for_each(process_slow),
    }
}
```

### 5. Bounds on All Loops and Queues

Every loop must have a clear termination condition. Every queue must have a capacity.

```rust
// BAD: Unbounded
while let Some(item) = queue.pop() {
    process(item);
}

// GOOD: Bounded with safety limit
const MAX_ITERATIONS: usize = 10_000;
let mut iterations = 0;
while let Some(item) = queue.pop() {
    iterations += 1;
    assert!(iterations < MAX_ITERATIONS, "Infinite loop detected");
    process(item);
}
```

---

## Naming Conventions

### Variables and Functions: snake_case

```rust
fn find_definition_at_position(pos: Position) -> Option<Location>
let current_scope = get_scope();
```

### Units as Suffixes, Ordered by Significance

```rust
// GOOD: Descending significance
let latency_ms_max = 100;
let timeout_seconds_default = 30;
let buffer_bytes_capacity = 1024;

// BAD
let max_latency_ms = 100;
let default_timeout_seconds = 30;
```

### No Abbreviations (Except Mathematical Contexts)

```rust
// GOOD
let definition_count = 0;
let reference_locations = vec![];

// BAD
let def_cnt = 0;
let ref_locs = vec![];
```

### Acronyms: Proper Capitalization

```rust
// GOOD
struct FqnResolver;
struct LspServer;
struct AstNode;

// BAD
struct FQNResolver;
struct LSPServer;
struct ASTNode;
```

---

## Comments and Documentation

### Complete Sentences with Punctuation

```rust
// GOOD
// This function resolves the fully qualified name by traversing
// the scope chain from innermost to outermost.

// BAD
// resolve fqn using scope chain
```

### Explain "Why", Not "What"

```rust
// GOOD
// We use a two-phase approach because single-phase indexing
// cannot resolve forward references in Ruby's flexible syntax.

// BAD
// Index files in two phases.
```

### Document Edge Cases and Invariants

```rust
/// Finds all references to the symbol at the given position.
///
/// # Invariants
/// - Position must be within document bounds
/// - Document must be indexed before calling
///
/// # Edge Cases
/// - Returns empty vec if position is in whitespace
/// - Includes declaration in results if `include_declaration` is true
fn find_references(pos: Position, include_declaration: bool) -> Vec<Location>
```

---

## Error Handling

### CRITICAL: Fail Fast and Loudly (MANDATORY)

**Never use `debug_assert!` - Use `assert!` and `panic!` instead.**

Production correctness is more important than "graceful degradation" that hides bugs.

```rust
// ❌ NEVER DO THIS: Silent failure or wrong defaults
let fqn_id = self.get_fqn_id(fqn).unwrap_or_default();

// ❌ NEVER DO THIS: Debug-only checks
debug_assert!(fqn.is_valid());  // WRONG! Production won't catch this

// ✅ ALWAYS DO THIS: Crash loudly with clear message
let fqn_id = self.get_fqn_id(fqn).expect(
    "INVARIANT VIOLATED: FQN not in index. \
     This is a bug - index the file first."
);

// ✅ ALWAYS DO THIS: Assert in production too
assert!(fqn.is_valid(), "Invalid FQN: {}", fqn);
```

**Why**: Better to crash and fix the bug than silently produce wrong results that corrupt data or mislead users.

### Clear Error Messages

Every panic/assert must explain WHAT, WHY, and HOW:

```rust
// ✅ GOOD: Explains what, why, and how to fix
assert!(
    matches!(fqn, Namespace(_, _)),
    "INVARIANT VIOLATED: get_ancestor_chain called with {}.\n\
     Only Namespace FQNs have ancestors (not Constants/Methods).\n\
     Fix: Check the FQN type before calling this function.",
    fqn
);

// ❌ BAD: Vague message
assert!(fqn.is_namespace(), "Invalid FQN");
```

### Never Ignore Errors

"Almost all catastrophic system failures result from incorrect handling of non-fatal errors explicitly signaled in software."

```rust
// BAD: Silent failure
let _ = file.write_all(data);

// GOOD: Handle or propagate
file.write_all(data)?;

// GOOD: Log if truly optional
if let Err(e) = file.write_all(data) {
    warn!("Failed to write cache: {}", e);
}
```

### Use Result for Recoverable, Option for Expected Absence

```rust
// Result: Operation can fail
fn read_file(path: &Path) -> Result<String, IoError>

// Option: Absence is normal/expected
fn find_symbol(name: &str) -> Option<Symbol>
```

### No Assumptions or Guessing

If data is missing or invalid, PANIC - don't guess:

```rust
// ❌ BAD: Assuming/guessing
let kind = fqn.namespace_kind().unwrap_or(Instance);  // Guessing!

// ✅ GOOD: Fail if wrong
let kind = match fqn {
    Namespace(_, k) => k,
    All|Variants => panic!("Expected Namespace FQN, got: {}", fqn), // Never use wildcard for panic/unreachable
};
```

---

## Memory and Performance

### Prefer Static Allocation

Allocate at startup when possible. Avoid dynamic allocation in hot paths.

```rust
// GOOD: Pre-allocated buffer
let mut buffer = Vec::with_capacity(EXPECTED_SIZE);

// BAD: Repeated allocations
for item in items {
    let mut temp = Vec::new();  // Allocates each iteration
    // ...
}
```

### Use Explicitly-Sized Types

```rust
// GOOD
let count: u32 = 0;
let offset: usize = 0;

// BAD: Ambiguous sizing
let count = 0;  // What type?
```

### String Interning for Repeated Strings

Use `Ustr` for frequently compared or stored strings.

```rust
use ustr::Ustr;

// GOOD: Interned string
let name: Ustr = "MyClass".into();

// Compare with pointer equality
if name == other_name { ... }
```

---

## Code Organization

### Order Within Files

1. `main` or primary entry point first
2. Public functions
3. Private helpers
4. Tests at bottom

### Order Within Structs

1. Fields
2. Associated types
3. Methods (public, then private)

### Alphabetize When No Natural Order

```rust
// GOOD: Alphabetized imports
use crate::analyzer;
use crate::indexer;
use crate::query;

// GOOD: Alphabetized match arms (when equivalent)
match node {
    Node::Class(c) => ...,
    Node::Constant(c) => ...,
    Node::Method(m) => ...,
    Node::Module(m) => ...,
}
```

---

## Line Length: 100 Characters Maximum

Break long lines for readability:

```rust
// GOOD
let result = some_function(
    first_argument,
    second_argument,
    third_argument,
);

// BAD
let result = some_function(first_argument, second_argument, third_argument, fourth_argument);
```

---

## Checklist for Every Change

- [ ] **NO `debug_assert!` - Use `assert!` instead** (CRITICAL)
- [ ] **No `.unwrap_or_default()` or silent failures** (CRITICAL)
- [ ] **Clear panic messages** explaining WHAT, WHY, HOW (CRITICAL)
- [ ] No function exceeds 70 lines
- [ ] At least 2 assertions per function
- [ ] No recursion (explicit stacks instead)
- [ ] All loops have bounds
- [ ] Errors are handled, not ignored
- [ ] Comments are complete sentences
- [ ] Names follow conventions (snake_case, units as suffixes)
- [ ] Line length under 100 characters
