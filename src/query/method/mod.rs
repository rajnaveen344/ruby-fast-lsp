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

use crate::analyzer_prism::MethodReceiver;
use crate::indexer::entry::NamespaceKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::utils::deduplicate_locations;
use log::trace;
pub use ruby_analysis_core::MethodCalleeResolution;
use ruby_analysis_core::TypeSubject;
use tower_lsp::lsp_types::{Location, Position};

use super::IndexQuery;

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

impl IndexQuery {
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
        let locations = self
            .resolve_method_callees(receiver, method, namespace, namespace_kind, position)
            .into_iter()
            .filter(|callee| callee.resolution == MethodCalleeResolution::Exact)
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
        let namespace_fqn =
            match self.resolve_receiver_to_namespace(receiver, namespace, namespace_kind, position)
            {
                Some(namespace_fqn) => namespace_fqn,
                None => return Vec::new(),
            };

        if let Some(callees) = analysis::resolve_method_callees(self, &namespace_fqn, method) {
            return callees;
        }

        Vec::new()
    }
}

// ============================================================================
// Receiver Resolution (Receiver → Namespace FQN)
// ============================================================================

impl IndexQuery {
    /// Convert method receiver to namespace FQN.
    /// Used by both go-to-definition and find-references.
    pub(crate) fn resolve_receiver_to_namespace(
        &self,
        receiver: &MethodReceiver,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        match receiver {
            MethodReceiver::Constant(path) => {
                self.resolve_constant_receiver(path, current_namespace)
            }

            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                self.resolve_current_scope(current_namespace, namespace_kind)
            }

            MethodReceiver::LocalVariable(name)
            | MethodReceiver::InstanceVariable(name)
            | MethodReceiver::ClassVariable(name)
            | MethodReceiver::GlobalVariable(name) => {
                self.resolve_variable_receiver(name, position)
            }

            MethodReceiver::MethodCall {
                inner_receiver,
                method_name,
            } => self.resolve_method_call_receiver(
                inner_receiver,
                method_name,
                current_namespace,
                namespace_kind,
                position,
            ),

            MethodReceiver::Literal(t) => self.convert_type_to_namespace(t),
            MethodReceiver::Expression => None, // No type info available
        }
    }

    /// Resolve constant receiver: `Foo.bar` → `Namespace(["Foo"], Singleton)`
    fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        if let Some(receiver_fqn) =
            analysis::resolve_constant_receiver(self, path, current_namespace)
        {
            return Some(receiver_fqn);
        }
        if self.analysis_engine().is_some() {
            return Some(FullyQualifiedName::namespace_with_kind(
                path.to_vec(),
                NamespaceKind::Singleton,
            ));
        }

        Some(FullyQualifiedName::namespace_with_kind(
            path.to_vec(),
            NamespaceKind::Singleton,
        ))
    }

    /// Resolve current scope: `bar` in context of current namespace
    fn resolve_current_scope(
        &self,
        namespace: &[RubyConstant],
        kind: NamespaceKind,
    ) -> Option<FullyQualifiedName> {
        Some(FullyQualifiedName::namespace_with_kind(
            namespace.to_vec(),
            kind,
        ))
    }

    /// Resolve variable receiver: `x.bar` where x is a variable
    fn resolve_variable_receiver(
        &self,
        var_name: &str,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        if let Some(var_type) = self.variable_receiver_type_from_analysis(var_name, position) {
            trace!(
                "Inferred type for '{}': {:?} via analysis",
                var_name,
                var_type
            );
            return self.convert_type_to_namespace(&var_type);
        }
        if self.analysis_engine().is_some() {
            return None;
        }
        None
    }

    /// Resolve method call receiver: `a.b.c` where we need a.b's return type
    fn resolve_method_call_receiver(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<FullyQualifiedName> {
        if let Some(chain_type) = self.method_call_receiver_type_from_analysis(
            inner_receiver,
            method_name,
            current_namespace,
            namespace_kind,
            position,
        ) {
            trace!(
                "Inferred type for '{}.{}': {:?} via analysis",
                receiver_to_string(inner_receiver),
                method_name,
                chain_type
            );
            return self.convert_type_to_namespace(&chain_type);
        }
        if self.analysis_engine().is_some() {
            return None;
        }
        None
    }

    fn variable_receiver_type_from_analysis(
        &self,
        var_name: &str,
        position: Position,
    ) -> Option<RubyType> {
        let doc_arc = self.doc.as_ref()?;
        let doc = doc_arc.read();
        if let Some(scope_id) = doc
            .variable_scopes()
            .find_scope_for_variable_at(var_name, position)
            .or_else(|| doc.variable_scopes().scope_at_position(position))
        {
            if let Some(ty) = doc
                .variable_scopes()
                .get_type_at_position(var_name, scope_id, position)
            {
                if *ty != RubyType::Unknown {
                    return Some(ty.clone());
                }
            }
        }

        let file_id = doc.analysis_file_id();
        let byte_offset = u32::try_from(doc.position_to_offset(position)).expect(
            "INVARIANT VIOLATED: LSP position offset exceeded u32. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        drop(doc);

        let engine = self.analysis_engine()?.lock();
        engine
            .type_store()
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.range.start_byte <= byte_offset)
            .filter_map(|fact| match &fact.subject {
                TypeSubject::Local { name, .. } if name == var_name => Some(fact),
                TypeSubject::InstanceVariable { name, .. } if name == var_name => Some(fact),
                TypeSubject::ClassVariable { name, .. } if name == var_name => Some(fact),
                TypeSubject::GlobalVariable(name) if name == var_name => Some(fact),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
            })
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.ruby_type)
    }

    fn method_call_receiver_type_from_analysis(
        &self,
        inner_receiver: &MethodReceiver,
        method_name: &str,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<RubyType> {
        if method_name == "new" {
            if let MethodReceiver::Constant(path) = inner_receiver {
                return Some(RubyType::Class(FullyQualifiedName::Constant(path.clone())));
            }
        }

        let inner_namespace = self.resolve_receiver_to_namespace(
            inner_receiver,
            current_namespace,
            namespace_kind,
            position,
        )?;
        if method_name == "new"
            && inner_namespace.namespace_kind() == Some(NamespaceKind::Singleton)
        {
            return Some(RubyType::Class(FullyQualifiedName::Constant(
                inner_namespace.namespace_parts(),
            )));
        }

        let method = RubyMethod::new(method_name).ok()?;
        let engine = self.analysis_engine()?.lock();
        let query = ruby_analysis_engine::AnalysisQuery::new(&engine);
        query.method_return_type_for_receiver(&inner_namespace, &method)
    }

    /// Convert RubyType to namespace FQN
    fn convert_type_to_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    NamespaceKind::Instance,
                ))
            }

            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                Some(FullyQualifiedName::namespace_with_kind(
                    fqn.namespace_parts(),
                    NamespaceKind::Singleton,
                ))
            }

            RubyType::Array(_) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Array").ok()?],
                NamespaceKind::Instance,
            )),

            RubyType::Hash(_, _) => Some(FullyQualifiedName::namespace_with_kind(
                vec![RubyConstant::new("Hash").ok()?],
                NamespaceKind::Instance,
            )),

            RubyType::Union(_) | RubyType::Unknown => None,
        }
    }
}

fn receiver_to_string(receiver: &MethodReceiver) -> String {
    match receiver {
        MethodReceiver::None => "".to_string(),
        MethodReceiver::SelfReceiver => "self".to_string(),
        MethodReceiver::Constant(path) => path
            .iter()
            .map(|constant| constant.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        MethodReceiver::LocalVariable(name)
        | MethodReceiver::InstanceVariable(name)
        | MethodReceiver::ClassVariable(name)
        | MethodReceiver::GlobalVariable(name) => name.clone(),
        MethodReceiver::MethodCall {
            inner_receiver,
            method_name,
        } => format!("{}.{}", receiver_to_string(inner_receiver), method_name),
        MethodReceiver::Literal(ruby_type) => format!("<literal:{:?}>", ruby_type),
        MethodReceiver::Expression => "<expr>".to_string(),
    }
}
