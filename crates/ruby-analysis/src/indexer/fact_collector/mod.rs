use crate::core::method_store::{MethodVisibility, MethodVisibilityOverrideFact};
use crate::core::type_store::NamedTypeResolution;
use crate::core::{
    ConstantTypeDependency, ConstantTypeEquation, ConstantTypeTarget, DiagnosticCandidate,
    DiagnosticFact, DiagnosticSeverity, ExecutionContextFact, FullyQualifiedName, GraphEdgeFact,
    GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact, GraphNodeKind, InferenceEvidence,
    InferenceTelemetry, MethodAvailability, MethodFact, MethodParamFact, MethodReturnEquation,
    NamespaceKind, ReferenceCandidate, RubyConstant, RubyMethod, ShapeConstructionError,
    SymbolFact, SymbolKind, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeStore,
    TypeSubject, UnknownReason, UnresolvedGraphEdgeFact,
};
use crate::engine::{AnalysisEngine, AnalysisQueryCache, VariableTypeKind};
use ruby_fast_lsp_extension_api::{IndexPatch, Receiver, ResolvedCall, SourceRange};
use ruby_prism::*;

use super::AnalysisIndex;
use crate::inference::method::recursive::solve_method_return_equations_with_telemetry;
use crate::inference::r#type::literal::{
    infer_array_literal_type_fallible, infer_hash_literal_type_fallible, literal_key,
    literal_shape_construction_unknown_reason, project_immediate_hash_receiver_type,
    LiteralAnalyzer,
};
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::TypeTracker;
use crate::inference::RubyType;
use crate::yard::parser::{CommentLineInfo, YardParser};
use crate::RubyDocument;
use crate::{control_flow, utf8_str, ScopeTracker};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

mod alias_method_node;
mod bad_splat;
mod block_node;
mod call_node;
mod class_node;
mod class_variable_write_node;
mod constant_path_node;
mod constant_path_write_node;
mod constant_read_node;
mod constant_write_node;
mod def_node;
mod global_variable_write_node;
mod instance_variable_write_node;
mod local_variable_read_node;
mod local_variable_write_node;
mod method_return;
mod module_node;
mod parameters_node;
mod singleton_class_node;
mod super_node;
mod variable_read_node;

pub struct FactCollector {
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub literal_analyzer: LiteralAnalyzer,
    pub analysis_diagnostics: Vec<DiagnosticFact>,
    pub type_store: TypeStore,
    pub extension_call_stack: Vec<ruby_fast_lsp_extension_api::ResolvedCall>,
    pub extension_project_context: Option<ruby_fast_lsp_extension_api::ProjectContext>,
    pub extension_call_stack_marks: Vec<bool>,
    pub extension_handled_call_marks: Vec<bool>,
    pub extension_index_patches: Vec<IndexPatch>,
    pub extension_execution_context_facts: Vec<ExecutionContextFact>,
    pending_block_execution_context: Option<BlockExecutionContext>,
    pub extension_host: Arc<dyn FactCollectorExtensionHost>,
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    analysis_query_cache: Arc<AnalysisQueryCache>,
    pub include_local_vars: bool,
    record_local_read_unknown_reasons: bool,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub resolve_analysis_method_returns: bool,
    pub infer_expression_receivers: bool,
    pub diagnostics_enabled: bool,
    pub direct_facts: AnalysisIndex,
    /// Append-only range index into `direct_facts.types` for expression facts.
    ///
    /// Recursive receiver inference consults expressions frequently while a
    /// file is being traversed. Retaining compact vector indexes avoids an
    /// O(expressions × all prior type facts) scan without creating a second
    /// semantic store or duplicating RubyType payloads.
    direct_expression_fact_indexes: HashMap<TextRange, smallvec::SmallVec<[usize; 1]>>,
    pub block_param_type_stack: Vec<Vec<RubyType>>,
    pub pattern_capture_type_stack: Vec<HashMap<String, RubyType>>,
    /// Positional RHS element types for the active `MultiWriteNode`, consumed by
    /// `ConstantTargetNode` in left-to-right order.
    pub multi_write_lhs_types: Vec<Vec<RubyType>>,
    pub yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,
    pub(crate) proc_return_types_by_local:
        HashMap<String, crate::inference::higher_order::KnownProcType>,
    /// Nonlocal writes currently being traversed. Their target facts are
    /// collected before Prism visits the RHS, but reads inside that RHS must
    /// observe the previous value rather than the not-yet-completed write.
    active_nonlocal_writes: Vec<(TypeSubject, TextRange)>,
    /// Compact method-return equations collected during the ordinary semantic
    /// traversal and solved once when the program traversal completes.
    method_return_equations: BTreeMap<Vec<RubyConstant>, Vec<MethodReturnEquation>>,
    finalized_method_return_equation_counts: HashMap<Vec<RubyConstant>, usize>,
    method_return_telemetry_by_namespace: BTreeMap<Vec<RubyConstant>, InferenceTelemetry>,
    max_live_shape_aliases: usize,
    method_return_outcomes: BTreeMap<FullyQualifiedName, TypeInferenceOutcome>,
    call_expression_outcomes: Vec<(TextRange, TypeInferenceOutcome)>,
    /// Call expressions that have a retained method candidate capable of
    /// producing a concrete outcome after complete engine resolution. This is
    /// collector-local and prevents terminal Unknown chains from becoming
    /// retained outer method candidates.
    deferred_call_outcome_ranges: HashSet<TextRange>,
    local_read_types: Vec<(TextRange, RubyType)>,
    expression_unknown_reasons: Vec<(TextRange, UnknownReason)>,
    constant_type_equations: Vec<ConstantTypeEquation>,
    constant_callable_bodies: Vec<crate::core::ConstantCallableBodyFact>,
    local_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    /// Same-pass method identities whose complete collected declaration set is
    /// currently public and available. `Arc::make_mut` keeps updates O(1) in
    /// the ordinary traversal because each method tracker releases its clone
    /// before the next declaration is installed.
    local_public_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    direct_known_namespaces: HashSet<FullyQualifiedName>,
    shared_direct_known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
}

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

impl FactCollector {
    pub(crate) fn invalidate_escaped_callables_in_value(&mut self, value: &Node<'_>) {
        if self.proc_return_types_by_local.is_empty() {
            return;
        }
        if crate::indexer::is_static_callable_literal(value)
            || value.as_local_variable_read_node().is_some()
        {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        escaped.visit(value);
        self.invalidate_callable_names(escaped.names);
    }

    fn invalidate_callable_names(&mut self, names: HashSet<String>) {
        for name in names {
            let Some(identity) = self
                .proc_return_types_by_local
                .get(&name)
                .map(|callable| callable.identity)
            else {
                continue;
            };
            for callable in self.proc_return_types_by_local.values_mut() {
                if callable.identity == identity {
                    callable.summary = Err(UnknownReason::EscapedCallableValue);
                }
            }
        }
    }

    fn invalidate_escaped_callables_in_call(&mut self, node: &CallNode<'_>) {
        if self.proc_return_types_by_local.is_empty() {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        if let Some(receiver) = node.receiver() {
            let direct_invoke = node.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                escaped.visit(&receiver);
            }
        }
        if let Some(arguments) = node.arguments() {
            escaped.visit_arguments_node(&arguments);
        }
        if node
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            escaped.visit(node.block().as_ref().expect("checked block presence"));
        }
        self.invalidate_callable_names(escaped.names);
    }

    pub(crate) fn bind_local_callable(
        &mut self,
        name: String,
        mut callable: crate::inference::higher_order::KnownProcType,
    ) {
        if callable
            .summary
            .as_ref()
            .is_ok_and(|summary| summary.captures.binary_search(&name).is_ok())
        {
            callable.summary = Err(UnknownReason::CallableRecursionUnsupported);
        }
        let alias_count = self
            .proc_return_types_by_local
            .iter()
            .filter(|(existing_name, existing)| {
                existing_name.as_str() != name && existing.identity == callable.identity
            })
            .count();
        if alias_count >= crate::core::callable_body::MAX_CALLABLE_BODY_ALIASES {
            callable.summary = Err(UnknownReason::CallableBodyBoundExceeded);
            for existing in self.proc_return_types_by_local.values_mut() {
                if existing.identity == callable.identity {
                    existing.summary = Err(UnknownReason::CallableBodyBoundExceeded);
                }
            }
        }
        self.proc_return_types_by_local.insert(name, callable);
    }

    fn merge_local_callables(
        left: HashMap<String, crate::inference::higher_order::KnownProcType>,
        right: HashMap<String, crate::inference::higher_order::KnownProcType>,
    ) -> HashMap<String, crate::inference::higher_order::KnownProcType> {
        let names = left
            .keys()
            .chain(right.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut merged = HashMap::with_capacity(names.len());
        for name in names {
            let callable = match (left.get(&name), right.get(&name)) {
                (Some(left), Some(right)) if left == right => left.clone(),
                (Some(left), Some(right)) => crate::inference::higher_order::KnownProcType {
                    identity: left.identity.min(right.identity),
                    summary: Err(UnknownReason::AmbiguousCallableValue),
                },
                (Some(callable), None) | (None, Some(callable)) => {
                    crate::inference::higher_order::KnownProcType {
                        identity: callable.identity,
                        summary: Err(UnknownReason::AmbiguousCallableValue),
                    }
                }
                (None, None) => panic!(
                    "INVARIANT VIOLATED: callable merge key `{name}` is absent from both branches. This is a bug because keys are derived from those exact maps. Fix: keep key collection and lookup atomic."
                ),
            };
            merged.insert(name, callable);
        }
        merged
    }

    pub fn push_warning_diagnostic(
        &mut self,
        range: TextRange,
        code: &'static str,
        message: String,
    ) {
        if !self.diagnostics_enabled {
            return;
        }
        self.analysis_diagnostics.push(DiagnosticFact::new(
            range,
            DiagnosticSeverity::Warning,
            code,
            message,
        ));
    }

    pub fn push_error_diagnostic(&mut self, range: TextRange, code: &'static str, message: String) {
        if !self.diagnostics_enabled {
            return;
        }
        self.analysis_diagnostics.push(DiagnosticFact::new(
            range,
            DiagnosticSeverity::Error,
            code,
            message,
        ));
    }

    pub fn text_range_from_offsets(&self, start: usize, end: usize) -> TextRange {
        let start_byte = u32::try_from(start).expect(
            "INVARIANT VIOLATED: diagnostic start offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let end_byte = u32::try_from(end).expect(
            "INVARIANT VIOLATED: diagnostic end offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        TextRange::new(self.document.analysis_file_id(), start_byte, end_byte)
    }

    pub fn body_text_range(
        &self,
        body_location: Option<ruby_prism::Location>,
        node_location: &ruby_prism::Location,
    ) -> TextRange {
        if let Some(body_location) = body_location {
            self.document.prism_location_to_text_range(&body_location)
        } else {
            self.document.prism_location_to_text_range(node_location)
        }
    }

    pub fn analysis_only(
        document: RubyDocument,
        extension_host: Arc<dyn FactCollectorExtensionHost>,
        analysis_engine: Arc<RwLock<AnalysisEngine>>,
    ) -> Self {
        let scope_tracker = ScopeTracker::new();
        let file_id = document.analysis_file_id();
        let local_method_candidates = Arc::new(
            analysis_engine
                .read()
                .method_facts_in_file(file_id)
                .into_iter()
                .map(|fact| fact.fqn)
                .collect(),
        );
        Self {
            document,
            scope_tracker,
            literal_analyzer: LiteralAnalyzer::new(),
            analysis_diagnostics: Vec::new(),
            type_store: TypeStore::new(),
            extension_call_stack: Vec::new(),
            extension_project_context: None,
            extension_call_stack_marks: Vec::new(),
            extension_handled_call_marks: Vec::new(),
            extension_index_patches: Vec::new(),
            extension_execution_context_facts: Vec::new(),
            pending_block_execution_context: None,
            extension_host,
            analysis_engine,
            analysis_query_cache: Arc::new(AnalysisQueryCache::default()),
            include_local_vars: true,
            record_local_read_unknown_reasons: true,
            reference_candidates: Vec::new(),
            diagnostic_candidates: Vec::new(),
            resolve_analysis_method_returns: true,
            infer_expression_receivers: true,
            diagnostics_enabled: true,
            direct_facts: AnalysisIndex::default(),
            direct_expression_fact_indexes: HashMap::new(),
            block_param_type_stack: Vec::new(),
            pattern_capture_type_stack: Vec::new(),
            multi_write_lhs_types: Vec::new(),
            yield_param_types_by_method: HashMap::new(),
            proc_return_types_by_local: HashMap::new(),
            active_nonlocal_writes: Vec::new(),
            method_return_equations: BTreeMap::new(),
            finalized_method_return_equation_counts: HashMap::new(),
            method_return_telemetry_by_namespace: BTreeMap::new(),
            max_live_shape_aliases: 0,
            method_return_outcomes: BTreeMap::new(),
            call_expression_outcomes: Vec::new(),
            deferred_call_outcome_ranges: HashSet::new(),
            local_read_types: Vec::new(),
            expression_unknown_reasons: Vec::new(),
            constant_type_equations: Vec::new(),
            constant_callable_bodies: Vec::new(),
            local_method_candidates,
            local_public_method_candidates: Arc::new(HashSet::new()),
            direct_known_namespaces: HashSet::new(),
            shared_direct_known_namespaces: None,
        }
    }

    pub fn set_pending_block_execution_context(&mut self, context: BlockExecutionContext) {
        assert!(
            self.pending_block_execution_context.is_none(),
            "INVARIANT VIOLATED: more than one block execution context was applied to the same call. This is a bug because extension conflicts must be resolved before AST traversal. Fix: validate and deterministically resolve extension execution contexts in the host."
        );
        self.pending_block_execution_context = Some(context);
    }

    pub fn without_analysis_method_return_resolution(mut self) -> Self {
        self.resolve_analysis_method_returns = false;
        self
    }

    /// Skip local-read proof-failure evidence when the owning source is an
    /// immutable dependency. Local scopes are still collected for semantic
    /// traversal, but dependency-local hover evidence is not retained by the
    /// engine and must not add work to cold indexing.
    pub fn without_local_read_unknown_reasons(mut self) -> Self {
        self.record_local_read_unknown_reasons = false;
        self
    }

    pub fn without_expression_receiver_inference(mut self) -> Self {
        self.infer_expression_receivers = false;
        self
    }

    pub fn without_diagnostics(mut self) -> Self {
        self.diagnostics_enabled = false;
        self
    }

    pub fn with_direct_known_namespaces(
        mut self,
        known_namespaces: HashSet<FullyQualifiedName>,
    ) -> Self {
        self.direct_known_namespaces = known_namespaces;
        self
    }

    pub fn with_shared_direct_known_namespaces(
        mut self,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Self {
        self.shared_direct_known_namespaces = Some(known_namespaces);
        self
    }

    pub fn extend_direct_known_namespaces(
        &mut self,
        known_namespaces: impl IntoIterator<Item = FullyQualifiedName>,
    ) {
        self.direct_known_namespaces.extend(known_namespaces);
    }

    fn direct_namespace_is_known(&self, fqn: &FullyQualifiedName) -> bool {
        self.direct_known_namespaces.contains(fqn)
            || self
                .shared_direct_known_namespaces
                .as_ref()
                .is_some_and(|known| known.contains(fqn))
    }

    fn static_eval_block_context(
        &self,
        node: &CallNode,
    ) -> Option<(Vec<RubyConstant>, NamespaceKind, NamespaceKind)> {
        let (implicit_receiver_kind, method_definition_kind) = match node.name().as_slice() {
            b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => {
                (NamespaceKind::Singleton, NamespaceKind::Instance)
            }
            b"instance_eval" | b"instance_exec" => {
                (NamespaceKind::Singleton, NamespaceKind::Singleton)
            }
            _ => return None,
        };
        node.block()?;
        let namespace = match node.receiver() {
            None => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                    .then_some(namespace)?
            }
            Some(receiver) if receiver.as_self_node().is_some() => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty())
                    .then_some(namespace)?
            }
            Some(receiver) => {
                let eval_ref = crate::mixin_ref_from_node(&receiver)?;
                self.resolve_static_eval_namespace(&eval_ref.parts, eval_ref.absolute)?
            }
        };
        Some((namespace, implicit_receiver_kind, method_definition_kind))
    }

    fn static_dynamic_definition_block_context(
        &self,
        node: &CallNode,
    ) -> Option<(
        Vec<RubyConstant>,
        NamespaceKind,
        Vec<RubyConstant>,
        NamespaceKind,
    )> {
        node.block()?;
        let (definition_namespace, definition_kind) =
            self.scope_tracker.method_definition_context();
        let (implicit_namespace, implicit_kind) = match node.receiver() {
            None => {
                let target_kind = match node.name().as_slice() {
                    b"define_method"
                        if !self.scope_tracker.execution_context_active()
                            && self.scope_tracker.in_singleton() =>
                    {
                        NamespaceKind::Singleton
                    }
                    b"define_method" => NamespaceKind::Instance,
                    b"define_singleton_method" => NamespaceKind::Singleton,
                    _ => return None,
                };
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
                    return None;
                }
                (namespace, target_kind)
            }
            Some(receiver) if node.name().as_slice() == b"define_singleton_method" => (
                self.resolve_constant_receiver_namespace(&receiver)?,
                NamespaceKind::Singleton,
            ),
            Some(receiver)
                if matches!(
                    node.name().as_slice(),
                    b"send" | b"public_send" | b"__send__"
                ) =>
            {
                let arguments = node.arguments()?;
                let selector = arguments.arguments().iter().next()?;
                let target_kind = if let Some(symbol) = selector.as_symbol_node() {
                    match symbol.unescaped() {
                        b"define_method" => NamespaceKind::Instance,
                        b"define_singleton_method" => NamespaceKind::Singleton,
                        _ => return None,
                    }
                } else if let Some(string) = selector.as_string_node() {
                    match string.unescaped() {
                        b"define_method" => NamespaceKind::Instance,
                        b"define_singleton_method" => NamespaceKind::Singleton,
                        _ => return None,
                    }
                } else {
                    return None;
                };
                if node.name().as_slice() == b"public_send"
                    && target_kind == NamespaceKind::Instance
                {
                    return None;
                }
                (
                    self.resolve_constant_receiver_namespace(&receiver)?,
                    target_kind,
                )
            }
            Some(_) => return None,
        };
        Some((
            implicit_namespace,
            implicit_kind,
            definition_namespace,
            definition_kind,
        ))
    }

    fn concern_class_methods_block_namespace(
        &mut self,
        node: &CallNode,
    ) -> Option<Vec<RubyConstant>> {
        if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
            return None;
        }
        node.block()?;

        let current_namespace = self.scope_tracker.get_ns_stack();
        if current_namespace.is_empty() {
            return None;
        }

        let class_methods = RubyConstant::new("ClassMethods").expect(
            "INVARIANT VIOLATED: static Concern ClassMethods constant is invalid. \
             This is a bug because `ClassMethods` is a valid Ruby constant. \
             Fix: inspect RubyConstant validation.",
        );
        let mut target_namespace = current_namespace.clone();
        target_namespace.push(class_methods);
        let target_fqn = FullyQualifiedName::namespace(target_namespace);
        let range = self.direct_range(&node.location());
        self.direct_push_namespace_facts(target_fqn, GraphNodeKind::Module, range, range);
        self.direct_push_edge(
            FullyQualifiedName::namespace(current_namespace),
            &[class_methods],
            false,
            GraphEdgeKind::Extend,
            range,
        );

        Some(vec![class_methods])
    }

    fn resolve_static_eval_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<Vec<RubyConstant>> {
        if parts.is_empty() {
            return None;
        }

        let current_namespace = self.scope_tracker.get_ns_stack();
        if absolute {
            let fqn = FullyQualifiedName::namespace(parts.to_vec());
            return self.namespace_is_known(&fqn).then(|| parts.to_vec());
        }

        let mut search = current_namespace.clone();
        loop {
            let mut candidate = search.clone();
            candidate.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.namespace_is_known(&fqn) {
                return Some(candidate);
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.namespace_is_known(&fqn).then(|| parts.to_vec())
    }

    pub fn direct_range(&self, location: &ruby_prism::Location<'_>) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(location.start_offset()),
            u32_offset(location.end_offset()),
        )
    }

    pub fn direct_terminal_name_range(
        &self,
        path: &ruby_prism::Location<'_>,
        name: &[u8],
    ) -> TextRange {
        let end = path.end_offset();
        let start = end.checked_sub(name.len()).expect(
            "INVARIANT VIOLATED: constant name is longer than its Prism path location. \
             This is a bug because the terminal name must be contained in the constant path. \
             Fix: inspect Prism constant path locations before deriving declaration ranges.",
        );
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(start),
            u32_offset(end),
        )
    }

    pub fn direct_push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
        name_range: TextRange,
    ) {
        self.direct_known_namespaces.insert(fqn.clone());
        self.direct_facts.symbols.push(
            SymbolFact::new(
                fqn.clone(),
                match kind {
                    GraphNodeKind::Class => SymbolKind::Class,
                    GraphNodeKind::Module => SymbolKind::Module,
                },
                range,
            )
            .with_name_range(name_range),
        );
        self.direct_facts
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));
        let constant_fqn = FullyQualifiedName::constant(fqn.namespace_parts());
        self.direct_facts.types.push(TypeFact::new(
            TypeSubject::Constant(constant_fqn.clone()),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            },
            range,
            TypeProvenance::Inferred,
        ));

        let singleton_fqn = fqn.to_singleton_namespace().expect(
            "INVARIANT VIOLATED: namespace fact could not convert to singleton namespace. \
             This is a bug because class/module graph nodes must be namespace FQNs. \
             Fix: only call direct_push_namespace_facts with Namespace facts.",
        );
        self.direct_known_namespaces.insert(singleton_fqn.clone());
        self.direct_facts
            .graph_nodes
            .push(GraphNodeFact::new(singleton_fqn, kind, range));
    }

    pub fn direct_resolve_namespace(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
    ) -> Option<FullyQualifiedName> {
        self.direct_resolve_namespace_from(parts, absolute, &self.scope_tracker.get_ns_stack())
    }

    pub fn direct_resolve_namespace_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(probe);
            if self.direct_namespace_is_known(&fqn) {
                return Some(fqn);
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.direct_namespace_is_known(&fqn).then_some(fqn)
    }

    pub fn namespace_is_known(&self, fqn: &FullyQualifiedName) -> bool {
        if self.direct_namespace_is_known(fqn) {
            return true;
        }
        let engine = self.analysis_engine.read();
        !crate::engine::AnalysisQuery::new(&engine)
            .graph_nodes_for(fqn)
            .is_empty()
    }

    pub fn resolve_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let mut search = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };
        let engine = self.analysis_engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let constant = FullyQualifiedName::constant(probe);
            if let Some(ruby_type) = self
                .direct_constant_value_type(&constant)
                .or_else(|| query.constant_value_type(&constant))
            {
                return Some((constant, ruby_type));
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        None
    }

    pub fn resolve_declaration_constant_value_type_from(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let mut candidates = Vec::new();
        let mut exact = if absolute {
            Vec::new()
        } else {
            lexical_context.to_vec()
        };
        exact.extend(parts.iter().cloned());
        candidates.push(exact);
        if !absolute && parts.len() > 1 && !lexical_context.is_empty() {
            candidates.push(parts.to_vec());
        }

        let engine = self.analysis_engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        candidates.into_iter().find_map(|candidate| {
            let constant = FullyQualifiedName::constant(candidate);
            self.direct_constant_value_type(&constant)
                .or_else(|| query.constant_value_type(&constant))
                .map(|ruby_type| (constant, ruby_type))
        })
    }

    pub fn direct_push_edge(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        range: TextRange,
    ) {
        self.direct_push_edge_with_provenance(
            source,
            parts,
            absolute,
            kind,
            GraphEdgeProvenance::Explicit,
            range,
        );
    }

    pub fn direct_push_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) {
        let Some(target) = self.direct_resolve_namespace(parts, absolute) else {
            self.direct_facts.unresolved_graph_edges.push(
                UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack()),
                    kind,
                    range,
                )
                .with_provenance(provenance),
            );
            return;
        };
        self.direct_push_resolved_edge_with_provenance(source, target, kind, provenance, range);
    }

    pub fn direct_push_resolved_edge(
        &mut self,
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        range: TextRange,
    ) -> bool {
        self.direct_push_resolved_edge_with_provenance(
            source,
            target,
            kind,
            GraphEdgeProvenance::Explicit,
            range,
        )
    }

    fn direct_push_resolved_edge_with_provenance(
        &mut self,
        source: FullyQualifiedName,
        target: FullyQualifiedName,
        kind: GraphEdgeKind,
        provenance: GraphEdgeProvenance,
        range: TextRange,
    ) -> bool {
        if self
            .direct_facts
            .graph_edges
            .iter()
            .any(|edge| edge.source == source && edge.target == target && edge.kind == kind)
        {
            return true;
        }

        if kind == GraphEdgeKind::Superclass {
            if let Some(existing) = self
                .direct_facts
                .graph_edges
                .iter()
                .find(|edge| edge.source == source && edge.kind == GraphEdgeKind::Superclass)
            {
                self.push_error_diagnostic(
                    range,
                    "conflicting-superclass",
                    format!(
                        "Class `{source}` already inherits `{}` and cannot also inherit `{target}`",
                        existing.target
                    ),
                );
                return false;
            }
        }

        if ancestry_edge_kind(kind)
            && (source == target || self.direct_ancestry_path_exists(&target, &source))
        {
            self.push_error_diagnostic(
                range,
                "cyclic-inheritance",
                format!("Inheritance edge `{source}` -> `{target}` creates a cycle"),
            );
            return false;
        }

        self.direct_facts
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range).with_provenance(provenance));
        true
    }

    fn direct_ancestry_path_exists(
        &self,
        start: &FullyQualifiedName,
        destination: &FullyQualifiedName,
    ) -> bool {
        let mut pending = vec![start.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if &current == destination {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            pending.extend(
                self.direct_facts
                    .graph_edges
                    .iter()
                    .filter(|edge| edge.source == current && ancestry_edge_kind(edge.kind))
                    .map(|edge| edge.target.clone()),
            );
        }
        false
    }

    pub fn direct_push_method_fact(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
    ) {
        self.direct_push_method_fact_with_signature(
            namespace, owner_kind, method, range, params, None, None,
        );
    }

    pub fn direct_push_method_fact_with_signature(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) {
        self.direct_push_method_fact_with_signature_and_name_range(
            namespace,
            owner_kind,
            method,
            range,
            range,
            params,
            documentation,
            return_type_label,
        );
    }

    pub fn direct_push_method_fact_with_signature_and_name_range(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        name_range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) {
        self.direct_push_method_fact_with_signature_name_range_and_availability(
            namespace,
            owner_kind,
            method,
            range,
            name_range,
            params,
            documentation,
            return_type_label,
            crate::core::MethodAvailability::Available,
        );
    }

    pub fn direct_push_method_fact_with_signature_name_range_and_availability(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        name_range: TextRange,
        params: Vec<MethodParamFact>,
        documentation: Option<String>,
        return_type_label: Option<String>,
        availability: crate::core::MethodAvailability,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.direct_facts.symbols.push(
            SymbolFact::new(fqn.clone(), SymbolKind::Method, range).with_name_range(name_range),
        );
        self.push_direct_method_fact(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_name_range(name_range)
                .with_signature_metadata(documentation, return_type_label)
                .with_availability(availability)
                .with_visibility(self.scope_tracker.current_visibility()),
        );
    }

    pub fn direct_push_method_fact_with_visibility(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        visibility: MethodVisibility,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.direct_facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.push_direct_method_fact(
            MethodFact::new(fqn, owner, range).with_visibility(visibility),
        );
    }

    pub fn direct_set_visibility(&mut self, visibility: MethodVisibility) {
        self.scope_tracker.set_current_visibility(visibility);
    }

    pub fn direct_set_method_visibility(
        &mut self,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) {
        let owner = FullyQualifiedName::namespace_with_kind(
            self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_macro_definition_context(),
        );
        self.direct_facts
            .method_visibility_overrides
            .push(MethodVisibilityOverrideFact::new(
                owner.clone(),
                method,
                visibility,
                range,
            ));
        let mut changed_direct_fact = false;
        for fact in &mut self.direct_facts.methods {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                continue;
            };
            if *fact_method == method && fact.owner == owner {
                fact.visibility = visibility;
                changed_direct_fact = true;
            }
        }
        if changed_direct_fact {
            let fqn = FullyQualifiedName::method(owner.namespace_parts(), method);
            self.refresh_local_public_method_candidate(&fqn);
        }
    }

    pub(super) fn push_direct_method_fact(&mut self, fact: MethodFact) {
        let fqn = fact.fqn.clone();
        self.direct_facts.methods.push(fact);
        self.refresh_local_public_method_candidate(&fqn);
    }

    fn refresh_local_public_method_candidate(&mut self, fqn: &FullyQualifiedName) {
        let mut matching = self
            .direct_facts
            .methods
            .iter()
            .filter(|fact| &fact.fqn == fqn)
            .peekable();
        assert!(
            matching.peek().is_some(),
            "INVARIANT VIOLATED: public-method candidate refresh has no matching direct method fact. This is a bug because refresh must run only after insertion or visibility mutation. Fix: pass the exact inserted method FQN to refresh_local_public_method_candidate."
        );
        let proven_public = matching.all(|fact| {
            fact.visibility == MethodVisibility::Public
                && matches!(&fact.availability, MethodAvailability::Available)
        });
        let candidates = Arc::make_mut(&mut self.local_public_method_candidates);
        if proven_public {
            candidates.insert(fqn.clone());
        } else {
            candidates.remove(fqn);
        }
    }

    pub fn direct_push_variable_symbol(
        &mut self,
        fqn: FullyQualifiedName,
        kind: SymbolKind,
        location: &ruby_prism::Location<'_>,
    ) {
        self.direct_facts
            .symbols
            .push(SymbolFact::new(fqn, kind, self.direct_range(location)));
    }

    pub fn direct_push_assignment_type(
        &mut self,
        subject: TypeSubject,
        ruby_type: RubyType,
        location: &ruby_prism::Location<'_>,
    ) {
        self.direct_push_type(subject, ruby_type, location, TypeProvenance::Assignment);
    }

    pub fn direct_push_type(
        &mut self,
        subject: TypeSubject,
        ruby_type: RubyType,
        location: &ruby_prism::Location<'_>,
        provenance: TypeProvenance,
    ) {
        if ruby_type == RubyType::Unknown {
            return;
        }
        self.direct_facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            self.direct_range(location),
            provenance,
        ));
    }

    fn push_direct_expression_fact(&mut self, fact: TypeFact) {
        let TypeSubject::Expression(subject_range) = &fact.subject else {
            panic!(
                "INVARIANT VIOLATED: the direct expression index received a named type subject. This is a bug because the range index may only point to TypeSubject::Expression facts. Fix: route named facts through direct_push_type and expression facts through push_direct_expression_fact."
            );
        };
        assert_eq!(
            *subject_range,
            fact.range,
            "INVARIANT VIOLATED: a direct expression subject differs from its fact range. This is a bug because the compact range index uses that identity for exact lookup. Fix: construct both ranges from the same Prism node location."
        );
        let range = *subject_range;
        let index = self.direct_facts.types.len();
        self.direct_facts.types.push(fact);
        self.direct_expression_fact_indexes
            .entry(range)
            .or_default()
            .push(index);
    }

    fn direct_expression_fact(
        &self,
        range: TextRange,
        provenance: Option<TypeProvenance>,
    ) -> Option<&TypeFact> {
        self.direct_expression_fact_indexes
            .get(&range)?
            .iter()
            .rev()
            .find_map(|index| {
                let fact = self.direct_facts.types.get(*index).expect(
                    "INVARIANT VIOLATED: the direct expression index points outside the append-only fact vector. This is a bug because direct facts are never removed during collection. Fix: record each index only after appending its owning fact and never reorder direct_facts.types.",
                );
                assert!(
                    matches!(&fact.subject, TypeSubject::Expression(subject_range) if *subject_range == range)
                        && fact.range == range,
                    "INVARIANT VIOLATED: the direct expression range index points to a different semantic fact. This is a bug because an indexed lookup would return evidence for the wrong AST node. Fix: update the range index atomically with every expression-fact append."
                );
                provenance
                    .is_none_or(|expected| fact.provenance == expected)
                    .then_some(fact)
            })
    }

    pub fn assignment_type_and_provenance(&self, value: &Node<'_>) -> (RubyType, TypeProvenance) {
        let value_range = self.direct_range(&value.location());
        if let Some(runtime_fact) =
            self.direct_expression_fact(value_range, Some(TypeProvenance::Runtime))
        {
            return (runtime_fact.ruby_type.clone(), TypeProvenance::Runtime);
        }
        (
            self.infer_assignment_type_from_value(value),
            TypeProvenance::Assignment,
        )
    }

    pub fn direct_push_expression_type(
        &mut self,
        node: &Node<'_>,
        ruby_type: RubyType,
        provenance: TypeProvenance,
    ) {
        if ruby_type == RubyType::Unknown {
            return;
        }
        let range = self.direct_range(&node.location());
        if self
            .direct_expression_fact_indexes
            .get(&range)
            .into_iter()
            .flatten()
            .any(|index| {
                self.direct_facts
                    .types
                    .get(*index)
                    .expect(
                        "INVARIANT VIOLATED: the direct expression deduplication index points outside the append-only fact vector. This is a bug because expression indexes and facts must be appended atomically. Fix: use push_direct_expression_fact for every expression fact.",
                    )
                    .ruby_type
                    == ruby_type
            })
        {
            return;
        }
        let fact = TypeFact::new(TypeSubject::Expression(range), ruby_type, range, provenance);
        self.type_store.add(fact.clone());
        self.push_direct_expression_fact(fact);
    }

    /// Infer type from a value node during indexing.
    ///
    /// This recursively walks the AST to infer types:
    /// - Literals → their type (String, Integer, etc.)
    /// - Constants → ClassReference
    /// - Local variables → look up their type
    /// - Method calls → recursively infer receiver type, then resolve method return type
    pub fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        self.infer_type_from_value_with_locals(value_node, &HashMap::new())
    }

    fn infer_type_from_value_with_locals(
        &self,
        value_node: &Node,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let expression_range = self.direct_range(&value_node.location());
        if let Some(fact) = self.direct_expression_fact(expression_range, None) {
            return fact.ruby_type.clone();
        }
        if let Some(statements) = value_node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .map(|node| self.infer_type_from_value_with_locals(&node, local_types))
                .unwrap_or_else(RubyType::nil_class);
        }
        if let Some(result) = self.infer_collection_type_from_value(value_node, local_types) {
            return result.unwrap_or(RubyType::Unknown);
        }
        if let Some(if_node) = value_node.as_if_node() {
            return self.infer_if_expression_type(&if_node, local_types);
        }
        if let Some(unless_node) = value_node.as_unless_node() {
            return self.infer_unless_expression_type(&unless_node, local_types);
        }
        if let Some(case_node) = value_node.as_case_node() {
            return self.infer_case_expression_type(&case_node, local_types);
        }
        if let Some(begin_node) = value_node.as_begin_node() {
            return self.infer_begin_expression_type(&begin_node, local_types);
        }
        if let Some(rescue_modifier) = value_node.as_rescue_modifier_node() {
            return self.infer_rescue_modifier_expression_type(&rescue_modifier, local_types);
        }

        // 1. Try literal analysis first (String, Integer, Array, Hash, Symbol, etc.)
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // 2. Constant read/path: resolve against the active lexical namespace before
        // projecting the class object. This also preserves the target identity when
        // one constant aliases another class/module object.
        if let Some(reference) = crate::mixin_ref_from_node(value_node) {
            let lexical_context = self.scope_tracker.get_ns_stack();
            if let Some((_constant, ruby_type)) = self.resolve_constant_value_type_from(
                &reference.parts,
                reference.absolute,
                &lexical_context,
            ) {
                return ruby_type;
            }
            if let Some(fqn) = self.constant_reference_type(value_node) {
                return RubyType::ClassReference(fqn);
            }
        }

        if let Some(ret) = value_node.as_return_node() {
            let Some(arguments) = ret.arguments() else {
                return RubyType::nil_class();
            };
            let args = arguments.arguments().iter().collect::<Vec<_>>();
            return match args.len() {
                0 => RubyType::nil_class(),
                1 => self.infer_type_from_value_with_locals(&args[0], local_types),
                2.. => RubyType::Array(RubyType::canonical_union_members(
                    args.iter()
                        .map(|arg| self.infer_type_from_value_with_locals(arg, local_types)),
                )),
            };
        }

        // 4. Local variable read: look up the variable's type
        if let Some(lvar_read) = value_node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(lvar_read.name().as_slice()).to_string();
            if let Some(ty) = local_types.get(&var_name) {
                return ty.clone();
            }
            if let Some(ty) = self.get_local_var_type(&var_name, &lvar_read.location()) {
                return ty;
            }
            return RubyType::Unknown;
        }

        // 5. Method call: recursively infer receiver type, then resolve method.
        // Keep the proof outcome until the final RubyType projection so the
        // indexing pass can retain the exact reason for every withheld call.
        if let Some(call_node) = value_node.as_call_node() {
            return self
                .infer_call_type_outcome_with_locals(&call_node, local_types)
                .into_ruby_type();
        }

        RubyType::Unknown
    }

    fn infer_collection_type_from_value(
        &self,
        value_node: &Node<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Option<Result<RubyType, ShapeConstructionError>> {
        if let Some(hash) = value_node.as_hash_node() {
            return Some(infer_hash_literal_type_fallible(&hash, |value| {
                self.infer_collection_type_from_value(value, local_types)
                    .unwrap_or_else(|| {
                        Ok(self.infer_type_from_value_with_locals(value, local_types))
                    })
            }));
        }
        value_node.as_array_node().map(|array| {
            infer_array_literal_type_fallible(&array, |value| {
                self.infer_collection_type_from_value(value, local_types)
                    .unwrap_or_else(|| {
                        Ok(self.infer_type_from_value_with_locals(value, local_types))
                    })
            })
        })
    }

    fn infer_call_type_outcome_with_locals(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> TypeInferenceOutcome {
        let expression_range = self.direct_range(&call_node.location());
        if let Some(fact) = self
            .direct_expression_fact(expression_range, None)
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
        {
            return TypeInferenceOutcome::proven(fact.ruby_type.clone());
        }

        if let Some(const_get_type) = self.const_get_reference_type(call_node) {
            return TypeInferenceOutcome::proven(const_get_type);
        }
        if let Some(outcome) =
            self.infer_rbs_higher_order_call_outcome(call_node, local_types, None)
        {
            return outcome;
        }
        if let Some(block_return_type) = self.infer_yielding_block_return_type_for_call(call_node) {
            return TypeInferenceOutcome::proven(block_return_type);
        }
        if let Some(proc_return_type) = self.infer_proc_call_return_type(call_node, local_types) {
            return proc_return_type;
        }

        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        let receiver_type = if let Some(receiver) = call_node.receiver() {
            let inferred = self.infer_type_from_value_with_locals(&receiver, local_types);
            project_immediate_hash_receiver_type(&receiver, inferred)
        } else {
            // No receiver means `self`, which may differ from lexical constant
            // scope inside eval- or extension-provided execution contexts.
            let (namespace, kind) = self.scope_tracker.implicit_receiver_context();
            if namespace.is_empty() {
                return TypeInferenceOutcome::unknown(UnknownReason::UnknownReceiver);
            }
            let current_fqn = FullyQualifiedName::namespace(namespace);
            match kind {
                NamespaceKind::Instance => RubyType::Class(current_fqn),
                NamespaceKind::Singleton => RubyType::ClassReference(current_fqn),
            }
        };

        if receiver_type == RubyType::Unknown {
            let reason = call_node
                .receiver()
                .and_then(|receiver| {
                    let range = self.direct_range(&receiver.location());
                    self.expression_unknown_reasons
                        .iter()
                        .rev()
                        .find_map(|(candidate, reason)| (*candidate == range).then_some(*reason))
                })
                .unwrap_or(UnknownReason::UnknownReceiver);
            return TypeInferenceOutcome::unknown(reason);
        }

        if shape_reads::is_shape_only(&receiver_type) {
            let argument_nodes = call_node
                .arguments()
                .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
                .unwrap_or_default();
            let argument_types = argument_nodes
                .iter()
                .map(|argument| self.infer_type_from_value_with_locals(argument, local_types))
                .collect::<Vec<_>>();
            let precise = match method_name.as_ref() {
                "[]" if argument_nodes.len() == 1 => Some(shape_reads::indexed_read(
                    &receiver_type,
                    literal_key(&argument_nodes[0]).as_ref(),
                )),
                "fetch" if matches!(argument_nodes.len(), 1 | 2) => Some(shape_reads::fetch(
                    &receiver_type,
                    literal_key(&argument_nodes[0]).as_ref(),
                    argument_types.get(1),
                )),
                "dig" if !argument_nodes.is_empty() => {
                    let keys = argument_nodes.iter().map(literal_key).collect::<Vec<_>>();
                    Some(shape_reads::dig(&receiver_type, &keys))
                }
                "key?" | "has_key?" | "include?" | "member?" if argument_nodes.len() == 1 => {
                    Some(shape_reads::key_presence(
                        &receiver_type,
                        literal_key(&argument_nodes[0]).as_ref(),
                    ))
                }
                "keys" if argument_nodes.is_empty() => Some(shape_reads::keys(&receiver_type)),
                "values" if argument_nodes.is_empty() => Some(shape_reads::values(&receiver_type)),
                "each" | "each_pair" | "each_key" | "each_value" if argument_nodes.is_empty() => {
                    Some(shape_reads::each_return(
                        &receiver_type,
                        call_node.block().is_some(),
                    ))
                }
                _ => None,
            };
            if let Some(outcome) = precise {
                return match outcome {
                    Ok(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
                    Err(reason) => TypeInferenceOutcome::unknown(reason),
                };
            }
        }

        // Object#freeze preserves the receiver identity and type. RBS
        // expresses this as `self`, which is a substitution contract rather
        // than a named return type.
        if method_name == "freeze" {
            return TypeInferenceOutcome::proven(receiver_type);
        }

        self.resolve_method_return_type_outcome_with_private(
            &receiver_type,
            &method_name,
            call_node.receiver().is_none(),
        )
    }

    fn prepare_higher_order_call_for_node(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason> {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        let receiver_type = call_node.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(
                &receiver,
                self.infer_type_from_value_with_locals(&receiver, local_types),
            )
        });
        if receiver_type.as_ref().is_some_and(|receiver| {
            receiver == &RubyType::Unknown || RubyType::contains_unknown(receiver)
        }) {
            return Err(UnknownReason::IncompleteBlockInput);
        }
        let argument_types = call_node
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.infer_type_from_value_with_locals(&argument, local_types))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let engine = self.analysis_engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        let direct = receiver_type.as_ref().map_or_else(
            || Err(UnknownReason::UnsupportedCallable),
            |receiver_type| {
                crate::inference::rbs::prepare_higher_order_call(
                    Some(&query),
                    receiver_type,
                    &method_name,
                    &argument_types,
                )
            },
        );
        direct.or_else(|_| {
            let namespace = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
            crate::inference::rbs::prepare_forwarded_higher_order_call(
                &query,
                receiver_type.as_ref(),
                Some(&namespace),
                &method_name,
                &argument_types,
            )
            .or_else(|_| {
                crate::inference::rbs::prepare_direct_yield_higher_order_call(
                    &query,
                    receiver_type.as_ref(),
                    Some(&namespace),
                    &method_name,
                    &argument_types,
                )
            })
        })
    }

    fn infer_rbs_higher_order_call_outcome(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
        prepared: Option<
            Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>,
        >,
    ) -> Option<TypeInferenceOutcome> {
        let block_expression = call_node.block()?;
        let method_name = String::from_utf8_lossy(call_node.name().as_slice());
        if method_name.ends_with('!') {
            return None;
        }
        let prepared = prepared
            .unwrap_or_else(|| self.prepare_higher_order_call_for_node(call_node, local_types))
            .ok()?;
        if let Some(block) = block_expression.as_block_node() {
            if block.body().as_ref().is_some_and(|body| {
                crate::inference::control_flow::has_unsupported_higher_order_exit(body)
            }) {
                return Some(TypeInferenceOutcome::unknown(
                    UnknownReason::UnsupportedBlockFlow,
                ));
            }
            let parameter_types = prepared.block_parameter_types().to_vec();
            let parameter_names = block_parameter_names(&block);
            let mut block_locals = local_types.clone();
            for (index, name) in parameter_names.iter().enumerate() {
                let parameter_type = parameter_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(RubyType::nil_class);
                block_locals.insert(name.clone(), parameter_type);
            }
            let tracked_parameters = parameter_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    (
                        name.clone(),
                        parameter_types
                            .get(index)
                            .cloned()
                            .unwrap_or_else(RubyType::nil_class),
                    )
                })
                .collect::<Vec<_>>();
            let mut tracker = TypeTracker::new(self.document.content.as_bytes())
                .with_analysis_engine(self.analysis_engine.clone())
                .with_analysis_query_cache(self.analysis_query_cache.clone());
            let namespace = self.scope_tracker.get_ns_stack();
            if !namespace.is_empty() {
                tracker.set_current_class(Some(FullyQualifiedName::namespace(namespace)));
            }
            let (block_return_type, tracked_post_types) =
                tracker.track_isolated_block_body(block.body(), &block_locals, &tracked_parameters);
            let mut post_parameter_types = parameter_types.clone();
            for (index, ruby_type) in tracked_post_types.into_iter().enumerate() {
                if let Some(slot) = post_parameter_types.get_mut(index) {
                    *slot = ruby_type;
                }
            }
            return Some(
                prepared.finish_with_proven_block_state(&block_return_type, &post_parameter_types),
            );
        }

        let Some(block_argument) = block_expression.as_block_argument_node() else {
            return Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnsupportedCallable,
            ));
        };
        let Some(expression) = block_argument.expression() else {
            return Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnsupportedCallable,
            ));
        };
        if let Some(symbol) = expression.as_symbol_node() {
            let target = std::str::from_utf8(symbol.unescaped()).ok()?;
            return Some(
                prepared.finish_static_method(target, |receiver_type, target| {
                    self.resolve_method_return_type_outcome_with_private(
                        receiver_type,
                        target,
                        false,
                    )
                }),
            );
        }
        let callable = if let Some(local) = expression.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.proc_return_types_by_local.get(&name).cloned().map(Ok)
        } else {
            self.constant_callable_body_for_node(&expression)
                .map(|result| {
                    result.map(|summary| crate::inference::higher_order::KnownProcType {
                        identity: u32::MAX,
                        summary: Ok(summary),
                    })
                })
        };
        Some(callable.map_or_else(
            || TypeInferenceOutcome::unknown(UnknownReason::UnsupportedCallable),
            |callable| match callable {
                Err(reason) => TypeInferenceOutcome::unknown(reason),
                Ok(callable) => {
                    let mut stack = vec![callable.identity];
                    prepared.finish_known_proc(
                        &callable,
                        |capture| {
                            local_types.get(capture).cloned().or_else(|| {
                                self.get_local_var_type(capture, &expression.location())
                            })
                        },
                        |capture, arguments| {
                            let nested = self.proc_return_types_by_local.get(capture)?.clone();
                            Some(self.instantiate_known_proc_with_stack(
                                &nested,
                                arguments,
                                local_types,
                                &expression.location(),
                                &mut stack,
                            ))
                        },
                        |receiver, method, _arguments| {
                            self.resolve_method_return_type_outcome_with_private(
                                receiver,
                                method.as_str(),
                                false,
                            )
                        },
                    )
                }
            },
        ))
    }

    fn infer_yielding_block_return_type_for_call(
        &self,
        call_node: &CallNode<'_>,
    ) -> Option<RubyType> {
        let block = call_node.block()?.as_block_node()?;
        let param_types = self.infer_block_param_types_from_yielding_method(call_node)?;
        if param_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return None;
        }

        let param_names = block_parameter_names(&block);
        let mut local_types = HashMap::new();
        for (index, name) in param_names.iter().enumerate() {
            if let Some(param_type) = param_types.get(index) {
                if *param_type != RubyType::Unknown {
                    local_types.insert(name.clone(), param_type.clone());
                }
            }
        }

        let return_type = block
            .body()
            .map(|body| self.infer_type_from_value_with_locals(&body, &local_types))
            .unwrap_or_else(RubyType::nil_class);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    fn infer_if_expression_type(
        &self,
        if_node: &IfNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let then_diverges = if_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let then_type = if_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let (else_type, else_diverges) = if let Some(subsequent) = if_node.subsequent() {
            let diverges = control_flow::diverges(&subsequent);
            let ty = if let Some(else_node) = subsequent.as_else_node() {
                else_node
                    .statements()
                    .map(|statements| {
                        self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                    })
                    .unwrap_or_else(RubyType::nil_class)
            } else if let Some(elsif_node) = subsequent.as_if_node() {
                self.infer_if_expression_type(&elsif_node, local_types)
            } else {
                RubyType::nil_class()
            };
            (ty, diverges)
        } else {
            (RubyType::nil_class(), false)
        };

        join_non_diverging_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    fn infer_unless_expression_type(
        &self,
        unless_node: &UnlessNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let then_diverges = unless_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let then_type = unless_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let else_diverges = unless_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let else_type = unless_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        join_non_diverging_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    fn infer_case_expression_type(
        &self,
        case_node: &CaseNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let mut branches = Vec::new();
        for condition in case_node.conditions().iter() {
            let Some(when_node) = condition.as_when_node() else {
                continue;
            };
            let diverges = when_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = when_node
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
        }

        if let Some(else_clause) = case_node.else_clause() {
            let diverges = else_clause
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = else_clause
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
        } else {
            branches.push((RubyType::nil_class(), false));
        }

        join_non_diverging_types(&branches)
    }

    fn infer_begin_expression_type(
        &self,
        begin_node: &BeginNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let body_diverges = begin_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let body_type = begin_node
            .statements()
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or_else(RubyType::nil_class);

        let normal_type = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| {
                self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
            })
            .unwrap_or(body_type);
        let else_diverges = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let normal_diverges = body_diverges || else_diverges;

        let mut branches = vec![(normal_type, normal_diverges)];
        let mut rescue_clause = begin_node.rescue_clause();
        while let Some(rescue_node) = rescue_clause {
            let diverges = rescue_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let ty = rescue_node
                .statements()
                .map(|statements| {
                    self.infer_type_from_value_with_locals(&statements.as_node(), local_types)
                })
                .unwrap_or_else(RubyType::nil_class);
            branches.push((ty, diverges));
            rescue_clause = rescue_node.subsequent();
        }

        join_non_diverging_types(&branches)
    }

    fn infer_rescue_modifier_expression_type(
        &self,
        rescue_modifier: &RescueModifierNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> RubyType {
        let expression = rescue_modifier.expression();
        let rescue_expression = rescue_modifier.rescue_expression();
        join_non_diverging_types(&[
            (
                self.infer_type_from_value_with_locals(&expression, local_types),
                control_flow::diverges(&expression),
            ),
            (
                self.infer_type_from_value_with_locals(&rescue_expression, local_types),
                control_flow::diverges(&rescue_expression),
            ),
        ])
    }

    pub fn infer_proc_literal_return_type(&self, value_node: &Node) -> Option<RubyType> {
        let callable = self.infer_known_proc_type(value_node)?;
        let summary = callable.summary.ok()?;
        crate::inference::callable_body::instantiate_callable_body(
            &summary,
            &[],
            |capture| self.get_local_var_type(capture, &value_node.location()),
            |_, _| None,
            |receiver, method, _arguments| {
                self.resolve_method_return_type_outcome_with_private(
                    receiver,
                    method.as_str(),
                    false,
                )
            },
        )
        .into_proven_type()
    }

    pub(crate) fn infer_known_proc_type(
        &self,
        value_node: &Node,
    ) -> Option<crate::inference::higher_order::KnownProcType> {
        crate::indexer::is_static_callable_literal(value_node).then(|| {
            let scope_id = self.document.variable_scopes().current_scope().expect(
                "INVARIANT VIOLATED: callable lowering ran without an active lexical scope. This is a bug because FactCollector and VariableScopes must enter and exit scopes together. Fix: keep callable lowering inside the ordinary collector traversal.",
            );
            let outer_locals = self
                .document
                .variable_scopes()
                .get_visible_variables(scope_id)
                .into_iter()
                .map(|variable| variable.name.to_string());
            crate::inference::higher_order::KnownProcType {
                identity: u32::try_from(value_node.location().start_offset()).expect(
                    "INVARIANT VIOLATED: callable literal offset exceeded u32. This is a bug because analysis ranges already require u32 offsets. Fix: reject oversized source before callable lowering.",
                ),
                summary: crate::indexer::lower_callable_literal_with_outer_locals(
                    value_node,
                    outer_locals,
                ),
            }
        })
    }

    fn instantiate_known_proc_with_stack(
        &self,
        callable: &crate::inference::higher_order::KnownProcType,
        arguments: &[RubyType],
        local_types: &HashMap<String, RubyType>,
        location: &Location<'_>,
        stack: &mut Vec<u32>,
    ) -> TypeInferenceOutcome {
        if stack.contains(&callable.identity) {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableRecursionUnsupported);
        }
        if stack.len() >= crate::core::callable_body::MAX_CALLABLE_BODY_INSTANTIATIONS {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableBodyBoundExceeded);
        }
        let summary = match &callable.summary {
            Ok(summary) => summary,
            Err(reason) => return TypeInferenceOutcome::unknown(*reason),
        };
        stack.push(callable.identity);
        let result = crate::inference::callable_body::instantiate_callable_body(
            summary,
            arguments,
            |capture| {
                local_types
                    .get(capture)
                    .cloned()
                    .or_else(|| self.get_local_var_type(capture, location))
            },
            |capture, nested_arguments| {
                let nested = self.proc_return_types_by_local.get(capture)?.clone();
                Some(self.instantiate_known_proc_with_stack(
                    &nested,
                    nested_arguments,
                    local_types,
                    location,
                    stack,
                ))
            },
            |receiver, method, _arguments| {
                self.resolve_method_return_type_outcome_with_private(
                    receiver,
                    method.as_str(),
                    false,
                )
            },
        );
        let popped = stack.pop().expect(
            "INVARIANT VIOLATED: callable instantiation stack underflowed. This is a bug because every accepted callable pushes exactly one identity. Fix: keep push/evaluate/pop in one function.",
        );
        assert_eq!(
            popped, callable.identity,
            "INVARIANT VIOLATED: callable instantiation stack order changed during evaluation. This is a bug because nested evaluation must be strictly LIFO. Fix: do not retain or reorder stack entries."
        );
        result
    }

    fn infer_proc_body(&self, body: Option<Node<'_>>) -> Option<RubyType> {
        let return_type = body
            .map(|body| self.infer_type_from_value(&body))
            .unwrap_or_else(RubyType::nil_class);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    /// Infer the value returned by a call's block using the current lexical,
    /// local, and execution-context state. Extension adapters may request this
    /// framework-neutral operation for DSL-generated methods such as memoized
    /// helpers; the adapter remains responsible for declaring that relationship.
    pub fn infer_call_block_return_type(&self, call: &CallNode<'_>) -> Option<RubyType> {
        let block = call.block()?.as_block_node()?;
        self.infer_proc_body(block.body())
    }

    fn infer_proc_call_return_type(
        &self,
        call_node: &CallNode<'_>,
        local_types: &HashMap<String, RubyType>,
    ) -> Option<TypeInferenceOutcome> {
        if call_node.name().as_slice() != b"call" {
            return None;
        }
        let receiver = call_node.receiver()?;
        let callable = if let Some(local) = receiver.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.proc_return_types_by_local.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&receiver)? {
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
                Err(reason) => return Some(TypeInferenceOutcome::unknown(reason)),
            }
        };
        let argument_types = call_node
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.infer_type_from_value_with_locals(&argument, local_types))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(self.instantiate_known_proc_with_stack(
            &callable,
            &argument_types,
            local_types,
            &call_node.location(),
            &mut Vec::new(),
        ))
    }

    fn constant_callable_body_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
        let reference = crate::mixin_ref_from_node(node)?;
        let lexical_context = self.scope_tracker.get_ns_stack();
        let (constant, _) = self.resolve_constant_value_type_from(
            &reference.parts,
            reference.absolute,
            &lexical_context,
        )?;
        let mut local = self
            .constant_callable_bodies
            .iter()
            .filter(|fact| fact.constant == constant)
            .map(|fact| &fact.summary);
        if let Some(first) = local.next() {
            if local.any(|summary| summary != first) {
                return Some(Err(UnknownReason::AmbiguousCallableValue));
            }
            return Some(Ok(first.clone()));
        }
        let engine = self.analysis_engine.read();
        crate::engine::AnalysisQuery::new(&engine).constant_callable_body(&constant)
    }

    pub fn infer_assignment_type_from_value(&self, value_node: &Node) -> RubyType {
        let expression_range = self.direct_range(&value_node.location());
        if let Some(fact) = self.direct_expression_fact(expression_range, None) {
            return fact.ruby_type.clone();
        }
        if self.resolve_analysis_method_returns {
            return self.infer_type_from_value(value_node);
        }

        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }
        if let Some(call) = value_node.as_call_node() {
            if let Some(const_get_type) = self.const_get_reference_type(&call) {
                return const_get_type;
            }

            if call.name().as_slice() == b"new" {
                if let Some(receiver) = call.receiver() {
                    if let Some(fqn) = self.constant_reference_type(&receiver) {
                        return RubyType::Class(fqn);
                    }
                }
            }
            return RubyType::Unknown;
        }
        self.constant_reference_type(value_node)
            .map(RubyType::ClassReference)
            .unwrap_or(RubyType::Unknown)
    }

    pub(super) fn infer_assignment_type_from_value_with_reason(
        &self,
        value_node: &Node<'_>,
    ) -> (RubyType, Option<UnknownReason>) {
        match self.infer_collection_type_from_value(value_node, &HashMap::new()) {
            Some(Ok(ruby_type)) => (ruby_type, None),
            Some(Err(error)) => (
                RubyType::Unknown,
                Some(literal_shape_construction_unknown_reason(error)),
            ),
            None => (self.infer_assignment_type_from_value(value_node), None),
        }
    }

    pub fn assign_current_block_parameter_type(
        &mut self,
        param_name: &str,
        param_location: &ruby_prism::Location<'_>,
        param_index: usize,
    ) {
        let Some(param_type) = self
            .block_param_type_stack
            .last()
            .and_then(|types| types.get(param_index))
            .filter(|ty| **ty != RubyType::Unknown)
            .cloned()
        else {
            return;
        };

        let Some(current_scope_id) = self.document.variable_scopes().current_scope() else {
            return;
        };
        let range = self.document.prism_location_to_text_range(param_location);
        self.document
            .variable_scopes_mut()
            .define_variable(param_name, range);
        self.document.variable_scopes_mut().add_type_assignment(
            current_scope_id,
            param_name,
            range,
            param_type.clone(),
        );
        let scope_id = u32::try_from(current_scope_id).expect(
            "INVARIANT VIOLATED: block parameter scope id exceeded u32. \
             This is a bug because ruby-analysis::core TypeSubject::Local stores u32 scope ids. \
             Fix: widen TypeSubject::Local scope_id before indexing more than u32::MAX scopes.",
        );
        self.type_store.add(TypeFact::new(
            TypeSubject::Local {
                scope_id,
                name: param_name.to_string(),
            },
            param_type,
            range,
            TypeProvenance::Assignment,
        ));
    }

    fn infer_block_param_types_for_call(
        &self,
        node: &CallNode<'_>,
    ) -> (
        Vec<RubyType>,
        Option<Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>>,
    ) {
        if node.block().is_none() {
            return (Vec::new(), None);
        }
        if let Some(yield_param_types) = self.infer_block_param_types_from_yielding_method(node) {
            return (yield_param_types, None);
        }
        let method_name = node.name().as_slice();
        let param_count = block_required_param_count(node);
        let receiver_type = node.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(&receiver, self.infer_type_from_value(&receiver))
        });

        let prepared = self.prepare_higher_order_call_for_node(node, &HashMap::new());
        let preparation_reason = match prepared {
            Ok(prepared) => {
                let parameter_types = prepared.block_parameter_types().to_vec();
                return (parameter_types, Some(Ok(prepared)));
            }
            Err(reason) => reason,
        };
        let Some(receiver_type) = receiver_type else {
            return (Vec::new(), Some(Err(preparation_reason)));
        };

        if shape_reads::is_shape_only(&receiver_type)
            && matches!(
                method_name,
                b"each" | b"each_pair" | b"each_key" | b"each_value"
            )
        {
            let key_type = match shape_reads::keys(&receiver_type) {
                Ok(RubyType::Array(types)) => RubyType::union(types),
                Ok(ruby_type) => panic!(
                    "INVARIANT VIOLATED: shape keys projection returned `{ruby_type}` instead of Array. This is a bug because Hash#keys always returns an Array. Fix: keep shape_reads::keys canonical."
                ),
                Err(_) => return (Vec::new(), Some(Err(preparation_reason))),
            };
            let value_type = match shape_reads::values(&receiver_type) {
                Ok(RubyType::Array(types)) => RubyType::union(types),
                Ok(ruby_type) => panic!(
                    "INVARIANT VIOLATED: shape values projection returned `{ruby_type}` instead of Array. This is a bug because Hash#values always returns an Array. Fix: keep shape_reads::values canonical."
                ),
                Err(_) => return (Vec::new(), Some(Err(preparation_reason))),
            };
            if matches!(method_name, b"each" | b"each_pair") {
                return if param_count == 1 {
                    (
                        vec![RubyType::Array(vec![key_type, value_type])],
                        Some(Err(preparation_reason)),
                    )
                } else {
                    (vec![key_type, value_type], Some(Err(preparation_reason)))
                };
            }
            if method_name == b"each_key" {
                return (vec![key_type], Some(Err(preparation_reason)));
            }
            if method_name == b"each_value" {
                return (vec![value_type], Some(Err(preparation_reason)));
            }
            panic!(
                "INVARIANT VIOLATED: non-Hash iterator reached shape block parameter projection. This is a bug because the method-name guard accepts only each variants. Fix: keep the guard and exhaustive projection branches aligned."
            );
        }

        match receiver_type {
            // Enumerable#each_with_index currently exposes its pair through a
            // tuple signature, which is outside the first callable-template
            // release. Keep this pre-existing precision until tuple templates
            // are represented; collection transforms above are signature-only.
            RubyType::Array(element_types) if method_name == b"each_with_index" => (
                vec![RubyType::union(element_types), RubyType::integer()],
                Some(Err(preparation_reason)),
            ),
            RubyType::Hash(key_types, value_types) if method_name == b"each" => {
                let key_type = RubyType::union(key_types);
                let value_type = RubyType::union(value_types);
                if param_count == 1 {
                    (
                        vec![RubyType::Array(vec![key_type, value_type])],
                        Some(Err(preparation_reason)),
                    )
                } else {
                    (vec![key_type, value_type], Some(Err(preparation_reason)))
                }
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Literal(_)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => (Vec::new(), Some(Err(preparation_reason))),
        }
    }

    fn infer_block_param_types_from_yielding_method(
        &self,
        node: &CallNode<'_>,
    ) -> Option<Vec<RubyType>> {
        let method = RubyMethod::new(utf8_str(node.name().as_slice())).ok()?;
        let method_fqn = match node.receiver() {
            None => FullyQualifiedName::method(self.scope_tracker.get_ns_stack(), method),
            Some(receiver) if receiver.as_self_node().is_some() => {
                FullyQualifiedName::method(self.scope_tracker.get_ns_stack(), method)
            }
            Some(receiver) => {
                let fqn = self.constant_reference_type(&receiver)?;
                FullyQualifiedName::method(fqn.namespace_parts(), method)
            }
        };

        self.yield_param_types_by_method
            .get(&method_fqn)
            .filter(|types| types.iter().any(|ty| *ty != RubyType::Unknown))
            .cloned()
    }

    fn record_current_method_yield_types(&mut self, node: &YieldNode<'_>) {
        let Some(method_fqn) = self.scope_tracker.current_method_fqn().cloned() else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            return;
        };
        let yield_types = arguments
            .arguments()
            .iter()
            .map(|arg| self.infer_type_from_value(&arg))
            .collect::<Vec<_>>();
        if yield_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return;
        }
        let existing = self
            .yield_param_types_by_method
            .entry(method_fqn)
            .or_default();
        merge_position_types(existing, yield_types);
    }

    fn record_current_method_forwarded_yield_types(&mut self, node: &CallNode<'_>) {
        if !call_forwards_anonymous_block(node) {
            return;
        }
        let Some(method_fqn) = self.scope_tracker.current_method_fqn().cloned() else {
            return;
        };
        let Some(yield_types) = self.infer_block_param_types_from_yielding_method(node) else {
            return;
        };
        if yield_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return;
        }
        let existing = self
            .yield_param_types_by_method
            .entry(method_fqn)
            .or_default();
        merge_position_types(existing, yield_types);
    }

    fn record_call_expression_type(
        &mut self,
        node: &CallNode<'_>,
        prepared_higher_order: Option<
            Result<crate::inference::higher_order::PreparedCallableSet, UnknownReason>,
        >,
    ) {
        let higher_order_outcome =
            self.infer_rbs_higher_order_call_outcome(node, &HashMap::new(), prepared_higher_order);
        let range = self.direct_range(&node.location());
        if let Some(outcome) = higher_order_outcome.as_ref() {
            if outcome.unknown_reason().is_some() {
                self.call_expression_outcomes
                    .retain(|(outcome_range, _)| *outcome_range != range);
                self.call_expression_outcomes.push((range, outcome.clone()));
                self.suppress_deferred_call_outcome(range, "higher-order Unknown outcome");
                return;
            }
        }
        let proc_outcome = self.infer_proc_call_return_type(node, &HashMap::new());
        if let Some(outcome) = proc_outcome.as_ref() {
            if outcome.unknown_reason().is_some() {
                self.call_expression_outcomes
                    .retain(|(outcome_range, _)| *outcome_range != range);
                self.call_expression_outcomes.push((range, outcome.clone()));
                self.suppress_deferred_call_outcome(range, "callable-body Unknown outcome");
                return;
            }
        }

        let Some(return_type) = higher_order_outcome
            .and_then(TypeInferenceOutcome::into_proven_type)
            .or_else(|| self.infer_yielding_block_return_type_for_call(node))
            .or_else(|| proc_outcome.and_then(TypeInferenceOutcome::into_proven_type))
            .or_else(|| {
                crate::indexer::inlay_hints::has_multiline_chain_continuation(
                    self.document.analysis_content().as_bytes(),
                    node.location().end_offset(),
                )
                .then(|| self.infer_type_from_value(&node.as_node()))
                .filter(|ruby_type| *ruby_type != RubyType::Unknown)
            })
        else {
            return;
        };
        self.call_expression_outcomes
            .retain(|(outcome_range, _)| *outcome_range != range);
        self.suppress_deferred_call_outcome(range, "proven special-call outcome");
        let fact = TypeFact::new(
            TypeSubject::Expression(range),
            return_type,
            range,
            TypeProvenance::Inferred,
        );
        self.type_store.add(fact.clone());
        self.push_direct_expression_fact(fact);
    }

    fn suppress_deferred_call_outcome(&mut self, range: TextRange, proof_kind: &str) {
        let mut suppressed_candidates = 0usize;
        for candidate in &mut self.reference_candidates {
            let crate::core::ReferenceCandidateKind::Method {
                call_expression_range,
                ..
            } = &mut candidate.kind
            else {
                continue;
            };
            if *call_expression_range == Some(range) {
                *call_expression_range = None;
                suppressed_candidates = suppressed_candidates.checked_add(1).expect(
                    "INVARIANT VIOLATED: suppressed call candidate count overflowed usize. This is a bug because one file cannot contain more candidates than addressable memory. Fix: bound candidate collection by the source size.",
                );
            }
        }
        assert!(
            suppressed_candidates <= 1,
            "INVARIANT VIOLATED: one {proof_kind} suppressed multiple deferred outcomes. This is a bug because each CallNode owns at most one method candidate. Fix: emit exactly one candidate for the runtime dispatch."
        );
        if suppressed_candidates == 1 {
            assert!(
                self.deferred_call_outcome_ranges.remove(&range),
                "INVARIANT VIOLATED: a suppressed deferred call candidate has no collector-local range marker. This is a bug because candidate and marker lifecycles must be identical. Fix: insert and remove deferred ranges with the owning method candidate."
            );
        }
    }

    fn constant_reference_type(&self, node: &Node) -> Option<FullyQualifiedName> {
        let reference = crate::mixin_ref_from_node(node)?;
        if let Some(namespace) = self.direct_resolve_namespace(&reference.parts, reference.absolute)
        {
            return Some(namespace);
        }
        let lexical_context = self.scope_tracker.get_ns_stack();
        let engine = self.analysis_engine.read();
        if let Some(resolved) = crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(&reference.parts, &lexical_context)
        {
            return Some(resolved);
        }
        let mut parts = if reference.absolute {
            Vec::new()
        } else {
            lexical_context
        };
        parts.extend(reference.parts);
        Some(FullyQualifiedName::constant(parts))
    }

    pub(super) fn constant_type_dependency(
        &self,
        node: &Node<'_>,
    ) -> Option<ConstantTypeDependency> {
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"new" {
                let receiver = call.receiver()?;
                let reference = crate::mixin_ref_from_node(&receiver)?;
                return Some(ConstantTypeDependency::constructor(
                    reference.parts,
                    reference.absolute,
                    self.scope_tracker.get_ns_stack(),
                ));
            }
        }
        let reference = crate::mixin_ref_from_node(node)?;
        Some(ConstantTypeDependency::new(
            reference.parts,
            reference.absolute,
            self.scope_tracker.get_ns_stack(),
        ))
    }

    pub(super) fn push_constant_type_equation(
        &mut self,
        subject: TypeSubject,
        range: TextRange,
        dependency: ConstantTypeDependency,
    ) {
        let equation = ConstantTypeEquation::dependency(
            ConstantTypeTarget::Fact { subject, range },
            dependency,
        );
        if !self.constant_type_equations.contains(&equation) {
            self.constant_type_equations.push(equation);
        }
    }

    pub(super) fn push_constant_local_assignment_equation(
        &mut self,
        name: String,
        range: TextRange,
        dependency: ConstantTypeDependency,
    ) {
        let equation = ConstantTypeEquation::dependency(
            ConstantTypeTarget::LocalAssignment { name, range },
            dependency,
        );
        if !self.constant_type_equations.contains(&equation) {
            self.constant_type_equations.push(equation);
        }
    }

    fn const_get_reference_type(&self, call: &CallNode<'_>) -> Option<RubyType> {
        let parts = self.const_get_target_parts(call)?;
        let constant_fqn = FullyQualifiedName::constant(parts.clone());
        if let Some(value_type) = self.direct_constant_value_type(&constant_fqn) {
            return Some(value_type);
        }

        let namespace_fqn = FullyQualifiedName::namespace(parts.clone());
        if let Some(kind) = self
            .direct_facts
            .graph_nodes
            .iter()
            .filter(|fact| fact.fqn == namespace_fqn)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.kind)
        {
            return Some(match kind {
                GraphNodeKind::Class => RubyType::ClassReference(constant_fqn),
                GraphNodeKind::Module => RubyType::ModuleReference(constant_fqn),
            });
        }

        let engine = self.analysis_engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        query
            .constant_value_type(&constant_fqn)
            .or_else(|| query.constant_reference_type(&parts))
            .or_else(|| Some(RubyType::ClassReference(constant_fqn)))
    }

    fn direct_constant_value_type(&self, constant_fqn: &FullyQualifiedName) -> Option<RubyType> {
        let direct = self
            .direct_facts
            .types
            .iter()
            .filter(|fact| match &fact.subject {
                TypeSubject::Constant(fqn) => {
                    fqn == constant_fqn && fact.ruby_type != RubyType::Unknown
                }
                TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => false,
            })
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            });
        let subject = TypeSubject::Constant(constant_fqn.clone());
        let stored = self.type_store.latest_non_unknown_type_with_range(&subject);

        match (direct, stored) {
            (None, None) => None,
            (Some(fact), None) => Some(fact.ruby_type.clone()),
            (None, Some((ruby_type, _))) => Some(ruby_type.clone()),
            (Some(fact), Some((ruby_type, range))) => {
                let direct_key = (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                );
                let stored_key = (range.file_id, range.start_byte, range.end_byte);
                if direct_key > stored_key {
                    Some(fact.ruby_type.clone())
                } else {
                    Some(ruby_type.clone())
                }
            }
        }
    }

    fn const_get_target_parts(&self, call: &CallNode<'_>) -> Option<Vec<RubyConstant>> {
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        self.const_lookup_target_parts(call)
    }

    fn const_lookup_target_parts(&self, call: &CallNode<'_>) -> Option<Vec<RubyConstant>> {
        if !matches!(call.name().as_slice(), b"const_get" | b"const_defined?") {
            return None;
        }
        let arguments = call.arguments()?;
        let arg = arguments.arguments().iter().next()?;
        let constant = const_get_arg_constant(&arg)?;
        let mut parts = match call.receiver() {
            Some(receiver) if receiver.as_self_node().is_some() => {
                self.scope_tracker.get_ns_stack()
            }
            Some(receiver) => self.const_lookup_base_parts(&receiver)?,
            None => self.scope_tracker.get_ns_stack(),
        };
        parts.push(constant);
        Some(parts)
    }

    fn const_lookup_base_parts(&self, receiver: &Node<'_>) -> Option<Vec<RubyConstant>> {
        if let Some(parts) = receiver
            .as_call_node()
            .and_then(|call| self.const_lookup_target_parts(&call))
        {
            return Some(parts);
        }

        let receiver_ref = crate::mixin_ref_from_node(receiver)?;
        if let Some(fqn) = self.direct_resolve_namespace(&receiver_ref.parts, receiver_ref.absolute)
        {
            return Some(fqn.namespace_parts());
        }

        let context = if receiver_ref.absolute {
            Vec::new()
        } else {
            self.scope_tracker.get_ns_stack()
        };
        let engine = self.analysis_engine.read();
        if let Some(fqn) = crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(&receiver_ref.parts, &context)
        {
            return Some(fqn.namespace_parts());
        }

        Some(receiver_ref.parts)
    }

    /// Helper to get the type of a local variable by name at a given location.
    fn get_local_var_type(&self, var_name: &str, location: &Location) -> Option<RubyType> {
        let byte_offset = u32::try_from(location.start_offset()).expect(
            "INVARIANT VIOLATED: Prism location offset exceeded u32. \
             This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
        );
        let file_id = self.document.analysis_file_id();

        // Fact collection traverses the AST while keeping VariableScopes aligned with the
        // current lexical node. Starting from that scope preserves block capture and hard-scope
        // boundaries through get_type_at_position without rescanning every variable location.
        let scope_id = self.document.variable_scopes().current_scope().expect(
            "INVARIANT VIOLATED: local variable type inference ran without an active lexical scope. \
             This is a bug because FactCollector and VariableScopes must enter and exit AST scopes together. \
             Fix: balance the variable-scope lifecycle around every collector traversal branch.",
        );

        let ty = self.document.variable_scopes().get_type_at_position(
            var_name,
            scope_id,
            file_id,
            byte_offset,
        )?;

        if *ty != RubyType::Unknown {
            Some(ty.clone())
        } else {
            None
        }
    }

    /// Extract YARD documentation from comments preceding a method definition using Prism comments.
    pub fn extract_doc_comments(
        &self,
        method_start: usize,
    ) -> Option<crate::yard::types::YardMethodDoc> {
        // Find the first comment that starts AFTER or AT method_start.
        // We want the ones BEFORE it.
        let idx = self
            .document
            .get_comments()
            .partition_point(|c| c.0 < method_start);

        if idx == 0 {
            return None;
        }

        let mut comment_indices = Vec::new();
        let mut current_idx = idx - 1;

        // Check last comment is attached to method
        let (_, end) = self.document.get_comments()[current_idx];
        let range_between = &self.document.content[end..method_start];
        if !range_between.trim().is_empty() {
            return None;
        }
        comment_indices.push(current_idx);

        // Walk backwards to collect contiguous comment block
        while current_idx > 0 {
            let prev_idx = current_idx - 1;
            let (_, prev_end) = self.document.get_comments()[prev_idx];
            let (curr_start, _) = self.document.get_comments()[current_idx];

            let range_between = &self.document.content[prev_end..curr_start];
            if !range_between.trim().is_empty() {
                break;
            }
            comment_indices.push(prev_idx);
            current_idx = prev_idx;
        }

        comment_indices.reverse(); // Now in order top-down

        let mut line_infos = Vec::new();
        for &i in &comment_indices {
            let (start, end) = self.document.get_comments()[i];
            let raw_content = &self.document.content[start..end];
            let trimmed = raw_content.trim();
            // Prism comments include the #.
            let content = trimmed.trim_start_matches('#').trim_start();

            // Calculate precise location info for diagnostics
            // We need the position of the *content*, so find where it starts relative to the comment start
            let hash_offset = raw_content.find('#').unwrap_or(0);

            // Find content offset. If empty content, point to end of hash
            let content_offset_in_raw = if content.is_empty() {
                hash_offset + 1
            } else {
                raw_content.find(content).unwrap_or(hash_offset + 1)
            };

            let abs_content_start = start + content_offset_in_raw;
            let abs_pos = self.document.offset_to_position(abs_content_start);
            // YardParser uses line_length for diagnostic range end calculation in some cases
            // (end char is usually start char + content len, but passed as line_length in parser?)
            // Actually parser uses:
            // start: Position { line: line_info.line_number, character: line_info.content_start_char }
            // end: Position { line: line_info.line_number, character: line_info.line_length }
            // So line_length should be the COLUMN index of the end of the line (or length)
            let abs_end_pos = self.document.offset_to_position(end);

            line_infos.push(CommentLineInfo {
                content,
                line_number: abs_pos.line,
                content_start_char: abs_pos.character,
                line_length: abs_end_pos.character,
            });
        }

        let doc = YardParser::parse_lines(&line_infos, true);

        if doc.has_type_info() || doc.description.is_some() {
            Some(doc)
        } else {
            None
        }
    }
}

fn ancestry_edge_kind(kind: GraphEdgeKind) -> bool {
    match kind {
        GraphEdgeKind::Superclass
        | GraphEdgeKind::Include
        | GraphEdgeKind::Prepend
        | GraphEdgeKind::Extend => true,
        GraphEdgeKind::ExecutionContextApplication => false,
    }
}

fn source_range(visitor: &FactCollector, location: &ruby_prism::Location) -> SourceRange {
    let range = visitor.document.prism_location_to_source_range(location);
    SourceRange {
        start: ruby_fast_lsp_extension_api::SourcePosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: ruby_fast_lsp_extension_api::SourcePosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

fn const_get_arg_constant(arg: &Node<'_>) -> Option<RubyConstant> {
    if let Some(symbol) = arg.as_symbol_node() {
        let name = String::from_utf8_lossy(symbol.unescaped()).to_string();
        return RubyConstant::new(&name).ok();
    }
    if let Some(string) = arg.as_string_node() {
        let name = String::from_utf8_lossy(string.unescaped()).to_string();
        return RubyConstant::new(&name).ok();
    }
    None
}

fn block_required_param_count(node: &CallNode<'_>) -> usize {
    let Some(block) = node.block() else {
        return 0;
    };
    let Some(parameters) = block.as_block_node().and_then(|block| block.parameters()) else {
        return 0;
    };
    if let Some(numbered) = parameters.as_numbered_parameters_node() {
        return usize::from(numbered.maximum());
    }
    let Some(params_node) = parameters
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return 0;
    };
    params_node.requireds().iter().count()
}

fn block_parameter_names(block: &BlockNode<'_>) -> Vec<String> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
        return numbered_parameter_names(numbered);
    }
    let Some(params_node) = parameters_node
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for required in params_node.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    for optional in params_node.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    if let Some(rest) = params_node.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                names.push(String::from_utf8_lossy(name.as_slice()).to_string());
            }
        }
    }
    for post in params_node.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    names
}

fn numbered_parameter_names(params: NumberedParametersNode<'_>) -> Vec<String> {
    (1..=usize::from(params.maximum()))
        .map(|index| format!("_{index}"))
        .collect()
}

fn merge_position_types(existing: &mut Vec<RubyType>, incoming: Vec<RubyType>) {
    for (index, incoming_type) in incoming.into_iter().enumerate() {
        if incoming_type == RubyType::Unknown {
            continue;
        }
        if existing.len() <= index {
            existing.resize(index, RubyType::Unknown);
            existing.push(incoming_type);
            continue;
        }
        let merged = RubyType::union([existing[index].clone(), incoming_type]);
        existing[index] = merged;
    }
}

fn call_forwards_anonymous_block(node: &CallNode<'_>) -> bool {
    if node
        .block()
        .and_then(|block| block.as_block_argument_node())
        .is_some_and(|block_argument| block_argument.expression().is_none())
    {
        return true;
    }

    node.arguments()
        .map(|arguments| {
            arguments.arguments().iter().any(|argument| {
                if argument.as_forwarding_arguments_node().is_some() {
                    return true;
                }

                argument
                    .as_block_argument_node()
                    .is_some_and(|block_argument| block_argument.expression().is_none())
            })
        })
        .unwrap_or(false)
}

fn join_non_diverging_types(branches: &[(RubyType, bool)]) -> RubyType {
    let surviving = branches
        .iter()
        .filter(|(_, diverges)| !*diverges)
        .map(|(ty, _)| ty.clone())
        .collect::<Vec<_>>();
    if surviving.is_empty() {
        RubyType::Unknown
    } else {
        RubyType::union(surviving)
    }
}

impl FactCollector {
    fn finalize_method_return_equations_for_namespace(&mut self, namespace: &[RubyConstant]) {
        let Some(equations) = self.method_return_equations.get(namespace) else {
            return;
        };
        let equation_count = equations.len();
        if self
            .finalized_method_return_equation_counts
            .get(namespace)
            .is_some_and(|finalized| *finalized == equation_count)
        {
            return;
        }
        let solve_result = solve_method_return_equations_with_telemetry(equations);
        let solved = solve_result.outcomes;
        let file_id = self.document.analysis_file_id();
        self.type_store.update_inferred_method_return_types_in_file(
            file_id,
            solved
                .iter()
                .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
        );
        self.method_return_outcomes.extend(solved);
        self.method_return_telemetry_by_namespace
            .insert(namespace.to_vec(), solve_result.telemetry);
        self.finalized_method_return_equation_counts
            .insert(namespace.to_vec(), equation_count);
    }

    fn finalize_all_method_return_equations(&mut self) {
        let namespaces = self
            .method_return_equations
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for namespace in namespaces {
            self.finalize_method_return_equations_for_namespace(&namespace);
        }
    }

    pub fn method_return_outcomes(&self) -> &BTreeMap<FullyQualifiedName, TypeInferenceOutcome> {
        &self.method_return_outcomes
    }

    pub fn inference_telemetry(&self) -> InferenceTelemetry {
        let mut aggregate = InferenceTelemetry::default();
        for telemetry in self.method_return_telemetry_by_namespace.values() {
            aggregate.merge(telemetry);
        }
        aggregate.observe_max_live_shape_aliases(self.max_live_shape_aliases);
        aggregate
    }

    /// Return the exact file-owned proof results consumed by all adapters.
    pub fn inference_evidence(&self) -> InferenceEvidence {
        let mut deferred_call_ranges = self
            .reference_candidates
            .iter()
            .filter_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method {
                    call_expression_range,
                    ..
                } => *call_expression_range,
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .collect::<Vec<_>>();
        deferred_call_ranges.sort_unstable();
        for adjacent in deferred_call_ranges.windows(2) {
            assert!(
                adjacent[0] != adjacent[1],
                "INVARIANT VIOLATED: one call expression produced multiple deferred method-return candidates. This is a bug because final resolution cannot choose one runtime dispatch from competing candidates. Fix: attach exactly one method candidate to each CallNode outcome."
            );
        }

        let mut call_expression_outcomes = self.call_expression_outcomes.clone();
        call_expression_outcomes.sort_unstable_by_key(|(range, _)| *range);
        for adjacent in call_expression_outcomes.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one call expression produced more than one immediate proof outcome. This is a bug because one AST call has exactly one result. Fix: classify an immediate call once and leave all other calls to deferred engine resolution."
            );
        }

        let mut expression_unknown_reasons = self.expression_unknown_reasons.clone();
        expression_unknown_reasons.sort_unstable();
        for adjacent in expression_unknown_reasons.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one expression range produced more than one Unknown reason. This is a bug because one AST expression has exactly one proof result. Fix: record expression evidence once during its node-entry callback."
            );
        }
        let mut method_return_equations = self
            .method_return_equations
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        method_return_equations.sort_unstable();

        InferenceEvidence {
            method_return_outcomes: self.method_return_outcomes.clone(),
            method_return_equations,
            constant_type_equations: self.constant_type_equations.clone(),
            constant_callable_bodies: self.constant_callable_bodies.clone(),
            call_expression_outcomes,
            expression_unknown_reasons,
            telemetry: self.inference_telemetry(),
        }
    }

    /// Return sparse concrete local-read flow evidence separately from the
    /// per-file method inference record. Most files have no flow delta, so the
    /// engine stores this only for files that prove one.
    pub fn local_read_type_evidence(&self) -> Box<[(TextRange, RubyType)]> {
        let mut local_read_types = self.local_read_types.clone();
        local_read_types.sort_unstable_by_key(|(range, _)| *range);
        for (range, ruby_type) in &local_read_types {
            assert!(
                *ruby_type != RubyType::Unknown,
                "INVARIANT VIOLATED: compact local-read evidence retained Unknown at {range:?}. This is a bug because Unknown reads belong in expression_unknown_reasons and cannot be published as concrete proof. Fix: filter unresolved TypeTracker reads while installing file evidence."
            );
        }
        for adjacent in local_read_types.windows(2) {
            assert!(
                adjacent[0].0 != adjacent[1].0,
                "INVARIANT VIOLATED: one local-variable read produced multiple flow types. This is a bug because bounded solver revisits must overwrite the same AST read. Fix: retain local reads in TypeTracker's range-keyed map before installing evidence."
            );
        }
        local_read_types.into_boxed_slice()
    }

    pub(super) fn begin_nonlocal_write(&mut self, subject: TypeSubject, range: TextRange) {
        self.active_nonlocal_writes.push((subject, range));
    }

    pub(super) fn finish_nonlocal_write(&mut self) {
        self.active_nonlocal_writes.pop().expect(
            "INVARIANT VIOLATED: nonlocal write traversal stack underflowed. This is a bug because every variable-write exit must match one entry. Fix: keep FactCollector variable write callbacks balanced.",
        );
    }

    /// Resolve a nonlocal variable from facts collected before this exact
    /// source position. A write whose RHS is still being traversed is excluded
    /// because Ruby evaluates the RHS before updating the target.
    pub(super) fn collected_nonlocal_variable_type_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        let outcome =
            self.collected_nonlocal_variable_outcome_before(kind, name, owner, byte_offset);
        if let Some(ruby_type) = outcome.proven_type() {
            return Some(ruby_type.clone());
        }
        match outcome.unknown_reason().expect(
            "INVARIANT VIOLATED: an unproven nonlocal-variable result lost its Unknown reason. This is a bug because TypeInferenceOutcome cannot represent a reasonless failure. Fix: construct every failed reaching-assignment proof with TypeInferenceOutcome::unknown.",
        ) {
            UnknownReason::NoReachingAssignment => None,
            UnknownReason::UnresolvedAssignmentValue
            | UnknownReason::AmbiguousReachingAssignment
            | UnknownReason::ShapeBoundExceeded
            | UnknownReason::MutableShapeInvalidated => Some(RubyType::Unknown),
            UnknownReason::UnknownReceiver
            | UnknownReason::InvalidMethodName
            | UnknownReason::UnresolvedMethodReturn
            | UnknownReason::IncompleteUnionMember
            | UnknownReason::UnprovenRecursiveCycle
            | UnknownReason::UnsupportedCallable
            | UnknownReason::IncompleteBlockInput
            | UnknownReason::IncompleteBlockResult
            | UnknownReason::IncompleteGenericSubstitution
            | UnknownReason::AmbiguousCallableOverload
            | UnknownReason::HigherOrderBoundExceeded
            | UnknownReason::UnsupportedBlockFlow
            | UnknownReason::UnsupportedCallableBody
            | UnknownReason::IncompleteCallableInput
            | UnknownReason::IncompleteCallableCapture
            | UnknownReason::AmbiguousCallableValue
            | UnknownReason::EscapedCallableValue
            | UnknownReason::CallableBodyBoundExceeded
            | UnknownReason::CallableRecursionUnsupported
            | UnknownReason::UnsupportedCallableFlow => panic!(
                "INVARIANT VIOLATED: nonlocal reaching-assignment inference produced a method-call Unknown reason. This is a bug because the selector owns only assignment proof failures. Fix: keep reaching-assignment and method-call reason construction in their respective inference paths."
            ),
        }
    }

    pub(super) fn collected_nonlocal_variable_outcome_before(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        byte_offset: u32,
    ) -> TypeInferenceOutcome {
        self.collected_variable_type_outcome_before(byte_offset, |subject| match (subject, kind) {
            (
                TypeSubject::InstanceVariable {
                    owner: fact_owner,
                    name: fact_name,
                },
                VariableTypeKind::Instance,
            ) => fact_owner == owner && fact_name == name,
            (
                TypeSubject::ClassVariable {
                    owner: fact_owner,
                    name: fact_name,
                },
                VariableTypeKind::Class,
            ) => fact_owner.namespace_parts() == owner.namespace_parts() && fact_name == name,
            (TypeSubject::GlobalVariable(fact_name), VariableTypeKind::Global) => fact_name == name,
            (
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_),
                VariableTypeKind::Local
                | VariableTypeKind::Instance
                | VariableTypeKind::Class
                | VariableTypeKind::Global
                | VariableTypeKind::Constant,
            ) => false,
        })
    }

    fn collected_variable_type_outcome_before(
        &self,
        byte_offset: u32,
        matches_subject: impl Fn(&TypeSubject) -> bool,
    ) -> TypeInferenceOutcome {
        match self.type_store.named_type_in_file_before_matching(
            self.document.analysis_file_id(),
            byte_offset,
            |subject, range| {
                matches_subject(subject)
                    && !self
                        .active_nonlocal_writes
                        .iter()
                        .any(|(active_subject, active_range)| {
                            active_subject == subject && *active_range == range
                        })
            },
        ) {
            NamedTypeResolution::Unresolved => {
                TypeInferenceOutcome::unknown(UnknownReason::NoReachingAssignment)
            }
            NamedTypeResolution::Ambiguous => {
                TypeInferenceOutcome::unknown(UnknownReason::AmbiguousReachingAssignment)
            }
            NamedTypeResolution::Resolved(RubyType::Unknown) => {
                TypeInferenceOutcome::unknown(UnknownReason::UnresolvedAssignmentValue)
            }
            NamedTypeResolution::Resolved(ruby_type) => {
                TypeInferenceOutcome::proven(ruby_type.clone())
            }
        }
    }

    /// Positional RHS element types for `A, B = 1, "x"` / `A, B = [1, "x"]`.
    ///
    /// Non-array RHS (e.g. method call) yields an empty vec so targets stay untyped.
    fn multi_write_element_types(&self, value: &Node<'_>) -> Vec<RubyType> {
        let Some(array) = value.as_array_node() else {
            return Vec::new();
        };
        array
            .elements()
            .iter()
            .map(|element| self.infer_assignment_type_from_value(&element))
            .collect()
    }

    fn pattern_capture_types_for_value(
        &self,
        pattern: &Node<'_>,
        value: &Node<'_>,
    ) -> HashMap<String, RubyType> {
        let mut captures = HashMap::new();
        self.collect_pattern_capture_types(pattern, value, &mut captures);
        captures
    }

    fn collect_pattern_capture_types(
        &self,
        pattern: &Node<'_>,
        value: &Node<'_>,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(target) = pattern.as_local_variable_target_node() {
            let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
            captures.insert(name, self.infer_type_from_value(value));
            return;
        }

        if let Some(pattern_hash) = pattern.as_hash_pattern_node() {
            let Some(value_hash) = value.as_hash_node() else {
                return;
            };
            let value_elements = value_hash
                .elements()
                .iter()
                .filter_map(|element| {
                    let assoc = element.as_assoc_node()?;
                    Some((symbol_key(&assoc.key())?, assoc.value()))
                })
                .collect::<Vec<_>>();

            for element in pattern_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some(key) = symbol_key(&assoc.key()) else {
                    continue;
                };
                let Some((_, value_node)) = value_elements
                    .iter()
                    .find(|(value_key, _)| value_key == &key)
                else {
                    continue;
                };
                self.collect_pattern_capture_types(&assoc.value(), value_node, captures);
            }
            return;
        }

        if let Some(pattern_array) = pattern.as_array_pattern_node() {
            let Some(value_array) = value.as_array_node() else {
                return;
            };
            let value_elements = value_array.elements().iter().collect::<Vec<_>>();
            for (index, required) in pattern_array.requireds().iter().enumerate() {
                let Some(value_node) = value_elements.get(index) else {
                    continue;
                };
                self.collect_pattern_capture_types(&required, value_node, captures);
            }
        }
    }
}

fn symbol_key(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}

fn u32_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect(
        "INVARIANT VIOLATED: source byte offset exceeded u32. \
         This is a bug because analysis facts currently store u32 ranges. \
         Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
    )
}

impl Visit<'_> for FactCollector {
    fn visit_case_node(&mut self, node: &CaseNode<'_>) {
        if let Some(predicate) = node.predicate() {
            self.visit(&predicate);
        }
        let before = self.proc_return_types_by_local.clone();
        let mut surviving = Vec::new();
        for condition in node.conditions().iter() {
            let when = condition.as_when_node().expect(
                "INVARIANT VIOLATED: an ordinary CaseNode contains a non-When condition. This is a bug because Prism's CaseNode schema permits only WhenNode conditions. Fix: route pattern cases through CaseMatchNode instead of weakening this invariant.",
            );
            self.proc_return_types_by_local = before.clone();
            for expression in when.conditions().iter() {
                self.visit(&expression);
            }
            if let Some(statements) = when.statements() {
                self.visit(&statements.as_node());
                if !control_flow::diverges(&statements.as_node()) {
                    surviving.push(self.proc_return_types_by_local.clone());
                }
            } else {
                surviving.push(self.proc_return_types_by_local.clone());
            }
        }
        self.proc_return_types_by_local = before.clone();
        if let Some(else_clause) = node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.visit(&statements.as_node());
                if !control_flow::diverges(&statements.as_node()) {
                    surviving.push(self.proc_return_types_by_local.clone());
                }
            } else {
                surviving.push(self.proc_return_types_by_local.clone());
            }
        } else {
            surviving.push(before.clone());
        }
        self.proc_return_types_by_local = surviving
            .into_iter()
            .reduce(Self::merge_local_callables)
            .unwrap_or(before);
    }

    fn visit_if_node(&mut self, node: &IfNode<'_>) {
        self.visit(&node.predicate());
        let before = self.proc_return_types_by_local.clone();

        self.proc_return_types_by_local = before.clone();
        if let Some(statements) = node.statements() {
            self.visit(&statements.as_node());
        }
        let then_callables = self.proc_return_types_by_local.clone();
        let then_diverges = node
            .statements()
            .is_some_and(|statements| control_flow::diverges(&statements.as_node()));

        self.proc_return_types_by_local = before.clone();
        if let Some(subsequent) = node.subsequent() {
            self.visit(&subsequent);
        }
        let else_callables = self.proc_return_types_by_local.clone();
        let else_diverges = node
            .subsequent()
            .is_some_and(|subsequent| control_flow::diverges(&subsequent));

        self.proc_return_types_by_local = match (then_diverges, else_diverges) {
            (true, true) => before,
            (true, false) => else_callables,
            (false, true) => then_callables,
            (false, false) => Self::merge_local_callables(then_callables, else_callables),
        };
    }

    fn visit_unless_node(&mut self, node: &UnlessNode<'_>) {
        self.visit(&node.predicate());
        let before = self.proc_return_types_by_local.clone();

        self.proc_return_types_by_local = before.clone();
        if let Some(statements) = node.statements() {
            self.visit(&statements.as_node());
        }
        let then_callables = self.proc_return_types_by_local.clone();
        let then_diverges = node
            .statements()
            .is_some_and(|statements| control_flow::diverges(&statements.as_node()));

        self.proc_return_types_by_local = before.clone();
        if let Some(else_clause) = node.else_clause() {
            self.visit(&else_clause.as_node());
        }
        let else_callables = self.proc_return_types_by_local.clone();
        let else_diverges = node
            .else_clause()
            .is_some_and(|else_clause| control_flow::diverges(&else_clause.as_node()));

        self.proc_return_types_by_local = match (then_diverges, else_diverges) {
            (true, true) => before,
            (true, false) => else_callables,
            (false, true) => then_callables,
            (false, false) => Self::merge_local_callables(then_callables, else_callables),
        };
    }

    fn visit_program_node(&mut self, node: &ProgramNode<'_>) {
        // Install exact root-scope flow evidence before the ordinary semantic
        // traversal consumes local receivers. Method bodies do the same from
        // `process_def_node_entry`; without the corresponding program pass,
        // a top-level alias mutation or escape would be analyzed through the
        // older assignment-only view and could publish stale shape fields.
        if self.record_local_read_unknown_reasons {
            let mut tracker = TypeTracker::new(self.document.content.as_bytes())
                .with_analysis_engine(self.analysis_engine.clone())
                .with_analysis_query_cache(self.analysis_query_cache.clone())
                .with_local_read_types();
            tracker.track_program(node);
            self.install_local_read_types(tracker.take_local_read_types());
        }
        visit_program_node(self, node);
        assert!(
            self.active_nonlocal_writes.is_empty(),
            "INVARIANT VIOLATED: nonlocal write traversal remained active after the program walk. This is a bug because every variable-write entry must have a matching exit. Fix: balance the FactCollector write callbacks for every Prism write-node form."
        );
        self.finalize_all_method_return_equations();
    }

    fn visit_case_match_node(&mut self, node: &CaseMatchNode) {
        let predicate = node.predicate();
        if let Some(predicate) = &predicate {
            self.visit(predicate);
        }

        for condition in node.conditions().iter() {
            let Some(in_node) = condition.as_in_node() else {
                self.visit(&condition);
                continue;
            };

            let pattern = in_node.pattern();
            let captures = predicate
                .as_ref()
                .map(|value| self.pattern_capture_types_for_value(&pattern, value))
                .unwrap_or_default();
            self.pattern_capture_type_stack.push(captures);
            self.visit(&pattern);
            if let Some(statements) = in_node.statements() {
                self.visit(&statements.as_node());
            }
            self.pattern_capture_type_stack.pop().expect(
                "INVARIANT VIOLATED: pattern capture type stack underflow after case/in branch. \
                 This is a bug because each pushed pattern capture frame must be popped exactly once. \
                 Fix: keep FactCollector::visit_case_match_node branch traversal balanced.",
            );
        }

        if let Some(else_clause) = node.else_clause() {
            self.visit(&else_clause.as_node());
        }
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        self.invalidate_escaped_callables_in_call(node);
        self.process_call_node_entry(node);
        let mut prepared_higher_order = None;
        let extension_context = self.pending_block_execution_context.take();
        if let Some(context) = extension_context {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect(
                "INVARIANT VIOLATED: extension execution context was applied to a call without a block. This is a bug because the host must validate the context against the current AST call. Fix: reject execution contexts whose call has no block.",
            );
            assert_eq!(
                context.block_range,
                self.direct_range(&block.location()),
                "INVARIANT VIOLATED: extension execution context block range differs from the traversed block. This is a bug because a guest must not redirect execution semantics to unrelated source. Fix: validate the exact call and block ranges at the extension boundary."
            );
            self.scope_tracker.push_block_execution_context(
                context.implicit_receiver,
                context.implicit_receiver_kind,
                context.method_definition_owner,
                context.method_definition_kind,
            );
            self.block_param_type_stack.push(Vec::new());
            self.visit(&block);
            self.block_param_type_stack.pop().expect(
                "INVARIANT VIOLATED: block parameter type stack underflow after extension execution context. This is a bug because each pushed block type frame must be popped exactly once. Fix: keep FactCollector::visit_call_node extension traversal balanced.",
            );
            self.scope_tracker.pop_execution_context();
        } else if let Some((
            implicit_namespace,
            implicit_kind,
            definition_namespace,
            definition_kind,
        )) = self.static_dynamic_definition_block_context(node)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            let block = node.block().expect(
                "INVARIANT VIOLATED: dynamic-definition block context lost its block. This is a bug because static_dynamic_definition_block_context required the same immutable Prism call to have a block. Fix: keep call traversal and context matching atomic.",
            );
            self.scope_tracker.push_block_execution_context(
                implicit_namespace.clone(),
                implicit_kind,
                definition_namespace,
                definition_kind,
            );
            self.push_direct_dynamic_definition_block_return_type(node, implicit_namespace);
            self.block_param_type_stack.push(Vec::new());
            self.visit(&block);
            self.block_param_type_stack.pop().expect(
                "INVARIANT VIOLATED: block parameter type stack underflow after dynamic-definition block. This is a bug because every pushed block type frame must be popped exactly once. Fix: keep FactCollector::visit_call_node dynamic-definition traversal balanced.",
            );
            self.scope_tracker.pop_execution_context();
        } else if let Some((eval_namespace, implicit_kind, definition_kind)) =
            self.static_eval_block_context(node)
        {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_block_execution_context(
                    eval_namespace.clone(),
                    implicit_kind,
                    eval_namespace,
                    definition_kind,
                );
                self.block_param_type_stack.push(Vec::new());
                self.visit(&block);
                self.block_param_type_stack.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after static eval block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
                self.scope_tracker.pop_execution_context();
            }
        } else if let Some(class_methods_namespace) =
            self.concern_class_methods_block_namespace(node)
        {
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_ns_scopes(class_methods_namespace);
                self.block_param_type_stack.push(Vec::new());
                self.visit(&block);
                self.block_param_type_stack.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after Concern class_methods block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
                self.scope_tracker.pop_ns_scope();
            }
        } else {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                let (block_param_types, prepared) = self.infer_block_param_types_for_call(node);
                prepared_higher_order = prepared;
                self.block_param_type_stack.push(block_param_types);
                let framework_instance_block =
                    crate::is_framework_instance_block_call_name(node.name().as_slice())
                        && node.receiver().is_none();
                if framework_instance_block {
                    self.scope_tracker
                        .push_scope_kind(crate::LocalScopeKind::FrameworkInstanceBlock);
                }
                self.visit(&block);
                if framework_instance_block {
                    self.scope_tracker.pop_scope_kind();
                }
                self.block_param_type_stack.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after call block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
            }
        }
        self.process_nested_receiver_call_reference_candidate(node);
        self.record_call_expression_type(node, prepared_higher_order);
        self.record_current_method_forwarded_yield_types(node);
        self.process_call_node_exit(node);
    }

    fn visit_yield_node(&mut self, node: &YieldNode) {
        self.record_current_method_yield_types(node);
        visit_yield_node(self, node);
    }

    fn visit_forwarding_super_node(&mut self, node: &ForwardingSuperNode) {
        self.process_forwarding_super_node_entry(node);
        visit_forwarding_super_node(self, node);
    }

    fn visit_super_node(&mut self, node: &SuperNode) {
        self.process_super_node_entry(node);
        visit_super_node(self, node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        if !self.process_module_node_entry(node) {
            visit_module_node(self, node);
            return;
        }
        visit_module_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        if !self.process_class_node_entry(node) {
            visit_class_node(self, node);
            return;
        }
        visit_class_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_class_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        let namespace = self.scope_tracker.get_ns_stack();
        self.finalize_method_return_equations_for_namespace(&namespace);
        self.process_singleton_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        if !self.process_def_node_entry(node) {
            visit_def_node(self, node);
            return;
        }
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_alias_method_node(&mut self, node: &AliasMethodNode) {
        self.process_alias_method_node_entry(node);
        visit_alias_method_node(self, node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
        self.process_constant_write_node_exit(node);
    }

    fn visit_constant_or_write_node(&mut self, node: &ConstantOrWriteNode) {
        self.process_constant_or_write_node_entry(node);
        visit_constant_or_write_node(self, node);
        self.process_constant_or_write_node_exit(node);
    }

    fn visit_constant_and_write_node(&mut self, node: &ConstantAndWriteNode) {
        self.process_constant_and_write_node_entry(node);
        visit_constant_and_write_node(self, node);
        self.process_constant_and_write_node_exit(node);
    }

    fn visit_constant_operator_write_node(&mut self, node: &ConstantOperatorWriteNode) {
        self.process_constant_operator_write_node_entry(node);
        visit_constant_operator_write_node(self, node);
        self.process_constant_operator_write_node_exit(node);
    }

    fn visit_constant_target_node(&mut self, node: &ConstantTargetNode) {
        self.process_constant_target_node_entry(node);
        visit_constant_target_node(self, node);
        self.process_constant_target_node_exit(node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode) {
        self.process_constant_path_write_node_entry(node);
        visit_constant_path_write_node(self, node);
        self.process_constant_path_write_node_exit(node);
    }

    fn visit_constant_path_or_write_node(&mut self, node: &ConstantPathOrWriteNode) {
        self.process_constant_path_or_write_node_entry(node);
        visit_constant_path_or_write_node(self, node);
        self.process_constant_path_or_write_node_exit(node);
    }

    fn visit_constant_path_and_write_node(&mut self, node: &ConstantPathAndWriteNode) {
        self.process_constant_path_and_write_node_entry(node);
        visit_constant_path_and_write_node(self, node);
        self.process_constant_path_and_write_node_exit(node);
    }

    fn visit_constant_path_operator_write_node(&mut self, node: &ConstantPathOperatorWriteNode) {
        self.process_constant_path_operator_write_node_entry(node);
        visit_constant_path_operator_write_node(self, node);
        self.process_constant_path_operator_write_node_exit(node);
    }

    fn visit_multi_write_node(&mut self, node: &MultiWriteNode) {
        // Visit the RHS first so expression/method-return facts exist, then
        // push positional element types for ConstantTarget consumers.
        self.visit(&node.value());
        let element_types = self.multi_write_element_types(&node.value());
        self.multi_write_lhs_types.push(element_types);
        for target in node.lefts().iter() {
            self.visit(&target);
        }
        if let Some(rest) = node.rest() {
            self.visit(&rest);
        }
        for target in node.rights().iter() {
            self.visit(&target);
        }
        self.multi_write_lhs_types.pop().expect(
            "INVARIANT VIOLATED: multi-write LHS type stack underflow. \
             This is a bug because each MultiWriteNode push must be balanced by one pop. \
             Fix: keep FactCollector::visit_multi_write_node stack frames paired.",
        );
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write_node_entry(node);
        visit_local_variable_write_node(self, node);
        self.process_local_variable_write_node_exit(node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_target_node_entry(node);
        visit_local_variable_target_node(self, node);
        self.process_local_variable_target_node_exit(node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_or_write_node_entry(node);
        visit_local_variable_or_write_node(self, node);
        self.process_local_variable_or_write_node_exit(node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        self.process_local_variable_and_write_node_entry(node);
        visit_local_variable_and_write_node(self, node);
        self.process_local_variable_and_write_node_exit(node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        self.process_local_variable_operator_write_node_entry(node);
        visit_local_variable_operator_write_node(self, node);
        self.process_local_variable_operator_write_node_exit(node);
    }

    fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'_>) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write_node_entry(node);
        visit_class_variable_write_node(self, node);
        self.process_class_variable_write_node_exit(node);
    }

    fn visit_class_variable_read_node(&mut self, node: &ClassVariableReadNode) {
        self.process_class_variable_read_node_entry(node);
        visit_class_variable_read_node(self, node);
    }

    fn visit_class_variable_target_node(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_target_node_entry(node);
        visit_class_variable_target_node(self, node);
        self.process_class_variable_target_node_exit(node);
    }

    fn visit_class_variable_or_write_node(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_or_write_node_entry(node);
        visit_class_variable_or_write_node(self, node);
        self.process_class_variable_or_write_node_exit(node);
    }

    fn visit_class_variable_and_write_node(&mut self, node: &ClassVariableAndWriteNode) {
        self.process_class_variable_and_write_node_entry(node);
        visit_class_variable_and_write_node(self, node);
        self.process_class_variable_and_write_node_exit(node);
    }

    fn visit_class_variable_operator_write_node(&mut self, node: &ClassVariableOperatorWriteNode) {
        self.process_class_variable_operator_write_node_entry(node);
        visit_class_variable_operator_write_node(self, node);
        self.process_class_variable_operator_write_node_exit(node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write_node_entry(node);
        visit_instance_variable_write_node(self, node);
        self.process_instance_variable_write_node_exit(node);
    }

    fn visit_instance_variable_read_node(&mut self, node: &InstanceVariableReadNode) {
        self.process_instance_variable_read_node_entry(node);
        visit_instance_variable_read_node(self, node);
    }

    fn visit_instance_variable_target_node(&mut self, node: &InstanceVariableTargetNode) {
        self.process_instance_variable_target_node_entry(node);
        visit_instance_variable_target_node(self, node);
        self.process_instance_variable_target_node_exit(node);
    }

    fn visit_instance_variable_or_write_node(&mut self, node: &InstanceVariableOrWriteNode) {
        self.process_instance_variable_or_write_node_entry(node);
        visit_instance_variable_or_write_node(self, node);
        self.process_instance_variable_or_write_node_exit(node);
    }

    fn visit_instance_variable_and_write_node(&mut self, node: &InstanceVariableAndWriteNode) {
        self.process_instance_variable_and_write_node_entry(node);
        visit_instance_variable_and_write_node(self, node);
        self.process_instance_variable_and_write_node_exit(node);
    }

    fn visit_instance_variable_operator_write_node(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_operator_write_node_entry(node);
        visit_instance_variable_operator_write_node(self, node);
        self.process_instance_variable_operator_write_node_exit(node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write_node_entry(node);
        visit_global_variable_write_node(self, node);
        self.process_global_variable_write_node_exit(node);
    }

    fn visit_global_variable_read_node(&mut self, node: &GlobalVariableReadNode) {
        self.process_global_variable_read_node_entry(node);
        visit_global_variable_read_node(self, node);
    }

    fn visit_global_variable_target_node(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_target_node_entry(node);
        visit_global_variable_target_node(self, node);
        self.process_global_variable_target_node_exit(node);
    }

    fn visit_global_variable_or_write_node(&mut self, node: &GlobalVariableOrWriteNode) {
        self.process_global_variable_or_write_node_entry(node);
        visit_global_variable_or_write_node(self, node);
        self.process_global_variable_or_write_node_exit(node);
    }

    fn visit_global_variable_and_write_node(&mut self, node: &GlobalVariableAndWriteNode) {
        self.process_global_variable_and_write_node_entry(node);
        visit_global_variable_and_write_node(self, node);
        self.process_global_variable_and_write_node_exit(node);
    }

    fn visit_global_variable_operator_write_node(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_operator_write_node_entry(node);
        visit_global_variable_operator_write_node(self, node);
        self.process_global_variable_operator_write_node_exit(node);
    }
}

#[derive(Default)]
struct EscapedCallableReadCollector {
    names: HashSet<String>,
}

impl<'pr> Visit<'pr> for EscapedCallableReadCollector {
    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode<'pr>) {
        self.names
            .insert(String::from_utf8_lossy(node.name().as_slice()).to_string());
    }

    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        if let Some(receiver) = node.receiver() {
            let direct_invoke = node.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                self.visit(&receiver);
            }
        }
        if let Some(arguments) = node.arguments() {
            self.visit_arguments_node(&arguments);
        }
        if node
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            self.visit(node.block().as_ref().expect("checked block presence"));
        }
    }
}

#[cfg(test)]
mod execution_context_tests {
    use super::*;
    use crate::core::{GeneratedOwnerId, GraphNodeKind, SourceKind, TypeProvenance};
    use crate::engine::{FileFacts, ResolveMode, SourceFileInput};
    use std::path::PathBuf;
    use url::Url;

    #[derive(Debug)]
    struct SyntheticExecutionContextHost {
        owner: RubyConstant,
    }

    #[test]
    fn nested_shape_invalidation_installs_a_fail_closed_local_read() {
        let source = r#"def collect(condition)
  entry = { id: 1 }
  entries = [entry]
  if condition
    dynamic_sink(entry)
  end
  entries
end
"#;
        let uri = Url::parse("file:///workspace/lib/collection.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/collection.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let start = u32::try_from(source.rfind("entries\nend").unwrap()).unwrap();
        let range = TextRange::new(
            file_id,
            start,
            start + u32::try_from("entries".len()).unwrap(),
        );
        assert!(
            collector
                .local_read_type_evidence()
                .iter()
                .any(|(candidate, ruby_type)| *candidate == range
                    && *ruby_type == RubyType::Array(vec![RubyType::Unknown])),
            "the safe outer Array constructor remains available internally"
        );
        assert!(
            collector
                .inference_evidence()
                .expression_unknown_reasons
                .contains(&(range, UnknownReason::MutableShapeInvalidated)),
            "the exact expression remains fail-closed for public inference"
        );
    }

    #[test]
    fn direct_expression_range_index_preserves_latest_provenance_and_type_deduplication() {
        let source = "1";
        let uri = Url::parse("file:///workspace/lib/value.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/value.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());
        let program = parse.node().as_program_node().unwrap();
        let node = program.statements().body().iter().next().unwrap();
        let range = collector.direct_range(&node.location());

        collector.direct_push_expression_type(&node, RubyType::string(), TypeProvenance::Runtime);
        collector.direct_push_expression_type(
            &node,
            RubyType::integer(),
            TypeProvenance::Assignment,
        );
        collector.direct_push_expression_type(&node, RubyType::string(), TypeProvenance::Literal);

        assert_eq!(
            collector.direct_expression_fact_indexes[&range].len(),
            2,
            "a repeated type at one range must keep the existing direct-fact deduplication rule"
        );
        assert_eq!(
            collector
                .direct_expression_fact(range, None)
                .map(|fact| fact.ruby_type.clone()),
            Some(RubyType::integer()),
            "the unfiltered lookup must select the latest appended expression fact"
        );
        assert_eq!(
            collector
                .direct_expression_fact(range, Some(TypeProvenance::Runtime))
                .map(|fact| fact.ruby_type.clone()),
            Some(RubyType::string()),
            "a provenance-specific lookup must retain the latest matching fact"
        );
    }

    impl FactCollectorExtensionHost for SyntheticExecutionContextHost {
        fn process_call_node(&self, visitor: &mut FactCollector, node: &CallNode) -> bool {
            if node.name().as_slice() != b"describe" {
                return false;
            }
            let block = node.block().expect("test describe call must have a block");
            visitor.set_pending_block_execution_context(BlockExecutionContext {
                block_range: visitor.direct_range(&block.location()),
                implicit_receiver: vec![self.owner],
                implicit_receiver_kind: NamespaceKind::Instance,
                method_definition_owner: vec![self.owner],
                method_definition_kind: NamespaceKind::Instance,
            });
            true
        }
    }

    #[test]
    fn attr_macros_use_the_method_definition_context_in_direct_facts() {
        let source = "class User\n  attr_accessor :name\n  class << self\n    attr_reader :count\n  end\nend\n";
        let uri = Url::parse("file:///workspace/lib/user.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/user.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine.clone(),
        );
        let parse = ruby_prism::parse(source.as_bytes());
        collector.visit(&parse.node());

        let user = vec![RubyConstant::new("User").unwrap()];
        let owner_for = |name: &str| {
            collector
                .direct_facts
                .methods
                .iter()
                .find(|fact| fact.fqn.name() == name)
                .unwrap_or_else(|| panic!("expected direct attr method `{name}`"))
                .owner
                .clone()
        };
        assert_eq!(
            owner_for("name"),
            FullyQualifiedName::namespace_with_kind(user.clone(), NamespaceKind::Instance),
            "an ordinary class-body attr reader must be instance-owned"
        );
        assert_eq!(
            owner_for("name="),
            FullyQualifiedName::namespace_with_kind(user.clone(), NamespaceKind::Instance),
            "an ordinary class-body attr writer must be instance-owned"
        );
        assert_eq!(
            owner_for("count"),
            FullyQualifiedName::namespace_with_kind(user, NamespaceKind::Singleton),
            "an attr reader inside class << self must remain singleton-owned"
        );
    }

    #[test]
    fn extension_context_rehomes_block_method_without_changing_lexical_namespace() {
        let source = "module Lexical\n  describe do\n    def helper\n    end\n    helper\n    VALUE\n  end\nend\n";
        let uri = Url::parse("file:///workspace/spec/context_spec.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/spec/context_spec.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let owner = RubyConstant::generated_owner(
            GeneratedOwnerId::new("test-extension", uri.as_str(), "group:1:2")
                .expect("test generated owner must be valid"),
        );
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(SyntheticExecutionContextHost { owner }),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());
        collector.visit(&parse.node());

        let helper = collector
            .direct_facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "helper")
            .expect("helper definition must be collected");
        assert_eq!(helper.owner.namespace_parts(), vec![owner]);
        assert!(
            collector.direct_facts.methods.iter().all(|fact| {
                fact.fqn.name() != "helper"
                    || fact.owner.namespace_parts() != vec![RubyConstant::new("Lexical").unwrap()]
            }),
            "execution-owned method must not also leak onto the lexical module"
        );
        assert_eq!(
            collector.scope_tracker.get_ns_stack(),
            Vec::<RubyConstant>::new(),
            "all lexical and execution frames must be balanced after traversal"
        );
    }

    #[test]
    fn local_receiver_inference_uses_the_active_lexical_scope() {
        let source = "class User\nend\nouter = User.new\n2.times do\n  outer.save\n  inner = \"value\"\n  1.times do\n    inner.upcase\n  end\nend\n";
        let uri = Url::parse("file:///workspace/lib/local_receiver.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/local_receiver.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine.clone(),
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let method_owner = |name: &str| {
            collector
                .reference_candidates
                .iter()
                .find_map(|candidate| match &candidate.kind {
                    crate::core::ReferenceCandidateKind::Method { owner, method, .. }
                        if method.as_str() == name =>
                    {
                        Some(owner.iter().map(ToString::to_string).collect::<Vec<_>>())
                    }
                    crate::core::ReferenceCandidateKind::Constant { .. }
                    | crate::core::ReferenceCandidateKind::Method { .. }
                    | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
                })
                .unwrap_or_else(|| panic!("expected a method reference candidate for {name}"))
        };

        assert_eq!(method_owner("save"), vec!["User"]);
        assert_eq!(method_owner("upcase"), vec!["String"]);
        assert_eq!(
            collector
                .document
                .variable_scopes()
                .scope_owner_scan_count_for_test(),
            0,
            "fact collection already owns the active lexical scope and must not scan every scope and variable to rediscover it"
        );
    }

    #[test]
    fn ordinary_block_records_unknown_for_an_implicit_receiver() {
        let source = "class Processor\n  def label = \"lexical\"\n  def run\n    configure do\n      label\n    end\n  end\nend\n";
        let uri = Url::parse("file:///workspace/lib/processor.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/processor.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let label_start = u32::try_from(source.rfind("label").unwrap()).unwrap();
        let label_range = TextRange::new(file_id, label_start, label_start + 5);
        assert!(
            collector.reference_candidates.iter().all(|candidate| {
                !matches!(
                    &candidate.kind,
                    crate::core::ReferenceCandidateKind::Method {
                        method,
                        call_expression_range,
                        ..
                    } if method.as_str() == "label" && *call_expression_range == Some(label_range)
                )
            }),
            "an unproven implicit receiver retained a deferred method candidate: {:?}",
            collector.reference_candidates
        );
        assert_eq!(
            collector
                .call_expression_outcomes
                .iter()
                .find(|(range, _)| *range == label_range)
                .map(|(_, outcome)| outcome.unknown_reason()),
            Some(Some(UnknownReason::UnknownReceiver))
        );
    }

    #[test]
    fn nested_value_constant_receiver_preserves_its_proven_type() {
        let source = "ARGV.first.upcase\n";
        let uri = Url::parse("file:///workspace/lib/argv.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let core_file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/embedded/core/constants.rbs"),
            content: "ARGV: Array[String]\n".to_string(),
            kind: SourceKind::Signature,
        });
        let argv = FullyQualifiedName::constant(vec![RubyConstant::new("ARGV").unwrap()]);
        engine.replace_facts(
            core_file_id,
            FileFacts {
                symbols: vec![SymbolFact::new(
                    argv.clone(),
                    SymbolKind::Constant,
                    TextRange::new(core_file_id, 0, 4),
                )],
                types: vec![TypeFact::new(
                    TypeSubject::Constant(argv),
                    RubyType::array_of(RubyType::string()),
                    TextRange::new(core_file_id, 0, 4),
                    TypeProvenance::Rbs,
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/argv.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let (_, outer_outcome) = collector
            .call_expression_outcomes
            .iter()
            .find(|(range, _)| range.start_byte == 0 && range.end_byte == 17)
            .expect("outer ARGV.first.upcase call must retain a type outcome");
        assert_eq!(
            outer_outcome.clone().into_proven_type(),
            Some(RubyType::string()),
            "nested value constants must use their proven value type instead of a guessed class reference"
        );
    }

    #[test]
    fn immediate_hash_literal_keeps_established_generic_read_methods() {
        let source = "{one: 1}.keys\n";
        let uri = Url::parse("file:///workspace/lib/immediate_hash.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/immediate_hash.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine.clone(),
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let range = TextRange::new(file_id, 0, 13);
        let (_, outcome) = collector
            .call_expression_outcomes
            .iter()
            .find(|(outcome_range, _)| *outcome_range == range)
            .expect("the immediate Hash#keys call must retain a proof outcome");
        assert_eq!(
            outcome.clone().into_proven_type(),
            Some(RubyType::array_of(RubyType::symbol())),
            "an immediate Hash literal has no pre-existing alias and may retain its established generic Hash read result"
        );

        engine.write().replace_facts(
            file_id,
            FileFacts {
                inference: collector.inference_evidence(),
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        let engine = engine.read();
        let query = crate::engine::AnalysisQuery::new(&engine);
        assert_eq!(
            query
                .call_expression_outcome_at_position(file_id, 10)
                .and_then(|outcome| outcome.proven_type().cloned()),
            Some(RubyType::array_of(RubyType::symbol())),
            "installing file-owned inference evidence must preserve the immediate Hash#keys proof"
        );
    }

    #[test]
    fn terminal_unknown_receiver_does_not_retain_the_rest_of_a_call_chain() {
        let source = "def normalize(value)\n  value.first.upcase\nend\n";
        let uri = Url::parse("file:///workspace/lib/terminal_unknown.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/terminal_unknown.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let retained = collector
            .reference_candidates
            .iter()
            .filter_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method { method, .. }
                    if matches!(method.as_str(), "first" | "upcase") =>
                {
                    Some(method.as_str())
                }
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Method { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .collect::<Vec<_>>();
        assert!(
            retained.is_empty(),
            "an untyped parameter makes `first` terminal Unknown, so `upcase` cannot become provable after complete graph resolution"
        );
    }

    #[test]
    fn potentially_provable_nested_calls_are_retained_inner_first() {
        let source = "Factory.build.name\nclass Product\n  def name\n    \"value\"\n  end\nend\nclass Factory\n  def self.build\n    Product.new\n  end\nend\n";
        let uri = Url::parse("file:///workspace/lib/deferred_chain.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/deferred_chain.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let retained = collector
            .reference_candidates
            .iter()
            .filter_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method {
                    method,
                    call_expression_range,
                    ..
                } if matches!(method.as_str(), "build" | "name") => {
                    Some((method.as_str(), call_expression_range.is_some()))
                }
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Method { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            retained,
            [("build", true), ("name", true)],
            "the inner call must resolve before its retained outer consumer"
        );
    }

    #[test]
    fn local_receiver_inference_does_not_borrow_an_assignment_from_another_method() {
        let source = "class User\nend\ndef inspect(user)\n  user.save\nend\ndef build\n  user = User.new\nend\n";
        let uri = Url::parse("file:///workspace/lib/source_order.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/source_order.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let save_owners = collector
            .reference_candidates
            .iter()
            .filter_map(|candidate| match &candidate.kind {
                crate::core::ReferenceCandidateKind::Method { owner, method, .. }
                    if method.as_str() == "save" =>
                {
                    Some(owner.iter().map(ToString::to_string).collect::<Vec<_>>())
                }
                crate::core::ReferenceCandidateKind::Constant { .. }
                | crate::core::ReferenceCandidateKind::Method { .. }
                | crate::core::ReferenceCandidateKind::Resolved { .. } => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            save_owners,
            Vec::<Vec<String>>::new(),
            "an untyped `user` parameter must not borrow `user = User.new` from a different method through a whole-file text scan"
        );
    }

    #[test]
    fn recovered_invalid_namespace_does_not_unbalance_an_enclosing_method_context() {
        let source = "def outer\n  def self.forName(module, name); end\nend\n";
        let uri = Url::parse("file:///workspace/lib/recovered.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/recovered.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        assert_eq!(
            collector.scope_tracker.get_ns_stack(),
            Vec::<RubyConstant>::new()
        );
        assert!(!collector.scope_tracker.execution_context_active());
    }

    #[test]
    fn shared_known_namespaces_are_immutable_while_file_declarations_stay_local() {
        let shared_namespace =
            FullyQualifiedName::namespace(vec![RubyConstant::new("Shared").unwrap()]);
        let local_namespace =
            FullyQualifiedName::namespace(vec![RubyConstant::new("Local").unwrap()]);
        let shared = Arc::new(HashSet::from([shared_namespace.clone()]));
        let source = "class Local\nend\n";
        let uri = Url::parse("file:///workspace/lib/local.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/local.rb"),
            content: source.to_string(),
            kind: SourceKind::Gem,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document =
            RubyDocument::with_analysis_file_id(uri.clone(), source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine.clone(),
        )
        .with_shared_direct_known_namespaces(shared.clone());
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        assert_eq!(
            collector.direct_resolve_namespace(&[RubyConstant::new("Shared").unwrap()], true),
            Some(shared_namespace),
            "the immutable batch snapshot must participate in direct lookup"
        );
        assert_eq!(
            collector.direct_resolve_namespace(&[RubyConstant::new("Local").unwrap()], true),
            Some(local_namespace),
            "declarations from the current file must remain directly visible"
        );
        assert_eq!(
            shared.len(),
            1,
            "file-local declarations must not mutate the shared batch snapshot"
        );

        let other_file_id = engine.write().register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/other.rb"),
            content: String::new(),
            kind: SourceKind::Gem,
        });
        let other_document = RubyDocument::with_analysis_file_id(
            Url::parse("file:///workspace/lib/other.rb").unwrap(),
            String::new(),
            0,
            other_file_id,
        );
        let other_collector = FactCollector::analysis_only(
            other_document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        )
        .with_shared_direct_known_namespaces(shared);
        assert_eq!(
            other_collector.direct_resolve_namespace(&[RubyConstant::new("Local").unwrap()], true),
            None,
            "one file's declarations must not leak into another file's local overlay"
        );
    }

    #[test]
    fn qualified_class_superclass_uses_predeclaration_lexical_context() {
        let source = "class BigDecimal\n  def to_s\n    \"base\"\n  end\nend\n\nmodule SitemapGenerator\nend\n\nclass SitemapGenerator::BigDecimal < BigDecimal\n  alias_method :original_to_s, :to_s\nend\n";
        let uri = Url::parse("file:///workspace/core_ext/big_decimal.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/core_ext/big_decimal.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let source = FullyQualifiedName::namespace(vec![
            RubyConstant::new("SitemapGenerator").unwrap(),
            RubyConstant::new("BigDecimal").unwrap(),
        ]);
        let target = FullyQualifiedName::namespace(vec![RubyConstant::new("BigDecimal").unwrap()]);
        assert!(
            collector
                .direct_facts
                .graph_edges
                .iter()
                .any(|edge| edge.kind == GraphEdgeKind::Superclass
                    && edge.source == source
                    && edge.target == target),
            "the qualified class must inherit the pre-existing lexical BigDecimal"
        );
        assert!(
            collector
                .direct_facts
                .graph_edges
                .iter()
                .all(|edge| edge.kind != GraphEdgeKind::Superclass
                    || edge.source != source
                    || edge.target != source),
            "declaring the class must not make it its own superclass"
        );
    }

    #[test]
    fn class_reindex_against_existing_class_reference_still_emits_graph_node() {
        let source = "class PlatformApp < Object\nend\n";
        let uri = Url::parse("file:///workspace/lib/api_app.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/api_app.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let platform_app =
            FullyQualifiedName::namespace(vec![RubyConstant::new("PlatformApp").unwrap()]);
        let constant =
            FullyQualifiedName::constant(vec![RubyConstant::new("PlatformApp").unwrap()]);
        // Prior didOpen / earlier pass left the ordinary class ClassReference in the engine.
        engine.replace_facts(
            file_id,
            FileFacts {
                graph_nodes: vec![GraphNodeFact::new(
                    platform_app.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 5),
                )],
                types: vec![TypeFact::new(
                    TypeSubject::Constant(constant.clone()),
                    RubyType::ClassReference(constant),
                    TextRange::new(file_id, 0, 5),
                    TypeProvenance::Inferred,
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());
        collector.visit(&parse.node());

        assert!(
            collector
                .direct_facts
                .graph_nodes
                .iter()
                .any(|fact| fact.fqn == platform_app && fact.kind == GraphNodeKind::Class),
            "recollecting class PlatformApp while its ClassReference remains visible must still emit the class graph node; nodes={:?}",
            collector
                .direct_facts
                .graph_nodes
                .iter()
                .map(|fact| fact.fqn.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            collector.direct_facts.graph_edges.iter().any(|edge| {
                edge.kind == GraphEdgeKind::Superclass && edge.source == platform_app
            }) || collector
                .direct_facts
                .unresolved_graph_edges
                .iter()
                .any(|edge| {
                    edge.kind == GraphEdgeKind::Superclass && edge.source == platform_app
                }),
            "superclass edge must still be emitted for the class declaration"
        );
    }

    #[test]
    fn class_reopening_through_a_constant_alias_keeps_the_original_owner_identity() {
        let source = "module Types\n\
                          class Original\n\
                          end\n\
                          Alias = Original\n\
                          class Alias\n\
                            def from_alias\n\
                            end\n\
                          end\n\
                        end\n";
        let uri = Url::parse("file:///workspace/lib/constant_alias.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/constant_alias.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let method = collector
            .direct_facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "from_alias")
            .expect("method in aliased class reopening must be collected");
        assert_eq!(
            method.owner,
            FullyQualifiedName::namespace(vec![
                RubyConstant::new("Types").unwrap(),
                RubyConstant::new("Original").unwrap(),
            ]),
            "class Alias must reopen the class object stored in Alias"
        );
        assert!(
            collector.direct_facts.graph_nodes.iter().all(|fact| {
                fact.fqn
                    != FullyQualifiedName::namespace(vec![
                        RubyConstant::new("Types").unwrap(),
                        RubyConstant::new("Alias").unwrap(),
                    ])
            }),
            "a value constant alias must not become a second class identity"
        );
    }

    #[test]
    fn explicit_subclass_does_not_reopen_an_alias_as_its_own_superclass() {
        let source = "class StringScanner\n\
                      end\n\
                      module Sass\n\
                        module Util\n\
                        end\n\
                      end\n\
                      Sass::Util::MultibyteStringScanner = StringScanner\n\
                      class Sass::Util::MultibyteStringScanner < StringScanner\n\
                        def wrapped_string\n\
                          string\n\
                        end\n\
                      end\n";
        let uri = Url::parse("file:///workspace/sass/multibyte_string_scanner.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/sass/multibyte_string_scanner.rb"),
            content: source.to_string(),
            kind: SourceKind::Gem,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let parse = ruby_prism::parse(source.as_bytes());

        collector.visit(&parse.node());

        let subclass = FullyQualifiedName::namespace(vec![
            RubyConstant::new("Sass").unwrap(),
            RubyConstant::new("Util").unwrap(),
            RubyConstant::new("MultibyteStringScanner").unwrap(),
        ]);
        let string_scanner =
            FullyQualifiedName::namespace(vec![RubyConstant::new("StringScanner").unwrap()]);
        let method = collector
            .direct_facts
            .methods
            .iter()
            .find(|fact| fact.fqn.name() == "wrapped_string")
            .expect("method in the explicit subclass must be collected");

        assert_eq!(
            method.owner, subclass,
            "an alias cannot reopen its target when that would make the target inherit itself"
        );
        assert!(
            collector
                .direct_facts
                .graph_edges
                .iter()
                .any(|edge| edge.kind == GraphEdgeKind::Superclass
                    && edge.source == subclass
                    && edge.target == string_scanner),
            "the feasible explicit subclass branch must retain its superclass"
        );
        assert!(
            collector
                .direct_facts
                .graph_edges
                .iter()
                .all(|edge| edge.kind != GraphEdgeKind::Superclass
                    || edge.source != string_scanner
                    || edge.target != string_scanner),
            "the flow-insensitive alias must not create StringScanner < StringScanner"
        );
    }

    #[test]
    fn local_graph_edge_validation_rejects_cycles_and_conflicting_superclasses() {
        let source = "";
        let uri = Url::parse("file:///workspace/lib/invalid_inheritance.rb").unwrap();
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from("/workspace/lib/invalid_inheritance.rb"),
            content: source.to_string(),
            kind: SourceKind::Project,
        });
        let engine = Arc::new(RwLock::new(engine));
        let document = RubyDocument::with_analysis_file_id(uri, source.to_string(), 0, file_id);
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            engine,
        );
        let range = TextRange::new(file_id, 0, 0);
        let a = FullyQualifiedName::namespace(vec![RubyConstant::new("A").unwrap()]);
        let b = FullyQualifiedName::namespace(vec![RubyConstant::new("B").unwrap()]);
        let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);

        assert!(collector.direct_push_resolved_edge(
            a.clone(),
            b.clone(),
            GraphEdgeKind::Include,
            range,
        ));
        assert!(
            !collector.direct_push_resolved_edge(
                b.clone(),
                a.clone(),
                GraphEdgeKind::Include,
                range,
            ),
            "the edge that closes a local ancestry cycle must be rejected"
        );
        assert!(collector.direct_push_resolved_edge(
            child.clone(),
            a.clone(),
            GraphEdgeKind::Superclass,
            range,
        ));
        assert!(
            !collector.direct_push_resolved_edge(
                child.clone(),
                b.clone(),
                GraphEdgeKind::Superclass,
                range,
            ),
            "a second distinct local superclass must be rejected"
        );

        assert_eq!(
            collector
                .analysis_diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.as_str())
                .collect::<Vec<_>>(),
            vec!["cyclic-inheritance", "conflicting-superclass"]
        );
        assert_eq!(
            collector.direct_facts.graph_edges.len(),
            2,
            "only the two valid ancestry edges may become same-pass semantic input"
        );
    }
}
