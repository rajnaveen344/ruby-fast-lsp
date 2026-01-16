# 📌 Instance Variable Definition Resolution

Documentation for how the LSP resolves instance variable definitions.

---

## 🚧 Status: Planned

This section will document:

- **Class vs instance context**: Different behavior in class and instance methods
- **Inheritance**: Instance variables are not inherited (just shared state)
- **Dynamic creation**: Instance variables are created on first assignment
- **Scope**: Instance variables belong to specific object instances
- **Class instance variables**: The `@var` in class methods (different from `@@var`)

---

## 📝 Key Concepts

Instance variables in Ruby:
- Prefixed with `@`
- Not declared, created on first use
- Not inherited (each class has its own)
- Different from class variables (`@@`)

---

## 🔍 Implementation

See `src/query/definition.rs:find_instance_variable_definitions` for the current implementation.

---

## 📚 Coming Soon

Detailed documentation with:
- Traversal examples
- Edge cases (class instance variables)
- Implementation details
- Visual diagrams
