//! Control-flow reachability analysis over Prism AST.
//!
//! Pure structural recursion — no index, no LSP types. Conservative:
//! unknown nodes return `Falls` (under-report, never over-report).
//!
//! Consumers:
//! - `capabilities/diagnostics.rs` (`unreachable-code` warning)
//! - `inferrer/return_type.rs` (skip diverging branches in union; future)
//! - guard narrowing (future)
//! - definite-return diagnostic (future)

use ruby_prism::{IfNode, Node, StatementsNode, UnlessNode, Visit};

/// Reachability outcome for a node or statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    /// Execution may continue past this node.
    Falls,
    /// Every path through this node exits via one of the kinds in the set.
    Diverges(ExitSet),
}

impl Reachability {
    pub fn is_diverges(self) -> bool {
        matches!(self, Reachability::Diverges(_))
    }
}

/// Kind of non-local exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Return = 1 << 0,
    Raise = 1 << 1,
    Break = 1 << 2,
    Next = 1 << 3,
    Redo = 1 << 4,
    Retry = 1 << 5,
}

/// Bitset of `Exit` kinds. `if a then return else raise end` → {Return, Raise}.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExitSet(u8);

impl ExitSet {
    pub const fn empty() -> Self {
        ExitSet(0)
    }

    pub const fn one(e: Exit) -> Self {
        ExitSet(e as u8)
    }

    pub fn insert(&mut self, e: Exit) {
        self.0 |= e as u8;
    }

    pub fn union(self, other: Self) -> Self {
        ExitSet(self.0 | other.0)
    }

    pub fn contains(self, e: Exit) -> bool {
        (self.0 & e as u8) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Compute reachability for a single node.
pub fn analyze(node: &Node<'_>) -> Reachability {
    use Reachability::*;

    if node.as_return_node().is_some() {
        return Diverges(ExitSet::one(Exit::Return));
    }
    if node.as_break_node().is_some() {
        return Diverges(ExitSet::one(Exit::Break));
    }
    if node.as_next_node().is_some() {
        return Diverges(ExitSet::one(Exit::Next));
    }
    if node.as_redo_node().is_some() {
        return Diverges(ExitSet::one(Exit::Redo));
    }
    if node.as_retry_node().is_some() {
        return Diverges(ExitSet::one(Exit::Retry));
    }

    if let Some(call) = node.as_call_node() {
        if is_terminating_call(&call) {
            return Diverges(ExitSet::one(Exit::Raise));
        }
        if is_diverging_loop(&call) {
            // Treated as Raise — the call never returns to its caller.
            // We don't introduce an `Exit::InfiniteLoop` kind; the only consumers
            // that distinguish exits care about Return vs Raise vs loop-control,
            // and "method never returns" maps cleanly onto Raise semantics.
            return Diverges(ExitSet::one(Exit::Raise));
        }
        return Falls;
    }

    if let Some(if_n) = node.as_if_node() {
        return analyze_if(&if_n);
    }
    if let Some(unless_n) = node.as_unless_node() {
        return analyze_unless(&unless_n);
    }
    if let Some(case_n) = node.as_case_node() {
        return analyze_case(&case_n);
    }
    if let Some(case_match_n) = node.as_case_match_node() {
        return analyze_case_match(&case_match_n);
    }
    if let Some(begin_n) = node.as_begin_node() {
        return analyze_begin(&begin_n);
    }
    if let Some(stmts) = node.as_statements_node() {
        return analyze_statements(&stmts);
    }
    if let Some(else_n) = node.as_else_node() {
        return else_n
            .statements()
            .map_or(Falls, |s| analyze_statements(&s));
    }

    Falls
}

/// Reachability of a statement list = reachability of its last statement.
/// Earlier statements that diverge make later ones unreachable; that is the
/// `unreachable-code` consumer's concern, not this function's.
pub fn analyze_statements(stmts: &StatementsNode<'_>) -> Reachability {
    let body: Vec<_> = stmts.body().iter().collect();
    body.last().map_or(Reachability::Falls, analyze)
}

/// Convenience: does any path exit?
pub fn diverges(node: &Node<'_>) -> bool {
    analyze(node).is_diverges()
}

/// Convenience: does every path exit via Return or Raise?
/// (Used for guard narrowing — Break/Next don't narrow method-scope types.)
pub fn exits_method(node: &Node<'_>) -> bool {
    match analyze(node) {
        Reachability::Diverges(es) => {
            !es.is_empty()
                && !es.contains(Exit::Break)
                && !es.contains(Exit::Next)
                && !es.contains(Exit::Redo)
                && !es.contains(Exit::Retry)
        }
        Reachability::Falls => false,
    }
}

// --- IfNode / UnlessNode --------------------------------------------------

fn analyze_if(if_n: &IfNode<'_>) -> Reachability {
    let then_r = if_n
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));
    // `subsequent` is None (no else), an ElseNode (terminal else), or another
    // IfNode (elsif chain). Both forms recurse through `analyze`.
    let Some(else_node) = if_n.subsequent() else {
        return Reachability::Falls;
    };
    let else_r = analyze(&else_node);
    join_branches(then_r, else_r)
}

fn analyze_unless(unless_n: &UnlessNode<'_>) -> Reachability {
    let then_r = unless_n
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));
    let Some(else_clause) = unless_n.else_clause() else {
        return Reachability::Falls;
    };
    let else_r = else_clause
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));
    join_branches(then_r, else_r)
}

fn join_branches(a: Reachability, b: Reachability) -> Reachability {
    match (a, b) {
        (Reachability::Diverges(x), Reachability::Diverges(y)) => {
            Reachability::Diverges(x.union(y))
        }
        _ => Reachability::Falls,
    }
}

// --- CaseNode (when) -----------------------------------------------------

fn analyze_case(case_n: &ruby_prism::CaseNode<'_>) -> Reachability {
    let mut combined = ExitSet::empty();
    let mut all_diverge = true;

    for cond in case_n.conditions().iter() {
        let when_r = if let Some(when_n) = cond.as_when_node() {
            when_n
                .statements()
                .map_or(Reachability::Falls, |s| analyze_statements(&s))
        } else {
            Reachability::Falls
        };
        match when_r {
            Reachability::Diverges(es) => combined = combined.union(es),
            Reachability::Falls => all_diverge = false,
        }
    }

    let Some(else_clause) = case_n.else_clause() else {
        return Reachability::Falls;
    };
    let else_r = else_clause
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));

    match else_r {
        Reachability::Diverges(es) if all_diverge => Reachability::Diverges(combined.union(es)),
        _ => Reachability::Falls,
    }
}

// --- CaseMatchNode (in) --------------------------------------------------

fn analyze_case_match(case_n: &ruby_prism::CaseMatchNode<'_>) -> Reachability {
    let mut combined = ExitSet::empty();
    let mut all_diverge = true;

    for cond in case_n.conditions().iter() {
        let in_r = if let Some(in_n) = cond.as_in_node() {
            in_n.statements()
                .map_or(Reachability::Falls, |s| analyze_statements(&s))
        } else {
            Reachability::Falls
        };
        match in_r {
            Reachability::Diverges(es) => combined = combined.union(es),
            Reachability::Falls => all_diverge = false,
        }
    }

    let Some(else_clause) = case_n.else_clause() else {
        // `case/in` without `else` raises NoMatchingPatternError — divergence
        // by exception is real, but conservatively treat as Falls (we'd need
        // to prove non-exhaustiveness to be useful).
        return Reachability::Falls;
    };
    let else_r = else_clause
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));

    match else_r {
        Reachability::Diverges(es) if all_diverge => Reachability::Diverges(combined.union(es)),
        _ => Reachability::Falls,
    }
}

// --- BeginNode (rescue / else / ensure) ----------------------------------

fn analyze_begin(begin_n: &ruby_prism::BeginNode<'_>) -> Reachability {
    // Ensure runs always — if it diverges, the whole begin diverges.
    if let Some(ensure) = begin_n.ensure_clause() {
        let ensure_r = ensure
            .statements()
            .map_or(Reachability::Falls, |s| analyze_statements(&s));
        if let Reachability::Diverges(es) = ensure_r {
            return Reachability::Diverges(es);
        }
    }

    // Body's outcome — if there's a begin-else, body must fall through to reach it.
    let body_r = begin_n
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));

    // The "happy path" (no exception): body executes; if there's a begin-else,
    // it runs after body falls through.
    let happy_r = match (body_r, begin_n.else_clause()) {
        (Reachability::Diverges(es), _) => Reachability::Diverges(es),
        (Reachability::Falls, None) => Reachability::Falls,
        (Reachability::Falls, Some(else_clause)) => else_clause
            .statements()
            .map_or(Reachability::Falls, |s| analyze_statements(&s)),
    };

    // Rescue path: if body raises, the first matching rescue handles it.
    // We don't track exception types — conservatively, every rescue is a possible
    // landing site. The whole begin diverges only if every rescue diverges AND
    // the happy path diverges.
    let Some(first_rescue) = begin_n.rescue_clause() else {
        // No rescue → a raise in body propagates. Reachability is body's
        // (or happy_r's) directly.
        return happy_r;
    };

    let mut rescues_r = analyze_rescue_chain(&first_rescue);
    // `rescues_r` represents: assuming body raises, what's the outcome?
    // The whole begin diverges iff happy_r diverges AND rescues_r diverges.
    match (happy_r, rescues_r) {
        (Reachability::Diverges(a), Reachability::Diverges(b)) => {
            Reachability::Diverges(a.union(b))
        }
        _ => {
            // Either path may fall through.
            // Special case: if body never raises (we can't prove this), happy_r alone
            // would suffice. Conservative: Falls.
            let _ = &mut rescues_r;
            Reachability::Falls
        }
    }
}

fn analyze_rescue_chain(rescue_n: &ruby_prism::RescueNode<'_>) -> Reachability {
    let this_r = rescue_n
        .statements()
        .map_or(Reachability::Falls, |s| analyze_statements(&s));
    let Some(next) = rescue_n.subsequent() else {
        return this_r;
    };
    join_branches(this_r, analyze_rescue_chain(&next))
}

// --- Terminating calls ---------------------------------------------------

/// True for `loop do ... end` / `loop { ... }` whose block body contains no
/// `break` that would exit *this* loop. Nested blocks/lambdas/while/until/for
/// shield their own breaks.
fn is_diverging_loop(call: &ruby_prism::CallNode<'_>) -> bool {
    if call.receiver().is_some() {
        return false;
    }
    let name = call.name();
    if String::from_utf8_lossy(name.as_slice()) != "loop" {
        return false;
    }
    let Some(block_node) = call.block() else {
        return false;
    };
    let Some(block) = block_node.as_block_node() else {
        return false;
    };
    let Some(body) = block.body() else {
        // empty `loop {}` — no break, infinite loop.
        return true;
    };
    !body_contains_breaking_break(&body)
}

/// True iff `body` contains a `break` not shielded by a nested block, lambda,
/// or inner loop construct. Used by `is_diverging_loop`.
fn body_contains_breaking_break(body: &Node<'_>) -> bool {
    let mut finder = BreakFinder { found: false };
    finder.visit(body);
    finder.found
}

struct BreakFinder {
    found: bool,
}

impl<'pr> ruby_prism::Visit<'pr> for BreakFinder {
    fn visit_break_node(&mut self, _node: &ruby_prism::BreakNode<'pr>) {
        self.found = true;
    }

    // Each of these introduces a new break-scope: a `break` inside them exits
    // *that* construct, not the outer `loop` we're analyzing. Override the visit
    // methods to NOT recurse, shielding nested breaks.

    fn visit_block_node(&mut self, _node: &ruby_prism::BlockNode<'pr>) {}
    fn visit_lambda_node(&mut self, _node: &ruby_prism::LambdaNode<'pr>) {}
    fn visit_while_node(&mut self, _node: &ruby_prism::WhileNode<'pr>) {}
    fn visit_until_node(&mut self, _node: &ruby_prism::UntilNode<'pr>) {}
    fn visit_for_node(&mut self, _node: &ruby_prism::ForNode<'pr>) {}
}

/// Calls that never return: `raise`, `fail`, `throw`, `exit`, `exit!`, `abort`,
/// and the `Process.exit*` / `Process.abort` family.
fn is_terminating_call(call: &ruby_prism::CallNode<'_>) -> bool {
    let name = call.name();
    let name_str = String::from_utf8_lossy(name.as_slice());

    let kernel_terminators = ["raise", "fail", "throw", "exit", "exit!", "abort"];

    match call.receiver() {
        None => kernel_terminators.contains(&name_str.as_ref()),
        Some(recv) => {
            // `Process.exit`, `Process.exit!`, `Process.abort`, `Kernel.raise`, etc.
            if let Some(c) = recv.as_constant_read_node() {
                let recv_name = String::from_utf8_lossy(c.name().as_slice()).to_string();
                if (recv_name == "Process" || recv_name == "Kernel")
                    && kernel_terminators.contains(&name_str.as_ref())
                {
                    return true;
                }
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_src(src: &str) -> Reachability {
        let result = ruby_prism::parse(src.as_bytes());
        let root = result.node();
        let stmts = root.as_program_node().unwrap().statements();
        analyze_statements(&stmts)
    }

    #[test]
    fn explicit_return_diverges() {
        let r = analyze_src("return 1");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn raise_diverges() {
        let r = analyze_src("raise \"x\"");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Raise)));
    }

    #[test]
    fn process_exit_diverges() {
        let r = analyze_src("Process.exit");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Raise)));
    }

    #[test]
    fn unrelated_call_falls() {
        let r = analyze_src("foo");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn literal_falls() {
        let r = analyze_src("1");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn if_both_branches_return_diverges() {
        let r = analyze_src("if x\n  return 1\nelse\n  return 2\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn if_then_returns_else_raises_diverges_with_both_kinds() {
        let r = analyze_src("if x\n  return 1\nelse\n  raise\nend");
        let Reachability::Diverges(es) = r else {
            panic!("expected diverges")
        };
        assert!(es.contains(Exit::Return));
        assert!(es.contains(Exit::Raise));
    }

    #[test]
    fn if_no_else_falls() {
        let r = analyze_src("if x\n  return 1\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn if_one_branch_falls_means_falls() {
        let r = analyze_src("if x\n  return 1\nelse\n  2\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn unless_both_branches_return_diverges() {
        let r = analyze_src("unless x\n  return 1\nelse\n  return 2\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn modifier_if_return_falls() {
        // `return if cond` is conditional → Falls
        let r = analyze_src("return if x");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn elsif_chain_all_return_diverges() {
        let r = analyze_src("if a\n  return 1\nelsif b\n  return 2\nelse\n  return 3\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn elsif_chain_missing_else_falls() {
        let r = analyze_src("if a\n  return 1\nelsif b\n  return 2\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn case_when_all_terminate_with_else_diverges() {
        let r = analyze_src("case x\nwhen 1 then return 1\nwhen 2 then raise\nelse return 3\nend");
        let Reachability::Diverges(es) = r else {
            panic!("expected diverges")
        };
        assert!(es.contains(Exit::Return));
        assert!(es.contains(Exit::Raise));
    }

    #[test]
    fn case_when_missing_else_falls() {
        let r = analyze_src("case x\nwhen 1 then return 1\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn case_when_one_branch_falls_means_falls() {
        let r = analyze_src("case x\nwhen 1 then return 1\nwhen 2 then 2\nelse return 3\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn case_in_all_terminate_with_else_diverges() {
        let r = analyze_src("case x\nin 1 then return 1\nin 2 then raise\nelse return 3\nend");
        assert!(r.is_diverges());
    }

    #[test]
    fn begin_body_and_rescue_both_return_diverges() {
        let r = analyze_src("begin\n  return 1\nrescue => e\n  return 2\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn begin_body_returns_no_rescue_diverges() {
        let r = analyze_src("begin\n  return 1\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn begin_ensure_diverges_makes_whole_diverge() {
        let r = analyze_src("begin\n  1\nensure\n  return 99\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn begin_with_else_clause_runs_else_when_body_falls() {
        // body falls → else runs → else returns → whole diverges
        let r = analyze_src("begin\n  1\nrescue => e\n  return 2\nelse\n  return 3\nend");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn begin_rescue_falls_means_falls() {
        let r = analyze_src("begin\n  return 1\nrescue => e\n  2\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn break_diverges() {
        let r = analyze_src("break");
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Break)));
    }

    #[test]
    fn nested_if_in_else_propagates() {
        let r = analyze_src(
            "if a\n  return 1\nelse\n  if b\n    return 2\n  else\n    return 3\n  end\nend",
        );
        assert_eq!(r, Reachability::Diverges(ExitSet::one(Exit::Return)));
    }

    #[test]
    fn exits_method_excludes_break() {
        let result = ruby_prism::parse(b"break");
        let stmts = result.node().as_program_node().unwrap().statements();
        let break_node = stmts.body().iter().next().unwrap();
        assert!(!exits_method(&break_node));
    }

    #[test]
    fn exits_method_includes_return_and_raise() {
        let result = ruby_prism::parse(b"return 1");
        let stmts = result.node().as_program_node().unwrap().statements();
        let ret_node = stmts.body().iter().next().unwrap();
        assert!(exits_method(&ret_node));

        let result = ruby_prism::parse(b"raise");
        let stmts = result.node().as_program_node().unwrap().statements();
        let raise_node = stmts.body().iter().next().unwrap();
        assert!(exits_method(&raise_node));
    }

    #[test]
    fn loop_no_break_diverges() {
        let r = analyze_src("loop { puts 1 }");
        assert!(r.is_diverges());
    }

    #[test]
    fn loop_with_break_falls() {
        let r = analyze_src("loop { break 5 }");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn loop_with_conditional_break_falls() {
        let r = analyze_src("loop { break if cond }");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn loop_with_nested_each_break_diverges() {
        // The inner block's `break` exits the each, not the outer loop.
        let r = analyze_src("loop { [1].each { break } }");
        assert!(r.is_diverges());
    }

    #[test]
    fn loop_with_nested_lambda_break_diverges() {
        // Break inside lambda doesn't exit the outer loop.
        let r = analyze_src("loop { lambda { break }.call }");
        assert!(r.is_diverges());
    }

    #[test]
    fn loop_with_nested_while_break_diverges() {
        let r = analyze_src("loop { while x; break; end }");
        assert!(r.is_diverges());
    }

    #[test]
    fn loop_empty_body_diverges() {
        let r = analyze_src("loop {}");
        assert!(r.is_diverges());
    }

    #[test]
    fn loop_with_method_receiver_falls() {
        // `obj.loop { ... }` is not Kernel#loop — don't treat as diverging.
        let r = analyze_src("obj.loop { puts 1 }");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn for_loop_falls() {
        // `for` may not iterate (empty collection) — body's terminator doesn't
        // make the for itself diverge.
        let r = analyze_src("for x in []\n  return\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn while_loop_falls() {
        let r = analyze_src("while x\n  return\nend");
        assert_eq!(r, Reachability::Falls);
    }

    #[test]
    fn exit_set_union() {
        let a = ExitSet::one(Exit::Return);
        let b = ExitSet::one(Exit::Raise);
        let u = a.union(b);
        assert!(u.contains(Exit::Return));
        assert!(u.contains(Exit::Raise));
        assert!(!u.contains(Exit::Break));
    }
}
