# Memory Profiling Analysis

## Summary

Analysis of `dhat-heap.json` after boxing `EntryKind` variants. Total memory ~600 MB.

## Top Memory Consumers

| Rank | MB       | Source                             | Code Location                  |
| ---- | -------- | ---------------------------------- | ------------------------------ |
| 1    | **704**  | `SlotMap` growth over time         | `RubyIndex::new` (index.rs:65) |
| 2-8  | **~470** | `Vec::clone` (scope stack cloning) | `parameters_node.rs:89,99`     |
| 9    | 59       | `Vec::push/grow`                   | `UnresolvedIndex::add`         |
| 10   | 52       | General `Vec::clone`               | Various                        |

## Issue 1: SlotMap Allocations (704 MB cumulative)

The 704 MB is **cumulative** over the lifetime of the program. As SlotMap grows, it allocates new memory and frees old memory. This is expected behavior.

## Issue 2: LVScopeStack Cloning (470 MB) ⚠️ HIGH PRIORITY

Every parameter causes the entire scope stack to be cloned **twice**:

```rust
// parameters_node.rs:89
self.scope_tracker.get_lv_stack().clone()

// parameters_node.rs:99 (inside EntryKind::new_local_variable)
self.scope_tracker.get_lv_stack().clone()
```

### Root Cause

`LVScopeStack = Vec<LVScope>` is stored in:

1. `FullyQualifiedName::LocalVariable(Ustr, LVScopeStack)` - used as HashMap key!
2. `LocalVariableData.scope_stack` - redundant storage

Each `LVScope` contains a `Location` which includes a heap-allocated URL string.

### Proposed Solutions

1. **Replace `LVScopeStack` with `LVScopeId`** - Store only the current scope ID, not the full stack
2. **Use `Rc<LVScopeStack>`** - Share instead of clone
3. **Remove from FQN** - Use (name, scope_id) as key instead of (name, full_stack)

## Entry Size After Boxing

- Entry: 160 bytes (down from ~360 bytes)
- EntryKind: 16 bytes (boxed pointer + discriminant)

## Next Steps

1. [x] Box EntryKind variants (completed, Entry now 160 bytes)
2. [ ] **Optimize LVScopeStack** - Replace Vec with scope_id
3. [ ] Consider boxing Entry in SlotMap storage
