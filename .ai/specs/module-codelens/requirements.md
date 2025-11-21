Module CodeLens shows how many times a module is used via `include`, `prepend`, and `extend` across the workspace. It appears above module definitions and is only shown when at least one usage is found.

## Goal

- Provide a concise, actionable CodeLens on module definitions that summarizes mixin usages and opens the references view when clicked.

## Display Format

- Format: `X include | Y prepend | Z extend`
- Only show categories with at least one usage (e.g., `2 include` or `1 include | 1 extend`).
- Clicking the CodeLens opens the references view filtered to these mixin calls.

## Functional Requirements

- Detect module definitions in a document, including nested and namespaced forms (e.g., `module A::B`).
- Compute the fully-qualified module name using the file’s namespace and parent modules.
- Query the index for mixin usages of the module: `include`, `prepend`, `extend`.
- Count usages per mixin type and render the CodeLens label accordingly.
- Do not render CodeLens for modules with zero usages.
- Recompute CodeLens when the document changes or when the index updates.
- Support cross-file usages across the workspace.

## Non-Functional Requirements

- Performance: Query the index in constant or logarithmic time; avoid full-file scans for each lens.
- Responsiveness: Debounce recompute on edits; avoid blocking the UI.
- Correctness: Name resolution must respect relative vs. absolute constants (`A::B` vs `::A::B`).
- Robustness: If the index is unavailable, show no lens or a subtle `indexing…` state; do not crash.

## Scope

- In scope: static `include`, `prepend`, `extend` calls with direct module constant references.
- Out of scope for v1: dynamic mixins (e.g., `send(:include, ...)`, `const_get`), refinements via `using`, metaprogramming that obscures constant identity.

## Edge Cases

- Nested modules and reopened modules: aggregate usages across definitions sharing the same fully-qualified name.
- Conditional mixins (inside `if`, `case`, etc.): count usages if statically resolvable.
- Namespaced references: handle `Outer::Inner` within different lexical scopes.
- `Module.new` assigned to a constant: treat the constant as the module’s name.

## Configuration

- `rubyFastLSP.codeLens.modules.enabled` (boolean, default: `true`): enable/disable module CodeLens.
- Respect editor-wide CodeLens setting (if disabled globally, do not render).

## Testing Criteria

- Include/prepend/extend single usage cases render correct counts and clicking opens references.
- Multiple categories render correctly and omit zero-count categories.
- Cross-file usages are counted; nested/namespaced modules resolve properly.
- `Module.new` assigned to a constant is counted.
- No usages → no lens.

## Examples

### Basic `include`

```ruby
# Expected CodeLens: 1 include
module MyModule; end

class MyClass
  include MyModule
end
```

### Basic `prepend`

```ruby
# Expected CodeLens: 1 prepend
module MyModule; end

class MyClass
  prepend MyModule
end
```

### Basic `extend`

```ruby
# Expected CodeLens: 1 extend
module MyModule; end

class MyClass
  extend MyModule
end
```

### Nested module

```ruby
module Outer
  # Expected CodeLens: 1 include
  module Inner; end
end

class MyClass
  include Outer::Inner
end
```

### Module.new

```ruby
# Expected CodeLens: 1 include
MyModule = Module.new

class MyClass
  include MyModule
end
```

### Cross-file usages

`file1.rb`
```ruby
# Expected CodeLens: 2 include
module MyModule; end
```

`file2.rb`
```ruby
class MyClass
  include MyModule
end

class AnotherClass
  include MyModule
end
```

### Multiple categories

```ruby
# Expected CodeLens: 1 include | 1 prepend | 1 extend
module MyModule; end

class MyClass
  include MyModule
end

class AnotherClass
  extend MyModule
end

module AnotherModule
  prepend MyModule
end
```