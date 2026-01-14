# Code Review Skill

Use this skill when reviewing code changes, pull requests, or before merging code in the Ruby Fast LSP project. Provides a comprehensive checklist based on TigerStyle principles and project standards. Triggers: review, code review, PR review, pull request, check code, review changes.

---

## Review Philosophy

> "Code review is not about finding bugs. It's about maintaining code quality and sharing knowledge."

### Goals of Review

1. **Correctness** - Does it work as intended?
2. **Maintainability** - Can others understand and modify it?
3. **Consistency** - Does it follow project patterns?
4. **Safety** - Are edge cases handled?
5. **Performance** - Is it efficient enough?

---

## Quick Review Checklist

### Must Check (Blocking)

- [ ] Tests pass: `cargo test`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Code formatted: `cargo fmt --check`
- [ ] No functions over 70 lines
- [ ] Errors are handled, not ignored
- [ ] No unwrap() in production code

### Should Check (Important)

- [ ] New tests for new functionality
- [ ] Documentation for public APIs
- [ ] Consistent naming conventions
- [ ] No obvious performance issues
- [ ] Edge cases considered

### Nice to Check (Optional)

- [ ] Could be simplified further
- [ ] Opportunities for reuse
- [ ] Improvement suggestions

---

## Detailed Review Categories

### 1. TigerStyle Compliance

#### Function Length

```rust
// REJECT: Over 70 lines
fn massive_function() {
    // 150 lines of code
}

// ACCEPT: Decomposed
fn main_function() {
    let step1 = do_step1();
    let step2 = do_step2(step1);
    finalize(step2)
}
```

#### Assertions

```rust
// REJECT: No assertions
fn process(data: &Data) -> Result {
    // just processes, no validation
}

// ACCEPT: Validates invariants
fn process(data: &Data) -> Result {
    assert!(!data.is_empty(), "Data cannot be empty");
    assert!(data.len() < MAX_SIZE, "Data exceeds maximum size");
    // process...
}
```

#### Control Flow

```rust
// REJECT: Recursion
fn traverse(node: &Node) {
    traverse(node.left);  // Recursive
}

// ACCEPT: Explicit stack
fn traverse(root: &Node) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
    }
}
```

### 2. Error Handling

#### Silent Failures

```rust
// REJECT: Silent swallow
let _ = file.write(data);

// ACCEPT: Handle or log
file.write(data).context("Failed to write file")?;

// OR
if let Err(e) = file.write(data) {
    warn!("Cache write failed: {}", e);
}
```

#### Result vs Option

```rust
// REJECT: Option for errors
fn parse_config(path: &Path) -> Option<Config>  // Why did it fail?

// ACCEPT: Result with error info
fn parse_config(path: &Path) -> Result<Config, ConfigError>
```

#### Unwrap Usage

```rust
// REJECT: Unwrap in production
let doc = self.docs.get(&uri).unwrap();

// ACCEPT: Handle absence
let doc = self.docs.get(&uri).ok_or(QueryError::DocumentNotFound)?;

// OR for truly impossible cases
let doc = self.docs.get(&uri).expect("Document must exist after indexing");
```

### 3. Naming and Style

#### Naming Conventions

```rust
// REJECT
fn getDef() -> Def  // camelCase
let maxCnt = 10;    // abbreviation

// ACCEPT
fn get_definition() -> Definition  // snake_case
let max_count = 10;                // full words
```

#### Units in Names

```rust
// REJECT
let timeout = 30;     // 30 what?
let max_size = 1024;  // bytes? KB?

// ACCEPT
let timeout_seconds = 30;
let max_size_bytes = 1024;
```

### 4. Architecture Compliance

#### Layer Dependencies

```rust
// REJECT: Handler directly uses indexer
// handlers/request.rs
use crate::indexer::RubyIndex;

// ACCEPT: Handler uses capabilities
// handlers/request.rs
use crate::capabilities::definition;
```

#### Query Layer Usage

```rust
// REJECT: Scattered index access
fn find_def(&self) {
    let index = self.index.lock();
    let result = index.definitions.get(&name);
    // manual lookup
}

// ACCEPT: Use IndexQuery
fn find_def(&self) {
    let query = IndexQuery::new(&self.index, &uri);
    query.find_definitions_at_position(pos)
}
```

### 5. Testing

#### New Features Must Have Tests

```rust
// REJECT: New capability without tests
pub fn new_feature() -> Result { ... }
// No corresponding test file

// ACCEPT: Feature with tests
pub fn new_feature() -> Result { ... }
// Plus: tests/integration/new_feature.rs
```

#### Test Quality

```rust
// REJECT: Incomplete test
#[test]
fn test_find_definition() {
    let result = find_definition("MyClass");
    assert!(result.is_some());  // Doesn't verify content
}

// ACCEPT: Thorough test
#[test]
fn test_find_definition() {
    let result = find_definition("MyClass");
    assert!(result.is_some());
    let loc = result.unwrap();
    assert_eq!(loc.uri.path(), "/src/my_class.rb");
    assert_eq!(loc.range.start.line, 5);
}
```

### 6. Performance

#### Obvious Issues

```rust
// REJECT: Clone in loop
for item in items {
    process(item.clone());  // Clones every iteration
}

// ACCEPT: Borrow
for item in &items {
    process(item);
}
```

#### Allocation Patterns

```rust
// REJECT: Repeated allocation
let mut results = Vec::new();
for i in 0..1000 {
    results.push(compute(i));
}

// ACCEPT: Pre-allocated
let mut results = Vec::with_capacity(1000);
for i in 0..1000 {
    results.push(compute(i));
}
```

---

## Review Comments Guide

### Blocking Comments

Use for issues that must be fixed:

```
**[BLOCKING]** This function is 120 lines. Please split into smaller
functions (max 70 lines per TigerStyle).
```

```
**[BLOCKING]** Unwrap on user input can panic. Use proper error handling.
```

### Suggestion Comments

Use for improvements that aren't blocking:

```
**[Suggestion]** Consider using `Ustr` here since this string is
frequently compared.
```

```
**[Suggestion]** This could be simplified with `filter_map()`.
```

### Question Comments

Use to understand intent:

```
**[Question]** Why is this error being ignored? Is this intentional?
```

```
**[Question]** Would it make sense to add a test for the edge case
where the list is empty?
```

---

## Review Response Template

### For Approvals

```markdown
## Review: Approved

### Summary

Brief description of what was reviewed.

### Strengths

- Clean implementation of X
- Good test coverage for Y

### Minor Suggestions (non-blocking)

- Consider adding doc comment for public function
- Could simplify line 45 with iterator methods

Approved to merge!
```

### For Changes Requested

```markdown
## Review: Changes Requested

### Summary

Brief description of what was reviewed.

### Blocking Issues

1. **Function too long** (line 50-180): Split into smaller functions
2. **Missing error handling** (line 95): Don't ignore this error

### Non-blocking Suggestions

- Add test for empty input case
- Consider more descriptive variable name on line 30

Please address blocking issues before merge.
```

---

## Automated Checks

### Pre-Review Commands

Run before reviewing any PR:

```bash
# Must pass
cargo test
cargo clippy -- -D warnings
cargo fmt --check

# Should run
cargo doc --no-deps
```

### CI Requirements

Every PR should pass:

1. All tests green
2. No clippy warnings
3. Properly formatted
4. No new TODO/FIXME without issue reference

---

## Common Review Findings

### Most Frequent Issues

1. **Functions too long** - Extract helpers
2. **Missing error context** - Add `.context()`
3. **Unwrap usage** - Use `?` or `ok_or()`
4. **No tests for new code** - Add integration tests
5. **Inconsistent naming** - Follow snake_case

### Red Flags to Watch For

- `unwrap()` anywhere except tests
- `// TODO` without issue reference
- Functions over 70 lines
- Recursive functions
- Empty catch blocks / silent error swallowing
- Clone in hot loops
- Direct index access instead of IndexQuery
