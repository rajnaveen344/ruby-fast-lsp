# 🎯 Method Goto Definition Guide

Complete guide to understanding how the LSP resolves method definitions, including Ruby's `include`, `prepend`, and `extend`.

---

## 📋 Quick Reference

### The Three Operations

```
include M  → 📦 Instance methods AFTER class   → obj.m ✅  Class.m ❌
prepend M  → ⚡ Instance methods BEFORE class  → obj.m ✅  Class.m ❌
extend M   → 🔧 Class methods (singleton)      → obj.m ❌  Class.m ✅
```

### Priority Rules

```
⚡ prepend > 🎯 class > 📦 include > 🔗 superclass

Lookup: [Prepends] → [Class] → [Includes] → [Superclass] → [Object] → [BasicObject]
```

### Key Rules

1. ⚡ **Prepend beats everything** (even class methods)
2. 🎯 **Class beats include** (but not prepend)
3. 📌 **Last include wins** (multiple includes checked in reverse)
4. ⚡ **Prepends reverse order** (last prepend checked first)

---

## 🔍 How the LSP Resolves Methods

### Entry Point

```
src/capabilities/definitions.rs:find_definition_at_position()
  ↓
src/query/definition.rs:find_definitions_at_position()
  ↓
src/query/method.rs:find_method_definitions()
```

### Resolution Strategy

The LSP uses **two different strategies** based on context:

#### 1️⃣ Class Context (Search UP)

```rust
// src/query/method.rs:445-490
search_method_in_class_hierarchy_static()
```

**Used when**: Inside a class calling a method
**Strategy**: Walk up the inheritance chain + includes
**Returns**: First match

```
MyClass
  ↓ Check: MyClass itself
  ↓ Check: Includes (reverse order)
  ↓ Check: Superclass
  ↓ Check: Superclass includes
  ↓ Check: Object → Kernel → BasicObject
```

#### 2️⃣ Module Context (Search DOWN)

```rust
// src/query/method.rs:492-534
search_method_in_including_classes_static()
```

**Used when**: Inside a module calling a method
**Strategy**: Find all classes that include this module
**Returns**: ALL matches (multiple implementations)

```
SharedModule
  ↓ Check: Module itself
  ↓ Find: WHO includes me?
    → ClassA (check its hierarchy)
    → ClassB (check its hierarchy)
    → ClassC (check its hierarchy)
  ↓ Returns: All implementations
```

### Critical Decision Point

```rust
// src/query/method.rs:419-443
if is_class_context_static(&index, receiver_fqn) {
    // CLASS: Search UP
    search_method_in_class_hierarchy_static(...)
} else {
    // MODULE: Search DOWN
    search_method_in_including_classes_static(...)
}
```

---

## 🎬 Traversal Examples

### Example 1: Class with Include

```ruby
module M
  def helper
    "M"
  end
end

class MyClass
  include M

  def test
    helper  # <-- Goto definition here
  end
end
```

**Traversal:**

```
START: MyClass (class context)
  ↓
1. Check MyClass#helper → NOT FOUND
2. Check M#helper → ✅ FOUND

Result: M#helper at line 2
```

---

### Example 2: Class with Prepend

```ruby
module M
  def helper
    "M"
  end
end

class MyClass
  prepend M

  def helper
    "MyClass"
  end

  def test
    helper  # <-- What gets called?
  end
end
```

**Traversal:**

```
START: MyClass (class context)
  ↓
Lookup order: M → MyClass → Object...
  ↓
1. Check M#helper → ✅ FOUND (prepend wins!)

Result: M#helper (not MyClass#helper)
```

---

### Example 3: Module Calling Method in Multiple Classes

```ruby
module SharedModule
  def process
    helper_method  # <-- Goto definition
  end
end

class ClassA
  include SharedModule
  def helper_method; "A"; end
end

class ClassB
  include SharedModule
  def helper_method; "B"; end
end
```

**Traversal:**

```
START: SharedModule (module context)
  ↓
1. Check SharedModule#helper_method → NOT FOUND
2. Find classes that include SharedModule:
   - index.get_including_classes(SharedModule)
   - Returns: [ClassA, ClassB]
3. Search in ClassA hierarchy → ClassA#helper_method ✅
4. Search in ClassB hierarchy → ClassB#helper_method ✅

Result: BOTH implementations (line 8 and line 13)
```

**Key**: Module context returns **all possible implementations**!

---

### Example 4: Nested Includes

```ruby
module A
  def a_method; end
end

module B
  include A
  def b_method; end
end

class MyClass
  include B

  def test
    a_method  # <-- How does this resolve?
  end
end
```

**Traversal:**

```
START: MyClass
  ↓
Search space built by collect_all_searchable_modules_static():
  {MyClass, B, A, Object, Kernel, BasicObject}
  ↓
1. Check MyClass#a_method → NOT FOUND
2. Check B#a_method → NOT FOUND
3. Check A#a_method → ✅ FOUND

Result: A#a_method (through transitive include)
```

---

## 📊 Truth Table: Priority Rules

### Class Method vs Include

| Code                                                       | Result | Winner                 |
| ---------------------------------------------------------- | ------ | ---------------------- |
| `class C`<br>`  include M`<br>`  def m; "C"; end`<br>`end` | `"C"`  | 🎯 Class beats include |
| `class C`<br>`  prepend M`<br>`  def m; "C"; end`<br>`end` | `"M"`  | ⚡ Prepend beats class  |

### Multiple Includes

| Code                                                 | Result               | Reason               |
| ---------------------------------------------------- | -------------------- | -------------------- |
| `class C`<br>`  include M`<br>`  include N`<br>`end` | N's method           | 📌 Last include wins |
| `class C`<br>`  prepend M`<br>`  prepend N`<br>`end` | Check `C.ancestors`! | ⚡ Prepends reverse   |

**To check**: `C.ancestors # => [N, M, C, ...]` (last prepend is first in chain)

### Mix of All Three

```ruby
module M; def m; "M"; end; end
module N; def m; "N"; end; end
module P; def m; "P"; end; end

class C
  include M
  prepend N
  include P
  def m; "C"; end
end

C.new.m  # Result?
```

**Lookup**: `N → C → P → M → Object...`
**Result**: `"N"` (prepend always first)

---

## 🏗️ Implementation Details

### Data Structures

#### InheritanceGraph

```rust
// src/indexer/inheritance_graph.rs
pub struct InheritanceGraph {
    superclass: HashMap<FqnId, FqnId>,           // Class → Superclass
    includes: HashMap<FqnId, Vec<FqnId>>,        // includes tracking
    prepends: HashMap<FqnId, Vec<FqnId>>,        // prepends tracking
    extends: HashMap<FqnId, Vec<FqnId>>,         // extends tracking
    includers: HashMap<FqnId, Vec<FqnId>>,       // Reverse: Module → Classes
}
```

#### Index

```rust
// FQN-based HashMap for O(1) lookup
HashMap<FullyQualifiedName, Vec<Entry>>

// Examples:
"MyClass::MyModule#method_name" → Entry { location, ... }
"MyClass" → Entry { kind: Class, includes: [...], ... }
```

### Key Functions

#### 1. Building the Search Space

```rust
fn search_method_in_class_hierarchy_static() {
    let mut modules_to_search = HashSet::new();

    // Add class itself
    modules_to_search.insert(receiver_fqn);

    // Get ancestor chain (superclass → Object → BasicObject)
    let ancestor_chain = index.get_ancestor_chain(receiver_fqn, is_class_method);

    // For each ancestor, get its mixins
    for ancestor in ancestor_chain {
        let included_modules = get_included_modules_static(index, ancestor);

        // Recursively collect all searchable modules
        for module in included_modules {
            collect_all_searchable_modules_static(index, module, &mut modules_to_search);
        }
    }

    // Search for method in all collected modules
    for module in modules_to_search {
        let method_fqn = FullyQualifiedName::method(module, method_name);
        if let Some(entries) = index.get(&method_fqn) {
            return Some(entries); // First match wins
        }
    }
}
```

#### 2. Processing Mixins

```rust
fn process_entry_mixins_static(...) {
    // CRITICAL: Process in Ruby's order!

    // 1. Prepends (reverse order for correct priority)
    Self::process_mixins_static(prepends, reverse_order = true);

    // 2. Includes
    Self::process_mixins_static(includes, reverse_order = false);

    // 3. Extends
    Self::process_mixins_static(extends, reverse_order = false);
}
```

#### 3. Recursive Module Collection

```rust
fn collect_all_searchable_modules_static(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    modules_to_search: &mut HashSet<FullyQualifiedName>,
) {
    if modules_to_search.contains(fqn) {
        return; // Prevent infinite loops
    }

    modules_to_search.insert(fqn.clone());

    // Get this module's ancestor chain
    let ancestor_chain = index.get_ancestor_chain(fqn, false);
    for ancestor in ancestor_chain {
        modules_to_search.insert(ancestor);
    }

    // Get this module's includes
    let included_modules = get_included_modules_static(index, fqn);
    for module in included_modules {
        // Recursive call
        collect_all_searchable_modules_static(index, module, modules_to_search);
    }
}
```

---

## 💡 Common Patterns

### 📦 Shared Behavior (include)

```ruby
module Taggable
  def add_tag(tag)
    (@tags ||= []) << tag
  end
end

class Article; include Taggable; end
class Video; include Taggable; end
```

**Use**: Add instance methods that class can override

### 🎁 Decorator (prepend)

```ruby
module Timestamped
  def save
    self.updated_at = Time.now
    super  # Call original
  end
end

class User
  prepend Timestamped
  def save; ...; end
end
```

**Use**: Wrap existing methods with additional behavior

### 🔧 Class Methods (extend)

```ruby
module Findable
  def find_by_name(name)
    # search logic
  end
end

class User
  extend Findable  # User.find_by_name
end
```

**Use**: Add class-level utilities

### 🎨 Both Instance and Class Methods

```ruby
module Sortable
  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    def sorted; ...; end
  end

  def sort_key; ...; end
end

class User
  include Sortable
  # Now has: User.sorted AND user.sort_key
end
```

**Use**: Rails ActiveSupport::Concern pattern

---

## ⚠️ Common Gotchas

### 1. extend vs include Confusion

❌ `include M` for class methods
✅ `extend M` for class methods

### 2. Prepend Order

❌ Assuming `prepend M; prepend N` → M first
✅ Actually: `N → M → Class` (reverse!)

### 3. super Without Target

❌ Using `super` when nothing to call
✅ Check `.ancestors` or use `defined?(super)`

### 4. Module Class Methods Don't Transfer

❌ Expecting `module M; def self.foo; end; end` to transfer via include
✅ Only instance methods transfer

---

## 🧪 Testing

### Check Lookup Order

```ruby
MyClass.ancestors
# => [Prepends, MyClass, Includes, Superclass, Object, Kernel, BasicObject]
```

### Find Method Owner

```ruby
MyClass.instance_method(:method_name).owner
# => MyModule (or MyClass, etc.)
```

### Run Examples

```bash
ruby examples/metaprogramming_examples.rb
```

---

## 🎯 Decision Guide

| I want to...         | Use             | When                |
| -------------------- | --------------- | ------------------- |
| Add instance methods | `include M`     | Shared behavior     |
| Add class methods    | `extend M`      | Class utilities     |
| Wrap methods         | `prepend M`     | Decorators          |
| Provide defaults     | `include M`     | Class can override  |
| Force override       | `prepend M`     | Can't be overridden |
| Add to one object    | `obj.extend(M)` | Singleton           |

---

## 📌 Key Takeaways

1. **Two search strategies**: Class context (UP) vs Module context (DOWN)
2. **Priority order**: Prepend > Class > Include > Superclass
3. **Recursive collection**: Modules can include modules (transitive)
4. **FQN-based index**: O(1) lookup using fully qualified names
5. **Pre-computed graph**: InheritanceGraph built during indexing
6. **Visitor set**: Prevents infinite loops in circular includes

---

## 📚 See Also

- **examples/**: Runnable Ruby code demonstrating all scenarios
- **src/query/method.rs**: Full implementation
- **src/indexer/inheritance_graph.rs**: Graph data structure
