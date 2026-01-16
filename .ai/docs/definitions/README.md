# 🎯 Goto Definition Documentation

Comprehensive documentation for how the Ruby Fast LSP resolves definitions for all Ruby identifier types.

---

## 📁 Directory Structure

```
definitions/
├── shared_infrastructure.md         🔗 How features share resolution logic
│
├── methods/              ⭐ Method resolution (most complex)
│   ├── README.md                    - Overview and navigation
│   ├── goto_definition_guide.md     - Complete guide (all-in-one)
│   └── examples/                    - Runnable Ruby examples
│       ├── example_nested_modules.rb
│       ├── example_shared_module.rb
│       └── metaprogramming_examples.rb
│
├── local_variables/      🔷 Local variable resolution
│   └── README.md                    - Coming soon
│
├── constants/            💎 Constant resolution
│   └── README.md                    - Coming soon
│
├── instance_variables/   📌 Instance variable resolution
│   └── README.md                    - Coming soon
│
└── class_variables/      🔶 Class variable resolution
    └── README.md                    - Coming soon
```

---

## 🚀 Quick Start

### For Method Definitions (include/prepend/extend)
→ Start with [`methods/README.md`](./methods/README.md)

### For Other Identifier Types
→ See respective folders (coming soon)

---

## 🎯 What's Documented

### ✅ Methods (Complete)
- Full traversal examples for 6 scenarios
- Truth tables for all include/prepend/extend combinations
- Visual diagrams and flowcharts
- LSP implementation details
- Runnable examples

### 🚧 Local Variables (Planned)
- Scope resolution
- Shadowing rules
- Block scoping

### 🚧 Constants (Planned)
- Lexical scoping
- Constant lookup paths
- Module nesting resolution

### 🚧 Instance Variables (Planned)
- Class vs instance context
- Inheritance behavior

### 🚧 Class Variables (Planned)
- Shared state across hierarchy
- Inheritance and mixins

---

## 📚 Key Concepts

### Method Resolution is Complex Because:
1. **Multiple lookup paths**: Class hierarchy, includes, prepends, extends
2. **Priority rules**: Prepend > Class > Include > Superclass
3. **Reverse lookups**: Module searching for methods in including classes
4. **Nested chains**: Modules including modules including modules...

See [`methods/`](./methods/) for complete documentation.

### Other Identifiers are Simpler:
- Local variables: Scope-based lookup
- Constants: Lexical scope with nesting
- Instance/Class variables: Straightforward class hierarchy

---

## 🔍 Finding What You Need

| I want to... | Go to |
|-------------|-------|
| **How resolution is shared across features** | [`shared_infrastructure.md`](./shared_infrastructure.md) |
| **Everything about methods** | [`methods/goto_definition_guide.md`](./methods/goto_definition_guide.md) |
| Run examples | [`methods/examples/`](./methods/examples/) |

---

## 💡 Contributing

When adding new documentation:

1. **Methods** go in `methods/` (include/prepend/extend complexity)
2. **Local vars** go in `local_variables/` (scope resolution)
3. **Constants** go in `constants/` (lexical scope)
4. **Instance vars** go in `instance_variables/`
5. **Class vars** go in `class_variables/`

Keep it organized by identifier type, not by feature.

---

## 🎓 Learning Path

1. **Read the guide**: [`methods/goto_definition_guide.md`](./methods/goto_definition_guide.md) (complete reference)
2. **Run examples**: `ruby methods/examples/metaprogramming_examples.rb`
3. **Check source**: `src/query/method.rs` (actual implementation)

---

## ⭐ Key Insights

1. **Method resolution is the hardest part** because Ruby's metaprogramming (include/prepend/extend) creates complex lookup chains. That's why `methods/` has the most documentation.

2. **Resolution logic is shared across all features** - Goto definition, hover, completion, and references all use the same `InheritanceGraph::get_ancestor_chain()` method. See [`shared_infrastructure.md`](./shared_infrastructure.md) for details.

3. **Other identifier types follow simpler rules** - Local variables use scope-based lookup, constants use lexical scoping, and instance/class variables follow straightforward hierarchy rules.
