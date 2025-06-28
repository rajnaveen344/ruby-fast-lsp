# Visitor Stack Refactor

## Motivation
All Prism visitors (`IdentifierVisitor`, `ReferenceVisitor`, `IndexVisitor`, `ScopeVisitor`, `TokenVisitor`, …) currently carry their own copies of:

* `namespace_stack: Vec<Vec<RubyConstant>>`
* `scope_stack / lv_stack: Vec<LVScope>`
* `current_method: Option<RubyMethod>`
* `in_singleton_node: bool`

The push/pop helpers for these are duplicated across the files, which makes future fixes error-prone and noisy.

## Design Goals
1. **Single source of truth** – every visitor keeps exactly _one_ field for runtime scope information.
2. **Separation of concerns** – low-level stack manipulation is isolated from higher-level analysis.
3. **Nesting‐safe** – must support arbitrary nesting of `class`, `module`, `def`, `block`, and `class << self` constructs.
4. **Zero functional change** – refactor only, no change in behaviour.

## Proposed Structure
```rust
// src/analyzer_prism/visitors/common.rs (new)
/// Mixed stack frame – either a namespace or a `class << self` marker
#[derive(Debug)]
pub enum ScopeFrame {
    Namespace(Vec<RubyConstant>),
    Singleton,
}

#[derive(Default)]
pub struct ScopeTracker {
    /// Ordered stack of namespace/singleton frames.
    frames: Vec<ScopeFrame>,

    /// Local-variable scopes (method/block/rescue/lambda)
    lv_stack: Vec<LVScope>,

    /// Current Ruby method we are inside (used by Identifier / Index visitors)
    current_method: Option<RubyMethod>,
}
```

### Helper API (excerpt)
```rust
impl ScopeTracker {
    // ---------- namespace helpers ----------
    pub fn push_ns_scope(&mut self, ns: RubyConstant)          { … }
    pub fn push_ns_scopes(&mut self, v: Vec<RubyConstant>)     { … }
    pub fn pop_ns_scope(&mut self)                             { … }
    pub fn current_namespace(&self) -> Vec<RubyConstant>       { … }

    // ---------- lv-scope helpers ----------
    pub fn push_lv_scope(&mut self, scope: LVScope)            { … }
    pub fn pop_lv_scope(&mut self)                             { … }
    pub fn current_lv_scope(&self) -> Option<&LVScope>         { … }

    // ---------- method helpers ----------
    pub fn enter_method(&mut self, m: RubyMethod)              { … }
    pub fn exit_method(&mut self)                              { … }
    pub fn current_method(&self) -> Option<&RubyMethod>        { … }

    // ---------- singleton helpers ----------
    pub fn enter_singleton(&mut self) {
        self.frames.push(ScopeFrame::Singleton);
    }

    pub fn exit_singleton(&mut self) {
        if matches!(self.frames.last(), Some(ScopeFrame::Singleton)) {
            self.frames.pop();
        }
    }

    /// Returns true if there is a `Singleton` frame above the last `Namespace`.
    pub fn in_singleton(&self) -> bool {
        let mut iter = self.frames.iter().rev();
        while let Some(frame) = iter.next() {
            match frame {
                ScopeFrame::Singleton => return true,
                ScopeFrame::Namespace(_) => return false,
            }
        }
        false
    }
}
```

### Why keep **two** logical stacks?
* **Namespace stack** changes only on `class` / `module` keywords – _not_ on blocks or defs.
* **LV-stack** changes on every method, block, lambda, etc.

Merging them would require tagging every frame and filtering on every lookup. Keeping them separate preserves a clear mental model and simplifies constant / variable resolution.

### Representing `class << self`
* _Not_ a new `RubyConstant`.
* Track via `ScopeFrame::Singleton` markers pushed onto the same stack as namespace frames (no counter needed).
* Supports arbitrary nesting and automatically resets when the enclosing namespace frame is popped (see example below).

````ruby
class A
  class << self           # depth = 1, Frame = A
    class B
      class << self       # depth = 1, Frame = A::B
        def hello; end    # resolves to A.singleton_class::B.singleton_class#hello
      end
    end
  end
end
````

### Singleton-depth scoping
Singleton context never leaks outside the *innermost* `class`/`module` frame. In other words, each namespace frame owns its own singleton depth starting at **0**.

Two implementation patterns:
1. **Depth inside each namespace frame**
   ```rust
   struct NamespaceFrame {
       parts: Vec<RubyConstant>,
       singleton_depth: usize, // reset to 0 when the frame is pushed
   }
   ```
2. **Mixed stack with markers** (no counter): push `ScopeFrame::Singleton` entries that sit *above* the current `Namespace`. Popping the namespace automatically discards any nested singleton markers.

Either approach ensures that `class << self` depth is always measured relative to the most recent namespace.

## Visitor Changes
Each visitor becomes roughly:
```rust
pub struct IdentifierVisitor {
    tracker: ScopeTracker,
    document: RubyDocument,
    position: Position,
    // …analysis-specific fields…
}
```
No more duplicated push/pop code; they delegate to `tracker`.

## Migration Plan
1. **Introduce `ScopeTracker`** in `visitors/common.rs` with full helper API.
2. Refactor one visitor (e.g. `IdentifierVisitor`) to use the tracker; run tests.
3. Incrementally port the remaining visitors.
4. Delete old duplicated helper methods.

## Benefits
* ~300 LOC deleted across visitors.
* Harder to forget a pop or mismatch stacks.
* Easier to add new context (e.g. visibility, refinements) – just extend `ScopeTracker`.

---
_Updated 2025-06-28_
