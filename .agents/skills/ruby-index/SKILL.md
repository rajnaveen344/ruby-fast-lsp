---
name: ruby-index
description: "Debug Ruby Index tree projections, namespace identity, include/prepend/extend edges, MRO, and navigation ordering."
---

# Ruby Index and namespace debugging

1. Reproduce with a small generic namespace graph. Distinguish a wrong fact or
   lookup result from an incorrect tree projection or editor display.
2. Inspect the engine-owned query/debug output and source facts. Read
   `crates/ruby-analysis/src/engine/README.md` for graph/query ownership and
   `docs/features/definition-navigation.md` for selection and ordering.
3. Check class/module/singleton kind identity, lexical constant resolution,
   include/prepend/extend direction, ancestor order, and unresolved dependency
   edges. Incomplete lookup evidence must not produce a missing-method claim.
4. Check `src/query/namespace_tree.rs` and `src/capabilities/namespace_tree.rs`
   for protocol projection, then `editors/vscode/vsix/ruby_index_tree.js` for
   editor display. External-type filtering is a projection policy; it must not
   delete reusable semantic facts.
5. For navigation, preserve token identity and the effective implementations
   selected by the engine. Do not impose lexical filepath order as a semantic
   preference or add shadowed ancestors to an exact known-receiver result.
6. Add a focused engine or integration regression at the broken boundary.
   Use FakeEditor for edit/root/provenance lifecycle issues and editor tests only
   when server output is correct but rendering is wrong.

No completed goal file or historical performance rating defines current tree
behavior. Follow the source and tests, and update the nearest guide when the
public projection contract changes.
