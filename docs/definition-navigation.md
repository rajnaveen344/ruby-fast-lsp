# Definition navigation and ordering

Navigation resolves the meaning of the selected token before ranking its valid
destinations. An earlier result expresses a proven lookup preference, never a
guess about runtime receiver probability or cross-file execution order.

| Selected token | Selection and ordering contract |
| --- | --- |
| Method call | Use existing receiver, MRO, visibility, and Unknown rules. Keep each possible receiver's effective implementation. Among returned owners that appear together in a participating lookup chain, preserve that chain's precedence: prepends, own methods, includes, then ancestors. |
| `super` | Resolve after the enclosing method using the existing super query; never reinsert the enclosing override. |
| `instance_method` | Inspect the explicitly named namespace, without borrowing an includer's override. |
| Constant, class, module, YARD type | Resolve lexical/absolute identity first. Prefer implementation sources over bundled stubs and signatures. Equivalent reopenings are ties; source registration order does not establish an original declaration. |
| Local variable | Preserve lexical/reaching-binding selection, independently of inferred value type. |
| Instance, class, global variable | Preserve the variable query's selected identity and declaration set, applying the common source precedence. Do not invent method ancestry for a variable range. |
| `require`, `require_relative` | Preserve the owning project's ordered load-path resolver and its selected target. |
| Generated or external target | Honor verified explicit target/range selection and the same source precedence and lookup rules. |

Method ranking is a partial order. Unrelated receiver branches are tied.
Contradictory lookup orders across possible receivers form a tied component;
they must not produce a non-transitive sorting comparator or a fabricated
winner. Collapse those components and use a deterministic topological order.
An incomplete dependency chain contributes no ordering claim; retain its
navigation candidates under the existing resolution policy.
Implementation quality, source path, and start/end position break remaining
ties. Ranking never adds shadowed ancestors, removes a distinct receiver's
winner, or changes reference identity, diagnostics, or inferred types.

The analysis engine owns ranking. Definition queries capture the lookup chains
used by the ordinary resolver; other queries do not retain that navigation-only
data. The LSP adapter converts the already ranked ranges to locations without
resorting. VS Code preserves the provider's first destination for preferred
navigation, but its native results tree groups and sorts by file and position.
The server's full semantic order cannot override that tree layout. This change
does not introduce a custom results picker.

Validation covers reverse indexing order, inheritance, prepend/include order,
multiple receivers, contradictory branches, edits, known receivers, `super`,
reflection, implementation/signature precedence, lexical constants, locals,
and require selection. Generated simulations check the full target set and
independent precedence constraints, not unconditional filename order.
