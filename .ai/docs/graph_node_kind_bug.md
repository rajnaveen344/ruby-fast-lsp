# Graph NodeKind Default Bug

## Issue Summary

Modules were incorrectly appearing as classes in the "Included By Classes" list because the graph's `NodeKind` was defaulting to `Class` when nodes were created implicitly during edge operations.

## Root Cause

### The Default Kind Problem

In `src/indexer/graph.rs`, the `NodeKind` enum has `Class` as its default:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeKind {
    #[default]
    Class,  // <-- Default is Class!
    Module,
}
```

### Order of Operations Issue

When building the inheritance graph, edges can be added before the actual class/module entry is processed. For example:

1. `ClassA` includes `ModuleB`
2. When processing `ClassA`'s include edge, if `ModuleB` hasn't been indexed yet, the graph creates a placeholder node for `ModuleB`
3. The placeholder is created using `or_default()`, which uses `NodeKind::Class`
4. Later, when `ModuleB` is actually indexed, `ensure_node` was called but it only inserted if the node didn't exist - it never updated the kind

### The Problematic Code

In edge-adding methods like `set_superclass`:

```rust
pub fn set_superclass(&mut self, child: FqnId, parent: FqnId, file_id: FileId) {
    self.nodes.entry(child).or_default();  // Creates with default Class kind!
    self.nodes.entry(parent).or_default(); // Creates with default Class kind!
    // ...
}
```

And the original `ensure_node`:

```rust
pub fn ensure_node(&mut self, fqn_id: FqnId, kind: NodeKind) {
    self.nodes
        .entry(fqn_id)
        .or_insert_with(|| GraphNode::new(kind));  // Only inserts, never updates!
}
```

## The Fix

Updated `ensure_node` to always set the kind, even if the node already exists:

```rust
pub fn ensure_node(&mut self, fqn_id: FqnId, kind: NodeKind) {
    self.nodes
        .entry(fqn_id)
        .and_modify(|node| node.kind = kind)  // Update existing nodes
        .or_insert_with(|| GraphNode::new(kind));  // Insert new nodes
}
```

## Impact

This bug affected:
- The "Included By Classes" feature in the Ruby Index tree view, which showed modules as classes
- Any code that relied on `NodeKind` to distinguish between classes and modules in the graph
- The `ruby/exportGraph` endpoint which exports the inheritance graph

## Prevention

Consider these alternatives for future-proofing:

1. **Remove the Default derive**: Make `NodeKind` not implement `Default`, forcing explicit kind specification everywhere
2. **Add an Unknown variant**: `NodeKind::Unknown` as default, then assert no Unknown nodes exist after indexing
3. **Two-phase graph building**: First collect all entries to know their kinds, then build edges

## Related Files

- `src/indexer/graph.rs` - Graph structure and `ensure_node` fix
- `src/indexer/index.rs` - Where `ensure_node` is called during indexing
- `src/capabilities/namespace_tree.rs` - `find_includers` BFS that checks `NodeKind`
