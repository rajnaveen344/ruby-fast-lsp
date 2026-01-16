# 🔗 Singleton Chain Example

Understanding the difference between instance method lookup and class method (singleton) lookup.

---

## 🎯 Ruby Code Example

```ruby
module ExtendedA
  def method_a; "ExtendedA"; end
end

module ExtendedB
  def method_b; "ExtendedB"; end
end

module IncludedM
  def method_m; "IncludedM"; end
end

module PrependedP
  def method_p; "PrependedP"; end
end

class Parent
  extend ExtendedA
  include IncludedM

  def self.parent_class_method
    "Parent.parent_class_method"
  end

  def parent_instance_method
    "Parent#parent_instance_method"
  end
end

class Child < Parent
  extend ExtendedB
  prepend PrependedP

  def self.child_class_method
    "Child.child_class_method"
  end

  def child_instance_method
    "Child#child_instance_method"
  end
end
```

---

## 📊 Instance Method Lookup: `Child.new.method_m`

When you call an **instance method** on `Child.new`:

### Ruby's `.ancestors`
```ruby
Child.ancestors
# => [PrependedP, Child, Parent, IncludedM, Object, Kernel, BasicObject]
```

### LSP's `get_ancestor_chain(Child, is_class_method: false)`
```
PrependedP → Child → Parent → IncludedM → Object → Kernel → BasicObject
```

**Lookup order**:
1. ✅ PrependedP (prepended in Child)
2. ✅ Child itself
3. ✅ Parent (superclass)
4. ✅ IncludedM (included in Parent)
5. ✅ Object, Kernel, BasicObject

**Note**: `ExtendedA` and `ExtendedB` are **NOT** in the instance chain!

---

## 🔍 Class Method Lookup: `Child.method_b`

When you call a **class method** on `Child`:

### Ruby's Singleton Class Ancestors
```ruby
Child.singleton_class.ancestors
# => [#<Class:Child>, ExtendedB, #<Class:Parent>, ExtendedA, #<Class:Object>, ...]
```

### LSP's `get_ancestor_chain(Child, is_class_method: true)`
```
ExtendedB → Child → Parent → ExtendedA → Object → Kernel → BasicObject
```

**Lookup order**:
1. ✅ **ExtendedB** (extended in Child) ← This is FIRST!
2. ✅ Child itself (class methods defined with `def self.`)
3. ✅ Parent (superclass)
4. ✅ **ExtendedA** (extended in Parent) ← Parent's extended modules!
5. ✅ Object, Kernel, BasicObject

**Note**: `PrependedP` and `IncludedM` are **NOT** in the singleton chain!

---

## 🔑 Key Differences

| Feature | Instance Chain (`is_class_method: false`) | Singleton Chain (`is_class_method: true`) |
|---------|------------------------------------------|------------------------------------------|
| **Prepends** | ✅ Included | ❌ Not included |
| **Includes** | ✅ Included | ❌ Not included |
| **Extends** | ❌ Not included | ✅ **Included FIRST** |
| **Superclass prepends** | ✅ Included | ❌ Not included |
| **Superclass includes** | ✅ Included | ❌ Not included |
| **Superclass extends** | ❌ Not included | ✅ **Included** (after superclass) |

---

## 💡 Why This Matters

### Example 1: Finding `method_b`

```ruby
Child.method_b  # ← Goto definition
```

**With `is_class_method: true`** (CORRECT):
1. Checks ExtendedB first
2. ✅ Finds `ExtendedB#method_b`

**If we used `is_class_method: false`** (WRONG):
1. Checks PrependedP, Child, Parent, IncludedM
2. ❌ Never finds it! ExtendedB is not in the instance chain

---

### Example 2: Finding `method_m`

```ruby
Child.new.method_m  # ← Goto definition
```

**With `is_class_method: false`** (CORRECT):
1. Checks PrependedP, Child, Parent, IncludedM
2. ✅ Finds `IncludedM#method_m`

**If we used `is_class_method: true`** (WRONG):
1. Checks ExtendedB, Child, Parent, ExtendedA
2. ❌ Never finds it! IncludedM is not in the singleton chain

---

## 🔄 Complete Chains Compared

### Instance Method Chain
```
prepend PrependedP          ← Instance methods from prepended modules
    ↓
  Child                     ← Instance methods defined in Child
    ↓
 Parent                     ← Instance methods from superclass
    ↓
include IncludedM           ← Instance methods from included modules
    ↓
 Object → Kernel → BasicObject
```

### Singleton (Class Method) Chain
```
extend ExtendedB            ← Class methods from extended modules in Child
    ↓
  Child                     ← Class methods (def self.foo) in Child
    ↓
 Parent                     ← Inherited class methods from superclass
    ↓
extend ExtendedA            ← Class methods from extended modules in Parent
    ↓
 Object → Kernel → BasicObject
```

---

## 🎯 LSP Implementation

```rust
// src/indexer/graph.rs:382-397
pub fn singleton_lookup_chain(&self, fqn_id: FqnId) -> Vec<FqnId> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    // 1. First add extended modules (THIS IS THE KEY DIFFERENCE!)
    if let Some(node) = self.nodes.get(&fqn_id) {
        for module_id in node.extends.iter().rev() {
            self.build_instance_mro(*module_id, &mut chain, &mut visited);
        }
    }

    // 2. Then add the instance method chain (for inherited class methods)
    self.build_instance_mro(fqn_id, &mut chain, &mut visited);

    chain
}
```

**The critical difference**:
- Singleton chain adds **extended modules FIRST**
- Then processes the class itself and superclass chain
- But **skips** prepends and includes from the superclass (they don't affect class methods)

---

## 📝 Quick Reference

| Method Call | Chain Type | Checks |
|-------------|-----------|--------|
| `obj.foo` | Instance | prepend, class, include, superclass prepend/include |
| `Class.foo` | Singleton | extend, class, superclass extend |
| `self.foo` (in class body) | Singleton | Same as `Class.foo` |

---

## 🧪 Test It Yourself

Run this Ruby code to see the actual chains:

```ruby
# Instance chain
puts "Instance ancestors:"
p Child.ancestors

# Singleton chain
puts "\nSingleton ancestors:"
p Child.singleton_class.ancestors

# What methods can Child (the class) call?
puts "\nChild class methods:"
p Child.singleton_methods(false)

# What methods can Child.new (an instance) call?
puts "\nChild instance methods:"
p Child.new.methods - Object.new.methods
```
