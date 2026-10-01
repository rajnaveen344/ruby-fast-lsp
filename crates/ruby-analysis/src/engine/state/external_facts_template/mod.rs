//! Project-neutral dependency fact templates and their persistent snapshot
//! schema.

mod callable_codec;
mod fact_codec;
#[cfg(test)]
mod tests;
mod type_codec;

use crate::core::storage::memory_estimate::{
    fqn_heap_bytes, ruby_type_heap_bytes, string_heap_bytes, type_subject_heap_bytes,
    vec_payload_bytes,
};
use crate::core::{
    FullyQualifiedName, MethodAvailability, RubyConstant, RubyType, SourceFileId, SymbolKind,
    TextRange, TypeSubject,
};
use crate::engine::FileFacts;
use fact_codec::{restore_declaration_facts, snapshot_declaration_facts};
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
    invariant_eq!(
        range.file_id,
        source,
        what = "a semantic fact template contains a range from a foreign file",
        why = "template construction validates every supported source range before caching",
        fix = "add validation and rebinding for the new range-bearing fact field",
    );
    range.file_id = target;
}
