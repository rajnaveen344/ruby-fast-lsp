# 🎯 Method Goto Definition Guide

Complete guide to how the LSP resolves method definitions using Ruby's method resolution order (MRO), including support for `include`, `prepend`, and `extend`.

---

## 📋 Quick Reference

### The Three Operations

```
include M  → 📦 Instance methods AFTER class   → obj.m ✅  Class.m ❌
prepend M  → ⚡ Instance methods BEFORE class  → obj.m ✅  Class.m ❌
extend M   → 🔧 Singleton methods              → obj.m ❌  Class.m ✅
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

## 🏗️ Architecture Overview

### Namespace-Based FQN System

The LSP uses a namespace-based approach where each class/module is represented as **two separate entries**:

```rust
// Instance namespace - for instance method lookup
Namespace(vec![RubyConstant("Foo")], NamespaceKind::Instance)

// Singleton namespace - for class method lookup
Namespace(vec![RubyConstant("Foo")], NamespaceKind::Singleton)
```

**Why Two Entries?**

This matches Ruby's internal object model where:
- Instance methods belong to the class itself (`Foo`)
- Class methods belong to the singleton class (`#<Class:Foo>`)

### FQN Display Format

```rust
// Instance namespace
Foo::Bar  // Namespace(["Foo", "Bar"], Instance)

// Singleton namespace
#<Class:Foo::Bar>  // Namespace(["Foo", "Bar"], Singleton)

// Methods (all use # since they're instance methods of their namespace)
Foo::Bar#method_name  // Method(["Foo", "Bar"], RubyMethod("method_name"))
```

### Indexing Strategy

During file indexing, the LSP creates:

1. **Two namespace entries per class/module**:
   ```rust
   // For: class Foo; end
   Namespace(["Foo"], Instance)   → Entry { kind: Class, ... }
   Namespace(["Foo"], Singleton)  → Entry { kind: Class, ... }
   ```

2. **Methods indexed under owner namespace**:
   ```rust
   // For: def bar; end (instance method)
   Method(["Foo"], "bar")
   // Stored with owner: Namespace(["Foo"], Instance)

   // For: def self.bar; end (class method)
   Method(["Foo"], "bar")
   // Stored with owner: Namespace(["Foo"], Singleton)
   ```

3. **Inheritance relationships** stored in `InheritanceGraph`:
   ```rust
   struct InheritanceGraph {
       superclass: HashMap<FqnId, FqnId>,
       includes: HashMap<FqnId, Vec<FqnId>>,
       prepends: HashMap<FqnId, Vec<FqnId>>,
       extends: HashMap<FqnId, Vec<FqnId>>,
       includers: HashMap<FqnId, Vec<FqnId>>,  // Reverse index
   }
   ```

---

## 🔍 How Method Resolution Works

### Entry Point

```
src/capabilities/definitions.rs:find_definition_at_position()
  ↓
src/query/definition.rs:find_definitions_at_position()
  ↓
src/query/definition.rs:find_method_definitions()
  ↓
src/query/method.rs (resolution logic)
```

### Resolution Flow

```rust
fn find_method_definitions(
    receiver: &MethodReceiver,
    method_name: &str,
    ancestors: &[RubyConstant],
    // ...
) -> Option<Vec<Location>> {
    // 1. Determine namespace kind from receiver
    let namespace_kind = match receiver {
        MethodReceiver::SelfReceiver => {
            // self.foo or Foo.bar → Singleton
            NamespaceKind::Singleton
        }
        MethodReceiver::Implicit | MethodReceiver::InstanceReceiver(..) => {
            // foo or obj.foo → Instance
            NamespaceKind::Instance
        }
    };

    // 2. Build namespace FQN with appropriate kind
    let receiver_fqn = FullyQualifiedName::namespace_with_kind(
        receiver_namespace_parts,
        namespace_kind
    );

    // 3. Get ancestor chain (automatically uses correct chain based on kind)
    let ancestor_chain = index.get_ancestor_chain(&receiver_fqn);

    // 4. Search for method in ancestor chain
    for ancestor_fqn in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(
            ancestor_fqn.namespace_parts(),
            method
        );

        if let Some(entries) = index.get(&method_fqn) {
            return Some(to_locations(entries));  // First match wins
        }
    }
}
```

### Ancestor Chain Computation

```rust
// src/indexer/index.rs
pub fn get_ancestor_chain(&self, fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName> {
    // Extract namespace kind from FQN
    let kind = fqn.namespace_kind().unwrap_or(NamespaceKind::Instance);

    // Get the FqnId for lookup
    let Some(fqn_id) = self.get_fqn_id(fqn) else {
        return vec![];
    };

    // Dispatch to appropriate chain based on namespace kind
    let fqn_ids = match kind {
        NamespaceKind::Singleton => {
            // Singleton chain: #<Class:Foo> → #<Class:Object> → #<Class:BasicObject>
            self.graph.singleton_lookup_chain(fqn_id)
        }
        NamespaceKind::Instance => {
            // Instance chain: Foo → Object → BasicObject
            self.graph.method_lookup_chain(fqn_id)
        }
    };

    // Convert FqnIds back to FullyQualifiedNames
    fqn_ids.into_iter()
        .filter_map(|id| self.get_fqn(id).cloned())
        .collect()
}
```

**Key Points:**
- Single `get_ancestor_chain()` method - kind is embedded in the FQN
- Returns different chains for Instance vs Singleton namespaces
- Chains are pre-computed during indexing for O(1) lookup

---

## 🎬 Resolution Examples

### Example 1: Instance Method Call

```ruby
class Foo
  def bar
    baz  # Resolve this
  end

  def baz
    "found"
  end
end
```

**Resolution:**
```
1. Determine kind: NamespaceKind::Instance (implicit receiver)
2. Build FQN: Namespace(["Foo"], Instance)
3. Get ancestor chain: [Namespace(["Foo"], Instance), Namespace(["Object"], Instance), ...]
4. Search for Method(["Foo"], "baz") → ✅ FOUND
```

### Example 2: Class Method Call

```ruby
class Foo
  def self.create
    validate  # Resolve this
  end

  def self.validate
    "found"
  end
end
```

**Resolution:**
```
1. Determine kind: NamespaceKind::Singleton (inside class method)
2. Build FQN: Namespace(["Foo"], Singleton)
3. Get ancestor chain: [Namespace(["Foo"], Singleton), Namespace(["Object"], Singleton), ...]
4. Search for Method(["Foo"], "validate") → ✅ FOUND (in singleton namespace)
```

### Example 3: Include Module

```ruby
module Helper
  def util_method
    "helper"
  end
end

class MyClass
  include Helper

  def test
    util_method  # Resolve this
  end
end
```

**Resolution:**
```
1. Determine kind: NamespaceKind::Instance
2. Build FQN: Namespace(["MyClass"], Instance)
3. Get ancestor chain:
   - Namespace(["MyClass"], Instance)
   - Namespace(["Helper"], Instance)  ← include adds to chain!
   - Namespace(["Object"], Instance)
4. Search ancestors:
   - Check Method(["MyClass"], "util_method") → Not found
   - Check Method(["Helper"], "util_method") → ✅ FOUND
```

### Example 4: Prepend Module

```ruby
module Logging
  def save
    puts "logging"
    super
  end
end

class User
  prepend Logging

  def save
    "original"
  end
end
```

**Ancestor Chain:**
```
User.new.save calls:
  [Namespace(["Logging"], Instance),     ← prepend comes FIRST
   Namespace(["User"], Instance),
   Namespace(["Object"], Instance),
   ...]
```

**Resolution:**
```
1. Check Method(["Logging"], "save") → ✅ FOUND (prepend wins!)
```

### Example 5: Extend Module

```ruby
module Findable
  def find(id)
    "found"
  end
end

class User
  extend Findable
end

# User.find(1) - how does this resolve?
```

**Resolution:**
```
1. Receiver: User (constant) → NamespaceKind::Singleton
2. Build FQN: Namespace(["User"], Singleton)
3. Get ancestor chain:
   - Namespace(["User"], Singleton)
   - Namespace(["Findable"], Instance)  ← extend adds module's instance methods!
4. Check Method(["Findable"], "find") → ✅ FOUND
```

**Note:** `extend` adds the module's **instance** methods to the receiver's **singleton** class.

---

## 📊 Implementation Details

### Data Structures

#### FullyQualifiedName Enum

```rust
pub enum FullyQualifiedName {
    /// Namespace (class/module) with kind
    Namespace(Vec<RubyConstant>, NamespaceKind),

    /// Value constant (not a class/module)
    Constant(Vec<RubyConstant>),

    /// Method (just namespace + name, no kind)
    Method(Vec<RubyConstant>, RubyMethod),

    // Variables...
}

pub enum NamespaceKind {
    Instance,   // Regular namespace
    Singleton,  // Singleton class
}
```

#### InheritanceGraph

```rust
pub struct InheritanceGraph {
    // Direct relationships
    superclass: HashMap<FqnId, FqnId>,
    includes: HashMap<FqnId, Vec<FqnId>>,
    prepends: HashMap<FqnId, Vec<FqnId>>,
    extends: HashMap<FqnId, Vec<FqnId>>,

    // Reverse index for module → classes lookup
    includers: HashMap<FqnId, Vec<FqnId>>,
}
```

### Key Methods

#### Method Lookup Chain

```rust
// src/indexer/inheritance_graph.rs

/// Instance method lookup chain (include/prepend/superclass)
pub fn method_lookup_chain(&self, fqn_id: FqnId) -> Vec<FqnId> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    self.collect_mro(fqn_id, &mut chain, &mut visited);
    chain
}

/// Singleton method lookup chain (extend/superclass singleton)
pub fn singleton_lookup_chain(&self, fqn_id: FqnId) -> Vec<FqnId> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    // Add singleton's extends
    if let Some(extends) = self.extends.get(&fqn_id) {
        for ext_id in extends.iter().rev() {
            self.collect_mro(*ext_id, &mut chain, &mut visited);
        }
    }

    // Add the singleton class itself
    if visited.insert(fqn_id) {
        chain.push(fqn_id);
    }

    // Walk up singleton class chain
    let mut current = fqn_id;
    while let Some(&parent_id) = self.superclass.get(&current) {
        if !visited.insert(parent_id) {
            break;  // Cycle detected
        }
        chain.push(parent_id);
        current = parent_id;
    }

    chain
}

/// Collect method resolution order (prepends → self → includes → superclass)
fn collect_mro(&self, fqn_id: FqnId, chain: &mut Vec<FqnId>, visited: &mut HashSet<FqnId>) {
    // 1. Add prepends (in reverse order)
    if let Some(prepends) = self.prepends.get(&fqn_id) {
        for prepend_id in prepends.iter().rev() {
            self.collect_mro(*prepend_id, chain, visited);
        }
    }

    // 2. Add self
    if visited.insert(fqn_id) {
        chain.push(fqn_id);
    }

    // 3. Add includes (in reverse order)
    if let Some(includes) = self.includes.get(&fqn_id) {
        for include_id in includes.iter().rev() {
            self.collect_mro(*include_id, chain, visited);
        }
    }

    // 4. Add superclass
    if let Some(&superclass_id) = self.superclass.get(&fqn_id) {
        self.collect_mro(superclass_id, chain, visited);
    }
}
```

#### Creating Namespace Entries

```rust
// src/analyzer_prism/visitors/index_visitor/class_node.rs

pub fn process_class_node_entry(&mut self, node: &ClassNode) {
    let namespace_parts = /* ... build from node ... */;

    // Create BOTH Instance and Singleton namespace entries
    let instance_fqn = FullyQualifiedName::namespace_with_kind(
        namespace_parts.clone(),
        NamespaceKind::Instance
    );

    let singleton_fqn = FullyQualifiedName::namespace_with_kind(
        namespace_parts.clone(),
        NamespaceKind::Singleton
    );

    // Index both
    self.index.add(instance_fqn, Entry { kind: EntryKind::Class, ... });
    self.index.add(singleton_fqn, Entry { kind: EntryKind::Class, ... });

    // Store inheritance relationships for both
    if let Some(superclass) = node.superclass() {
        self.index.graph.add_superclass(instance_fqn, superclass_fqn);
        self.index.graph.add_superclass(singleton_fqn, superclass_singleton_fqn);
    }
}
```

#### Indexing Methods

```rust
// src/analyzer_prism/visitors/index_visitor/def_node.rs

pub fn process_def_node_entry(&mut self, node: &DefNode) {
    let method_name = /* ... */;
    let namespace_parts = self.scope_tracker.get_ns_stack();

    // Determine which namespace owns this method
    let owner_kind = if let Some(receiver) = node.receiver() {
        if receiver.as_self_node().is_some() {
            NamespaceKind::Singleton  // def self.foo
        } else {
            NamespaceKind::Instance   // def obj.foo
        }
    } else if self.scope_tracker.in_singleton() {
        NamespaceKind::Singleton      // class << self; def foo
    } else {
        NamespaceKind::Instance       // def foo
    };

    // Create method FQN (no kind - just namespace + name)
    let method_fqn = FullyQualifiedName::method(
        namespace_parts.clone(),
        RubyMethod::new(method_name)?
    );

    // Store with owner namespace that has the kind
    let owner_fqn = FullyQualifiedName::namespace_with_kind(
        namespace_parts,
        owner_kind
    );

    self.index.add(method_fqn, Entry {
        kind: EntryKind::Method(MethodData {
            owner: owner_fqn,
            return_type: /* infer */,
        }),
        location,
    });
}
```

---

## 💡 Common Patterns

### Instance Methods (include)

**Use:** Add shared behavior that the class can override

```ruby
module Taggable
  def add_tag(tag)
    (@tags ||= []) << tag
  end
end

class Article
  include Taggable  # Article#add_tag available
end
```

### Decorator Pattern (prepend)

**Use:** Wrap existing methods with additional behavior

```ruby
module Timestamped
  def save
    self.updated_at = Time.now
    super  # Call original
  end
end

class User
  prepend Timestamped  # Timestamped#save wraps User#save
  def save
    # original implementation
  end
end
```

### Class Methods (extend)

**Use:** Add class-level utilities

```ruby
module Findable
  def find_by_name(name)
    # search logic
  end
end

class User
  extend Findable  # User.find_by_name available
end
```

---

## ⚠️ Common Gotchas

### 1. extend vs include Confusion

❌ `include M` for class methods
✅ `extend M` for class methods

### 2. Prepend Order

❌ Assuming `prepend M; prepend N` → M first
✅ Actually: `N → M → Class` (reverse!)

### 3. Module Class Methods Don't Transfer

❌ Expecting `module M; def self.foo; end; end` to transfer via include
✅ Only instance methods transfer - use `extend` or `included` hook

---

## 📌 Key Takeaways

1. **Namespace-based indexing**: Each class/module creates TWO namespace entries (Instance and Singleton)
2. **Kind embedded in FQN**: `get_ancestor_chain()` automatically dispatches based on `NamespaceKind`
3. **Methods are namespace-agnostic**: Method FQN has no kind - owner namespace determines instance vs class method
4. **Pre-computed chains**: Inheritance graph computed during indexing for O(1) lookup
5. **MRO order**: Prepend → Self → Include → Superclass (matches Ruby semantics)
6. **Cycle prevention**: `visited` set prevents infinite loops in circular relationships

---

## 🧪 Testing

Run integration tests to verify method resolution:

```bash
cargo test --test integration methods
```

Key test files:
- `src/test/integration/methods/definition.rs`
- `src/test/integration/methods/include_prepend.rs`
- `src/test/integration/methods/singleton.rs`

---

## 📚 See Also

- **src/query/method.rs**: Method resolution implementation
- **src/indexer/index.rs**: Ancestor chain computation
- **src/indexer/inheritance_graph.rs**: Graph data structure and MRO
- **src/analyzer_prism/visitors/index_visitor/**: Indexing logic
- **src/test/integration/methods/**: Comprehensive test suite
