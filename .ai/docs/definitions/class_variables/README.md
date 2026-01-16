# 🔶 Class Variable Definition Resolution

Documentation for how the LSP resolves class variable definitions.

---

## 🚧 Status: Planned

This section will document:

- **Shared across hierarchy**: Class variables are shared between class and subclasses
- **Module behavior**: How class variables work with mixins
- **Inheritance pitfalls**: Common issues with shared state
- **Resolution order**: How Ruby finds the right class variable
- **vs Instance variables**: Key differences

---

## 📝 Key Concepts

Class variables in Ruby:
- Prefixed with `@@`
- Shared across entire class hierarchy
- Can cause surprising behavior with inheritance
- Different from class instance variables (`@var` in class methods)

---

## 🔍 Implementation

See `src/query/definition.rs:find_class_variable_definitions` for the current implementation.

---

## 📚 Coming Soon

Detailed documentation with:
- Traversal examples
- Edge cases (inheritance issues)
- Implementation details
- Visual diagrams
