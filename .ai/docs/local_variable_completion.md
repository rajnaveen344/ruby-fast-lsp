# Local Variable Completion Algorithm

This document describes the algorithm for implementing local variable completion in the Ruby LSP server using a `BTreeMap<ScopeId, Vec<LocalVarEntry>>` data structure.

## Data Structures

```rust
type ScopeId = usize;  // Start-byte offset of the scope node (unique within the file)

struct LocalVarEntry {
    name: String,  // Name of the local variable
    pos: u32,      // Byte position where the variable is defined
}

// Maps scope start byte offset to a list of local variables defined in that scope
// The Vec<LocalVarEntry> is maintained in sorted order by position
BTreeMap<ScopeId, Vec<LocalVarEntry>> lvars;
```

### Scope ID (how & why)

A `ScopeId` is **the `start_byte` of the syntax node that opens the scope**, as provided by Prism parser.

* `start_byte` is guaranteed to be **unique** for every node inside a single file, so it cleanly distinguishes scopes.
* It is a plain integer (`usize`), making it cheap to hash/order and store.
* Outer scopes always have a smaller `start_byte` than their inner children, which matches lexical nesting and can be handy for range queries.
* We never need to reconstruct the node from the id; we only group local-variable entries that belong to that same scope.
* When the file changes we re-parse and rebuild the map, so new/shifted scopes naturally receive fresh ids.

```rust
let scope_id: ScopeId = node.start_byte();
```

This id is pushed onto the `LVScopeStack` whenever we enter a new scope. During completion we look up the current (innermost) `scope_id` and fetch that vector of variables from the `BTreeMap`.

## Algorithm

### 1. Indexing Local Variables

When parsing a Ruby file, we maintain a stack of active scopes. For each local variable definition:

1. Get the current scope's ID (start byte offset of the scope)
2. Create a `LocalVarEntry` with the variable name and its position
3. Insert the entry into the `BTreeMap`:
   - If the scope ID doesn't exist, create a new vector with the entry
   - If the scope ID exists, insert the entry in sorted order by position

```rust
fn add_local_var(&mut self, scope_id: ScopeId, var_name: String, pos: u32) {
    let entry = LocalVarEntry { name: var_name, pos };
    let entries = self.lvars.entry(scope_id).or_default();
    
    // Insert in sorted order by position
    match entries.binary_search_by_key(&pos, |e| e.pos) {
        Ok(_) => {}  // Duplicate, skip
        Err(idx) => entries.insert(idx, entry),
    }
}
```

### 2. Querying Visible Local Variables

To find all visible local variables at a given position:

1. Start with the innermost scope and move outward
2. For each scope in the scope stack (in reverse order):
   - Get all variables in the scope that were defined before the cursor position
   - Add them to the results if not already present (shadowing)
   - Stop at hard scope boundaries (like method/class boundaries)

```rust
fn locals_at(&self, cursor_pos: u32, scope_stack: &[ScopeId]) -> Vec<String> {
    let mut visible_vars = HashSet::new();
    let mut results = Vec::new();

    for &scope_id in scope_stack.iter().rev() {
        if let Some(vars) = self.lvars.get(&scope_id) {
            // Variables are stored in sorted order by position
            for var in vars {
                if var.pos < cursor_pos {
                    // Only add if not shadowed by a variable with the same name
                    if visible_vars.insert(&var.name) {
                        results.push(var.name.clone());
                    }
                } else {
                    // Variables are in order, so we can break early
                    break;
                }
            }
        }

        
        // Stop at hard scope boundaries if needed
        if is_hard_scope_boundary(scope_id) {
            break;
        }
    }

    results
}
```

## Performance Characteristics

- **Space Complexity**: O(N) where N is the total number of local variables
- **Query Time**: O(M log K) where M is the number of scopes and K is the number of variables per scope
- **Insertion Time**: O(log M + K) where M is the number of scopes and K is the number of variables in the scope

## Advantages

1. **Efficient Lookup**: BTreeMap provides O(log n) lookup time for scopes
2. **Order Preservation**: Variables within a scope are stored in order of definition
3. **Shadowing Support**: Natural handling of variable shadowing
4. **Memory Efficient**: Only stores the scope IDs and variable positions, not the entire scope hierarchy

## Example

For the following Ruby code:

```ruby
def example
  a = 1
  if true
    b = 2
    # Cursor here
  end
  c = 3
end
```

The scope tree would be:
- Scope 0 (method): a, c
- Scope 1 (if): b

At the cursor position, the visible variables would be `["a", "b"]` (in that order).
