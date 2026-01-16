# 🔷 Local Variable Definition Resolution

Documentation for how the LSP resolves local variable definitions.

---

## 🚧 Status: Planned

This section will document:

- **Scope resolution**: How local variables are resolved within method, block, and class scopes
- **Shadowing rules**: How inner scopes shadow outer scopes
- **Block scoping**: Variable scope in blocks and closures
- **Assignment tracking**: How the LSP tracks where variables are first assigned
- **Scope boundaries**: Understanding where variable scope begins and ends

---

## 📝 Key Concepts

Local variables in Ruby follow lexical scoping rules:
- Variables are local to the scope they're defined in
- Inner scopes can access outer scope variables
- Assignment in inner scope creates new local variable (shadowing)
- Block parameters create new local scope

---

## 🔍 Implementation

See `src/query/definition.rs:find_local_variable_definitions_at_position` for the current implementation.

---

## 📚 Coming Soon

Detailed documentation with:
- Traversal examples
- Edge cases (shadowing, closures)
- Implementation details
- Visual diagrams
