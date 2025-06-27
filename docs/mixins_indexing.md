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


By explicitly modelling the three mixin calls and reproducing Ruby’s method lookup order in the indexer we enable *Go to Definition* to jump from a method call to the correct definition, whether it lives in the class itself, an included module, a prepended concern, or a superclass – even across different files in the workspace.
