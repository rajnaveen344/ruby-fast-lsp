//! Eval blocks in method bodies whose constant receiver the file declares
//! later. A method body runs after its file has loaded, so such a block
//! replays once the walk knows every namespace the file declares.

use ruby_prism::{CallNode, Node, Visit};

use super::syntax::constant_parts_and_absolute;
use super::AnalysisIndexer;
use crate::indexer::documents::scope_rules::eval_definition_kind;
use crate::indexer::documents::scope_tracker::ScopeTracker;

/// A skipped eval call, identified by its byte range, with the scope it
/// would have run in.
#[derive(Debug)]
pub(super) struct DeferredEvalBlock {
    start: usize,
    end: usize,
    scope: ScopeTracker,
}

impl AnalysisIndexer {
    /// Skip an eval call with a constant receiver in a method body, to
    /// replay it after the main walk.
    pub(super) fn defer_eval_block(&mut self, node: &CallNode<'_>) -> bool {
        let deferrable = !self.replaying_deferred
            && self.scope.current_method().is_some()
            && node.block().is_some()
            && eval_definition_kind(node).is_some()
            && node
                .receiver()
                .is_some_and(|receiver| constant_parts_and_absolute(&receiver).is_some());
        if deferrable {
            let location = node.location();
            self.deferred_eval_blocks.push(DeferredEvalBlock {
                start: location.start_offset(),
                end: location.end_offset(),
                scope: self.scope.clone(),
            });
        }
        deferrable
    }

    /// Replay every deferred eval call in `root` against the file's complete
    /// namespaces. A receiver that still resolves to nothing takes the same
    /// fallback it would have taken in place.
    pub(super) fn replay_deferred_eval_blocks(&mut self, root: &Node<'_>) {
        if self.deferred_eval_blocks.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.deferred_eval_blocks);
        self.replaying_deferred = true;
        let mut replay = Replay {
            indexer: self,
            pending,
        };
        replay.visit(root);
        let missed = replay.pending.len();
        self.replaying_deferred = false;
        invariant!(
            missed == 0,
            what = "a deferred eval call was not found when the walk replayed it",
            why = "deferral records the byte range of a call node in the same tree",
            fix = "replay deferred eval calls against the tree the walk visited",
        );
    }
}

struct Replay<'a> {
    indexer: &'a mut AnalysisIndexer,
    pending: Vec<DeferredEvalBlock>,
}

impl<'pr> Visit<'pr> for Replay<'_> {
    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        let location = node.location();
        let found = self.pending.iter().position(|deferred| {
            deferred.start == location.start_offset() && deferred.end == location.end_offset()
        });
        let Some(index) = found else {
            ruby_prism::visit_call_node(self, node);
            return;
        };
        let deferred = self.pending.swap_remove(index);
        let scope = std::mem::replace(&mut self.indexer.scope, deferred.scope);
        self.indexer.visit_call_node(node);
        self.indexer.scope = scope;
    }
}
