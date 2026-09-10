use super::source::source_range;
use crate::core::{NamespaceKind, RubyConstant, TextRange};
use crate::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_extension_api::{ProjectContext, Receiver, ResolvedCall};
use ruby_prism::CallNode;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockExecutionContext {
    pub block_range: TextRange,
    pub implicit_receiver: Vec<RubyConstant>,
    pub implicit_receiver_kind: NamespaceKind,
    pub method_definition_owner: Vec<RubyConstant>,
    pub method_definition_kind: NamespaceKind,
}

pub trait FactCollectorExtensionHost: std::fmt::Debug + Send + Sync {
    fn process_call_node(&self, _visitor: &mut FactCollector, _node: &CallNode) -> bool {
        false
    }

    fn should_track_enclosing_call(&self, _visitor: &FactCollector, _node: &CallNode) -> bool {
        false
    }

    fn resolved_call_for_stack(&self, visitor: &FactCollector, node: &CallNode) -> ResolvedCall {
        let call_range = source_range(visitor, &node.location());
        let message_range = node
            .message_loc()
            .map(|loc| source_range(visitor, &loc))
            .unwrap_or(call_range);
        ResolvedCall {
            method_name: String::from_utf8_lossy(node.name().as_slice()).to_string(),
            receiver: Receiver::Expression,
            arguments: Vec::new(),
            resolved_callees: Vec::new(),
            call_range,
            message_range,
            frame_extension_ids: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct NullFactCollectorExtensionHost;

impl FactCollectorExtensionHost for NullFactCollectorExtensionHost {}

// One frame owns both decisions for a call, even when that call is untracked.
struct ExtensionCallFrame {
    handled: bool,
    tracked: bool,
}

pub(in crate::indexer::fact_collector) struct ExtensionState {
    pub(in crate::indexer::fact_collector) host: Arc<dyn FactCollectorExtensionHost>,
    pub(in crate::indexer::fact_collector) project: Option<ProjectContext>,
    enclosing_calls: Vec<ResolvedCall>,
    call_frames: Vec<ExtensionCallFrame>,
    pub(in crate::indexer::fact_collector) pending_block: Option<BlockExecutionContext>,
}

impl ExtensionState {
    pub(in crate::indexer::fact_collector) fn new(
        host: Arc<dyn FactCollectorExtensionHost>,
    ) -> Self {
        Self {
            host,
            project: None,
            enclosing_calls: Vec::new(),
            call_frames: Vec::new(),
            pending_block: None,
        }
    }

    pub(in crate::indexer::fact_collector) fn enter_call(
        &mut self,
        handled: bool,
        resolved_call: Option<ResolvedCall>,
    ) {
        self.call_frames.push(ExtensionCallFrame {
            handled,
            tracked: resolved_call.is_some(),
        });
        if let Some(call) = resolved_call {
            self.enclosing_calls.push(call);
        }
    }

    pub(in crate::indexer::fact_collector) fn current_call_handled(&self) -> bool {
        self.call_frames
            .last()
            .expect(
                "INVARIANT VIOLATED: an extension call frame is missing while collecting a nested receiver. \
                 This is a bug because every call entry must precede candidate collection. \
                 Fix: keep call entry, candidate collection, and exit balanced.",
            )
            .handled
    }

    pub(in crate::indexer::fact_collector) fn exit_call(&mut self) {
        let frame = self.call_frames.pop().expect(
            "INVARIANT VIOLATED: the extension call frame stack underflowed. \
             This is a bug because every call exit must match one entry. \
             Fix: keep FactCollector call callbacks balanced.",
        );
        if frame.tracked {
            self.enclosing_calls.pop().expect(
                "INVARIANT VIOLATED: a tracked extension call has no enclosing-call frame. \
                 This is a bug because tracked calls must retain their resolved call until exit. \
                 Fix: push and pop resolved calls with their owning call frame.",
            );
        }
    }
}

impl FactCollector {
    pub fn extension_project_context(&self) -> Option<&ProjectContext> {
        self.extensions.project.as_ref()
    }

    pub fn set_extension_project_context(&mut self, project: Option<ProjectContext>) {
        self.extensions.project = project;
    }

    pub fn enclosing_extension_calls(&self) -> &[ResolvedCall] {
        &self.extensions.enclosing_calls
    }

    pub fn set_pending_block_execution_context(&mut self, context: BlockExecutionContext) {
        assert!(
            self.extensions.pending_block.is_none(),
            "INVARIANT VIOLATED: more than one block execution context was applied to the same call. This is a bug because extension conflicts must be resolved before AST traversal. Fix: validate and deterministically resolve extension execution contexts in the host."
        );
        self.extensions.pending_block = Some(context);
    }
}
