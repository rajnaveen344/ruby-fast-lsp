# Indexing Ruby Mixins (include / prepend / extend)

Ruby-fast-LSP must understand Ruby’s mixin mechanism so that features such as *Go to Definition* and *Find References* can jump to methods that live in modules pulled in via `include`, `prepend`, or `extend` – even across files.

This guide explains **what information the index stores**, **how it is collected in one CST walk**, and **how on-demand resolution turns that information into an ancestor chain that matches Ruby’s runtime lookup**.

---

## 1  Runtime refresher – where does Ruby look?

Given a receiver `obj` and message `:foo`, MRI searches:

1. `obj`’s **singleton class** (this is where `extend` adds modules for that *instance*).
2. Modules **prepended** to `obj.class` (newest first – *last* `prepend` wins).
3. `obj.class` itself.
4. Modules **included** in the class (search order is declaration order – *first* `include` wins).
5. The **superclass**, repeating steps 2-4 until `BasicObject`.

`extend` inside a *class/module* body is sugar for:

```ruby
class << self
  include M      # affects class methods
end
```

---

## 2  What the index stores per class / module

| Field                         | Purpose                                                      |
| ----------------------------- | ------------------------------------------------------------ |
| `superclass` (optional FQN)   | Builds classic inheritance chain                             |
| `prepends: Vec<MixinRef>`     | Highest precedence after `extend`                            |
| `includes: Vec<MixinRef>`     | Normal mixins for instance methods                           |
| `extends:  Vec<MixinRef>`     | Mixins that affect *class* methods                           |

Where **`MixinRef`** is a *purely textual* reference that does **not** require the module to be indexed yet:

```rust
struct MixinRef {
    parts: Vec<RubyConstant>, // ["Foo", "Bar"]
    absolute: bool,           // true for ::Foo::Bar
}
```

---

## 3  Single-pass indexing workflow

1. **Walk the Prism CST once.**  When inside a `class` / `module` body hit by `IndexVisitor`:
   * Build an `Entry` for the class/module and push it to `RubyIndex`.

2. **During the same walk**, whenever you encounter a call node that matches

   ```ruby
   (receiver == nil) && name in {include, prepend, extend}
   ```

   – **and** each argument is a `ConstantReadNode` or `ConstantPathNode` –
   convert every argument to a `MixinRef` and push it into the *current* entry’s `includes / prepends / extends` vector.

That is **all** the work the indexer does.  No second traversal, no validation at this stage.

> ❓ *Why not resolve now?*  Because the module may be defined **after** the mixin call or in another file that has not been parsed yet.  Deferring keeps the indexer one-pass and incremental-friendly.

---

## 4  On-demand resolution (ancestor chain)

When a capability needs to know "where would Ruby look for method `foo` on `SomeClass`?" we call:

```rust
get_ancestor_chain(&index, &receiver_fqn, is_class_method)
```

Inside that helper we process each `MixinRef` as follows:

```
if ref.absolute {
    probe([parts])
    probe([parts[1..]])
    …
} else {
    // lexical fallback: prepend nesting then pop one segment at a time
    candidate = current_namespace + parts
    while candidate not empty {
        probe(candidate)
        pop_left(candidate)
    }
    // finally treat as absolute
    probe(parts)
    pop_left(parts) …
}
```

`probe(vec)` means: convert the vector to `FullyQualifiedName::Constant(vec)` and look it up in `index.definitions`.  The **first** match wins; if nothing matches the reference is ignored for this query.

### Example – nested module include

```ruby
module Outer
  module Inner
    include Mixin   # ← constant without ::
  end
end
```

At index time we store `MixinRef {parts:["Mixin"], absolute:false}`.

During resolution for `Outer::Inner` the namespace stack is `[Outer, Inner]` and the probe sequence is:

1. `[Outer, Inner, Mixin]` – not found
2. `[Outer, Mixin]` – not found
3. `[Mixin]` – found ⇒ stop

If the workspace later gains `Outer::Inner::Mixin`, step 1 will succeed without code changes.

---

## 5  Why not a dedicated meta-programming pass?

*   **Extra cost** – every save would run two sweeps or complex dependency invalidation.
*   **Cycles / forward refs** – still need lazy handling.
*   **Interactive latency** – single pass keeps indexing snappy.

That said, a future optimisation could cache resolved `MixinRef → FQN` bindings or run a targeted fix-up pass when new definitions enter the index.

---

## 6  Edge cases & current limitations

*   Only `ConstantReadNode` / `ConstantPathNode` arguments are supported.  Dynamic includes (`include const_get(x)`) are ignored.
*   Mixins inside `class << self` bodies are **already** captured because the visitor treats that as a `Module` entry for the singleton class.
*   Refinements and `using` are out of scope for now.
*   `extend self` is treated as `extend <current module>`; it affects class methods and works with the same mechanism.

---

## 7  Implementation checklist (quick reference)

1. `EntryKind::{Class, Module}` owns `includes / prepends / extends: Vec<MixinRef>`.
2. `IndexVisitor::process_call_node_entry` builds `MixinRef`s.
3. `get_ancestor_chain` resolves refs with fallback algorithm above.
4. Capabilities query the ancestor chain; first match wins.

With this design Ruby-fast-LSP resolves methods through mixins accurately, across files, and with minimal indexing overhead.
# Indexing Ruby Mixins (`include`, `extend`, `prepend`)

This document outlines how the Ruby-fast-LSP indexer should understand Ruby’s mixin mechanisms so that navigation features such as *Go to Definition* produce accurate results across files.

## 1. Ruby’s Runtime Lookup Rules (refresher)

Given a receiver `obj` and message `:foo` Ruby searches for a matching method in the following order:

1. **Singleton class** of `obj` (where `extend` inserts modules).
2. For each module **prepended** to the receiver’s class (in the order they were declared, *last `prepend` wins*)
3. The **receiver’s class** itself.
4. For each module **included** in the class (first‐declared wins – i.e. search order is declaration order)
5. **Superclass** and repeat the same steps (prepend → self → include) up the hierarchy until `BasicObject`.

`extend` on an *object* adds the module high in its singleton class chain (step 1 above). `extend` on a *class/module* behaves like `class << self; include M; end` – affecting *class methods*.

## 2. What the Indexer Must Record

For every `class`/`module` definition we need to capture:

* **Direct superclass** (for classes)
* **Prepended modules** – ordered vector
* **Included modules** – ordered vector
* **Extended modules (class-level)** – ordered vector (for navigation of class methods)

### Detecting mixin declarations

While walking the CST produced by Prism parser we flag a `call` node when:

```
(receiver is nil) && call node name in {include, prepend, extend}
```

and the current scope is a `Module` or `Class` body. We then:

1. For each argument **that is a `ConstantPathNode` or `ConstantReadNode`** resolve the constant name. (Arguments of other node types are skipped for now.)
2. Push the target FQN into the appropriate vector (`includes`, `prepends`, or `extends`) on the `EntryBuilder` for the current class/module.  
   The vector’s order preserves declaration order so the runtime lookup precedence is retained.

For `extend` without an explicit receiver **inside a method body** we treat it as *instance extend*; that influences only the specific object, which is harder to statically know – these can be ignored for now or handled heuristically.

## 3. Building the Ancestor Chain

When the server starts or when a file changes the workspace manager can ask the index for an **AncestorChain** for a given class/module:

1. Start with prepends (latest first)
2. Self
3. Includes (declaration order)
4. Recurse into superclass (if any)

For *class methods* we prepend the chain with modules recorded via `extend`.

## 4. Go to Definition Algorithm (mixin-aware)

1. From the LSP position determine the **receiver type** (`self` inside a class body, constant receiver, variable -> static inference fallback).
2. Obtain the `AncestorChain` for that type.
3. Iterate over ancestors in order; for each, consult the per-file symbol index for a method with the requested name and correct visibility (instance vs class). The first hit is the runtime-correct definition.
4. If not found within the file, continue searching across workspace files (reuse existing cross-file lookup that now respects ancestor ordering).

## 5. Example: `A` mixes into `B`

```ruby
module A
  def method_a; end
end

module B
  include A

  def method_b
    method_a # cursor here
  end
end
```

**Indexing**

- When `include A` is encountered, the indexer pushes `A` onto `B.includes`.
- The resulting entry stored is roughly:
  ```rust
  EntryKind::Module {
      includes: vec![A],
      prepends: vec![],
      extends: vec![],
  }
  ```

**Go to Definition flow**

1. Cursor on `method_a` – receiver is implicit `self`, i.e. the `B` instance.
2. Ancestor chain for `B` is computed: `[B] + prepends + includes => [B, A]`.
3. Analyzer searches `B` (no `method_a`), then `A` where it finds `method_a`; its location is returned.

## 6. Edge Cases & Future Work

* Multiple includes of the same module – we can skip duplicates when building chains to avoid infinite loops.
* `Module.prepend_features` / `append_features` metaprogramming may alter lookup; out of scope initially.
* Refinements override mixins; this will require separate modelling.
* `extend self` pattern – translates to including the module into its own singleton class; treat as `extend`.

## 7. Summary

By explicitly modelling the three mixin calls and reproducing Ruby’s method lookup order in the indexer we enable *Go to Definition* to jump from a method call to the correct definition, whether it lives in the class itself, an included module, a prepended concern, or a superclass – even across different files in the workspace.

---

## 8. Implementation Road-map

1. **Indexer Visitor Updates**  
   • In module/class visitors, detect `include|extend|prepend` call nodes with `nil` receiver.  
   • Accept only `ConstantPathNode`/`ConstantReadNode` arguments, resolve FQNs, and push into `includes|prepends|extends` vectors on the current `EntryBuilder`.

2. **Ancestor-Chain Helper**  
   Implement a utility that, given a class/module FQN and whether we’re looking for class or instance methods, returns an ordered list of ancestors following: `extends?` → `prepends` → self → `includes` → superclass recursion.

3. **Definition Capability**  
   Integrate the ancestor helper in `src/capabilities/definition.rs`; iterate ancestors to locate the first matching method.

4. **Index Maintenance**  
   Ensure module entries referenced by mixins are indexed so `methods_by_name` and `definitions` contain their methods.

5. **Testing**  
   Add fixtures covering:  
   • Instance mixin (`include`) receiver-less calls.  
   • Prepended modules overriding methods.  
   • Class methods via `extend`.

6. **Cleanup**  
   Remove deprecated `Mixin` edge logic and update docs/changelog.

---

## 9. Single-Pass vs Two-Pass Meta-Programming Indexing

There are two viable strategies for capturing meta-programming constructs such as `include`, `extend`, and `prepend`:

### 9.1 Single-Pass with **On-Demand** Lazy Resolution (current approach)
*   **Indexer walk (pass 1)** → for every `include / extend / prepend` argument build a `MixinRef` structure:
    ```rust
    struct MixinRef {
      parts: Vec<RubyConstant>,   // constant tokens, e.g. ["Foo","Bar"]
      absolute: bool,             // true if path began with ::
    }
    ```
    This is pushed into the current class/module’s `includes | prepends | extends` vector – no lookup is attempted yet.
*   **Nothing else happens at the end of indexing.** We deliberately *do not* run a second sweep.
*   **On-demand resolution** – when a feature such as *Go to Definition* calls `get_ancestor_chain` we resolve each `MixinRef` in that moment:
    1. If `absolute == true` probe the exact path, then progressively drop the left-most namespace segment.
    2. If `absolute == false` first prepend the caller’s current namespace stack, probe, then drop from the left (lexical fallback), finally treat as absolute.
    3. Use the first match that has an `Entry` in `index.definitions`; unresolved refs are ignored for this query.
*   **Pros**: single traversal, zero extra work on every file save, incremental by nature. The ancestor chain is always consistent with the latest workspace state.
*   **Cons**: ancestor-chain helper contains the fallback algorithm; the very first lookup after a new definition lands may spend extra microseconds resolving pending edges.

### 9.2 Separate Meta-Programming Pass
*   **Pass 1** – structural definitions (classes, modules, methods, constants).
*   **Pass 2** – meta-programming calls; with the index now fully populated every mixin argument can be validated immediately.
*   **Pros**: simpler ancestor-chain logic, easier to surface "unknown constant in include" diagnostics.
*   **Cons**: extra traversal, more complex incremental updates (both passes must be re-run for affected files, plus any files whose mixins point to them), cycles/forward references still need deferred handling, performance overhead.

### 9.3 Recommendation
For an interactive LSP the single-pass + lazy resolution strikes the best balance between accuracy, performance, and incremental simplicity. A second pass can be introduced later as an optimisation layer if profiling shows repeated ancestor-chain resolution is a bottleneck.

By explicitly modelling the three mixin calls and reproducing Ruby’s method lookup order in the indexer we enable *Go to Definition* to jump from a method call to the correct definition, whether it lives in the class itself, an included module, a prepended concern, or a superclass – even across different files in the workspace.
