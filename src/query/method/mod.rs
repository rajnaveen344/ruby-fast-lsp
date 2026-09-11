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

use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::core::RubyType;
pub use ruby_analysis::core::{MethodCalleeResolution, ResolvedMethodCallee};
use ruby_analysis::indexer::{
    resolve_receiver_to_namespace, resolve_receiver_type, MethodReceiver, ReceiverResolutionContext,
};
use tower_lsp::lsp_types::{Location, Position};

use super::EngineQuery;
use crate::utils::lsp::source_position;

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

enum MethodLookupReceiver {
    Namespace(FullyQualifiedName),
    Type(RubyType),
    Super(FullyQualifiedName),
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
        let receiver =
            self.method_lookup_receiver(receiver, namespace, namespace_kind, position)?;
        analysis::find_method_definitions(self, &receiver, method, allow_private, protected_caller)
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
        let Some(receiver) =
            self.method_lookup_receiver(receiver, namespace, namespace_kind, position)
        else {
            return Vec::new();
        };
        match receiver {
            MethodLookupReceiver::Super(owner) => {
                analysis::resolve_super_method_callee(self, &owner, method)
                    .into_iter()
                    .collect()
            }
            MethodLookupReceiver::Type(receiver_type) => analysis::resolve_method_callees_for_type(
                self,
                &receiver_type,
                method,
                allow_private,
                protected_caller,
            )
            .unwrap_or_default(),
            MethodLookupReceiver::Namespace(owner) => {
                let callees = if allow_private {
                    analysis::resolve_method_callees(self, &owner, method)
                } else if let Some(caller) = protected_caller {
                    analysis::resolve_protected_method_callees(self, &owner, method, caller)
                } else {
                    analysis::resolve_public_method_callees(self, &owner, method)
                };
                callees.unwrap_or_default()
            }
        }
    }

    fn method_lookup_receiver(
        &self,
        receiver: &MethodReceiver,
        namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<MethodLookupReceiver> {
        if matches!(receiver, MethodReceiver::Super) {
            return Some(MethodLookupReceiver::Super(
                FullyQualifiedName::namespace_with_kind(namespace.to_vec(), namespace_kind),
            ));
        }
        if let Some(owner) =
            self.resolve_receiver_to_namespace(receiver, namespace, namespace_kind, position)
        {
            return Some(MethodLookupReceiver::Namespace(owner));
        }
        let receiver_type =
            self.resolve_receiver_type(receiver, namespace, namespace_kind, position);
        matches!(receiver_type, RubyType::Union(_))
            .then_some(MethodLookupReceiver::Type(receiver_type))
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
            .map(|doc| doc.position_to_analysis_offset(source_position(position)))
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
            .map(|doc| doc.position_to_analysis_offset(source_position(position)))
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
