# Ruby Edge Cases for Static Analysis

This document tracks Ruby language features that affect static analysis, particularly for the inheritance graph and method resolution.

## Known Limitations (Not Supported)

### Dynamic Mixins

```ruby
class User
  include SomeModule if Rails.env.production?
  include(*MIXINS)  # Splat from array
end
```

**Why:** Requires runtime evaluation. Static analysis cannot determine which modules are included.

### Runtime Includes

```ruby
class User
  def self.enable_feature!
    include FeatureModule  # Called at runtime
  end
end
```

**Why:** `include` happens during method execution, not at parse time.

### Anonymous Modules

```ruby
class User
  include Module.new {
    def dynamic_method; end
  }
end
```

**Why:** No FQN for anonymous modules - can't be indexed or referenced.

### ActiveSupport::Concern Magic

```ruby
module Nameable
  extend ActiveSupport::Concern

  included do
    # Block executed when included
  end

  class_methods do
    def find_by_name(n); end
  end
end
```

**Why:** `class_methods` creates a nested `ClassMethods` module that gets auto-extended via ActiveSupport runtime magic. We can index `Nameable::ClassMethods`, but the automatic extend happens at runtime.

---

## Supported Features

### Prepend Ordering

```ruby
module Logging
  def save
    puts "Saving..."
    super
  end
end

class User
  prepend Logging
  def save; end
end
```

**Status:** Fully supported. The graph has separate `prepends` list, and `method_lookup_chain` processes prepends before the class itself.

### Extend Self

```ruby
module Utils
  extend self

  def helper; end  # Callable as Utils.helper AND as instance method
end
```

**Status:** Should work. Creates self-referential edge `Utils.extends = [Utils]`.

**TODO:** Add explicit test case.

---

## Bugs / Fixes Needed

### `include` Inside Singleton Class

**Current behavior (BUG):**

```ruby
class User
  class << self
    include AdminMethods  # Currently indexed as include
  end
end
```

The indexer treats this as a regular `include`, but it should be treated as an `extend` because it's adding methods to the singleton class (class methods).

**Location:** `src/analyzer_prism/visitors/index_visitor/call_node/mod.rs`

**Fix:**

```rust
match method_name.as_ref() {
    "include" => {
        if self.scope_tracker.in_singleton() {
            entry.add_extends(mixin_refs);  // Singleton include = extend
        } else {
            entry.add_includes(mixin_refs);
        }
    }
    "extend" => entry.add_extends(mixin_refs),
    "prepend" => {
        if self.scope_tracker.in_singleton() {
            // prepend in singleton affects class method lookup
            // May need separate handling for singleton prepends
            entry.add_extends(mixin_refs);
        } else {
            entry.add_prepends(mixin_refs);
        }
    }
    _ => {}
}
```

**Priority:** Medium - affects correctness of class method resolution when using singleton class syntax.

---

## Future Considerations

### Singleton Class Prepends

Ruby allows prepending in singleton class context:

```ruby
class User
  class << self
    prepend LoggingMethods
  end
end
```

This affects the MRO for class methods. Currently we don't have a separate `singleton_prepends` list - need to decide if:

1. Treat as regular extends (loses prepend semantics)
2. Add `singleton_prepends` to `ClassData`
3. Track separately in the graph

### Method Visibility in Mixins

```ruby
module Secret
  private
  def hidden; end
end

class User
  include Secret
  # hidden is private in User too
end
```

Visibility propagates through mixins. The graph doesn't track visibility - it's stored in `MethodData.visibility`. Method resolution should respect visibility when filtering results.

### Refinements

```ruby
module StringRefinements
  refine String do
    def shout
      upcase + "!"
    end
  end
end

using StringRefinements
"hello".shout  # => "HELLO!"
```

Refinements are lexically scoped and extremely complex for static analysis. Currently not supported and likely won't be in the near future.
