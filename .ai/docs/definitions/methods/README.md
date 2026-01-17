# 🎯 Method Goto Definition

How the LSP resolves method definitions using Ruby's method resolution order (MRO), including support for `include`, `prepend`, and `extend`.

---

## 📖 Documentation

**→ Read [goto_definition_guide.md](./goto_definition_guide.md)**

This comprehensive guide covers:
- ✅ Quick reference (include/prepend/extend rules)
- ✅ Current implementation architecture
- ✅ Namespace-based method resolution
- ✅ Singleton class handling
- ✅ Ancestor chain lookup
- ✅ Priority rules and common patterns

---

## 🚀 Quick Start

1. Read [goto_definition_guide.md](./goto_definition_guide.md) for complete understanding
2. Check implementation in `src/query/method.rs` and `src/indexer/index.rs`
3. Review integration tests in `src/test/integration/methods/`

---

## 📋 Quick Reference

```
include M  → 📦 Instance methods AFTER class
prepend M  → ⚡ Instance methods BEFORE class
extend M   → 🔧 Singleton methods (class methods)

Priority: ⚡ prepend > 🎯 class > 📦 include > 🔗 superclass
```

---

## 🔍 Key Architecture

**Namespace-Based Resolution:**
- Each class/module exists as **TWO** namespace FQNs:
  - `Namespace(Foo, Instance)` - for instance method lookup
  - `Namespace(Foo, Singleton)` - for class method lookup
- Methods are indexed under their owner namespace with kind
- Ancestor chains are computed per namespace kind

**Benefits:**
- Type-safe distinction between instance and singleton methods
- Efficient O(1) FQN lookup
- Matches Ruby's internal object model

---

## 🎯 Method Resolution

The LSP resolves methods by:

1. **Determine context namespace** - Instance or Singleton
2. **Build search space** - Get ancestor chain for that namespace
3. **Search hierarchy** - Walk ancestors in MRO order
4. **Return first match** - Ruby semantics (first definition wins)

See the guide for detailed implementation.
