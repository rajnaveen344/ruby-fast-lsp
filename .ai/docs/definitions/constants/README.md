# 💎 Constant Definition Resolution

Documentation for how the LSP resolves constant definitions.

---

## 🚧 Status: Planned

This section will document:

- **Lexical scoping**: How constants are resolved through lexical nesting
- **Constant lookup paths**: The order Ruby searches for constants
- **Module nesting**: How `::` affects constant resolution
- **Absolute vs relative**: Difference between `::Constant` and `Constant`
- **Inheritance**: How constants are inherited from superclasses

---

## 📝 Key Concepts

Constants in Ruby have unique resolution rules:
- Lexical scope is checked first
- Then inheritance chain
- Then top-level constants
- `::` forces absolute lookup from top level

---

## 🔍 Implementation

See `src/query/definition.rs:find_constant_definitions_by_path` for the current implementation.

---

## 📚 Coming Soon

Detailed documentation with:
- Traversal examples
- Edge cases (nested modules, reopening classes)
- Implementation details
- Visual diagrams
