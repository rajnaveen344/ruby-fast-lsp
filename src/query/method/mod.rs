//! Method Query - Method definition resolution
//!
//! ## Flow
//!
//! ```text
//! find_method_definitions()
//!   ↓
//! 1. Resolve receiver → namespace FQN
//! 2. Determine FQNs to search (class: [self], module: [all includers])
//! 3. Search each FQN's ancestors
//! 4. Collect and return all definitions
//! ```

mod analysis;

use crate::utils::deduplicate_locations;
use ruby_analysis::core::FullyQualifiedName;
pub use ruby_analysis::core::MethodCalleeResolution;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::indexer::{
    resolve_receiver_to_namespace, resolve_receiver_type, MethodReceiver, ReceiverResolutionContext,
};
use ruby_analysis::inference::RubyType;
use tower_lsp::lsp_types::{Location, Position};

use super::EngineQuery;

// ============================================================================
// Public API
// ============================================================================

/// Information about a resolved method.
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub fqn: FullyQualifiedName,
    pub return_type: Option<RubyType>,
    pub is_class_method: bool,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMethodCallee {
    pub owner: FullyQualifiedName,
    pub method: RubyMethod,
    pub resolution: MethodCalleeResolution,
    pub definition_locations: Vec<Location>,
}

impl EngineQuery {
    /// Find definitions for a Ruby method call.
    ///
    /// Algorithm:
    /// 1. Resolve receiver → namespace FQN
    /// 2. Determine FQNs to search:
    ///    - Class: [class_fqn]
    ///    - Module instance: [all includer FQNs]
    /// 3. Search each FQN's ancestor chain
    /// 4. Collect all definitions
    pub fn find_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        self.find_method_definitions_with_private(
            receiver,
            method,
            namespace,
            namespace_kind,
            position,
            true,
            None,
        )
    }

    pub fn find_public_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        self.find_method_definitions_with_private(
            receiver,
            method,
            namespace,
            namespace_kind,
            position,
            false,
            None,
        )
    }

    pub fn find_protected_method_definitions(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        self.find_method_definitions_with_private(
            receiver,
            method,
            namespace,
            namespace_kind,
            position,
            false,
            Some(caller_namespace_fqn),
        )
    }

    fn find_method_definitions_with_private(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Option<Vec<Location>> {
        let locations = self
            .resolve_method_callees_with_private(
                receiver,
                method,
                namespace,
                namespace_kind,
                position,
                allow_private,
                protected_caller,
            )
            .into_iter()
            .filter(|callee| matches!(callee.resolution, MethodCalleeResolution::Exact))
            .flat_map(|callee| callee.definition_locations)
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(deduplicate_locations(locations))
        }
    }

    pub fn resolve_method_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Vec<ResolvedMethodCallee> {
        self.resolve_method_callees_with_private(
            receiver,
            method,
            namespace,
            namespace_kind,
            position,
            true,
            None,
        )
    }

    fn resolve_method_callees_with_private(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Vec<ResolvedMethodCallee> {
        if matches!(receiver, MethodReceiver::Super) {
            let namespace_fqn =
                FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind);
            if let Some(callee) =
                analysis::resolve_super_method_callee(self, &namespace_fqn, method)
            {
                return vec![callee];
            }
            return Vec::new();
        }

        let namespace_fqn =
            match self.resolve_receiver_to_namespace(receiver, namespace, namespace_kind, position)
            {
                Some(namespace_fqn) => namespace_fqn,
                None => return Vec::new(),
            };

        let callees = if allow_private {
            analysis::resolve_method_callees(self, &namespace_fqn, method)
        } else if let Some(caller) = protected_caller {
            analysis::resolve_protected_method_callees(self, &namespace_fqn, method, caller)
        } else {
            analysis::resolve_public_method_callees(self, &namespace_fqn, method)
        };
        if let Some(callees) = callees {
            return callees;
        }

        Vec::new()
    }
}

// ============================================================================
// Receiver Resolution (Receiver → Namespace FQN)
// ============================================================================

impl EngineQuery {
    /// Convert method receiver to namespace FQN.
    /// Used by both go-to-definition and find-references.
    pub(crate) fn resolve_receiver_to_namespace(
        &self,
        receiver: &MethodReceiver,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        let doc_guard = self.doc.as_ref().map(|doc| doc.read());
        let byte_offset = doc_guard
            .as_ref()
            .map(|doc| doc.position_to_analysis_offset(position))
            .unwrap_or(0);
        let engine_guard = self.analysis_engine().map(|engine| engine.read());
        let analysis_query = engine_guard
            .as_ref()
            .map(|engine| ruby_analysis::engine::AnalysisQuery::new(engine));

        let context = ReceiverResolutionContext {
            query: analysis_query.as_ref(),
            document: doc_guard.as_deref(),
            current_namespace,
            namespace_kind,
            byte_offset,
        };

        resolve_receiver_to_namespace(receiver, &context)
    }

    pub(crate) fn resolve_receiver_type(
        &self,
        receiver: &MethodReceiver,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> RubyType {
        let doc_guard = self.doc.as_ref().map(|doc| doc.read());
        let byte_offset = doc_guard
            .as_ref()
            .map(|doc| doc.position_to_analysis_offset(position))
            .unwrap_or(0);
        let engine_guard = self.analysis_engine().map(|engine| engine.read());
        let analysis_query = engine_guard
            .as_ref()
            .map(|engine| ruby_analysis::engine::AnalysisQuery::new(engine));
        let context = ReceiverResolutionContext {
            query: analysis_query.as_ref(),
            document: doc_guard.as_deref(),
            current_namespace,
            namespace_kind,
            byte_offset,
        };
        resolve_receiver_type(receiver, &context)
    }
}
