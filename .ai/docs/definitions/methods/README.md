# 🎯 Method Goto Definition

How the LSP resolves method definitions, including Ruby's `include`, `prepend`, and `extend`.

---

## 📖 Documentation

**→ Read [goto_definition_guide.md](./goto_definition_guide.md)**

This single comprehensive guide covers:
- ✅ Quick reference (include/prepend/extend rules)
- ✅ How the LSP implements resolution
- ✅ Step-by-step traversal examples
- ✅ Truth tables and priority rules
- ✅ Implementation details
- ✅ Common patterns and gotchas

---

## 🧪 Examples

**→ See [examples/](./examples/)**

Runnable Ruby code demonstrating:
- Nested module hierarchies
- Shared modules with multiple includers
- All metaprogramming scenarios

```bash
ruby examples/metaprogramming_examples.rb
```

---

## 🚀 Quick Start

1. Read [goto_definition_guide.md](./goto_definition_guide.md) for complete understanding
2. Run [examples/metaprogramming_examples.rb](./examples/metaprogramming_examples.rb) to see it in action
3. Check actual implementation in `src/query/method.rs`

---

## 📋 Quick Reference

```
include M  → 📦 Instance methods AFTER class
prepend M  → ⚡ Instance methods BEFORE class
extend M   → 🔧 Class methods (singleton)

Priority: ⚡ prepend > 🎯 class > 📦 include > 🔗 superclass
```

---

## 🔍 Key Insight

**The LSP uses TWO different strategies:**
- **Class context**: Search UP the inheritance chain (first match)
- **Module context**: Search DOWN to including classes (all matches)

See the guide for full details.
