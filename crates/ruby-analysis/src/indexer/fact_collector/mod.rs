use crate::core::method_store::{MethodVisibility, MethodVisibilityOverrideFact};
use crate::core::{
    DiagnosticCandidate, DiagnosticFact, DiagnosticSeverity, FullyQualifiedName, GraphEdgeFact,
    GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact, MethodParamFact, NamespaceKind,
    ReferenceCandidate, RubyConstant, RubyMethod, SymbolFact, SymbolKind, TextRange, TypeFact,
    TypeProvenance, TypeStore, TypeSubject, UnresolvedGraphEdgeFact,
};
use crate::engine::AnalysisEngine;
use once_cell::unsync::OnceCell;
use ruby_fast_lsp_extension_api::{IndexPatch, Receiver, ResolvedCall, SourceRange};
use ruby_prism::*;

use super::AnalysisIndex;
use crate::inference::r#type::literal::LiteralAnalyzer;
use crate::inference::RubyType;
use crate::yard::parser::{CommentLineInfo, YardParser};
use crate::RubyDocument;
use crate::{collect_namespaces, control_flow, utf8_str, ScopeTracker};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
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

pub struct FactCollector {
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub literal_analyzer: LiteralAnalyzer,
    pub analysis_diagnostics: Vec<DiagnosticFact>,
    pub type_store: TypeStore,
    pub extension_call_stack: Vec<ruby_fast_lsp_extension_api::ResolvedCall>,
    pub extension_call_stack_marks: Vec<bool>,
    pub extension_index_patches: Vec<IndexPatch>,
    pub extension_host: Arc<dyn FactCollectorExtensionHost>,
    pub analysis_engine: Arc<RwLock<AnalysisEngine>>,
    pub include_local_vars: bool,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub variable_types: OnceCell<HashMap<String, RubyType>>,
    pub resolve_analysis_method_returns: bool,
    pub infer_expression_receivers: bool,
    pub diagnostics_enabled: bool,
    pub direct_facts: AnalysisIndex,
    pub block_param_type_stack: Vec<Vec<RubyType>>,
    pub pattern_capture_type_stack: Vec<HashMap<String, RubyType>>,
    pub yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,
    pub proc_return_types_by_local: HashMap<String, RubyType>,
    direct_known_namespaces: HashSet<FullyQualifiedName>,
}

pub trait FactCollectorExtensionHost: std::fmt::Debug + Send + Sync {
    fn process_call_node(&self, _visitor: &mut FactCollector, _node: &CallNode) {}

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
            resolved_callees: Vec::new(),
            call_range,
            message_range,
        }
    }
}

#[derive(Debug)]
pub struct NullFactCollectorExtensionHost;

impl FactCollectorExtensionHost for NullFactCollectorExtensionHost {}

impl FactCollector {
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

    pub fn body_lsp_location(
        &self,
        body_location: Option<ruby_prism::Location>,
        node_location: &ruby_prism::Location,
    ) -> tower_lsp::lsp_types::Location {
        if let Some(body_location) = body_location {
            self.document.prism_location_to_lsp_location(&body_location)
        } else {
            self.document.prism_location_to_lsp_location(node_location)
        }
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
        Self {
            document,
            scope_tracker,
            literal_analyzer: LiteralAnalyzer::new(),
            analysis_diagnostics: Vec::new(),
            type_store: TypeStore::new(),
            extension_call_stack: Vec::new(),
            extension_call_stack_marks: Vec::new(),
            extension_index_patches: Vec::new(),
            extension_host,
            analysis_engine,
            include_local_vars: true,
            reference_candidates: Vec::new(),
            diagnostic_candidates: Vec::new(),
            variable_types: OnceCell::new(),
            resolve_analysis_method_returns: true,
            infer_expression_receivers: true,
            diagnostics_enabled: true,
            direct_facts: AnalysisIndex::default(),
            block_param_type_stack: Vec::new(),
            pattern_capture_type_stack: Vec::new(),
            yield_param_types_by_method: HashMap::new(),
            proc_return_types_by_local: HashMap::new(),
            direct_known_namespaces: HashSet::new(),
        }
    }

    pub fn without_analysis_method_return_resolution(mut self) -> Self {
        self.resolve_analysis_method_returns = false;
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

    fn static_eval_block_namespace(&self, node: &CallNode) -> Option<Vec<RubyConstant>> {
        if !matches!(node.name().as_slice(), b"class_eval" | b"module_eval") {
            return None;
        }
        node.block()?;
        let receiver = node.receiver()?;
        let eval_ref = crate::mixin_ref_from_node(&receiver)?;
        self.resolve_static_eval_namespace(&eval_ref.parts, eval_ref.absolute)
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
        self.direct_push_namespace_facts(target_fqn, GraphNodeKind::Module, range);
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
            return self
                .direct_known_namespaces
                .contains(&fqn)
                .then(|| parts.to_vec())
                .and_then(|candidate| namespace_suffix_for_push(&current_namespace, &candidate));
        }

        let mut search = current_namespace.clone();
        loop {
            let mut candidate = search.clone();
            candidate.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.direct_known_namespaces.contains(&fqn) {
                return namespace_suffix_for_push(&current_namespace, &candidate);
            }
            if search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.direct_known_namespaces
            .contains(&fqn)
            .then(|| parts.to_vec())
            .and_then(|candidate| namespace_suffix_for_push(&current_namespace, &candidate))
    }

    pub fn direct_range(&self, location: &ruby_prism::Location<'_>) -> TextRange {
        TextRange::new(
            self.document.analysis_file_id(),
            u32_offset(location.start_offset()),
            u32_offset(location.end_offset()),
        )
    }

    pub fn direct_push_namespace_facts(
        &mut self,
        fqn: FullyQualifiedName,
        kind: GraphNodeKind,
        range: TextRange,
    ) {
        self.direct_known_namespaces.insert(fqn.clone());
        self.direct_facts.symbols.push(SymbolFact::new(
            fqn.clone(),
            match kind {
                GraphNodeKind::Class => SymbolKind::Class,
                GraphNodeKind::Module => SymbolKind::Module,
            },
            range,
        ));
        self.direct_facts
            .graph_nodes
            .push(GraphNodeFact::new(fqn.clone(), kind, range));
        self.direct_facts.types.push(TypeFact::new(
            TypeSubject::Constant(FullyQualifiedName::constant(fqn.namespace_parts())),
            match kind {
                GraphNodeKind::Class => RubyType::ClassReference(fqn.clone()),
                GraphNodeKind::Module => RubyType::ModuleReference(fqn.clone()),
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
        let mut search = if absolute {
            Vec::new()
        } else {
            self.scope_tracker.get_ns_stack()
        };

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(probe);
            if self.direct_known_namespaces.contains(&fqn) {
                return Some(fqn);
            }
            if absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(parts.to_vec());
        self.direct_known_namespaces.contains(&fqn).then_some(fqn)
    }

    pub fn direct_push_edge(
        &mut self,
        source: FullyQualifiedName,
        parts: &[RubyConstant],
        absolute: bool,
        kind: GraphEdgeKind,
        range: TextRange,
    ) {
        let Some(target) = self.direct_resolve_namespace(parts, absolute) else {
            self.direct_facts
                .unresolved_graph_edges
                .push(UnresolvedGraphEdgeFact::new(
                    source,
                    parts.to_vec(),
                    absolute,
                    FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack()),
                    kind,
                    range,
                ));
            return;
        };
        self.direct_facts
            .graph_edges
            .push(GraphEdgeFact::new(source, target, kind, range));
    }

    pub fn direct_push_method_fact(
        &mut self,
        namespace: Vec<RubyConstant>,
        owner_kind: NamespaceKind,
        method: RubyMethod,
        range: TextRange,
        params: Vec<MethodParamFact>,
    ) {
        let fqn = FullyQualifiedName::method(namespace.clone(), method);
        let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
        self.direct_facts
            .symbols
            .push(SymbolFact::new(fqn.clone(), SymbolKind::Method, range));
        self.direct_facts.methods.push(
            MethodFact::with_param_facts(fqn, owner, range, params)
                .with_visibility(self.scope_tracker.current_visibility()),
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
        for fact in &mut self.direct_facts.methods {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                continue;
            };
            if *fact_method == method && fact.owner == owner {
                fact.visibility = visibility;
            }
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
        if ruby_type == RubyType::Unknown {
            return;
        }
        self.direct_facts.types.push(TypeFact::new(
            subject,
            ruby_type,
            self.direct_range(location),
            TypeProvenance::Assignment,
        ));
    }

    pub fn infer_variable_type_cached(&self, var_name: &str) -> Option<RubyType> {
        let map = self
            .variable_types
            .get_or_init(|| build_variable_type_map(&self.document.content));
        map.get(var_name).cloned()
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
        if let Some(statements) = value_node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .map(|node| self.infer_type_from_value_with_locals(&node, local_types))
                .unwrap_or_else(RubyType::nil_class);
        }
        if let Some(array_node) = value_node.as_array_node() {
            return self.infer_array_type_from_elements(&array_node);
        }
        if let Some(hash_node) = value_node.as_hash_node() {
            return self.infer_hash_type_from_elements(&hash_node);
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

        // 2. Constant read: `User` → ClassReference(User)
        if let Some(const_node) = value_node.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_node.name().as_slice()).to_string();
            if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
                return RubyType::ClassReference(fqn);
            }
        }

        // 3. Constant path: `MyApp::User` → ClassReference(MyApp::User)
        if let Some(path_node) = value_node.as_constant_path_node() {
            if let Some(path_str) = Self::flatten_constant_path(&path_node) {
                if let Ok(fqn) = FullyQualifiedName::try_from(path_str.as_str()) {
                    return RubyType::ClassReference(fqn);
                }
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
                2.. => RubyType::Array(
                    args.iter()
                        .map(|arg| self.infer_type_from_value_with_locals(arg, local_types))
                        .collect(),
                ),
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

        // 5. Method call: recursively infer receiver type, then resolve method
        if let Some(call_node) = value_node.as_call_node() {
            if let Some(const_get_type) = self.const_get_reference_type(&call_node) {
                return const_get_type;
            }
            if let Some(block_return_type) =
                self.infer_yielding_block_return_type_for_call(&call_node)
            {
                return block_return_type;
            }

            let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();
            if let Some(proc_return_type) = self.infer_proc_call_return_type(&call_node) {
                return proc_return_type;
            }

            // Determine receiver type
            let receiver_type = if let Some(receiver) = call_node.receiver() {
                // Has explicit receiver - recursively infer its type
                self.infer_type_from_value_with_locals(&receiver, local_types)
            } else {
                // No receiver (implicit self) - use current class/module context
                let namespace = self.scope_tracker.get_ns_stack();
                if namespace.is_empty() {
                    return RubyType::Unknown;
                }
                let current_fqn = FullyQualifiedName::namespace(namespace);
                RubyType::Class(current_fqn)
            };

            if receiver_type == RubyType::Unknown {
                return RubyType::Unknown;
            }

            // Special case: `.new` on a ClassReference returns an instance
            if method_name == "new" {
                if let RubyType::ClassReference(fqn) = &receiver_type {
                    return RubyType::Class(fqn.clone());
                }
            }

            if let Some(return_type) = self.resolve_method_return_type_with_private(
                &receiver_type,
                &method_name,
                call_node.receiver().is_none(),
            ) {
                return return_type;
            }
        }

        RubyType::Unknown
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
        if let Some(lambda) = value_node.as_lambda_node() {
            return self.infer_proc_body(lambda.body());
        }

        let call = value_node.as_call_node()?;
        if call.name().as_slice() != b"new" {
            return None;
        }
        let receiver = call.receiver()?;
        let constant = receiver.as_constant_read_node()?;
        if constant.name().as_slice() != b"Proc" {
            return None;
        }
        let block = call.block()?.as_block_node()?;
        self.infer_proc_body(block.body())
    }

    fn infer_proc_body(&self, body: Option<Node<'_>>) -> Option<RubyType> {
        let return_type = body
            .map(|body| self.infer_type_from_value(&body))
            .unwrap_or_else(RubyType::nil_class);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    fn infer_proc_call_return_type(&self, call_node: &CallNode<'_>) -> Option<RubyType> {
        if call_node.name().as_slice() != b"call" {
            return None;
        }
        let receiver = call_node.receiver()?;
        let local = receiver.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
        self.proc_return_types_by_local
            .get(&name)
            .filter(|ty| **ty != RubyType::Unknown)
            .cloned()
    }

    fn infer_array_type_from_elements(&self, array_node: &ArrayNode<'_>) -> RubyType {
        let mut element_types = array_node
            .elements()
            .iter()
            .map(|element| self.infer_type_from_value(&element))
            .collect::<Vec<_>>();
        if element_types.is_empty() {
            return RubyType::Array(vec![RubyType::Unknown]);
        }
        normalize_type_list(&mut element_types);
        RubyType::Array(element_types)
    }

    fn infer_hash_type_from_elements(&self, hash_node: &HashNode<'_>) -> RubyType {
        let mut key_types = Vec::new();
        let mut value_types = Vec::new();
        for element in hash_node.elements().iter() {
            let Some(assoc) = element.as_assoc_node() else {
                key_types.push(RubyType::Unknown);
                value_types.push(RubyType::Unknown);
                continue;
            };
            key_types.push(self.infer_type_from_value(&assoc.key()));
            value_types.push(self.infer_type_from_value(&assoc.value()));
        }
        if key_types.is_empty() {
            key_types.push(RubyType::Unknown);
        }
        if value_types.is_empty() {
            value_types.push(RubyType::Unknown);
        }
        normalize_type_list(&mut key_types);
        normalize_type_list(&mut value_types);
        RubyType::Hash(key_types, value_types)
    }

    pub fn infer_assignment_type_from_value(&self, value_node: &Node) -> RubyType {
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

    fn infer_block_param_types_for_call(&self, node: &CallNode<'_>) -> Vec<RubyType> {
        if node.block().is_none() {
            return Vec::new();
        }
        if let Some(yield_param_types) = self.infer_block_param_types_from_yielding_method(node) {
            return yield_param_types;
        }
        let Some(receiver) = node.receiver() else {
            return Vec::new();
        };

        let method_name = node.name().as_slice();
        let param_count = block_required_param_count(node);
        let receiver_type = self.infer_type_from_value(&receiver);

        match receiver_type {
            RubyType::Array(element_types)
                if matches!(
                    method_name,
                    b"each"
                        | b"map"
                        | b"collect"
                        | b"select"
                        | b"filter"
                        | b"reject"
                        | b"find"
                        | b"detect"
                        | b"any?"
                        | b"all?"
                        | b"none?"
                ) =>
            {
                vec![RubyType::union(element_types)]
            }
            RubyType::Array(element_types) if method_name == b"each_with_index" => {
                vec![RubyType::union(element_types), RubyType::integer()]
            }
            RubyType::Hash(key_types, value_types) if method_name == b"each" => {
                let key_type = RubyType::union(key_types);
                let value_type = RubyType::union(value_types);
                if param_count == 1 {
                    vec![RubyType::Array(vec![key_type, value_type])]
                } else {
                    vec![key_type, value_type]
                }
            }
            RubyType::Class(_)
            | RubyType::Module(_)
            | RubyType::ClassReference(_)
            | RubyType::ModuleReference(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Union(_)
            | RubyType::Unknown => Vec::new(),
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

    fn record_call_expression_type(&mut self, node: &CallNode<'_>) {
        let Some(return_type) = self
            .infer_yielding_block_return_type_for_call(node)
            .or_else(|| self.infer_proc_call_return_type(node))
        else {
            return;
        };
        let range = self.direct_range(&node.location());
        let fact = TypeFact::new(
            TypeSubject::Expression(range),
            return_type,
            range,
            TypeProvenance::Inferred,
        );
        self.type_store.add(fact.clone());
        self.direct_facts.types.push(fact);
    }

    fn constant_reference_type(&self, node: &Node) -> Option<FullyQualifiedName> {
        if let Some(const_node) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(const_node.name().as_slice()).to_string();
            let constant = RubyConstant::new(&name).ok()?;
            return Some(FullyQualifiedName::constant(vec![constant]));
        }
        if let Some(path_node) = node.as_constant_path_node() {
            let mut parts = Vec::new();
            collect_namespaces(&path_node, &mut parts);
            if parts.is_empty() {
                None
            } else {
                Some(FullyQualifiedName::constant(parts))
            }
        } else {
            None
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
        self.direct_facts
            .types
            .iter()
            .filter(|fact| {
                fact.subject == TypeSubject::Constant(constant_fqn.clone())
                    && fact.ruby_type != RubyType::Unknown
            })
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| fact.ruby_type.clone())
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

        // Use VariableScopes tree for type lookup
        let scope_id = self
            .document
            .variable_scopes()
            .find_scope_for_variable_at(var_name, file_id, byte_offset)
            .or_else(|| {
                self.document
                    .variable_scopes()
                    .scope_at_position(file_id, byte_offset)
            })?;

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

    /// Helper to flatten a ConstantPathNode into a string (e.g., "Module::Class")
    fn flatten_constant_path(node: &ConstantPathNode) -> Option<String> {
        let mut parts = Vec::new();
        collect_namespaces(node, &mut parts);

        if parts.is_empty() {
            None
        } else {
            // Join parts with "::"
            let path_str = parts
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("::");
            Some(path_str)
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

fn source_range(visitor: &FactCollector, location: &ruby_prism::Location) -> SourceRange {
    let range = visitor.document.prism_location_to_lsp_range(location);
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

fn build_variable_type_map(content: &str) -> HashMap<String, RubyType> {
    let mut map: HashMap<String, RubyType> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(eq_idx) = trimmed.find('=') else {
            continue;
        };
        let lhs = trimmed[..eq_idx].trim();
        let mut chars = lhs.chars();
        match chars.next() {
            Some(c) if c.is_lowercase() || c == '_' => {}
            Some(_) | None => continue,
        }
        if !lhs.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rhs_full = trimmed[eq_idx + 1..].trim();
        let Some(new_pos) = rhs_full.find(".new") else {
            continue;
        };
        let class_part = rhs_full[..new_pos].trim();
        if !class_part.chars().next().is_some_and(|c| c.is_uppercase()) {
            continue;
        }
        let parts: Vec<_> = class_part
            .split("::")
            .filter_map(|s| RubyConstant::new(s.trim()).ok())
            .collect();
        if parts.is_empty() {
            continue;
        }
        map.entry(lhs.to_string())
            .or_insert_with(|| RubyType::Class(FullyQualifiedName::constant(parts)));
    }
    map
}

fn namespace_suffix_for_push(
    current: &[RubyConstant],
    target: &[RubyConstant],
) -> Option<Vec<RubyConstant>> {
    if current.is_empty() {
        return Some(target.to_vec());
    }
    target
        .starts_with(current)
        .then(|| target[current.len()..].to_vec())
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

fn normalize_type_list(types: &mut Vec<RubyType>) {
    types.sort_by_key(|ty| format!("{ty:?}"));
    types.dedup();
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
        self.process_call_node_entry(node);
        if let Some(eval_namespace) = self.static_eval_block_namespace(node) {
            if let Some(receiver) = node.receiver() {
                self.visit(&receiver);
            }
            if let Some(arguments) = node.arguments() {
                self.visit_arguments_node(&arguments);
            }
            if let Some(block) = node.block() {
                self.scope_tracker.push_ns_scopes(eval_namespace);
                self.block_param_type_stack.push(Vec::new());
                self.visit(&block);
                self.block_param_type_stack.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after static eval block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
                self.scope_tracker.pop_ns_scope();
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
                let block_param_types = self.infer_block_param_types_for_call(node);
                self.block_param_type_stack.push(block_param_types);
                self.visit(&block);
                self.block_param_type_stack.pop().expect(
                    "INVARIANT VIOLATED: block parameter type stack underflow after call block. \
                     This is a bug because each pushed block type frame must be popped exactly once. \
                     Fix: keep FactCollector::visit_call_node block traversal balanced.",
                );
            }
        }
        self.record_call_expression_type(node);
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
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        self.process_singleton_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
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

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode) {
        self.process_constant_path_write_node_entry(node);
        visit_constant_path_write_node(self, node);
        self.process_constant_path_write_node_exit(node);
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
