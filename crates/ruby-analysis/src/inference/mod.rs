//! Editor-agnostic, proof-first Ruby type inference.
//!
//! # Purpose
//!
//! This module owns reusable rules for deriving Ruby types. It is shared by
//! workspace indexing, semantic queries, the language server, and the
//! standalone checker. An editor feature may format or filter an inference
//! result, but it must not implement a competing type rule.
//!
//! The design optimizes for trustworthy answers rather than maximum hint
//! volume: publish a concrete type only when the available static evidence is
//! complete. Otherwise retain [`crate::core::RubyType::Unknown`] together with
//! a stable [`crate::core::UnknownReason`] where the evidence surface supports
//! one. A false Unknown is a coverage gap that can be improved later; a false
//! concrete type is a correctness defect.
//!
//! # Ownership boundaries
//!
//! The inference pipeline crosses four deliberately separate layers:
//!
//! 1. [`crate::indexer`] performs the offset-preserving Prism traversal,
//!    maintains lexical/execution scopes, and emits file-owned facts, call
//!    candidates, local-flow evidence, and compact value-constant and
//!    method-return equations.
//! 2. This module derives expression, flow, method-return, block/proc, and RBS
//!    types. It may ask semantic questions through domain query APIs, but it
//!    does not own a workspace, parse files, or use editor protocol types.
//! 3. [`crate::engine`] owns the complete project graph, the sole Ruby
//!    method/MRO/visibility/ambiguity policy, file replacement, cross-file
//!    resolution, and stored solved outcomes.
//! 4. Root LSP and CLI adapters project those same engine-owned domain results
//!    into hover, inlay, completion, navigation, diagnostics, or terminal/JSON
//!    output.
//!
//! This direction is intentional. Moving AST traversal into the engine would
//! couple persistent semantic state to Prism. Moving lookup into inference or
//! an adapter would allow hover, navigation, diagnostics, and checking to
//! disagree about the callable method. Moving type rules into an editor would
//! make headless checking a second implementation.
//!
//! # Proof model
//!
//! [`crate::core::TypeInferenceOutcome`] is the proof-carrying boundary. Its
//! private representation prevents `Proven(Unknown)`. The following rules are
//! invariants rather than presentation preferences:
//!
//! - A union is concrete only when every reachable member is proven. Unknown
//!   absorbs an incomplete union; known members must not be published as a
//!   plausible partial answer.
//! - A proven outer collection may retain an explicit unknown argument, such
//!   as `Array[Unknown]`. That proves the container shape, not its contents,
//!   and cannot prove an argument or assignment mismatch requiring a concrete
//!   element type.
//! - Method existence and method return proof are separate. A call may resolve
//!   to exact navigation/reference targets while its return remains Unknown.
//! - Missing-method and type diagnostics require the complete relevant lookup
//!   chain. Missing ancestors, mixins, signatures, dependencies, visibility
//!   information, or overload evidence suppress claims that depend on them.
//! - Explicit RBS, YARD, runtime, and validated extension types enter as
//!   provenance-bearing facts. They use the ordinary engine lifecycle and do
//!   not create a second signature precedence or semantic store here.
//! - Dynamic Ruby boundaries such as arbitrary string evaluation, reflective
//!   dispatch, data-dependent `method_missing`, and unsupported
//!   metaprogramming remain Unknown. Naming conventions, `Object`, observed
//!   call sites, confidence scores, and arbitrary overload selection are not
//!   substitutes for proof.
//!
//! # Hash-backed structural shapes
//!
//! [`crate::core::RubyType::Shape`] is the canonical structural representation
//! for a proven Ruby Hash value. Its fields retain Symbol/String literal keys,
//! required or optional presence, exact/open state, an optional generic rest
//! contract, and shallow frozen/tracked-mutable state. Literal discriminants
//! use [`crate::core::RubyType::Literal`]. Shape unions retain complete
//! variants so a `kind` field remains correlated with the other fields on the
//! same control-flow path; field-wise flattening is not a semantic operation.
//!
//! Shape construction rejects partial Unknown evidence and enforces fixed
//! bounds: 32 fields, eight nested shape levels, eight shape variants per
//! union, eight live aliases for one mutable identity, and 16 solver
//! iterations. Exceeding one of those limits produces
//! [`crate::core::UnknownReason::ShapeBoundExceeded`]. It never drops fields,
//! widens to `Object`, or retains a convenient known prefix.
//!
//! [`type_tracker`] owns mutable Hash identities only inside one bounded flow
//! pass. Known mutation updates every tracked alias and unsupported mutation,
//! escape, or ambiguous containment invalidates every affected alias. Frozen
//! proves only the outer Hash key set; nested mutable values keep independent
//! identities. No identity enters engine storage: file facts contain only
//! canonical `RubyType` outcomes and stable Unknown reasons.
//!
//! Literal keyed reads, `fetch`, `dig`, presence predicates, Hash patterns,
//! discriminated narrowing, key/value iteration, generic Hash projection, and
//! supported RBS record conversion all use this shared algebra. Cross-file
//! method returns and value constants propagate it through the ordinary
//! equation and file-replacement lifecycle. Hover, inlay hints, completion,
//! chained dispatch, diagnostics, and the standalone checker format or filter
//! the same engine-owned result; none may reconstruct a shape independently.
//!
//! # Flow, calls, and recursion
//!
//! [`type_tracker`] owns forward lexical flow, joins, narrowing, block-local
//! state, value-constant dependency terms, and method-body return equations. Source order and hard lexical
//! boundaries are semantic inputs: inference must not scan source text or
//! borrow a later same-named assignment when scope evidence is absent.
//!
//! [`method`] composes receiver types with engine-owned dispatch and RBS
//! signatures. Reopened definitions and union receivers are exhaustive: all
//! selected definitions or receiver members must participate. Recursive and
//! mutually recursive returns use compact equations and a bounded,
//! deterministic SCC fixed-point solve. Bottom is private solver state; a
//! base-free, incomplete, or non-converging component publishes an explained
//! Unknown rather than widening a result.
//!
//! [`rbs`] performs supported RBS conversion and generic substitution.
//! [`completion`] exposes reusable receiver/type probing; editor trigger
//! routing and snippet construction remain outside this crate.
//!
//! # Determinism and lifecycle
//!
//! Identical source, signatures, configuration, and dependency inputs must
//! produce identical canonical types, diagnostics, and semantic fingerprints
//! regardless of file discovery order, hash iteration, worker scheduling,
//! cache state, or LSP versus CLI execution. Preserve this by:
//!
//! - canonicalizing composite types structurally;
//! - sorting or using ordered collections at persisted/query boundaries;
//! - bounding loop and recursive solving rather than depending on traversal
//!   luck;
//! - storing evidence with its owning file and removing it through the same
//!   `register_file -> replace_facts -> resolve` lifecycle as other facts; and
//! - reusing compact bindings/equations instead of reparsing or walking Prism
//!   once per consumer.
//!
//! A body-only edit must not synchronously trigger a closed-workspace check.
//! New retained state, constraint work, union growth, or explanation graphs
//! require explicit bounds and release-profile evidence. Accuracy work may
//! reduce false Unknowns, but it does not get to regress the established
//! latency, CPU, or RSS gates.
//!
//! # Higher-order calls
//!
//! [`higher_order`] is the single callable-constraint model for block-bearing
//! core and project RBS methods, bounded direct Ruby `yield`, proven block
//! forwarding, statically known proc/lambda bodies, and static `&:method`.
//! It separates receiver type parameters from method-local type parameters,
//! instantiates block inputs, constrains every reachable block result, and
//! substitutes one canonical call result. Type variables and receiver
//! templates never escape into stored runtime types.
//!
//! Static `&:method` is equivalent to an explicit one-parameter block only
//! after lookup succeeds for every reachable input member. Dynamic callables,
//! conflicting overloads, incomplete substitutions, unsupported non-local
//! exits, or bound excess retain a stable explained Unknown. A higher-order
//! outcome atomically owns the call result while the ordinary method candidate
//! remains available for navigation.
//! Fixed limits are eight compatible overloads, eight type variables, four
//! block parameters, 16 binding iterations, eight template levels, and eight
//! block-result union variants. The solver never truncates a candidate set or
//! union and never widens an incomplete result to `Object`.
//!
//! [`callable_body`] evaluates the one AST-free summary emitted during the
//! indexer's ordinary Prism traversal. Direct `.call` and `&callable` bind
//! their proven inputs through that same evaluator. Local identities and
//! aliases remain bounded flow state; only capture-free constant summaries
//! become file-owned engine facts and persistent dependency products. Capture
//! reads resolve at invocation, method calls delegate to engine lookup, and
//! shapes reuse the canonical shape algebra. Unsupported escape invalidates
//! every alias; ambiguity, recursion, stale evidence, incomplete inputs, and
//! bound excess return stable whole-result Unknown reasons.
//!
//! Callable-body limits are four parameters, 64 summary nodes, eight captures,
//! eight aliases, eight nested instantiations, 16 call-constraint steps, eight
//! result-union variants, and eight structural/type levels. Neither the
//! summary nor engine facts retain Prism nodes or source snippets.
//!
//! # Acceptance contract
//!
//! `support/type_inference/scorecard.toml` is the machine-readable conformance
//! contract. The 9/10 threshold is at least 90/100 overall and at least 85%
//! in every critical category, with no known wrong concrete type in supported
//! cases. Conservative safety cases and the separately reviewed real-project
//! precision corpus protect the no-false-positive rule without inflating the
//! accuracy score. CLI/LSP parity, deterministic lifecycle behavior, package
//! smoke tests, and the performance/RSS contract in `AGENTS.md` are independent
//! completion gates; a high score alone is not sufficient.
//!
//! Keep measured results in the scorecard and `support/performance/` artifacts,
//! not in this Rustdoc. This document records why the system is shaped this
//! way; machine-readable evidence records whether the current implementation
//! satisfies it.
//!
//! # Change protocol
//!
//! For a new inference rule:
//!
//! 1. Add the smallest failing domain or integration test first and confirm
//!    the intended failure.
//! 2. Add a counterexample showing partial or ambiguous evidence remains
//!    Unknown.
//! 3. Implement the rule in this module or emit the necessary binding/fact in
//!    the indexer; keep lookup and persistence in the engine.
//! 4. Verify every affected consumer reads the shared outcome and that edit
//!    replacement removes stale evidence.
//! 5. Add or update the reviewed scorecard case and measure any material hot-
//!    path or retained-memory change with the release profiler.

pub(crate) mod callable_body;
pub mod completion;
pub(crate) mod constant;
pub mod control_flow;
pub(crate) mod higher_order;
pub mod method;
pub mod rbs;
pub mod r#type;
pub mod type_query;
pub mod type_tracker;

pub use crate::core::RubyType;
pub use method::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
pub use r#type::*;
pub use rbs::{get_rbs_method_return_type, has_rbs_class, rbs_declaration_count, rbs_method_count};
pub use type_query::TypeQuery;

#[cfg(test)]
mod architecture_tests {
    use std::path::Path;

    #[test]
    fn inference_layer_does_not_depend_on_editor_protocol_types() {
        let inference_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inference");
        let mut pending = vec![inference_dir];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: inference source directory `{}` could not be read: {error}. This is a bug because the architecture boundary test must inspect every inference module. Fix: keep inference sources under crates/ruby-analysis/src/inference or update the boundary root deliberately.",
                    directory.display(),
                )
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: an inference source entry could not be read: {error}. This is a bug because skipping a source file could hide an editor-protocol dependency. Fix: repair the source tree before running architecture tests."
                    )
                });
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: inference source `{}` could not be decoded as UTF-8: {error}. This is a bug because Rust source must be UTF-8 and the boundary test cannot inspect unreadable code. Fix: restore valid Rust source text.",
                        path.display(),
                    )
                });
                let tower_protocol = ["tower", "_lsp"].concat();
                let protocol_types = ["lsp", "_types"].concat();
                assert!(
                    !source.contains(&tower_protocol) && !source.contains(&protocol_types),
                    "INVARIANT VIOLATED: inference source `{}` imports editor protocol types. This is a bug because ruby-analysis inference must be reusable by the standalone checker without an LSP data model. Fix: accept SourceFileId, TextRange, or byte offsets and convert protocol positions in the root adapter.",
                    path.display(),
                );
            }
        }
    }
}
