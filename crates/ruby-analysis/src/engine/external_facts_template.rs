use crate::core::memory_estimate::{
    fqn_heap_bytes, ruby_type_heap_bytes, string_heap_bytes, type_subject_heap_bytes,
    vec_payload_bytes,
};
use crate::core::{
    CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind, CallableBodySummary,
    ConstantCallableBodyFact, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact,
    GraphNodeKind, LiteralKey, LiteralValue, MethodAvailability, MethodFact, MethodParamFact,
    MethodParamKind, MethodVisibilityOverrideFact, NamespaceKind, RubyConstant, RubyMethod,
    RubyType, ShapeExactness, ShapeField, ShapeFieldPresence, ShapeRest, ShapeStability, ShapeType,
    SourceFileId, SymbolFact, SymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use crate::engine::FileFacts;
use crate::method_store::MethodVisibility;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Immutable external semantic facts whose source identity must be rebound
/// before insertion into an isolated engine.
///
/// The wrapped facts are deliberately private: callers cannot accidentally
/// insert template-owned file IDs into an engine. This first cacheable slice
/// excludes reference, diagnostic, and execution-context facts because those
/// carry project/query/extension policy rather than project-neutral dependency
/// declarations. File-local symbols, expression/local types, flow evidence,
/// and local-read evidence are intentionally excluded: they cannot affect a
/// different file and an interactively opened dependency is reprocessed
/// through the ordinary file-owned lifecycle.
#[derive(Debug, Clone)]
pub struct ProjectNeutralFileFactsTemplate {
    source_file_id: SourceFileId,
    facts: FileFacts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectNeutralTemplateRejection {
    ProjectSpecificFacts,
    ForeignRange {
        expected: SourceFileId,
        actual: SourceFileId,
    },
}

/// Version-independent, file-identity-free representation of one validated
/// project-neutral fact template.
///
/// This is a persistence DTO, not another semantic store. Every string and
/// range is reconstructed through the ordinary Ruby domain constructors before
/// the snapshot can become a `ProjectNeutralFileFactsTemplate` again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectNeutralFileFactsSnapshot {
    symbols: Vec<SnapshotSymbolFact>,
    methods: Vec<SnapshotMethodFact>,
    method_visibility_overrides: Vec<SnapshotMethodVisibilityOverrideFact>,
    types: Vec<SnapshotTypeFact>,
    graph_nodes: Vec<SnapshotGraphNodeFact>,
    graph_edges: Vec<SnapshotGraphEdgeFact>,
    unresolved_graph_edges: Vec<SnapshotUnresolvedGraphEdgeFact>,
    constant_callable_bodies: Vec<SnapshotConstantCallableBodyFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotConstantCallableBodyFact {
    constant: SnapshotFqn,
    summary: SnapshotCallableBodySummary,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotCallableBodySummary {
    strict_arity: bool,
    parameters: Vec<SnapshotCallableBodyParameter>,
    captures: Vec<String>,
    result: SnapshotCallableBodyExpression,
    node_count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotCallableBodyParameter {
    name: String,
    kind: SnapshotCallableBodyParameterKind,
    default: Option<SnapshotCallableBodyExpression>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotCallableBodyParameterKind {
    Required,
    Optional,
    Rest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotCallableBodyExpression {
    Literal(SnapshotRubyType),
    Parameter(usize),
    Capture(String),
    Array(Vec<SnapshotCallableBodyExpression>),
    Shape(Vec<(SnapshotLiteral, SnapshotCallableBodyExpression)>),
    Call {
        receiver: Box<SnapshotCallableBodyExpression>,
        method: String,
        arguments: Vec<SnapshotCallableBodyExpression>,
        literal_argument_keys: Vec<Option<SnapshotLiteral>>,
    },
    ExhaustiveUnion(Vec<SnapshotCallableBodyExpression>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SnapshotRange {
    start_byte: u32,
    end_byte: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotFqn {
    Namespace { parts: Vec<String>, singleton: bool },
    Constant { parts: Vec<String> },
    Method { parts: Vec<String>, name: String },
    LocalVariable { name: String },
    InstanceVariable { name: String },
    ClassVariable { name: String },
    GlobalVariable { name: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotSymbolKind {
    Class,
    Module,
    Method,
    Constant,
    LocalVariable,
    InstanceVariable,
    ClassVariable,
    GlobalVariable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotSymbolFact {
    fqn: SnapshotFqn,
    kind: SnapshotSymbolKind,
    name_range: SnapshotRange,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotMethodParamKind {
    Required,
    Optional,
    Rest,
    RequiredKeyword,
    OptionalKeyword,
    KeywordRest,
    Block,
    Forwarding,
    AnonymousRest,
    AnonymousKeywordRest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotMethodVisibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotMethodAvailability {
    Available,
    Unavailable { reason: String },
    Absent { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMethodParamFact {
    name: String,
    kind: SnapshotMethodParamKind,
    type_label: Option<String>,
    documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotCallableTypeTemplate {
    Concrete(SnapshotRubyType),
    Receiver,
    Variable(String),
    Array(Box<SnapshotCallableTypeTemplate>),
    Hash(
        Box<SnapshotCallableTypeTemplate>,
        Box<SnapshotCallableTypeTemplate>,
    ),
    Union(Vec<SnapshotCallableTypeTemplate>),
    Unconstrained,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotCallableParameterTemplate {
    kind: SnapshotMethodParamKind,
    ruby_type: SnapshotCallableTypeTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotCallableBlockTemplate {
    parameters: Vec<SnapshotCallableTypeTemplate>,
    return_type: SnapshotCallableTypeTemplate,
    required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotCallableSignature {
    receiver_type_parameters: Vec<String>,
    type_parameters: Vec<String>,
    parameters: Vec<SnapshotCallableParameterTemplate>,
    block: SnapshotCallableBlockTemplate,
    return_type: SnapshotCallableTypeTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotForwardedBlockCall {
    receiver_parameter: String,
    method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotDirectYieldCall {
    parameter_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMethodFact {
    fqn: SnapshotFqn,
    owner: SnapshotFqn,
    range: SnapshotRange,
    name_range: SnapshotRange,
    params: Vec<String>,
    param_facts: Vec<SnapshotMethodParamFact>,
    parameter_shape_complete: bool,
    delegate_receiver: Option<String>,
    visibility: SnapshotMethodVisibility,
    availability: SnapshotMethodAvailability,
    documentation: Option<String>,
    return_type_label: Option<String>,
    callable_signatures: Vec<SnapshotCallableSignature>,
    forwarded_block_call: Option<SnapshotForwardedBlockCall>,
    direct_yield_call: Option<SnapshotDirectYieldCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMethodVisibilityOverrideFact {
    owner: SnapshotFqn,
    method: String,
    visibility: SnapshotMethodVisibility,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotRubyType {
    Class {
        fqn: SnapshotFqn,
    },
    Module {
        fqn: SnapshotFqn,
    },
    ClassReference {
        fqn: SnapshotFqn,
    },
    ModuleReference {
        fqn: SnapshotFqn,
    },
    Literal {
        value: SnapshotLiteral,
    },
    Array {
        elements: Vec<SnapshotRubyType>,
    },
    Hash {
        keys: Vec<SnapshotRubyType>,
        values: Vec<SnapshotRubyType>,
    },
    Shape {
        fields: Vec<SnapshotShapeField>,
        rest: Option<Box<SnapshotShapeRest>>,
        exactness: SnapshotShapeExactness,
        stability: SnapshotShapeStability,
    },
    Union {
        types: Vec<SnapshotRubyType>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotLiteral {
    Symbol(String),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotShapeField {
    key: SnapshotLiteral,
    value: SnapshotRubyType,
    presence: SnapshotShapeFieldPresence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotShapeRest {
    key: SnapshotRubyType,
    value: SnapshotRubyType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotShapeFieldPresence {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotShapeExactness {
    Exact,
    Open,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotShapeStability {
    TrackedMutable,
    Frozen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotTypeSubject {
    Constant { fqn: SnapshotFqn },
    Local { scope_id: u32, name: String },
    InstanceVariable { owner: SnapshotFqn, name: String },
    ClassVariable { owner: SnapshotFqn, name: String },
    GlobalVariable { name: String },
    MethodReturn { fqn: SnapshotFqn },
    Parameter { method: SnapshotFqn, name: String },
    Expression { range: SnapshotRange },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotTypeProvenance {
    Literal,
    Assignment,
    Flow,
    Rbs,
    Yard,
    Runtime,
    Extension,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotTypeFact {
    subject: SnapshotTypeSubject,
    ruby_type: SnapshotRubyType,
    range: SnapshotRange,
    provenance: SnapshotTypeProvenance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotGraphNodeKind {
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotGraphEdgeKind {
    Superclass,
    Include,
    Prepend,
    Extend,
    ExecutionContextApplication,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotGraphNodeFact {
    fqn: SnapshotFqn,
    kind: SnapshotGraphNodeKind,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotGraphEdgeFact {
    source: SnapshotFqn,
    target: SnapshotFqn,
    kind: SnapshotGraphEdgeKind,
    provenance: SnapshotGraphEdgeProvenance,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotUnresolvedGraphEdgeFact {
    source: SnapshotFqn,
    target_parts: Vec<String>,
    absolute: bool,
    context: SnapshotFqn,
    kind: SnapshotGraphEdgeKind,
    provenance: SnapshotGraphEdgeProvenance,
    range: SnapshotRange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotGraphEdgeProvenance {
    Explicit,
    ImplicitObject,
}

impl fmt::Display for ProjectNeutralTemplateRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectSpecificFacts => formatter.write_str(
                "file facts contain project-specific references, diagnostics, or execution contexts",
            ),
            Self::ForeignRange { expected, actual } => write!(
                formatter,
                "file facts contain range owned by {actual:?}; expected {expected:?}"
            ),
        }
    }
}

impl std::error::Error for ProjectNeutralTemplateRejection {}

impl ProjectNeutralFileFactsTemplate {
    pub fn try_new(
        source_file_id: SourceFileId,
        mut facts: FileFacts,
    ) -> Result<Self, ProjectNeutralTemplateRejection> {
        retain_project_neutral_declaration_facts(&mut facts);
        if !facts.reference_candidates.is_empty()
            || !facts.diagnostic_candidates.is_empty()
            || !facts.diagnostics.is_empty()
            || !facts.execution_contexts.is_empty()
            || declaration_facts_have_generated_owner(&facts)
        {
            return Err(ProjectNeutralTemplateRejection::ProjectSpecificFacts);
        }

        for fact in &facts.symbols {
            validate_range(fact.range, source_file_id)?;
            validate_range(fact.name_range, source_file_id)?;
        }
        for fact in &facts.methods {
            validate_range(fact.range, source_file_id)?;
            validate_range(fact.name_range, source_file_id)?;
        }
        for fact in &facts.method_visibility_overrides {
            validate_range(fact.range, source_file_id)?;
        }
        for fact in &facts.types {
            validate_range(fact.range, source_file_id)?;
            match &fact.subject {
                TypeSubject::Expression(range) => validate_range(*range, source_file_id)?,
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::MethodReturn(_)
                | TypeSubject::Parameter { .. } => {}
            }
        }
        for fact in &facts.graph_nodes {
            validate_range(fact.range, source_file_id)?;
        }
        for fact in &facts.graph_edges {
            validate_range(fact.range, source_file_id)?;
        }
        for fact in &facts.unresolved_graph_edges {
            validate_range(fact.range, source_file_id)?;
        }
        for fact in &facts.inference.constant_callable_bodies {
            validate_range(fact.range, source_file_id)?;
        }

        Ok(Self {
            source_file_id,
            facts,
        })
    }

    pub fn instantiate(&self, target_file_id: SourceFileId) -> FileFacts {
        let mut facts = self.facts.clone();
        rebind_all_ranges(&mut facts, self.source_file_id, target_file_id);
        facts
    }

    pub fn into_instantiated(mut self, target_file_id: SourceFileId) -> FileFacts {
        rebind_all_ranges(&mut self.facts, self.source_file_id, target_file_id);
        self.facts
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        let facts = &self.facts;
        vec_payload_bytes(&facts.symbols)
            + facts
                .symbols
                .iter()
                .map(|fact| fqn_heap_bytes(&fact.fqn))
                .sum::<usize>()
            + vec_payload_bytes(&facts.methods)
            + facts
                .methods
                .iter()
                .map(|fact| {
                    fqn_heap_bytes(&fact.fqn)
                        + fqn_heap_bytes(&fact.owner)
                        + vec_payload_bytes(&fact.params)
                        + fact.params.iter().map(string_heap_bytes).sum::<usize>()
                        + vec_payload_bytes(&fact.param_facts)
                        + fact
                            .param_facts
                            .iter()
                            .map(|parameter| {
                                string_heap_bytes(&parameter.name)
                                    + parameter
                                        .type_label
                                        .as_ref()
                                        .map(string_heap_bytes)
                                        .unwrap_or(0)
                                    + parameter
                                        .documentation
                                        .as_ref()
                                        .map(string_heap_bytes)
                                        .unwrap_or(0)
                            })
                            .sum::<usize>()
                        + match &fact.availability {
                            MethodAvailability::Available => 0,
                            MethodAvailability::Unavailable { reason }
                            | MethodAvailability::Absent { reason } => string_heap_bytes(reason),
                        }
                        + fact
                            .documentation
                            .as_ref()
                            .map(string_heap_bytes)
                            .unwrap_or(0)
                        + fact
                            .return_type_label
                            .as_ref()
                            .map(string_heap_bytes)
                            .unwrap_or(0)
                })
                .sum::<usize>()
            + vec_payload_bytes(&facts.method_visibility_overrides)
            + facts
                .method_visibility_overrides
                .iter()
                .map(|fact| fqn_heap_bytes(&fact.owner))
                .sum::<usize>()
            + vec_payload_bytes(&facts.types)
            + facts
                .types
                .iter()
                .map(|fact| {
                    type_subject_heap_bytes(&fact.subject) + ruby_type_heap_bytes(&fact.ruby_type)
                })
                .sum::<usize>()
            + vec_payload_bytes(&facts.graph_nodes)
            + facts
                .graph_nodes
                .iter()
                .map(|fact| fqn_heap_bytes(&fact.fqn))
                .sum::<usize>()
            + vec_payload_bytes(&facts.graph_edges)
            + facts
                .graph_edges
                .iter()
                .map(|fact| fqn_heap_bytes(&fact.source) + fqn_heap_bytes(&fact.target))
                .sum::<usize>()
            + vec_payload_bytes(&facts.unresolved_graph_edges)
            + facts
                .unresolved_graph_edges
                .iter()
                .map(|fact| {
                    fqn_heap_bytes(&fact.source)
                        + vec_payload_bytes(&fact.target_parts)
                        + fqn_heap_bytes(&fact.context)
                })
                .sum::<usize>()
            + facts.inference.estimated_heap_bytes()
    }

    pub fn to_persistent_snapshot(&self) -> Result<ProjectNeutralFileFactsSnapshot, String> {
        snapshot_declaration_facts(&self.facts)
    }

    pub fn try_from_persistent_snapshot(
        snapshot: ProjectNeutralFileFactsSnapshot,
    ) -> Result<Self, String> {
        let source_file_id = SourceFileId(0);
        let facts = restore_declaration_facts(snapshot, source_file_id)?;
        Self::try_new(source_file_id, facts).map_err(|error| error.to_string())
    }
}

fn retain_project_neutral_declaration_facts(facts: &mut FileFacts) {
    facts
        .symbols
        .retain(|fact| fact.kind != SymbolKind::LocalVariable);
    facts.types.retain(|fact| {
        !matches!(
            &fact.subject,
            TypeSubject::Local { .. } | TypeSubject::Expression(_)
        )
    });
    facts
        .inference
        .constant_callable_bodies
        .retain(|fact| fact.summary.is_capture_free());
    let constant_callable_bodies = std::mem::take(&mut facts.inference.constant_callable_bodies);
    facts.inference = Default::default();
    facts.inference.constant_callable_bodies = constant_callable_bodies;
    facts.local_read_types = Default::default();
}

fn fqn_has_generated_owner(fqn: &FullyQualifiedName) -> bool {
    fqn.namespace_parts_slice()
        .iter()
        .any(RubyConstant::is_generated_owner)
}

fn ruby_type_has_generated_owner(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Class(fqn)
        | RubyType::Module(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::ModuleReference(fqn) => fqn_has_generated_owner(fqn),
        RubyType::Array(elements) | RubyType::Union(elements) => {
            elements.iter().any(ruby_type_has_generated_owner)
        }
        RubyType::Hash(keys, values) => {
            keys.iter().any(ruby_type_has_generated_owner)
                || values.iter().any(ruby_type_has_generated_owner)
        }
        RubyType::Shape(shape) => {
            shape
                .fields()
                .iter()
                .any(|field| ruby_type_has_generated_owner(field.value()))
                || shape.rest().is_some_and(|rest| {
                    ruby_type_has_generated_owner(rest.key())
                        || ruby_type_has_generated_owner(rest.value())
                })
        }
        RubyType::Literal(_) | RubyType::Unknown => false,
    }
}

fn type_subject_has_generated_owner(subject: &TypeSubject) -> bool {
    match subject {
        TypeSubject::Constant(fqn)
        | TypeSubject::MethodReturn(fqn)
        | TypeSubject::Parameter { method: fqn, .. } => fqn_has_generated_owner(fqn),
        TypeSubject::InstanceVariable { owner, .. } | TypeSubject::ClassVariable { owner, .. } => {
            fqn_has_generated_owner(owner)
        }
        TypeSubject::Local { .. } | TypeSubject::GlobalVariable(_) | TypeSubject::Expression(_) => {
            false
        }
    }
}

fn declaration_facts_have_generated_owner(facts: &FileFacts) -> bool {
    facts
        .symbols
        .iter()
        .any(|fact| fqn_has_generated_owner(&fact.fqn))
        || facts
            .methods
            .iter()
            .any(|fact| fqn_has_generated_owner(&fact.fqn) || fqn_has_generated_owner(&fact.owner))
        || facts
            .method_visibility_overrides
            .iter()
            .any(|fact| fqn_has_generated_owner(&fact.owner))
        || facts.types.iter().any(|fact| {
            type_subject_has_generated_owner(&fact.subject)
                || ruby_type_has_generated_owner(&fact.ruby_type)
        })
        || facts
            .graph_nodes
            .iter()
            .any(|fact| fqn_has_generated_owner(&fact.fqn))
        || facts.graph_edges.iter().any(|fact| {
            fqn_has_generated_owner(&fact.source) || fqn_has_generated_owner(&fact.target)
        })
        || facts.unresolved_graph_edges.iter().any(|fact| {
            fqn_has_generated_owner(&fact.source)
                || fact
                    .target_parts
                    .iter()
                    .any(RubyConstant::is_generated_owner)
                || fqn_has_generated_owner(&fact.context)
        })
        || facts
            .inference
            .constant_callable_bodies
            .iter()
            .any(|fact| fqn_has_generated_owner(&fact.constant))
}

fn snapshot_declaration_facts(
    facts: &FileFacts,
) -> Result<ProjectNeutralFileFactsSnapshot, String> {
    Ok(ProjectNeutralFileFactsSnapshot {
        symbols: facts
            .symbols
            .iter()
            .map(snapshot_symbol)
            .collect::<Result<_, _>>()?,
        methods: facts
            .methods
            .iter()
            .map(snapshot_method)
            .collect::<Result<_, _>>()?,
        method_visibility_overrides: facts
            .method_visibility_overrides
            .iter()
            .map(snapshot_method_visibility_override)
            .collect::<Result<_, _>>()?,
        types: facts
            .types
            .iter()
            .map(snapshot_type_fact)
            .collect::<Result<_, _>>()?,
        graph_nodes: facts
            .graph_nodes
            .iter()
            .map(snapshot_graph_node)
            .collect::<Result<_, _>>()?,
        graph_edges: facts
            .graph_edges
            .iter()
            .map(snapshot_graph_edge)
            .collect::<Result<_, _>>()?,
        unresolved_graph_edges: facts
            .unresolved_graph_edges
            .iter()
            .map(snapshot_unresolved_graph_edge)
            .collect::<Result<_, _>>()?,
        constant_callable_bodies: facts
            .inference
            .constant_callable_bodies
            .iter()
            .map(snapshot_constant_callable_body)
            .collect::<Result<_, _>>()?,
    })
}

fn restore_declaration_facts(
    snapshot: ProjectNeutralFileFactsSnapshot,
    source_file_id: SourceFileId,
) -> Result<FileFacts, String> {
    let mut facts = FileFacts {
        symbols: snapshot
            .symbols
            .into_iter()
            .map(|fact| restore_symbol(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        methods: snapshot
            .methods
            .into_iter()
            .map(|fact| restore_method(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        method_visibility_overrides: snapshot
            .method_visibility_overrides
            .into_iter()
            .map(|fact| restore_method_visibility_override(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        types: snapshot
            .types
            .into_iter()
            .map(|fact| restore_type_fact(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        graph_nodes: snapshot
            .graph_nodes
            .into_iter()
            .map(|fact| restore_graph_node(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        graph_edges: snapshot
            .graph_edges
            .into_iter()
            .map(|fact| restore_graph_edge(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        unresolved_graph_edges: snapshot
            .unresolved_graph_edges
            .into_iter()
            .map(|fact| restore_unresolved_graph_edge(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        ..FileFacts::default()
    };
    facts.inference.constant_callable_bodies = snapshot
        .constant_callable_bodies
        .into_iter()
        .map(|fact| restore_constant_callable_body(fact, source_file_id))
        .collect::<Result<_, _>>()?;
    Ok(facts)
}

fn snapshot_constant_callable_body(
    fact: &ConstantCallableBodyFact,
) -> Result<SnapshotConstantCallableBodyFact, String> {
    if !fact.summary.is_capture_free() {
        return Err("project-neutral callable body unexpectedly retains captures".to_string());
    }
    Ok(SnapshotConstantCallableBodyFact {
        constant: snapshot_fqn(&fact.constant)?,
        summary: snapshot_callable_body_summary(&fact.summary)?,
        range: snapshot_range(fact.range),
    })
}

fn restore_constant_callable_body(
    fact: SnapshotConstantCallableBodyFact,
    file_id: SourceFileId,
) -> Result<ConstantCallableBodyFact, String> {
    let summary = restore_callable_body_summary(fact.summary)?;
    if !summary.is_capture_free() {
        return Err("persistent project-neutral callable body contains captures".to_string());
    }
    Ok(ConstantCallableBodyFact {
        constant: restore_fqn(fact.constant)?,
        summary,
        range: restore_range(fact.range, file_id)?,
    })
}

fn snapshot_callable_body_summary(
    summary: &CallableBodySummary,
) -> Result<SnapshotCallableBodySummary, String> {
    summary
        .validate()
        .map_err(|reason| format!("invalid callable body summary: {}", reason.code()))?;
    Ok(SnapshotCallableBodySummary {
        strict_arity: summary.strict_arity,
        parameters: summary
            .parameters
            .iter()
            .map(|parameter| {
                Ok(SnapshotCallableBodyParameter {
                    name: parameter.name.clone(),
                    kind: match parameter.kind {
                        CallableBodyParameterKind::Required => {
                            SnapshotCallableBodyParameterKind::Required
                        }
                        CallableBodyParameterKind::Optional => {
                            SnapshotCallableBodyParameterKind::Optional
                        }
                        CallableBodyParameterKind::Rest => SnapshotCallableBodyParameterKind::Rest,
                    },
                    default: parameter
                        .default
                        .as_ref()
                        .map(snapshot_callable_body_expression)
                        .transpose()?,
                })
            })
            .collect::<Result<_, String>>()?,
        captures: summary.captures.clone(),
        result: snapshot_callable_body_expression(&summary.result)?,
        node_count: summary.node_count,
    })
}

fn restore_callable_body_summary(
    summary: SnapshotCallableBodySummary,
) -> Result<CallableBodySummary, String> {
    use crate::core::callable_body::{
        MAX_CALLABLE_BODY_CAPTURES, MAX_CALLABLE_BODY_NODES, MAX_CALLABLE_BODY_PARAMETERS,
    };
    if summary.parameters.len() > MAX_CALLABLE_BODY_PARAMETERS
        || summary.captures.len() > MAX_CALLABLE_BODY_CAPTURES
        || usize::from(summary.node_count) > MAX_CALLABLE_BODY_NODES
    {
        return Err("persistent callable body exceeds a fixed proof bound".to_string());
    }
    if !summary.captures.is_empty() {
        return Err("persistent project-neutral callable body contains captures".to_string());
    }
    let summary = CallableBodySummary {
        strict_arity: summary.strict_arity,
        parameters: summary
            .parameters
            .into_iter()
            .map(|parameter| {
                Ok(CallableBodyParameter {
                    name: parameter.name,
                    kind: match parameter.kind {
                        SnapshotCallableBodyParameterKind::Required => {
                            CallableBodyParameterKind::Required
                        }
                        SnapshotCallableBodyParameterKind::Optional => {
                            CallableBodyParameterKind::Optional
                        }
                        SnapshotCallableBodyParameterKind::Rest => CallableBodyParameterKind::Rest,
                    },
                    default: parameter
                        .default
                        .map(|expression| restore_callable_body_expression(expression, 0))
                        .transpose()?,
                })
            })
            .collect::<Result<_, String>>()?,
        captures: summary.captures,
        result: restore_callable_body_expression(summary.result, 0)?,
        node_count: summary.node_count,
    };
    summary
        .validate()
        .map_err(|reason| format!("invalid persistent callable body: {}", reason.code()))?;
    Ok(summary)
}

fn snapshot_callable_body_expression(
    expression: &CallableBodyExpression,
) -> Result<SnapshotCallableBodyExpression, String> {
    Ok(match expression {
        CallableBodyExpression::Literal(ruby_type) => {
            SnapshotCallableBodyExpression::Literal(snapshot_ruby_type(ruby_type)?)
        }
        CallableBodyExpression::Parameter(index) => {
            SnapshotCallableBodyExpression::Parameter(*index)
        }
        CallableBodyExpression::Capture(name) => {
            SnapshotCallableBodyExpression::Capture(name.clone())
        }
        CallableBodyExpression::Array(values) => SnapshotCallableBodyExpression::Array(
            values
                .iter()
                .map(snapshot_callable_body_expression)
                .collect::<Result<_, _>>()?,
        ),
        CallableBodyExpression::Shape(fields) => SnapshotCallableBodyExpression::Shape(
            fields
                .iter()
                .map(|(key, value)| {
                    Ok((
                        snapshot_literal_key(key),
                        snapshot_callable_body_expression(value)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
        CallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => SnapshotCallableBodyExpression::Call {
            receiver: Box::new(snapshot_callable_body_expression(receiver)?),
            method: method.as_str().to_string(),
            arguments: arguments
                .iter()
                .map(snapshot_callable_body_expression)
                .collect::<Result<_, _>>()?,
            literal_argument_keys: literal_argument_keys
                .iter()
                .map(|key| key.as_ref().map(snapshot_literal_key))
                .collect(),
        },
        CallableBodyExpression::ExhaustiveUnion(values) => {
            SnapshotCallableBodyExpression::ExhaustiveUnion(
                values
                    .iter()
                    .map(snapshot_callable_body_expression)
                    .collect::<Result<_, _>>()?,
            )
        }
    })
}

fn restore_callable_body_expression(
    expression: SnapshotCallableBodyExpression,
    depth: usize,
) -> Result<CallableBodyExpression, String> {
    use crate::core::callable_body::{
        MAX_CALLABLE_BODY_TYPE_DEPTH, MAX_CALLABLE_BODY_UNION_VARIANTS,
    };
    if depth > MAX_CALLABLE_BODY_TYPE_DEPTH {
        return Err("persistent callable expression exceeds the fixed type depth".to_string());
    }
    Ok(match expression {
        SnapshotCallableBodyExpression::Literal(ruby_type) => {
            CallableBodyExpression::Literal(restore_ruby_type(ruby_type, 1)?)
        }
        SnapshotCallableBodyExpression::Parameter(index) => {
            CallableBodyExpression::Parameter(index)
        }
        SnapshotCallableBodyExpression::Capture(name) => CallableBodyExpression::Capture(name),
        SnapshotCallableBodyExpression::Array(values) => CallableBodyExpression::Array(
            values
                .into_iter()
                .map(|value| restore_callable_body_expression(value, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        SnapshotCallableBodyExpression::Shape(fields) => CallableBodyExpression::Shape(
            fields
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        restore_literal_key(key),
                        restore_callable_body_expression(value, depth + 1)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
        SnapshotCallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => {
            if arguments.len() != literal_argument_keys.len() {
                return Err("persistent callable call has mismatched argument metadata".to_string());
            }
            CallableBodyExpression::Call {
                receiver: Box::new(restore_callable_body_expression(*receiver, depth + 1)?),
                method: RubyMethod::new(&method).map_err(|error| {
                    format!("invalid persistent callable method `{method}`: {error}")
                })?,
                arguments: arguments
                    .into_iter()
                    .map(|value| restore_callable_body_expression(value, depth + 1))
                    .collect::<Result<_, _>>()?,
                literal_argument_keys: literal_argument_keys
                    .into_iter()
                    .map(|key| key.map(restore_literal_key))
                    .collect(),
            }
        }
        SnapshotCallableBodyExpression::ExhaustiveUnion(values) => {
            if values.len() > MAX_CALLABLE_BODY_UNION_VARIANTS {
                return Err("persistent callable union exceeds the fixed variant bound".to_string());
            }
            CallableBodyExpression::ExhaustiveUnion(
                values
                    .into_iter()
                    .map(|value| restore_callable_body_expression(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            )
        }
    })
}

fn snapshot_range(range: TextRange) -> SnapshotRange {
    SnapshotRange {
        start_byte: range.start_byte,
        end_byte: range.end_byte,
    }
}

fn restore_range(range: SnapshotRange, file_id: SourceFileId) -> Result<TextRange, String> {
    if range.start_byte > range.end_byte {
        return Err(format!(
            "persistent range starts at {} after ending at {}",
            range.start_byte, range.end_byte
        ));
    }
    Ok(TextRange::new(file_id, range.start_byte, range.end_byte))
}

fn snapshot_parts(parts: &[RubyConstant]) -> Result<Vec<String>, String> {
    Ok(parts.iter().map(|part| part.as_str().to_string()).collect())
}

fn restore_parts(parts: Vec<String>) -> Result<Vec<RubyConstant>, String> {
    if parts.len() > 4096 {
        return Err(format!(
            "persistent FQN has {} parts; maximum is 4096",
            parts.len()
        ));
    }
    parts
        .into_iter()
        .map(|part| {
            if part.starts_with('\0') {
                RubyConstant::from_canonical_generated_owner(&part).map_err(|error| {
                    format!("invalid persistent generated owner identity: {error}")
                })
            } else {
                RubyConstant::new(&part)
                    .map_err(|error| format!("invalid persistent Ruby constant `{part}`: {error}"))
            }
        })
        .collect()
}

fn snapshot_fqn(fqn: &FullyQualifiedName) -> Result<SnapshotFqn, String> {
    Ok(match fqn {
        FullyQualifiedName::Namespace(parts, kind) => SnapshotFqn::Namespace {
            parts: snapshot_parts(parts)?,
            singleton: matches!(kind, NamespaceKind::Singleton),
        },
        FullyQualifiedName::Constant(parts) => SnapshotFqn::Constant {
            parts: snapshot_parts(parts)?,
        },
        FullyQualifiedName::Method(parts, method) => SnapshotFqn::Method {
            parts: snapshot_parts(parts)?,
            name: method.as_str().to_string(),
        },
        FullyQualifiedName::LocalVariable(name) => SnapshotFqn::LocalVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::InstanceVariable(name) => SnapshotFqn::InstanceVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::ClassVariable(name) => SnapshotFqn::ClassVariable {
            name: name.as_str().to_string(),
        },
        FullyQualifiedName::GlobalVariable(name) => SnapshotFqn::GlobalVariable {
            name: name.as_str().to_string(),
        },
    })
}

fn restore_fqn(fqn: SnapshotFqn) -> Result<FullyQualifiedName, String> {
    match fqn {
        SnapshotFqn::Namespace { parts, singleton } => Ok(FullyQualifiedName::namespace_with_kind(
            restore_parts(parts)?,
            if singleton {
                NamespaceKind::Singleton
            } else {
                NamespaceKind::Instance
            },
        )),
        SnapshotFqn::Constant { parts } => Ok(FullyQualifiedName::constant(restore_parts(parts)?)),
        SnapshotFqn::Method { parts, name } => Ok(FullyQualifiedName::method(
            restore_parts(parts)?,
            RubyMethod::new(&name)
                .map_err(|error| format!("invalid persistent Ruby method `{name}`: {error}"))?,
        )),
        SnapshotFqn::LocalVariable { name } => FullyQualifiedName::local_variable(name)
            .map_err(|error| format!("invalid persistent local variable: {error}")),
        SnapshotFqn::InstanceVariable { name } => FullyQualifiedName::instance_variable(name)
            .map_err(|error| format!("invalid persistent instance variable: {error}")),
        SnapshotFqn::ClassVariable { name } => FullyQualifiedName::class_variable(name)
            .map_err(|error| format!("invalid persistent class variable: {error}")),
        SnapshotFqn::GlobalVariable { name } => FullyQualifiedName::global_variable(name)
            .map_err(|error| format!("invalid persistent global variable: {error}")),
    }
}

fn snapshot_symbol_kind(kind: SymbolKind) -> SnapshotSymbolKind {
    match kind {
        SymbolKind::Class => SnapshotSymbolKind::Class,
        SymbolKind::Module => SnapshotSymbolKind::Module,
        SymbolKind::Method => SnapshotSymbolKind::Method,
        SymbolKind::Constant => SnapshotSymbolKind::Constant,
        SymbolKind::LocalVariable => SnapshotSymbolKind::LocalVariable,
        SymbolKind::InstanceVariable => SnapshotSymbolKind::InstanceVariable,
        SymbolKind::ClassVariable => SnapshotSymbolKind::ClassVariable,
        SymbolKind::GlobalVariable => SnapshotSymbolKind::GlobalVariable,
    }
}

fn restore_symbol_kind(kind: SnapshotSymbolKind) -> SymbolKind {
    match kind {
        SnapshotSymbolKind::Class => SymbolKind::Class,
        SnapshotSymbolKind::Module => SymbolKind::Module,
        SnapshotSymbolKind::Method => SymbolKind::Method,
        SnapshotSymbolKind::Constant => SymbolKind::Constant,
        SnapshotSymbolKind::LocalVariable => SymbolKind::LocalVariable,
        SnapshotSymbolKind::InstanceVariable => SymbolKind::InstanceVariable,
        SnapshotSymbolKind::ClassVariable => SymbolKind::ClassVariable,
        SnapshotSymbolKind::GlobalVariable => SymbolKind::GlobalVariable,
    }
}

fn snapshot_symbol(fact: &SymbolFact) -> Result<SnapshotSymbolFact, String> {
    Ok(SnapshotSymbolFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        kind: snapshot_symbol_kind(fact.kind),
        name_range: snapshot_range(fact.name_range),
        range: snapshot_range(fact.range),
    })
}

fn restore_symbol(fact: SnapshotSymbolFact, file_id: SourceFileId) -> Result<SymbolFact, String> {
    let range = restore_range(fact.range, file_id)?;
    let name_range = restore_range(fact.name_range, file_id)?;
    validate_contained_range(name_range, range, "symbol name")?;
    Ok(SymbolFact {
        fqn: restore_fqn(fact.fqn)?,
        kind: restore_symbol_kind(fact.kind),
        name_range,
        range,
    })
}

fn snapshot_param_kind(kind: MethodParamKind) -> SnapshotMethodParamKind {
    match kind {
        MethodParamKind::Required => SnapshotMethodParamKind::Required,
        MethodParamKind::Optional => SnapshotMethodParamKind::Optional,
        MethodParamKind::Rest => SnapshotMethodParamKind::Rest,
        MethodParamKind::RequiredKeyword => SnapshotMethodParamKind::RequiredKeyword,
        MethodParamKind::OptionalKeyword => SnapshotMethodParamKind::OptionalKeyword,
        MethodParamKind::KeywordRest => SnapshotMethodParamKind::KeywordRest,
        MethodParamKind::Block => SnapshotMethodParamKind::Block,
        MethodParamKind::Forwarding => SnapshotMethodParamKind::Forwarding,
        MethodParamKind::AnonymousRest => SnapshotMethodParamKind::AnonymousRest,
        MethodParamKind::AnonymousKeywordRest => SnapshotMethodParamKind::AnonymousKeywordRest,
    }
}

fn restore_param_kind(kind: SnapshotMethodParamKind) -> MethodParamKind {
    match kind {
        SnapshotMethodParamKind::Required => MethodParamKind::Required,
        SnapshotMethodParamKind::Optional => MethodParamKind::Optional,
        SnapshotMethodParamKind::Rest => MethodParamKind::Rest,
        SnapshotMethodParamKind::RequiredKeyword => MethodParamKind::RequiredKeyword,
        SnapshotMethodParamKind::OptionalKeyword => MethodParamKind::OptionalKeyword,
        SnapshotMethodParamKind::KeywordRest => MethodParamKind::KeywordRest,
        SnapshotMethodParamKind::Block => MethodParamKind::Block,
        SnapshotMethodParamKind::Forwarding => MethodParamKind::Forwarding,
        SnapshotMethodParamKind::AnonymousRest => MethodParamKind::AnonymousRest,
        SnapshotMethodParamKind::AnonymousKeywordRest => MethodParamKind::AnonymousKeywordRest,
    }
}

fn snapshot_visibility(visibility: MethodVisibility) -> SnapshotMethodVisibility {
    match visibility {
        MethodVisibility::Public => SnapshotMethodVisibility::Public,
        MethodVisibility::Protected => SnapshotMethodVisibility::Protected,
        MethodVisibility::Private => SnapshotMethodVisibility::Private,
    }
}

fn restore_visibility(visibility: SnapshotMethodVisibility) -> MethodVisibility {
    match visibility {
        SnapshotMethodVisibility::Public => MethodVisibility::Public,
        SnapshotMethodVisibility::Protected => MethodVisibility::Protected,
        SnapshotMethodVisibility::Private => MethodVisibility::Private,
    }
}

fn snapshot_availability(availability: &MethodAvailability) -> SnapshotMethodAvailability {
    match availability {
        MethodAvailability::Available => SnapshotMethodAvailability::Available,
        MethodAvailability::Unavailable { reason } => SnapshotMethodAvailability::Unavailable {
            reason: reason.clone(),
        },
        MethodAvailability::Absent { reason } => SnapshotMethodAvailability::Absent {
            reason: reason.clone(),
        },
    }
}

fn restore_availability(
    availability: SnapshotMethodAvailability,
) -> Result<MethodAvailability, String> {
    match availability {
        SnapshotMethodAvailability::Available => Ok(MethodAvailability::Available),
        SnapshotMethodAvailability::Unavailable { reason } => {
            validate_reason(&reason)?;
            Ok(MethodAvailability::Unavailable { reason })
        }
        SnapshotMethodAvailability::Absent { reason } => {
            validate_reason(&reason)?;
            Ok(MethodAvailability::Absent { reason })
        }
    }
}

fn validate_reason(reason: &str) -> Result<(), String> {
    if reason.trim().is_empty() {
        return Err("persistent unavailable method reason is empty".to_string());
    }
    Ok(())
}

fn snapshot_method(fact: &MethodFact) -> Result<SnapshotMethodFact, String> {
    Ok(SnapshotMethodFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        owner: snapshot_fqn(&fact.owner)?,
        range: snapshot_range(fact.range),
        name_range: snapshot_range(fact.name_range),
        params: fact.params.clone(),
        param_facts: fact
            .param_facts
            .iter()
            .map(|parameter| SnapshotMethodParamFact {
                name: parameter.name.clone(),
                kind: snapshot_param_kind(parameter.kind),
                type_label: parameter.type_label.clone(),
                documentation: parameter.documentation.clone(),
            })
            .collect(),
        parameter_shape_complete: fact.parameter_shape_complete,
        delegate_receiver: fact
            .delegate_receiver
            .as_ref()
            .map(|method| method.as_str().to_string()),
        visibility: snapshot_visibility(fact.visibility),
        availability: snapshot_availability(&fact.availability),
        documentation: fact.documentation.clone(),
        return_type_label: fact.return_type_label.clone(),
        callable_signatures: fact
            .callable_signatures()
            .iter()
            .map(snapshot_callable_signature)
            .collect::<Result<Vec<_>, _>>()?,
        forwarded_block_call: fact.forwarded_block_call().map(|forwarded| {
            SnapshotForwardedBlockCall {
                receiver_parameter: forwarded.receiver_parameter.clone(),
                method: forwarded.method.as_str().to_string(),
            }
        }),
        direct_yield_call: fact
            .direct_yield_call()
            .map(|direct| SnapshotDirectYieldCall {
                parameter_names: direct.parameter_names.clone(),
            }),
    })
}

fn restore_method(fact: SnapshotMethodFact, file_id: SourceFileId) -> Result<MethodFact, String> {
    let range = restore_range(fact.range, file_id)?;
    let name_range = restore_range(fact.name_range, file_id)?;
    validate_contained_range(name_range, range, "method name")?;
    let param_facts = fact
        .param_facts
        .into_iter()
        .map(|parameter| {
            Ok(
                MethodParamFact::new(parameter.name, restore_param_kind(parameter.kind))
                    .with_signature_metadata(parameter.type_label, parameter.documentation),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let delegate_receiver = fact
        .delegate_receiver
        .map(|name| {
            RubyMethod::new(&name)
                .map_err(|error| format!("invalid persistent delegate method `{name}`: {error}"))
        })
        .transpose()?;
    let callable_signatures = fact
        .callable_signatures
        .into_iter()
        .map(restore_callable_signature)
        .collect::<Result<Vec<_>, _>>()?;
    let forwarded_block_call = fact
        .forwarded_block_call
        .map(|forwarded| {
            Ok::<crate::core::ForwardedBlockCall, String>(crate::core::ForwardedBlockCall {
                receiver_parameter: forwarded.receiver_parameter,
                method: RubyMethod::new(&forwarded.method).map_err(|error| {
                    format!(
                        "invalid persistent forwarded block method `{}`: {error}",
                        forwarded.method
                    )
                })?,
            })
        })
        .transpose()?;
    let direct_yield_call = fact
        .direct_yield_call
        .map(|direct| crate::core::DirectYieldCall {
            parameter_names: direct.parameter_names,
        });
    Ok(MethodFact {
        fqn: restore_fqn(fact.fqn)?,
        owner: restore_fqn(fact.owner)?,
        range,
        name_range,
        params: fact.params,
        param_facts,
        parameter_shape_complete: fact.parameter_shape_complete,
        delegate_receiver,
        visibility: restore_visibility(fact.visibility),
        availability: restore_availability(fact.availability)?,
        documentation: fact.documentation,
        return_type_label: fact.return_type_label,
        higher_order: None,
    }
    .with_callable_signatures(callable_signatures)
    .with_forwarded_block_call(forwarded_block_call)
    .with_direct_yield_call(direct_yield_call))
}

fn snapshot_callable_signature(
    signature: &crate::core::CallableSignature,
) -> Result<SnapshotCallableSignature, String> {
    Ok(SnapshotCallableSignature {
        receiver_type_parameters: signature.receiver_type_parameters.clone(),
        type_parameters: signature.type_parameters.clone(),
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| {
                Ok(SnapshotCallableParameterTemplate {
                    kind: snapshot_param_kind(parameter.kind),
                    ruby_type: snapshot_callable_template(&parameter.ruby_type)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        block: SnapshotCallableBlockTemplate {
            parameters: signature
                .block
                .parameters
                .iter()
                .map(snapshot_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
            return_type: snapshot_callable_template(&signature.block.return_type)?,
            required: signature.block.required,
        },
        return_type: snapshot_callable_template(&signature.return_type)?,
    })
}

fn snapshot_callable_template(
    template: &crate::core::CallableTypeTemplate,
) -> Result<SnapshotCallableTypeTemplate, String> {
    use crate::core::CallableTypeTemplate;
    Ok(match template {
        CallableTypeTemplate::Concrete(ruby_type) => {
            SnapshotCallableTypeTemplate::Concrete(snapshot_ruby_type(ruby_type)?)
        }
        CallableTypeTemplate::Receiver => SnapshotCallableTypeTemplate::Receiver,
        CallableTypeTemplate::Variable(name) => {
            SnapshotCallableTypeTemplate::Variable(name.clone())
        }
        CallableTypeTemplate::Array(element) => {
            SnapshotCallableTypeTemplate::Array(Box::new(snapshot_callable_template(element)?))
        }
        CallableTypeTemplate::Hash(key, value) => SnapshotCallableTypeTemplate::Hash(
            Box::new(snapshot_callable_template(key)?),
            Box::new(snapshot_callable_template(value)?),
        ),
        CallableTypeTemplate::Union(members) => SnapshotCallableTypeTemplate::Union(
            members
                .iter()
                .map(snapshot_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        CallableTypeTemplate::Unconstrained => SnapshotCallableTypeTemplate::Unconstrained,
    })
}

fn restore_callable_signature(
    signature: SnapshotCallableSignature,
) -> Result<crate::core::CallableSignature, String> {
    Ok(crate::core::CallableSignature {
        receiver_type_parameters: signature.receiver_type_parameters,
        type_parameters: signature.type_parameters,
        parameters: signature
            .parameters
            .into_iter()
            .map(|parameter| {
                Ok(crate::core::CallableParameterTemplate {
                    kind: restore_param_kind(parameter.kind),
                    ruby_type: restore_callable_template(parameter.ruby_type)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        block: crate::core::CallableBlockTemplate {
            parameters: signature
                .block
                .parameters
                .into_iter()
                .map(restore_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
            return_type: restore_callable_template(signature.block.return_type)?,
            required: signature.block.required,
        },
        return_type: restore_callable_template(signature.return_type)?,
    })
}

fn restore_callable_template(
    template: SnapshotCallableTypeTemplate,
) -> Result<crate::core::CallableTypeTemplate, String> {
    use crate::core::CallableTypeTemplate;
    Ok(match template {
        SnapshotCallableTypeTemplate::Concrete(ruby_type) => {
            CallableTypeTemplate::Concrete(restore_ruby_type(ruby_type, 1)?)
        }
        SnapshotCallableTypeTemplate::Receiver => CallableTypeTemplate::Receiver,
        SnapshotCallableTypeTemplate::Variable(name) => CallableTypeTemplate::Variable(name),
        SnapshotCallableTypeTemplate::Array(element) => {
            CallableTypeTemplate::Array(Box::new(restore_callable_template(*element)?))
        }
        SnapshotCallableTypeTemplate::Hash(key, value) => CallableTypeTemplate::Hash(
            Box::new(restore_callable_template(*key)?),
            Box::new(restore_callable_template(*value)?),
        ),
        SnapshotCallableTypeTemplate::Union(members) => CallableTypeTemplate::Union(
            members
                .into_iter()
                .map(restore_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        SnapshotCallableTypeTemplate::Unconstrained => CallableTypeTemplate::Unconstrained,
    })
}

fn snapshot_method_visibility_override(
    fact: &MethodVisibilityOverrideFact,
) -> Result<SnapshotMethodVisibilityOverrideFact, String> {
    Ok(SnapshotMethodVisibilityOverrideFact {
        owner: snapshot_fqn(&fact.owner)?,
        method: fact.method.as_str().to_string(),
        visibility: snapshot_visibility(fact.visibility),
        range: snapshot_range(fact.range),
    })
}

fn restore_method_visibility_override(
    fact: SnapshotMethodVisibilityOverrideFact,
    file_id: SourceFileId,
) -> Result<MethodVisibilityOverrideFact, String> {
    Ok(MethodVisibilityOverrideFact::new(
        restore_fqn(fact.owner)?,
        RubyMethod::new(&fact.method).map_err(|error| {
            format!(
                "invalid persistent visibility method `{}`: {error}",
                fact.method
            )
        })?,
        restore_visibility(fact.visibility),
        restore_range(fact.range, file_id)?,
    ))
}

fn snapshot_ruby_type(ruby_type: &RubyType) -> Result<SnapshotRubyType, String> {
    Ok(match ruby_type {
        RubyType::Class(fqn) => SnapshotRubyType::Class {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::Module(fqn) => SnapshotRubyType::Module {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::ClassReference(fqn) => SnapshotRubyType::ClassReference {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::ModuleReference(fqn) => SnapshotRubyType::ModuleReference {
            fqn: snapshot_fqn(fqn)?,
        },
        RubyType::Literal(value) => SnapshotRubyType::Literal {
            value: snapshot_literal_value(value),
        },
        RubyType::Array(elements) => SnapshotRubyType::Array {
            elements: elements
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Hash(keys, values) => SnapshotRubyType::Hash {
            keys: keys
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
            values: values
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Shape(shape) => SnapshotRubyType::Shape {
            fields: shape
                .fields()
                .iter()
                .map(|field| {
                    Ok(SnapshotShapeField {
                        key: snapshot_literal_key(field.key()),
                        value: snapshot_ruby_type(field.value())?,
                        presence: match field.presence() {
                            ShapeFieldPresence::Required => SnapshotShapeFieldPresence::Required,
                            ShapeFieldPresence::Optional => SnapshotShapeFieldPresence::Optional,
                        },
                    })
                })
                .collect::<Result<_, String>>()?,
            rest: shape
                .rest()
                .map(|rest| {
                    Ok::<_, String>(Box::new(SnapshotShapeRest {
                        key: snapshot_ruby_type(rest.key())?,
                        value: snapshot_ruby_type(rest.value())?,
                    }))
                })
                .transpose()?,
            exactness: match shape.exactness() {
                ShapeExactness::Exact => SnapshotShapeExactness::Exact,
                ShapeExactness::Open => SnapshotShapeExactness::Open,
            },
            stability: match shape.stability() {
                ShapeStability::TrackedMutable => SnapshotShapeStability::TrackedMutable,
                ShapeStability::Frozen => SnapshotShapeStability::Frozen,
            },
        },
        RubyType::Union(types) => SnapshotRubyType::Union {
            types: types
                .iter()
                .map(snapshot_ruby_type)
                .collect::<Result<_, _>>()?,
        },
        RubyType::Unknown => SnapshotRubyType::Unknown,
    })
}

fn restore_ruby_type(ruby_type: SnapshotRubyType, depth: usize) -> Result<RubyType, String> {
    if depth > 64 {
        return Err("persistent Ruby type exceeds the maximum nesting depth of 64".to_string());
    }
    match ruby_type {
        SnapshotRubyType::Class { fqn } => Ok(RubyType::Class(restore_fqn(fqn)?)),
        SnapshotRubyType::Module { fqn } => Ok(RubyType::Module(restore_fqn(fqn)?)),
        SnapshotRubyType::ClassReference { fqn } => Ok(RubyType::ClassReference(restore_fqn(fqn)?)),
        SnapshotRubyType::ModuleReference { fqn } => {
            Ok(RubyType::ModuleReference(restore_fqn(fqn)?))
        }
        SnapshotRubyType::Literal { value } => {
            Ok(RubyType::Literal(Box::new(restore_literal_value(value))))
        }
        SnapshotRubyType::Array { elements } => Ok(RubyType::Array(
            elements
                .into_iter()
                .map(|element| restore_ruby_type(element, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Hash { keys, values } => Ok(RubyType::Hash(
            keys.into_iter()
                .map(|key| restore_ruby_type(key, depth + 1))
                .collect::<Result<_, _>>()?,
            values
                .into_iter()
                .map(|value| restore_ruby_type(value, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Shape {
            fields,
            rest,
            exactness,
            stability,
        } => {
            let fields = fields
                .into_iter()
                .map(|field| {
                    let key = restore_literal_key(field.key);
                    let value = restore_ruby_type(field.value, depth + 1)?;
                    Ok(match field.presence {
                        SnapshotShapeFieldPresence::Required => ShapeField::required(key, value),
                        SnapshotShapeFieldPresence::Optional => ShapeField::optional(key, value),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let rest = rest
                .map(|rest| {
                    Ok::<_, String>(ShapeRest::new(
                        restore_ruby_type(rest.key, depth + 1)?,
                        restore_ruby_type(rest.value, depth + 1)?,
                    ))
                })
                .transpose()?;
            let exactness = match exactness {
                SnapshotShapeExactness::Exact => ShapeExactness::Exact,
                SnapshotShapeExactness::Open => ShapeExactness::Open,
            };
            let stability = match stability {
                SnapshotShapeStability::TrackedMutable => ShapeStability::TrackedMutable,
                SnapshotShapeStability::Frozen => ShapeStability::Frozen,
            };
            ShapeType::try_new(fields, rest, exactness, stability)
                .map(|shape| RubyType::Shape(Box::new(shape)))
                .map_err(|error| format!("invalid persistent shape type: {error}"))
        }
        SnapshotRubyType::Union { types } => Ok(RubyType::Union(
            types
                .into_iter()
                .map(|ruby_type| restore_ruby_type(ruby_type, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        SnapshotRubyType::Unknown => Ok(RubyType::Unknown),
    }
}

fn snapshot_literal_value(value: &LiteralValue) -> SnapshotLiteral {
    match value {
        LiteralValue::Symbol(value) => SnapshotLiteral::Symbol(value.clone()),
        LiteralValue::String(value) => SnapshotLiteral::String(value.clone()),
    }
}

fn snapshot_literal_key(key: &LiteralKey) -> SnapshotLiteral {
    match key {
        LiteralKey::Symbol(value) => SnapshotLiteral::Symbol(value.clone()),
        LiteralKey::String(value) => SnapshotLiteral::String(value.clone()),
    }
}

fn restore_literal_value(value: SnapshotLiteral) -> LiteralValue {
    match value {
        SnapshotLiteral::Symbol(value) => LiteralValue::Symbol(value),
        SnapshotLiteral::String(value) => LiteralValue::String(value),
    }
}

fn restore_literal_key(key: SnapshotLiteral) -> LiteralKey {
    match key {
        SnapshotLiteral::Symbol(value) => LiteralKey::Symbol(value),
        SnapshotLiteral::String(value) => LiteralKey::String(value),
    }
}

fn snapshot_type_subject(subject: &TypeSubject) -> Result<SnapshotTypeSubject, String> {
    Ok(match subject {
        TypeSubject::Constant(fqn) => SnapshotTypeSubject::Constant {
            fqn: snapshot_fqn(fqn)?,
        },
        TypeSubject::Local { scope_id, name } => SnapshotTypeSubject::Local {
            scope_id: *scope_id,
            name: name.clone(),
        },
        TypeSubject::InstanceVariable { owner, name } => SnapshotTypeSubject::InstanceVariable {
            owner: snapshot_fqn(owner)?,
            name: name.clone(),
        },
        TypeSubject::ClassVariable { owner, name } => SnapshotTypeSubject::ClassVariable {
            owner: snapshot_fqn(owner)?,
            name: name.clone(),
        },
        TypeSubject::GlobalVariable(name) => {
            SnapshotTypeSubject::GlobalVariable { name: name.clone() }
        }
        TypeSubject::MethodReturn(fqn) => SnapshotTypeSubject::MethodReturn {
            fqn: snapshot_fqn(fqn)?,
        },
        TypeSubject::Parameter { method, name } => SnapshotTypeSubject::Parameter {
            method: snapshot_fqn(method)?,
            name: name.clone(),
        },
        TypeSubject::Expression(range) => SnapshotTypeSubject::Expression {
            range: snapshot_range(*range),
        },
    })
}

fn restore_type_subject(
    subject: SnapshotTypeSubject,
    file_id: SourceFileId,
) -> Result<TypeSubject, String> {
    Ok(match subject {
        SnapshotTypeSubject::Constant { fqn } => TypeSubject::Constant(restore_fqn(fqn)?),
        SnapshotTypeSubject::Local { scope_id, name } => TypeSubject::Local { scope_id, name },
        SnapshotTypeSubject::InstanceVariable { owner, name } => TypeSubject::InstanceVariable {
            owner: restore_fqn(owner)?,
            name,
        },
        SnapshotTypeSubject::ClassVariable { owner, name } => TypeSubject::ClassVariable {
            owner: restore_fqn(owner)?,
            name,
        },
        SnapshotTypeSubject::GlobalVariable { name } => TypeSubject::GlobalVariable(name),
        SnapshotTypeSubject::MethodReturn { fqn } => TypeSubject::MethodReturn(restore_fqn(fqn)?),
        SnapshotTypeSubject::Parameter { method, name } => TypeSubject::Parameter {
            method: restore_fqn(method)?,
            name,
        },
        SnapshotTypeSubject::Expression { range } => {
            TypeSubject::Expression(restore_range(range, file_id)?)
        }
    })
}

fn snapshot_provenance(provenance: TypeProvenance) -> SnapshotTypeProvenance {
    match provenance {
        TypeProvenance::Literal => SnapshotTypeProvenance::Literal,
        TypeProvenance::Assignment => SnapshotTypeProvenance::Assignment,
        TypeProvenance::Flow => SnapshotTypeProvenance::Flow,
        TypeProvenance::Rbs => SnapshotTypeProvenance::Rbs,
        TypeProvenance::Yard => SnapshotTypeProvenance::Yard,
        TypeProvenance::Runtime => SnapshotTypeProvenance::Runtime,
        TypeProvenance::Extension => SnapshotTypeProvenance::Extension,
        TypeProvenance::Inferred => SnapshotTypeProvenance::Inferred,
    }
}

fn restore_provenance(provenance: SnapshotTypeProvenance) -> TypeProvenance {
    match provenance {
        SnapshotTypeProvenance::Literal => TypeProvenance::Literal,
        SnapshotTypeProvenance::Assignment => TypeProvenance::Assignment,
        SnapshotTypeProvenance::Flow => TypeProvenance::Flow,
        SnapshotTypeProvenance::Rbs => TypeProvenance::Rbs,
        SnapshotTypeProvenance::Yard => TypeProvenance::Yard,
        SnapshotTypeProvenance::Runtime => TypeProvenance::Runtime,
        SnapshotTypeProvenance::Extension => TypeProvenance::Extension,
        SnapshotTypeProvenance::Inferred => TypeProvenance::Inferred,
    }
}

fn snapshot_type_fact(fact: &TypeFact) -> Result<SnapshotTypeFact, String> {
    Ok(SnapshotTypeFact {
        subject: snapshot_type_subject(&fact.subject)?,
        ruby_type: snapshot_ruby_type(&fact.ruby_type)?,
        range: snapshot_range(fact.range),
        provenance: snapshot_provenance(fact.provenance),
    })
}

fn restore_type_fact(fact: SnapshotTypeFact, file_id: SourceFileId) -> Result<TypeFact, String> {
    Ok(TypeFact::new(
        restore_type_subject(fact.subject, file_id)?,
        restore_ruby_type(fact.ruby_type, 0)?,
        restore_range(fact.range, file_id)?,
        restore_provenance(fact.provenance),
    ))
}

fn snapshot_graph_node_kind(kind: GraphNodeKind) -> SnapshotGraphNodeKind {
    match kind {
        GraphNodeKind::Class => SnapshotGraphNodeKind::Class,
        GraphNodeKind::Module => SnapshotGraphNodeKind::Module,
    }
}

fn restore_graph_node_kind(kind: SnapshotGraphNodeKind) -> GraphNodeKind {
    match kind {
        SnapshotGraphNodeKind::Class => GraphNodeKind::Class,
        SnapshotGraphNodeKind::Module => GraphNodeKind::Module,
    }
}

fn snapshot_graph_edge_kind(kind: GraphEdgeKind) -> SnapshotGraphEdgeKind {
    match kind {
        GraphEdgeKind::Superclass => SnapshotGraphEdgeKind::Superclass,
        GraphEdgeKind::Include => SnapshotGraphEdgeKind::Include,
        GraphEdgeKind::Prepend => SnapshotGraphEdgeKind::Prepend,
        GraphEdgeKind::Extend => SnapshotGraphEdgeKind::Extend,
        GraphEdgeKind::ExecutionContextApplication => {
            SnapshotGraphEdgeKind::ExecutionContextApplication
        }
    }
}

fn restore_graph_edge_kind(kind: SnapshotGraphEdgeKind) -> GraphEdgeKind {
    match kind {
        SnapshotGraphEdgeKind::Superclass => GraphEdgeKind::Superclass,
        SnapshotGraphEdgeKind::Include => GraphEdgeKind::Include,
        SnapshotGraphEdgeKind::Prepend => GraphEdgeKind::Prepend,
        SnapshotGraphEdgeKind::Extend => GraphEdgeKind::Extend,
        SnapshotGraphEdgeKind::ExecutionContextApplication => {
            GraphEdgeKind::ExecutionContextApplication
        }
    }
}

fn snapshot_graph_edge_provenance(
    provenance: crate::core::GraphEdgeProvenance,
) -> SnapshotGraphEdgeProvenance {
    match provenance {
        crate::core::GraphEdgeProvenance::Explicit => SnapshotGraphEdgeProvenance::Explicit,
        crate::core::GraphEdgeProvenance::ImplicitObject => {
            SnapshotGraphEdgeProvenance::ImplicitObject
        }
    }
}

fn restore_graph_edge_provenance(
    provenance: SnapshotGraphEdgeProvenance,
) -> crate::core::GraphEdgeProvenance {
    match provenance {
        SnapshotGraphEdgeProvenance::Explicit => crate::core::GraphEdgeProvenance::Explicit,
        SnapshotGraphEdgeProvenance::ImplicitObject => {
            crate::core::GraphEdgeProvenance::ImplicitObject
        }
    }
}

fn snapshot_graph_node(fact: &GraphNodeFact) -> Result<SnapshotGraphNodeFact, String> {
    Ok(SnapshotGraphNodeFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        kind: snapshot_graph_node_kind(fact.kind),
        range: snapshot_range(fact.range),
    })
}

fn restore_graph_node(
    fact: SnapshotGraphNodeFact,
    file_id: SourceFileId,
) -> Result<GraphNodeFact, String> {
    Ok(GraphNodeFact::new(
        restore_fqn(fact.fqn)?,
        restore_graph_node_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    ))
}

fn snapshot_graph_edge(fact: &GraphEdgeFact) -> Result<SnapshotGraphEdgeFact, String> {
    Ok(SnapshotGraphEdgeFact {
        source: snapshot_fqn(&fact.source)?,
        target: snapshot_fqn(&fact.target)?,
        kind: snapshot_graph_edge_kind(fact.kind),
        provenance: snapshot_graph_edge_provenance(fact.provenance),
        range: snapshot_range(fact.range),
    })
}

fn restore_graph_edge(
    fact: SnapshotGraphEdgeFact,
    file_id: SourceFileId,
) -> Result<GraphEdgeFact, String> {
    Ok(GraphEdgeFact::new(
        restore_fqn(fact.source)?,
        restore_fqn(fact.target)?,
        restore_graph_edge_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    )
    .with_provenance(restore_graph_edge_provenance(fact.provenance)))
}

fn snapshot_unresolved_graph_edge(
    fact: &UnresolvedGraphEdgeFact,
) -> Result<SnapshotUnresolvedGraphEdgeFact, String> {
    Ok(SnapshotUnresolvedGraphEdgeFact {
        source: snapshot_fqn(&fact.source)?,
        target_parts: snapshot_parts(&fact.target_parts)?,
        absolute: fact.absolute,
        context: snapshot_fqn(&fact.context)?,
        kind: snapshot_graph_edge_kind(fact.kind),
        provenance: snapshot_graph_edge_provenance(fact.provenance),
        range: snapshot_range(fact.range),
    })
}

fn restore_unresolved_graph_edge(
    fact: SnapshotUnresolvedGraphEdgeFact,
    file_id: SourceFileId,
) -> Result<UnresolvedGraphEdgeFact, String> {
    Ok(UnresolvedGraphEdgeFact::new(
        restore_fqn(fact.source)?,
        restore_parts(fact.target_parts)?,
        fact.absolute,
        restore_fqn(fact.context)?,
        restore_graph_edge_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    )
    .with_provenance(restore_graph_edge_provenance(fact.provenance)))
}

fn validate_contained_range(inner: TextRange, outer: TextRange, label: &str) -> Result<(), String> {
    if inner.file_id != outer.file_id
        || inner.start_byte < outer.start_byte
        || inner.end_byte > outer.end_byte
    {
        return Err(format!(
            "persistent {label} range {}..{} is outside declaration {}..{}",
            inner.start_byte, inner.end_byte, outer.start_byte, outer.end_byte
        ));
    }
    Ok(())
}

fn validate_range(
    range: TextRange,
    expected: SourceFileId,
) -> Result<(), ProjectNeutralTemplateRejection> {
    if range.file_id != expected {
        return Err(ProjectNeutralTemplateRejection::ForeignRange {
            expected,
            actual: range.file_id,
        });
    }
    Ok(())
}

fn rebind_all_ranges(facts: &mut FileFacts, source: SourceFileId, target: SourceFileId) {
    for fact in &mut facts.symbols {
        rebind_range(&mut fact.range, source, target);
        rebind_range(&mut fact.name_range, source, target);
    }
    for fact in &mut facts.methods {
        rebind_range(&mut fact.range, source, target);
        rebind_range(&mut fact.name_range, source, target);
    }
    for fact in &mut facts.method_visibility_overrides {
        rebind_range(&mut fact.range, source, target);
    }
    for fact in &mut facts.types {
        rebind_range(&mut fact.range, source, target);
        match &mut fact.subject {
            TypeSubject::Expression(range) => rebind_range(range, source, target),
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. } => {}
        }
    }
    for fact in &mut facts.graph_nodes {
        rebind_range(&mut fact.range, source, target);
    }
    for fact in &mut facts.graph_edges {
        rebind_range(&mut fact.range, source, target);
    }
    for fact in &mut facts.unresolved_graph_edges {
        rebind_range(&mut fact.range, source, target);
    }
    for fact in &mut facts.inference.constant_callable_bodies {
        rebind_range(&mut fact.range, source, target);
    }
}

fn rebind_range(range: &mut TextRange, source: SourceFileId, target: SourceFileId) {
    assert_eq!(
        range.file_id, source,
        "INVARIANT VIOLATED: a semantic fact template contains a range from a foreign file. This is a bug because template construction validates every supported source range before caching. Fix: add validation and rebinding for the new range-bearing fact field."
    );
    range.file_id = target;
}

#[cfg(test)]
mod tests {
    use super::{
        restore_ruby_type, snapshot_ruby_type, ProjectNeutralFileFactsSnapshot,
        SnapshotCallableTypeTemplate,
    };
    use crate::core::{
        CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind,
        CallableBodySummary, ConstantCallableBodyFact, DiagnosticCandidate,
        DiagnosticCandidateKind, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact,
        GraphNodeKind, InferenceEvidence, LiteralKey, LiteralValue, MethodFact,
        MethodVisibilityOverrideFact, RubyConstant, RubyMethod, RubyType, ShapeExactness,
        ShapeField, ShapeRest, ShapeStability, ShapeType, SourceFileId, SourceKind, SymbolFact,
        SymbolKind, TextRange, TypeFact, TypeInferenceOutcome, TypeProvenance, TypeSubject,
        UnresolvedGraphEdgeFact,
    };
    use crate::engine::{
        AnalysisEngine, AnalysisQuery, FileFacts, ProjectNeutralFileFactsTemplate,
        ProjectNeutralTemplateRejection, ResolveMode, SourceFileInput,
    };
    use crate::method_store::MethodVisibility;
    use std::path::PathBuf;

    fn namespace(name: &str) -> FullyQualifiedName {
        FullyQualifiedName::namespace(vec![RubyConstant::new(name).unwrap()])
    }

    #[test]
    fn persistent_snapshot_round_trips_canonical_shape_and_literal_types() {
        let shape = ShapeType::try_new(
            [
                ShapeField::required(
                    LiteralKey::symbol("kind"),
                    RubyType::Literal(Box::new(LiteralValue::symbol("ready"))),
                ),
                ShapeField::optional(LiteralKey::string("name"), RubyType::string()),
            ],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::integer())),
            ShapeExactness::Open,
            ShapeStability::Frozen,
        )
        .unwrap();
        let original = RubyType::Shape(Box::new(shape));
        let snapshot = snapshot_ruby_type(&original).unwrap();
        assert_eq!(restore_ruby_type(snapshot, 0).unwrap(), original);
    }

    #[test]
    fn persistent_snapshot_round_trips_capture_free_callable_constant() {
        let source = SourceFileId(41);
        let target = SourceFileId(7);
        let range = TextRange::new(source, 3, 29);
        let constant = FullyQualifiedName::constant(vec![RubyConstant::new("CONVERT").unwrap()]);
        let summary = CallableBodySummary {
            strict_arity: true,
            parameters: vec![CallableBodyParameter {
                name: "value".to_string(),
                kind: CallableBodyParameterKind::Required,
                default: None,
            }],
            captures: Vec::new(),
            result: CallableBodyExpression::Parameter(0),
            node_count: 1,
        };
        let template = ProjectNeutralFileFactsTemplate::try_new(
            source,
            FileFacts {
                inference: InferenceEvidence {
                    constant_callable_bodies: vec![ConstantCallableBodyFact {
                        constant: constant.clone(),
                        summary: summary.clone(),
                        range,
                    }],
                    ..InferenceEvidence::default()
                },
                ..FileFacts::default()
            },
        )
        .unwrap();

        let snapshot = template.to_persistent_snapshot().unwrap();
        let encoded = postcard::to_allocvec(&snapshot).unwrap();
        let decoded: ProjectNeutralFileFactsSnapshot = postcard::from_bytes(&encoded).unwrap();
        let restored =
            ProjectNeutralFileFactsTemplate::try_from_persistent_snapshot(decoded).unwrap();
        let facts = restored.instantiate(target);
        assert_eq!(facts.inference.constant_callable_bodies.len(), 1);
        let fact = &facts.inference.constant_callable_bodies[0];
        assert_eq!(fact.constant, constant);
        assert_eq!(fact.summary, summary);
        assert_eq!(fact.range.file_id, target);
    }

    #[test]
    fn persistent_callable_template_uses_postcard_compatible_enum_encoding() {
        let template = SnapshotCallableTypeTemplate::Array(Box::new(
            SnapshotCallableTypeTemplate::Variable("element".to_string()),
        ));
        let encoded = postcard::to_allocvec(&template).unwrap();
        let decoded: SnapshotCallableTypeTemplate = postcard::from_bytes(&encoded).unwrap();
        let SnapshotCallableTypeTemplate::Array(element) = decoded else {
            panic!(
                "INVARIANT VIOLATED: persistent callable template changed variant during Postcard round-trip. This is a bug because dependency products must restore exact higher-order signatures. Fix: keep the persistence DTO externally tagged and add explicit wire migration for representation changes."
            );
        };
        assert!(matches!(
            *element,
            SnapshotCallableTypeTemplate::Variable(ref name) if name == "element"
        ));
    }

    fn assert_range_file(range: TextRange, expected: SourceFileId) {
        assert_eq!(range.file_id, expected);
    }

    #[test]
    fn template_rebinds_declarations_and_drops_file_local_evidence() {
        let source = SourceFileId(41);
        let target = SourceFileId(7);
        let owner = namespace("Widget");
        let method = RubyMethod::new("call").unwrap();
        let method_fqn = FullyQualifiedName::method(owner.namespace_parts(), method);
        let range = TextRange::new(source, 1, 20);
        let expression = TextRange::new(source, 8, 12);

        let template = ProjectNeutralFileFactsTemplate::try_new(
            source,
            FileFacts {
                symbols: vec![
                    SymbolFact::new(owner.clone(), SymbolKind::Class, range)
                        .with_name_range(TextRange::new(source, 1, 7)),
                    SymbolFact::new(owner.clone(), SymbolKind::LocalVariable, expression),
                ],
                methods: vec![MethodFact::new(method_fqn.clone(), owner.clone(), range)
                    .with_name_range(TextRange::new(source, 8, 12))],
                method_visibility_overrides: vec![MethodVisibilityOverrideFact::new(
                    owner.clone(),
                    method,
                    MethodVisibility::Private,
                    expression,
                )],
                types: vec![
                    TypeFact::new(
                        TypeSubject::Expression(expression),
                        RubyType::string(),
                        expression,
                        TypeProvenance::Inferred,
                    ),
                    TypeFact::new(
                        TypeSubject::MethodReturn(method_fqn),
                        RubyType::string(),
                        range,
                        TypeProvenance::Inferred,
                    ),
                ],
                graph_nodes: vec![GraphNodeFact::new(
                    owner.clone(),
                    GraphNodeKind::Class,
                    range,
                )],
                graph_edges: vec![GraphEdgeFact::new(
                    owner.clone(),
                    namespace("Object"),
                    GraphEdgeKind::Superclass,
                    range,
                )],
                unresolved_graph_edges: vec![UnresolvedGraphEdgeFact::new(
                    owner.clone(),
                    vec![RubyConstant::new("Enumerable").unwrap()],
                    false,
                    owner,
                    GraphEdgeKind::Include,
                    range,
                )],
                inference: InferenceEvidence {
                    call_expression_outcomes: vec![(
                        expression,
                        TypeInferenceOutcome::proven(RubyType::string()),
                    )],
                    ..Default::default()
                },
                local_read_types: vec![(expression, RubyType::string())].into_boxed_slice(),
                ..FileFacts::default()
            },
        )
        .unwrap();

        let snapshot = template.to_persistent_snapshot().unwrap();
        let restored =
            ProjectNeutralFileFactsTemplate::try_from_persistent_snapshot(snapshot).unwrap();
        let facts = restored.instantiate(target);
        assert_eq!(facts.symbols.len(), 1);
        assert_range_file(facts.symbols[0].range, target);
        assert_range_file(facts.symbols[0].name_range, target);
        assert_range_file(facts.methods[0].range, target);
        assert_range_file(facts.methods[0].name_range, target);
        assert_range_file(facts.method_visibility_overrides[0].range, target);
        assert_eq!(facts.types.len(), 1);
        assert_range_file(facts.types[0].range, target);
        let TypeSubject::MethodReturn(_) = facts.types[0].subject else {
            panic!("expected retained method-return type subject");
        };
        assert_eq!(facts.inference, InferenceEvidence::default());
        assert!(facts.local_read_types.is_empty());
        assert_range_file(facts.graph_nodes[0].range, target);
        assert_range_file(facts.graph_edges[0].range, target);
        assert_range_file(facts.unresolved_graph_edges[0].range, target);
    }

    #[test]
    fn template_rejects_project_specific_candidates() {
        let source = SourceFileId(1);
        let rejection = ProjectNeutralFileFactsTemplate::try_new(
            source,
            FileFacts {
                diagnostic_candidates: vec![DiagnosticCandidate::new(
                    TextRange::new(source, 0, 1),
                    DiagnosticCandidateKind::BadSplat {
                        operator: "*".to_string(),
                        arg_repr: "value".to_string(),
                        expected: "Array".to_string(),
                    },
                )],
                ..FileFacts::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            rejection,
            ProjectNeutralTemplateRejection::ProjectSpecificFacts
        );
    }

    #[test]
    fn rebound_templates_preserve_navigation_without_sharing_file_identity() {
        let template_file = SourceFileId(99);
        let owner = namespace("CachedWidget");
        let declaration = TextRange::new(template_file, 0, 12);
        let template = ProjectNeutralFileFactsTemplate::try_new(
            template_file,
            FileFacts {
                symbols: vec![SymbolFact::new(
                    owner.clone(),
                    SymbolKind::Class,
                    declaration,
                )],
                graph_nodes: vec![GraphNodeFact::new(owner, GraphNodeKind::Class, declaration)],
                ..FileFacts::default()
            },
        )
        .unwrap();

        let mut first = AnalysisEngine::new();
        let first_file = first.register_file(SourceFileInput {
            path: PathBuf::from("/cache/a/cached_widget.rb"),
            content: "class CachedWidget; end".to_string(),
            kind: SourceKind::Gem,
        });
        first.replace_facts(
            first_file,
            template.instantiate(first_file),
            ResolveMode::Immediate,
        );

        let mut second = AnalysisEngine::new();
        second.register_file(SourceFileInput {
            path: PathBuf::from("/other/preexisting.rb"),
            content: String::new(),
            kind: SourceKind::Project,
        });
        let second_file = second.register_file(SourceFileInput {
            path: PathBuf::from("/cache/b/cached_widget.rb"),
            content: "class CachedWidget; end".to_string(),
            kind: SourceKind::Gem,
        });
        second.replace_facts(
            second_file,
            template.instantiate(second_file),
            ResolveMode::Immediate,
        );

        let parts = [RubyConstant::new("CachedWidget").unwrap()];
        assert_eq!(
            AnalysisQuery::new(&first).constant_definition_ranges(&parts, &[]),
            vec![TextRange::new(first_file, 0, 12)]
        );
        assert_eq!(
            AnalysisQuery::new(&second).constant_definition_ranges(&parts, &[]),
            vec![TextRange::new(second_file, 0, 12)]
        );
        assert_ne!(first_file, second_file);
    }
}
