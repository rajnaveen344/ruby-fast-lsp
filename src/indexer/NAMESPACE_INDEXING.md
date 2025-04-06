# 📚 Ruby Modules and Classes via Constant Paths (ConstPathNode)

This document serves as a reference for understanding how Ruby handles module and class definitions using **constant paths** (`A::B::C`) and how to replicate this behavior in a Language Server Protocol (LSP) implementation.

---

## 📌 Terminology

- **ConstPathNode**: A node representing a constant path like `A::B::C`
- **ModuleNode / ClassNode**: AST nodes that define modules and classes
- **Namespace Tree**: A data structure that mimics Ruby's constant resolution across nested modules and classes

---

## 🔍 Constant Path Definitions: The Basics

When defining a module/class like this:

```ruby
class A::B::C
end
```

Ruby attempts to resolve the path **from left to right**:

1. `A` must exist
2. `A::B` must exist
3. Only then will Ruby define `C` under `A::B`

---

## ❌ No Auto-Vivification in Constant Paths

Ruby does **not** auto-create intermediate constants when using constant paths in `module` or `class` definitions.

```ruby
class A::B::C  # ❌ Raises NameError unless A::B exists
end
```

✅ You must predefine intermediate namespaces:

```ruby
module A
  module B
  end
end

class A::B::C  # ✅ Works now
end
```

---

## ✅ Where Auto-Vivification *Does* Work

Auto-vivification (automatic creation of intermediate modules) **only occurs** with nested block-style definitions:

```ruby
module A  # A is created
  module B  # B is created under A
    class C  # C is created under A::B
    end
  end
end
```

This pattern is safe and common in idiomatic Ruby.

---

## ⟳ Constant Path Resolution Rules

### When defining:

For `class A::B::C` or `module A::B::C`:

- Check that:
  - `A` is a defined constant
  - `A::B` is a defined constant under `A`
  - `C` is the name to be defined under `A::B`

### When accessing:

For `A::B::C::D` (e.g., `puts A::B::C::D`):

- All constants in the path **must already be defined**
- No auto-vivification ever applies to **access paths**

---

## ⚖️ Suggested Namespace Tree for LSP

To implement Ruby's constant behavior in an LSP:

1. **Build a tree** with each node representing a module or class
2. For each `ConstPathNode`, validate the path:
   - `resolve_namespace(['A', 'B'])` must succeed
   - Only then can `define_constant('C')` under it
3. Index constant definitions and references with full qualified names (`A::B::C`)
4. Maintain metadata (e.g., is_module?, is_class?, location, doc, etc.)

### Suggested Tree Node Structure

```ruby
class NamespaceNode
  attr_reader :name, :children, :type  # :module or :class

  def initialize(name, type)
    @name = name
    @type = type
    @children = {}  # map: String => NamespaceNode
  end
end
```

---

## 💥 Common Pitfalls

### ❌ Defining under an undefined path

```ruby
module A::B::C
end
# => Error: uninitialized constant A
```

### ✅ Safe definition by nesting

```ruby
module A
  module B
    module C  # ✅ Safe
    end
  end
end
```

### ❌ Expecting `A::B` to be auto-created

```ruby
class A::B::C
end
# => Error unless A::B already exists
```

---

## ✏️ Best Practices

- **Always predefine** intermediate namespaces when using `::` paths
- Prefer nested `module` / `class` blocks for safer structure
- For LSP:
  - Walk the full constant path
  - Verify each segment exists before defining
  - Emit warnings or diagnostics for missing parent paths

---

## 🔪 Test Cases for Your LSP

| Code                                             | Should Work? | Why                                  |
|--------------------------------------------------|--------------|---------------------------------------|
| `class A::B::C`                                  | ❌           | A and A::B must exist first           |
| `module A; module B; class C; end; end; end`     | ✅           | All parents defined in block style    |
| `module A::B::C`                                  | ❌           | A::B must exist first                 |
| `module A; end` followed by `module A::B`        | ✅           | A exists, so B can be defined under it|
| `puts A::B::C`                                   | ❌           | A, A::B, and A::B::C must exist       |

---

## ✅ Summary

- Constant path nodes require existing parent namespaces
- Ruby **does not** auto-create intermediate constants in `A::B::C` unless using nested blocks
- Build your LSP's constant indexer to resolve each step of the path
- Validate and report missing namespaces to match Ruby’s behavior

---

## 📌 Extras

- Consider implementing `Object.const_defined?(:X)` style resolution in your LSP
- Support explicit global lookups (`::A::B`) and lexical scopes later
- Be cautious about reopening namespaces from different files

---

Happy Hacking 🔧💫

