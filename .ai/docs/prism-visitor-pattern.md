# Prism Visitor Pattern — Dispatch Behavior

## Key Insight

Prism's `Visit` trait has **two dispatch layers** that behave differently. Understanding this is critical for writing correct visitors.

## The Two Layers

### Layer 1: Generic callbacks (`visit_branch_node_enter` / `visit_leaf_node_enter`)

These fire when a node is visited through the **generic `visit(&Node)` method**.

```rust
// This is what visit() does internally:
fn visit(&mut self, node: &Node<'pr>) {
    match node {
        Node::CallNode { .. } => {
            self.visit_branch_node_enter(node.as_node());  // ← fires here
            self.visit_call_node(&concrete);
            self.visit_branch_node_leave();
        }
        // ...
    }
}
```

### Layer 2: Typed visit methods (`visit_call_node`, `visit_arguments_node`, etc.)

These are called by **parent node visitors directly**, bypassing `visit()`:

```rust
// Default implementation of visit_call_node:
pub fn visit_call_node(visitor: &mut V, node: &CallNode) {
    if let Some(node) = node.receiver() {
        visitor.visit(&node);                    // ← goes through visit() → branch_node_enter fires
    }
    if let Some(node) = node.arguments() {
        visitor.visit_arguments_node(&node);     // ← DIRECT call, branch_node_enter does NOT fire
    }
    if let Some(node) = node.block() {
        visitor.visit(&node);                    // ← goes through visit()
    }
}
```

## The Gotcha

**`visit_branch_node_enter` does NOT fire for nodes dispatched via typed methods.**

For `foo(y)`:
- `ProgramNode` → `visit_branch_node_enter` fires (via `visit()`)
- `StatementsNode` → `visit_statements_node` called directly, **NO** `branch_node_enter`
- `CallNode` → `visit_branch_node_enter` fires (via `visit()` from StatementsNode)
- `ArgumentsNode` → `visit_arguments_node` called directly, **NO** `branch_node_enter`

## Correct Approach

If you need to intercept specific node types, **override the typed visit methods**, not `visit_branch_node_enter`:

```rust
impl<'pr> Visit<'pr> for MyVisitor {
    // WRONG: won't fire for ArgumentsNode, StatementsNode, etc.
    fn visit_branch_node_enter(&mut self, node: Node<'pr>) { /* ... */ }

    // CORRECT: override specific typed methods
    fn visit_arguments_node(&mut self, node: &ArgumentsNode<'pr>) {
        // your logic here
        ruby_prism::visit_arguments_node(self, node); // recurse into children
    }

    fn visit_statements_node(&mut self, node: &StatementsNode<'pr>) {
        // your logic here
        ruby_prism::visit_statements_node(self, node); // recurse into children
    }
}
```

## When to Use Each

| Approach | Use When |
|----------|----------|
| `visit_branch_node_enter` | You need to see ALL nodes generically (e.g., collecting all node locations) |
| Typed `visit_*_node` methods | You need to detect specific node types reliably |

## EmbeddedStatementsNode Caveat

`"hello #{x}"` parses as:
```
InterpolatedStringNode
  └─ EmbeddedStatementsNode (the #{} part)
       └─ StatementsNode
            └─ LocalVariableReadNode (x)
```

The `EmbeddedStatementsNode` contains a `StatementsNode`, but this is a **value context** (string interpolation), not a statement context. If your visitor tracks whether code is in a "statement position", you need to handle `EmbeddedStatementsNode` specially — don't let its inner `StatementsNode` override the value context.

## Default Recursion

Each typed visit function has a public default (`ruby_prism::visit_*_node`) that recurses into children. Call it explicitly if you override a typed method but still want child traversal:

```rust
fn visit_call_node(&mut self, node: &CallNode<'pr>) {
    // custom logic...
    ruby_prism::visit_call_node(self, node); // continue traversal
}
```

Omitting the default recursion call stops traversal at that node.
