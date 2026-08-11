use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::core::memory_estimate::{fqn_heap_bytes, vec_payload_bytes};
use crate::core::method_return_equation::MethodReturnBase;
use crate::core::method_store::StoredMethodFactMatch;
use crate::core::{
    ConstLookup, ConstLookupId, ConstantPath, ConstantTypeDependency, ConstantTypeEquation,
    ConstantTypeProjection, ConstantTypeTarget, DiagnosticCandidate, DiagnosticCandidateStore,
    DiagnosticFact, DiagnosticSeverity, DiagnosticStore, ExecutionContextFact, ExecutionScopeMode,
    FqnId, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphEdgeProvenance, GraphNodeFact,
    GraphNodeKind, InferenceEvidence, InferenceTelemetry, MethodAvailability, MethodFact,
    MethodParamKind, MethodReferenceAccess, MethodStore, MethodVisibilityOverrideFact,
    NamespaceKind, ReferenceCandidate, ReferenceCandidateKind, ReferenceCandidateStore,
    ReferenceFact, ReferenceStore, RubyConstant, RubyMethod, RubyType, SemanticGraph, SourceFileId,
    SourceKind, StoredGraphEdgeFact, StoredGraphNodeFact, StoredMethodFact,
    StoredReferenceCandidate, StoredSuperclassResolution, StoredSymbolFact,
    StoredUnresolvedGraphEdgeFact, SymbolFact, SymbolKind, SymbolStore, TextRange, TypeFact,
    TypeInferenceOutcome, TypeProvenance, TypeResolution, TypeStore, TypeSubject, UnknownReason,
    UnresolvedGraphEdgeFact,
};

use crate::engine::AnalysisQuery;
use crate::inference::constant::{
    solve_constant_type_equations, ConstantFactInput, ResolvedConstantDependency,
};
use crate::inference::method::recursive::solve_method_return_equations_with_telemetry;
use crate::method_store::MethodVisibility;
use crate::FileIdMap;
use indexmap::IndexSet;
use parking_lot::Mutex;

fn resolve_constant_dependency(
    query: &AnalysisQuery<'_>,
    dependency: &ConstantTypeDependency,
) -> Option<ResolvedConstantDependency> {
    let context = if dependency.absolute {
        &[][..]
    } else {
        dependency.lexical_context.as_slice()
    };
    let resolved = query.resolve_constant_in_context(&dependency.parts, context)?;
    let constant = FullyQualifiedName::constant(resolved.namespace_parts());
    match dependency.projection() {
        ConstantTypeProjection::Value => Some(ResolvedConstantDependency::Value(constant)),
        ConstantTypeProjection::ConstructorInstance => {
            if let Some(value_type) = query.constant_value_type(&constant) {
                return match value_type {
                    RubyType::ClassReference(target) => Some(
                        ResolvedConstantDependency::Projected(RubyType::Class(target)),
                    ),
                    RubyType::Class(_)
                    | RubyType::Module(_)
                    | RubyType::ModuleReference(_)
                    | RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Union(_)
                    | RubyType::Unknown => None,
                };
            }
            let namespace = FullyQualifiedName::namespace(constant.namespace_parts());
            match query.namespace_node_kind(&namespace) {
                Some(GraphNodeKind::Class) => Some(ResolvedConstantDependency::Projected(
                    RubyType::Class(constant),
                )),
                Some(GraphNodeKind::Module) | None => None,
            }
        }
    }
}

fn resolve_constant_dependency_type(
    query: &AnalysisQuery<'_>,
    dependency: &ConstantTypeDependency,
) -> Option<RubyType> {
    match resolve_constant_dependency(query, dependency)? {
        ResolvedConstantDependency::Value(constant) => query.constant_value_type(&constant),
        ResolvedConstantDependency::Projected(ruby_type) => Some(ruby_type),
    }
}

pub(super) enum EffectiveMethodFactMatch {
    Missing,
    Unique(MethodFact),
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: Option<String>,
    pub line_index: SourceLineIndex,
    pub content_hash: u64,
    pub kind: SourceKind,
    /// Present for `SourceKind::Gem` files bound from a locked package.
    pub library_package: Option<crate::core::LibraryPackageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredTypeInferenceOutcome {
    Proven(crate::core::type_store::RubyTypeId),
    Unknown(UnknownReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeInferenceOutcomeRef<'a> {
    Proven(&'a RubyType),
    Unknown(UnknownReason),
}

impl StoredTypeInferenceOutcome {
    fn from_domain(types: &mut TypeStore, outcome: TypeInferenceOutcome) -> Self {
        match outcome.unknown_reason() {
            Some(reason) => Self::Unknown(reason),
            None => Self::Proven(types.intern_ruby_type(
                outcome.into_proven_type().expect(
                    "INVARIANT VIOLATED: call-expression outcome is neither proven nor Unknown. This is a bug because TypeInferenceOutcome has exactly those two states. Fix: construct outcomes only through TypeInferenceOutcome::proven or TypeInferenceOutcome::unknown.",
                ),
            )),
        }
    }

    fn as_ref<'a>(self, types: &'a TypeStore) -> TypeInferenceOutcomeRef<'a> {
        match self {
            Self::Proven(ruby_type) => TypeInferenceOutcomeRef::Proven(types.ruby_type(ruby_type)),
            Self::Unknown(reason) => TypeInferenceOutcomeRef::Unknown(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLineIndex {
    line_offsets: Vec<u32>,
    len: u32,
    ascii: bool,
}

impl SourceLineIndex {
    fn new(source: &str) -> Self {
        let len = u32::try_from(source.len()).expect(
            "INVARIANT VIOLATED: source file byte length exceeded u32. This is a bug because \
             every analysis TextRange and SourceFileId-relative byte offset is represented as \
             u32. Fix: reject or segment files larger than u32::MAX before registration.",
        );
        let mut line_offsets = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_offsets.push(u32::try_from(idx + 1).expect(
                    "INVARIANT VIOLATED: source line offset exceeded u32 after the complete \
                     source length fit u32. This is a bug because a position within a bounded \
                     source cannot exceed its length. Fix: keep source length validation before \
                     line-index construction.",
                ));
            }
        }
        if line_offsets.last() != Some(&len) {
            line_offsets.push(len);
        }
        Self {
            line_offsets,
            len,
            ascii: source.is_ascii(),
        }
    }

    pub fn line_offsets(&self) -> &[u32] {
        &self.line_offsets
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_ascii(&self) -> bool {
        self.ascii
    }

    fn shrink_to_fit(&mut self) {
        self.line_offsets.shrink_to_fit();
    }
}

impl SourceFile {
    pub fn source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn byte_offset_to_line_character(&self, byte_offset: u32) -> Option<(u32, u32)> {
        let target = byte_offset;
        if target > self.line_index.len {
            return None;
        }
        let line_index = match self.line_index.line_offsets.binary_search(&target) {
            Ok(exact) => exact,
            Err(after) => after.saturating_sub(1),
        };
        let line_start = *self.line_index.line_offsets.get(line_index)?;
        let character = if self.line_index.ascii {
            target.checked_sub(line_start)?
        } else {
            let source = self.source.as_deref()?;
            let target = target as usize;
            let line_start = line_start as usize;
            if !source.is_char_boundary(target) {
                return None;
            }
            source[line_start..target]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>()
                .try_into()
                .ok()?
        };
        Some((
            u32::try_from(line_index).expect(
                "INVARIANT VIOLATED: source line index exceeded u32. \
                 This is a bug because LSP positions require u32 lines. \
                 Fix: reject or segment files with more than u32::MAX lines.",
            ),
            character,
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileFacts {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    pub types: Vec<TypeFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
    pub execution_contexts: Vec<ExecutionContextFact>,
    pub inference: InferenceEvidence,
    pub local_read_types: Box<[(TextRange, RubyType)]>,
}

impl SemanticExportFingerprint {
    fn from_facts(facts: &FileFacts) -> Self {
        let mut exports = Vec::new();

        for fact in &facts.symbols {
            if matches!(
                fact.kind,
                SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
            ) {
                exports.push(export_hash(|hasher| {
                    stable_u8(hasher, 1);
                    stable_fqn(hasher, &fact.fqn);
                    stable_symbol_kind(hasher, fact.kind);
                }));
            }
        }
        for fact in &facts.methods {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 2);
                stable_fqn(hasher, &fact.fqn);
                stable_fqn(hasher, &fact.owner);
                stable_strings(hasher, &fact.params);
                stable_len(hasher, fact.param_facts.len());
                for parameter in &fact.param_facts {
                    stable_string(hasher, &parameter.name);
                    stable_method_param_kind(hasher, parameter.kind);
                    stable_optional_string(hasher, parameter.type_label.as_deref());
                    stable_optional_string(hasher, parameter.documentation.as_deref());
                }
                stable_optional_method(hasher, fact.delegate_receiver);
                stable_method_visibility(hasher, fact.visibility);
                stable_method_availability(hasher, &fact.availability);
                stable_optional_string(hasher, fact.documentation.as_deref());
                stable_optional_string(hasher, fact.return_type_label.as_deref());
            }));
        }
        for fact in &facts.method_visibility_overrides {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 3);
                stable_fqn(hasher, &fact.owner);
                stable_method(hasher, fact.method);
                stable_method_visibility(hasher, fact.visibility);
            }));
        }
        for fact in &facts.types {
            if matches!(
                fact.subject,
                TypeSubject::Constant(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
            ) {
                exports.push(export_hash(|hasher| {
                    stable_u8(hasher, 4);
                    stable_type_subject(hasher, &fact.subject);
                    stable_ruby_type(hasher, &fact.ruby_type);
                    stable_type_provenance(hasher, fact.provenance);
                }));
            }
        }
        for equation in &facts.inference.method_return_equations {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 8);
                stable_fqn(hasher, equation.method());
                match equation.base() {
                    MethodReturnBase::Bottom => stable_u8(hasher, 0),
                    MethodReturnBase::Proven(ruby_type) => {
                        stable_u8(hasher, 1);
                        stable_ruby_type(hasher, ruby_type);
                    }
                    MethodReturnBase::Unknown(reason) => {
                        stable_u8(hasher, 2);
                        stable_string(hasher, reason.code());
                    }
                }
                stable_len(hasher, equation.dependencies().len());
                for dependency in equation.dependencies() {
                    stable_fqn(hasher, dependency);
                }
            }));
        }
        for fact in &facts.graph_nodes {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 5);
                stable_fqn(hasher, &fact.fqn);
                stable_graph_node_kind(hasher, fact.kind);
            }));
        }
        for fact in &facts.graph_edges {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 6);
                stable_fqn(hasher, &fact.source);
                stable_fqn(hasher, &fact.target);
                stable_graph_edge_kind(hasher, fact.kind);
                stable_graph_edge_provenance(hasher, fact.provenance);
            }));
        }
        for fact in &facts.unresolved_graph_edges {
            exports.push(export_hash(|hasher| {
                stable_u8(hasher, 7);
                stable_fqn(hasher, &fact.source);
                stable_len(hasher, fact.target_parts.len());
                for part in &fact.target_parts {
                    stable_string(hasher, part.as_str());
                }
                stable_bool(hasher, fact.absolute);
                stable_fqn(hasher, &fact.context);
                stable_graph_edge_kind(hasher, fact.kind);
                stable_graph_edge_provenance(hasher, fact.provenance);
            }));
        }

        exports.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        export_hash(|hasher| {
            stable_len(hasher, exports.len());
            for fingerprint in &exports {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }
}

/// Stable identity of the engine's user-visible semantic result.
///
/// Unlike [`SemanticExportFingerprint`], this includes exact declaration and
/// reference ranges, every type fact, resolved graph state, diagnostics, and
/// framework execution contexts. Physical paths and engine-local file/FQN IDs
/// remain excluded so equivalent cold and cached indexing runs can be compared
/// across fresh processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticResultFingerprint {
    high: u64,
    low: u64,
}

impl SemanticResultFingerprint {
    pub fn stable_bytes(self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.high.to_le_bytes());
        bytes[8..].copy_from_slice(&self.low.to_le_bytes());
        bytes
    }
}

impl SemanticChange {
    pub fn classify(
        previous: Option<SemanticExportFingerprint>,
        current: SemanticExportFingerprint,
    ) -> Self {
        match previous {
            None => Self::InitialIndex,
            Some(previous) if previous == current => Self::BodyOnly,
            Some(_) => Self::ExportsChanged,
        }
    }
}

fn stable_u8(hasher: &mut StableExportHasher, value: u8) {
    hasher.write(&[value]);
}

fn stable_bool(hasher: &mut StableExportHasher, value: bool) {
    stable_u8(hasher, u8::from(value));
}

fn stable_u32(hasher: &mut StableExportHasher, value: u32) {
    hasher.write(&value.to_le_bytes());
}

fn stable_u64(hasher: &mut StableExportHasher, value: u64) {
    hasher.write(&value.to_le_bytes());
}

fn stable_len(hasher: &mut StableExportHasher, value: usize) {
    stable_u64(
        hasher,
        u64::try_from(value).expect(
            "INVARIANT VIOLATED: semantic export collection length exceeded u64. This is a bug because one process cannot hold that many facts. Fix: reject oversized semantic inputs before fingerprinting.",
        ),
    );
}

fn stable_string(hasher: &mut StableExportHasher, value: &str) {
    stable_len(hasher, value.len());
    hasher.write(value.as_bytes());
}

fn stable_strings(hasher: &mut StableExportHasher, values: &[String]) {
    stable_len(hasher, values.len());
    for value in values {
        stable_string(hasher, value);
    }
}

fn stable_optional_string(hasher: &mut StableExportHasher, value: Option<&str>) {
    match value {
        Some(value) => {
            stable_u8(hasher, 1);
            stable_string(hasher, value);
        }
        None => stable_u8(hasher, 0),
    }
}

fn stable_method(hasher: &mut StableExportHasher, method: RubyMethod) {
    stable_string(hasher, method.as_str());
}

fn stable_optional_method(hasher: &mut StableExportHasher, method: Option<RubyMethod>) {
    match method {
        Some(method) => {
            stable_u8(hasher, 1);
            stable_method(hasher, method);
        }
        None => stable_u8(hasher, 0),
    }
}

fn stable_optional_fqn(hasher: &mut StableExportHasher, fqn: Option<&FullyQualifiedName>) {
    match fqn {
        Some(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        None => stable_u8(hasher, 0),
    }
}

fn stable_range_offsets(hasher: &mut StableExportHasher, range: TextRange) {
    stable_u32(hasher, range.start_byte);
    stable_u32(hasher, range.end_byte);
}

fn stable_fqn(hasher: &mut StableExportHasher, fqn: &FullyQualifiedName) {
    match fqn {
        FullyQualifiedName::Namespace(parts, kind) => {
            stable_u8(hasher, 1);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
            match kind {
                NamespaceKind::Instance => stable_u8(hasher, 1),
                NamespaceKind::Singleton => stable_u8(hasher, 2),
            }
        }
        FullyQualifiedName::Constant(parts) => {
            stable_u8(hasher, 2);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
        }
        FullyQualifiedName::Method(parts, method) => {
            stable_u8(hasher, 3);
            stable_len(hasher, parts.len());
            for part in parts {
                stable_string(hasher, part.as_str());
            }
            stable_method(hasher, *method);
        }
        FullyQualifiedName::LocalVariable(name) => {
            stable_u8(hasher, 4);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::InstanceVariable(name) => {
            stable_u8(hasher, 5);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::ClassVariable(name) => {
            stable_u8(hasher, 6);
            stable_string(hasher, name.as_str());
        }
        FullyQualifiedName::GlobalVariable(name) => {
            stable_u8(hasher, 7);
            stable_string(hasher, name.as_str());
        }
    }
}

fn stable_symbol_kind(hasher: &mut StableExportHasher, kind: SymbolKind) {
    match kind {
        SymbolKind::Class => stable_u8(hasher, 1),
        SymbolKind::Module => stable_u8(hasher, 2),
        SymbolKind::Method => stable_u8(hasher, 3),
        SymbolKind::Constant => stable_u8(hasher, 4),
        SymbolKind::LocalVariable => stable_u8(hasher, 5),
        SymbolKind::InstanceVariable => stable_u8(hasher, 6),
        SymbolKind::ClassVariable => stable_u8(hasher, 7),
        SymbolKind::GlobalVariable => stable_u8(hasher, 8),
    }
}

fn stable_method_param_kind(hasher: &mut StableExportHasher, kind: MethodParamKind) {
    match kind {
        MethodParamKind::Required => stable_u8(hasher, 1),
        MethodParamKind::Optional => stable_u8(hasher, 2),
        MethodParamKind::Rest => stable_u8(hasher, 3),
        MethodParamKind::RequiredKeyword => stable_u8(hasher, 4),
        MethodParamKind::OptionalKeyword => stable_u8(hasher, 5),
        MethodParamKind::KeywordRest => stable_u8(hasher, 6),
        MethodParamKind::Block => stable_u8(hasher, 7),
        MethodParamKind::Forwarding => stable_u8(hasher, 8),
        MethodParamKind::AnonymousRest => stable_u8(hasher, 9),
        MethodParamKind::AnonymousKeywordRest => stable_u8(hasher, 10),
    }
}

fn stable_method_visibility(hasher: &mut StableExportHasher, visibility: MethodVisibility) {
    match visibility {
        MethodVisibility::Public => stable_u8(hasher, 1),
        MethodVisibility::Protected => stable_u8(hasher, 2),
        MethodVisibility::Private => stable_u8(hasher, 3),
    }
}

fn stable_method_reference_access(hasher: &mut StableExportHasher, access: MethodReferenceAccess) {
    match access {
        MethodReferenceAccess::Normal => stable_u8(hasher, 1),
        MethodReferenceAccess::ExplicitReceiver => stable_u8(hasher, 2),
        MethodReferenceAccess::VisibilityBypass => stable_u8(hasher, 3),
    }
}

fn stable_method_availability(hasher: &mut StableExportHasher, availability: &MethodAvailability) {
    match availability {
        MethodAvailability::Available => stable_u8(hasher, 1),
        MethodAvailability::Unavailable { reason } => {
            stable_u8(hasher, 2);
            stable_string(hasher, reason);
        }
        MethodAvailability::Absent { reason } => {
            stable_u8(hasher, 3);
            stable_string(hasher, reason);
        }
    }
}

fn stable_type_subject(hasher: &mut StableExportHasher, subject: &TypeSubject) {
    match subject {
        TypeSubject::Constant(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        TypeSubject::Local { scope_id, name } => {
            stable_u8(hasher, 2);
            stable_u32(hasher, *scope_id);
            stable_string(hasher, name);
        }
        TypeSubject::InstanceVariable { owner, name } => {
            stable_u8(hasher, 3);
            stable_fqn(hasher, owner);
            stable_string(hasher, name);
        }
        TypeSubject::ClassVariable { owner, name } => {
            stable_u8(hasher, 4);
            stable_fqn(hasher, owner);
            stable_string(hasher, name);
        }
        TypeSubject::GlobalVariable(name) => {
            stable_u8(hasher, 5);
            stable_string(hasher, name);
        }
        TypeSubject::MethodReturn(fqn) => {
            stable_u8(hasher, 6);
            stable_fqn(hasher, fqn);
        }
        TypeSubject::Parameter { method, name } => {
            stable_u8(hasher, 7);
            stable_fqn(hasher, method);
            stable_string(hasher, name);
        }
        TypeSubject::Expression(range) => {
            stable_u8(hasher, 8);
            stable_u32(hasher, range.start_byte);
            stable_u32(hasher, range.end_byte);
        }
    }
}

fn stable_ruby_type(hasher: &mut StableExportHasher, ruby_type: &RubyType) {
    match ruby_type {
        RubyType::Class(fqn) => {
            stable_u8(hasher, 1);
            stable_fqn(hasher, fqn);
        }
        RubyType::Module(fqn) => {
            stable_u8(hasher, 2);
            stable_fqn(hasher, fqn);
        }
        RubyType::ClassReference(fqn) => {
            stable_u8(hasher, 3);
            stable_fqn(hasher, fqn);
        }
        RubyType::ModuleReference(fqn) => {
            stable_u8(hasher, 4);
            stable_fqn(hasher, fqn);
        }
        RubyType::Array(elements) => {
            stable_u8(hasher, 5);
            stable_len(hasher, elements.len());
            for element in elements {
                stable_ruby_type(hasher, element);
            }
        }
        RubyType::Hash(keys, values) => {
            stable_u8(hasher, 6);
            stable_len(hasher, keys.len());
            for key in keys {
                stable_ruby_type(hasher, key);
            }
            stable_len(hasher, values.len());
            for value in values {
                stable_ruby_type(hasher, value);
            }
        }
        RubyType::Union(types) => {
            stable_u8(hasher, 7);
            stable_len(hasher, types.len());
            for ruby_type in types {
                stable_ruby_type(hasher, ruby_type);
            }
        }
        RubyType::Unknown => stable_u8(hasher, 8),
    }
}

fn stable_type_provenance(hasher: &mut StableExportHasher, provenance: TypeProvenance) {
    match provenance {
        TypeProvenance::Literal => stable_u8(hasher, 1),
        TypeProvenance::Assignment => stable_u8(hasher, 2),
        TypeProvenance::Flow => stable_u8(hasher, 3),
        TypeProvenance::Rbs => stable_u8(hasher, 4),
        TypeProvenance::Yard => stable_u8(hasher, 5),
        TypeProvenance::Runtime => stable_u8(hasher, 6),
        TypeProvenance::Extension => stable_u8(hasher, 7),
        TypeProvenance::Inferred => stable_u8(hasher, 8),
    }
}

fn stable_graph_node_kind(hasher: &mut StableExportHasher, kind: GraphNodeKind) {
    match kind {
        GraphNodeKind::Class => stable_u8(hasher, 1),
        GraphNodeKind::Module => stable_u8(hasher, 2),
    }
}

fn stable_graph_edge_kind(hasher: &mut StableExportHasher, kind: GraphEdgeKind) {
    match kind {
        GraphEdgeKind::Superclass => stable_u8(hasher, 1),
        GraphEdgeKind::Include => stable_u8(hasher, 2),
        GraphEdgeKind::Prepend => stable_u8(hasher, 3),
        GraphEdgeKind::Extend => stable_u8(hasher, 4),
        GraphEdgeKind::ExecutionContextApplication => stable_u8(hasher, 5),
    }
}

fn stable_graph_edge_provenance(hasher: &mut StableExportHasher, provenance: GraphEdgeProvenance) {
    match provenance {
        GraphEdgeProvenance::Explicit => stable_u8(hasher, 1),
        GraphEdgeProvenance::ImplicitObject => stable_u8(hasher, 2),
    }
}

fn stable_source_kind(hasher: &mut StableExportHasher, kind: SourceKind) {
    match kind {
        SourceKind::Project => stable_u8(hasher, 1),
        SourceKind::Excluded => stable_u8(hasher, 2),
        SourceKind::Signature => stable_u8(hasher, 3),
        SourceKind::External => stable_u8(hasher, 4),
        SourceKind::Stub => stable_u8(hasher, 5),
        SourceKind::Stdlib => stable_u8(hasher, 6),
        SourceKind::Gem => stable_u8(hasher, 7),
    }
}

fn stable_diagnostic_severity(hasher: &mut StableExportHasher, severity: DiagnosticSeverity) {
    match severity {
        DiagnosticSeverity::Error => stable_u8(hasher, 1),
        DiagnosticSeverity::Warning => stable_u8(hasher, 2),
        DiagnosticSeverity::Information => stable_u8(hasher, 3),
        DiagnosticSeverity::Hint => stable_u8(hasher, 4),
    }
}

fn stable_execution_scope_mode(hasher: &mut StableExportHasher, mode: ExecutionScopeMode) {
    match mode {
        ExecutionScopeMode::Preserve => stable_u8(hasher, 1),
    }
}

fn export_hash(mut hash_fields: impl FnMut(&mut StableExportHasher)) -> SemanticExportFingerprint {
    let mut hasher = StableExportHasher::new(0xcbf2_9ce4_8422_2325, 0x8422_2325_cbf2_9ce4);
    hash_fields(&mut hasher);
    let (high, low) = hasher.finish_lanes();
    SemanticExportFingerprint { high, low }
}

fn result_hash(mut hash_fields: impl FnMut(&mut StableExportHasher)) -> SemanticResultFingerprint {
    let mut hasher = StableExportHasher::new(0x517c_c1b7_2722_0a95, 0x2722_0a95_517c_c1b7);
    hash_fields(&mut hasher);
    let (high, low) = hasher.finish_lanes();
    SemanticResultFingerprint { high, low }
}

#[cfg(test)]
mod stable_export_hasher_tests {
    use super::*;
    use std::cell::Cell;

    fn legacy_lane(seed: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(seed, |state, byte| {
            (state ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn export_hash_visits_fields_once_and_preserves_both_legacy_lanes() {
        let visits = Cell::new(0usize);
        let fingerprint = export_hash(|hasher| {
            visits.set(visits.get() + 1);
            hasher.write(b"semantic-export");
        });

        assert_eq!(visits.get(), 1);
        assert_eq!(
            fingerprint.high,
            legacy_lane(0xcbf2_9ce4_8422_2325, b"semantic-export")
        );
        assert_eq!(
            fingerprint.low,
            legacy_lane(0x8422_2325_cbf2_9ce4, b"semantic-export")
        );
    }
}

struct StableExportHasher {
    high: u64,
    low: u64,
}

impl StableExportHasher {
    fn new(high_seed: u64, low_seed: u64) -> Self {
        Self {
            high: high_seed,
            low: low_seed,
        }
    }

    fn finish_lanes(&self) -> (u64, u64) {
        (self.high, self.low)
    }
}

impl Hasher for StableExportHasher {
    fn finish(&self) -> u64 {
        self.high
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.high ^= u64::from(*byte);
            self.high = self.high.wrapping_mul(0x0000_0100_0000_01b3);
            self.low ^= u64::from(*byte);
            self.low = self.low.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileInput {
    pub path: PathBuf,
    pub content: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Immediate,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticExportFingerprint {
    high: u64,
    low: u64,
}

impl SemanticExportFingerprint {
    /// Stable semantic identity bytes for validated derived-product keys.
    ///
    /// This does not expose engine stores. It only makes the already-public
    /// fingerprint portable across process boundaries without relying on
    /// `Debug`, Rust's randomized hashers, or layout-dependent serialization.
    pub fn stable_bytes(self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.high.to_le_bytes());
        bytes[8..].copy_from_slice(&self.low.to_le_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticChange {
    InitialIndex,
    BodyOnly,
    ExportsChanged,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisStats {
    pub files: usize,
    pub source_bytes: usize,
    pub symbols: usize,
    pub methods: usize,
    pub reference_candidates: usize,
    pub constant_reference_candidates: usize,
    pub method_reference_candidates: usize,
    pub resolved_reference_candidates: usize,
    pub references: usize,
    pub types: usize,
    pub diagnostic_candidates: usize,
    pub diagnostics: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub unresolved_graph_edges: usize,
}

/// Measurement counters for the most recent full `AnalysisEngine::resolve` pass.
///
/// These are process-local profiler evidence only. They must not change semantic
/// resolution policy, diagnostic emission, or project ownership.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvePassStats {
    pub method_return_equation_solve_runs: usize,
    pub graph_retry_ns: u64,
    pub diagnostic_seed_ns: u64,
    pub constant_candidates_ns: u64,
    pub method_candidates_ns: u64,
    pub sort_all_ns: u64,
    pub diagnostic_rebuild_ns: u64,
    pub constant_cache_hits: usize,
    pub constant_cache_misses: usize,
    pub constant_cache_unique_keys: usize,
    pub method_cache_hits: usize,
    pub method_cache_misses: usize,
    pub method_cache_unique_keys: usize,
    pub method_lookup_chain_cache_entries: usize,
    pub method_namespace_exists_cache_entries: usize,
    pub method_suggestion_cache_entries: usize,
    pub incomplete_method_chain_cache_entries: usize,
    /// Method candidates whose explicit receiver is another call expression
    /// and therefore must wait for an earlier call outcome in source order.
    pub deferred_receiver_candidates: usize,
    /// Deferred receiver candidates whose inner call produced a concrete type.
    pub deferred_receiver_proven: usize,
    /// Deferred receiver candidates whose inner call remained Unknown.
    pub deferred_receiver_unknown: usize,
    pub method_return_cache_hits: usize,
    pub method_return_cache_misses: usize,
    pub method_return_cache_entries: usize,
    pub method_visibility_cache_hits: usize,
    pub method_visibility_cache_misses: usize,
    pub method_visibility_cache_entries: usize,
    pub ambiguous_method_return_cache_hits: usize,
    pub ambiguous_method_return_cache_misses: usize,
    pub ambiguous_method_return_cache_entries: usize,
}

pub(super) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: resolve-pass elapsed nanoseconds overflowed u64. \
             This is a bug because one resolution pass cannot exceed u64::MAX nanoseconds. \
             Fix: inspect hung resolve instrumentation or widen the counter type."
        )
    })
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisMemoryStats {
    pub names: usize,
    pub files: usize,
    pub symbols: usize,
    pub methods: usize,
    pub types: usize,
    pub reference_candidates: usize,
    pub references: usize,
    pub diagnostics: usize,
    pub diagnostic_candidates: usize,
    pub graph: usize,
    pub unresolved_graph_edges: usize,
    pub query_caches: usize,
}

impl AnalysisMemoryStats {
    pub fn total(&self) -> usize {
        self.files
            + self.names
            + self.symbols
            + self.methods
            + self.types
            + self.reference_candidates
            + self.references
            + self.diagnostics
            + self.diagnostic_candidates
            + self.graph
            + self.unresolved_graph_edges
            + self.query_caches
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct SourceRegistry {
    pub(super) ids: FileIdMap,
    pub(super) files: HashMap<SourceFileId, SourceFile>,
}

#[derive(Debug, Default)]
pub(super) struct NameRegistry {
    state: NameRegistryState,
    #[cfg(test)]
    fqn_lookup_count: AtomicUsize,
}

impl Clone for NameRegistry {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            #[cfg(test)]
            fqn_lookup_count: AtomicUsize::new(self.fqn_lookup_count.load(Ordering::Relaxed)),
        }
    }
}

impl NameRegistry {
    pub(super) fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        let state = &mut self.state;
        let (index, _) = state.fqns.insert_full(fqn);
        FqnId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: FQN interner exceeded u32 ids. \
                 This is a bug because FqnId stores u32. \
                 Fix: widen FqnId before interning more than u32::MAX names.",
        ))
    }

    pub(super) fn fqn_id(&self, fqn: &FullyQualifiedName) -> Option<FqnId> {
        #[cfg(test)]
        self.fqn_lookup_count.fetch_add(1, Ordering::Relaxed);
        self.state
            .fqns
            .get_index_of(fqn)
            .map(|index| FqnId(u32::try_from(index).expect(
                "INVARIANT VIOLATED: FQN interner returned an index above u32. This is a bug because every inserted index is validated before becoming an FqnId. Fix: widen FqnId and its insertion boundary together.",
            )))
    }

    pub(super) fn fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.state.fqns.get_index(id.0 as usize)
    }

    #[cfg(test)]
    fn reset_fqn_lookup_count_for_test(&self) {
        self.fqn_lookup_count.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn fqn_lookup_count_for_test(&self) -> usize {
        self.fqn_lookup_count.load(Ordering::Relaxed)
    }

    pub(super) fn intern_const_lookup(&mut self, lookup: ConstLookup) -> ConstLookupId {
        let state = &mut self.state;
        let (index, _) = state.const_lookups.insert_full(lookup);
        ConstLookupId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: constant lookup interner exceeded u32 ids. \
                 This is a bug because ConstLookupId stores u32. \
                 Fix: widen ConstLookupId before interning more than u32::MAX lookups.",
        ))
    }

    pub(super) fn const_lookup(&self, id: ConstLookupId) -> Option<&ConstLookup> {
        self.state.const_lookups.get_index(id.0 as usize)
    }

    fn estimated_heap_bytes(&self) -> usize {
        let state = &self.state;
        state.fqns.capacity() * (size_of::<FullyQualifiedName>() + size_of::<usize>() + 1)
            + state.fqns.iter().map(fqn_heap_bytes).sum::<usize>()
            + state.const_lookups.capacity() * (size_of::<ConstLookup>() + size_of::<usize>() + 1)
            + state
                .const_lookups
                .iter()
                .map(const_lookup_heap_bytes)
                .sum::<usize>()
    }
}

fn constant_path_heap_bytes(path: &ConstantPath) -> usize {
    if path.spilled() {
        path.capacity() * size_of::<RubyConstant>()
    } else {
        0
    }
}

fn const_lookup_heap_bytes(lookup: &ConstLookup) -> usize {
    constant_path_heap_bytes(&lookup.path)
}

#[derive(Debug, Clone, Default)]
struct NameRegistryState {
    fqns: IndexSet<FullyQualifiedName>,
    const_lookups: IndexSet<ConstLookup>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FactArena {
    pub(super) definitions: DefinitionFacts,
    pub(super) references: ReferenceFacts,
    pub(super) types: TypeStore,
    pub(super) diagnostics: DiagnosticFacts,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DefinitionFacts {
    pub(super) symbols: SymbolStore,
    pub(super) methods: MethodStore,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReferenceFacts {
    pub(super) candidates: ReferenceCandidateStore,
    pub(super) resolved: ReferenceStore,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DiagnosticFacts {
    pub(super) candidates: DiagnosticCandidateStore,
    pub(super) resolved: DiagnosticStore,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug)]
pub struct AnalysisEngine {
    instance_id: u64,
    semantic_revision: u64,
    pub(super) sources: SourceRegistry,
    pub(super) names: NameRegistry,
    pub(super) facts: FactArena,
    pub(super) graph: SemanticGraph,
    pub(super) method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    execution_contexts: HashMap<SourceFileId, Vec<ExecutionContextFact>>,
    inference_by_file: HashMap<SourceFileId, InferenceEvidence>,
    call_expression_outcomes_by_file:
        HashMap<SourceFileId, Box<[(TextRange, StoredTypeInferenceOutcome)]>>,
    local_read_types_by_file:
        HashMap<SourceFileId, Box<[(TextRange, crate::core::type_store::RubyTypeId)]>>,
    method_return_equations_dirty: bool,
    constant_type_equations_dirty: bool,
    method_return_solution_spans_files: bool,
    semantic_export_fingerprints: HashMap<SourceFileId, SemanticExportFingerprint>,
    top_level_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    universal_object_method_lookup_chain_cache: Mutex<Option<Vec<FullyQualifiedName>>>,
    last_resolve_pass: ResolvePassStats,
}

static NEXT_ANALYSIS_ENGINE_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_analysis_engine_instance_id() -> u64 {
    NEXT_ANALYSIS_ENGINE_INSTANCE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| {
            panic!(
                "INVARIANT VIOLATED: analysis engine instance identity exhausted u64. \
                 This is a bug because query caches require a unique engine identity. \
                 Fix: widen the identity before creating u64::MAX engine instances."
            )
        })
}

impl Default for AnalysisEngine {
    fn default() -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: 0,
            sources: SourceRegistry::default(),
            names: NameRegistry::default(),
            facts: FactArena::default(),
            graph: SemanticGraph::default(),
            method_visibility_overrides: Vec::new(),
            execution_contexts: HashMap::new(),
            inference_by_file: HashMap::new(),
            call_expression_outcomes_by_file: HashMap::new(),
            local_read_types_by_file: HashMap::new(),
            method_return_equations_dirty: false,
            constant_type_equations_dirty: false,
            method_return_solution_spans_files: false,
            semantic_export_fingerprints: HashMap::new(),
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: ResolvePassStats::default(),
        }
    }
}

impl Clone for AnalysisEngine {
    fn clone(&self) -> Self {
        Self {
            instance_id: next_analysis_engine_instance_id(),
            semantic_revision: self.semantic_revision,
            sources: self.sources.clone(),
            names: self.names.clone(),
            facts: self.facts.clone(),
            graph: self.graph.clone(),
            method_visibility_overrides: self.method_visibility_overrides.clone(),
            execution_contexts: self.execution_contexts.clone(),
            inference_by_file: self.inference_by_file.clone(),
            call_expression_outcomes_by_file: self.call_expression_outcomes_by_file.clone(),
            local_read_types_by_file: self.local_read_types_by_file.clone(),
            method_return_equations_dirty: self.method_return_equations_dirty,
            constant_type_equations_dirty: self.constant_type_equations_dirty,
            method_return_solution_spans_files: self.method_return_solution_spans_files,
            semantic_export_fingerprints: self.semantic_export_fingerprints.clone(),
            top_level_method_lookup_chain_cache: Mutex::new(None),
            universal_object_method_lookup_chain_cache: Mutex::new(None),
            last_resolve_pass: ResolvePassStats::default(),
        }
    }
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId {
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.register_indexed_file(file.path, file.kind, line_index, content_hash, source, None)
    }

    /// Register a locked gem source with explicit package identity for library-tree grouping.
    pub fn register_gem_file(
        &mut self,
        file: SourceFileInput,
        package: crate::core::LibraryPackageId,
    ) -> SourceFileId {
        assert!(
            file.kind == SourceKind::Gem,
            "INVARIANT VIOLATED: register_gem_file received SourceKind::{:?}. \
             This is a bug because only Gem sources carry locked package identity. \
             Fix: use register_file for non-gem sources or pass SourceKind::Gem.",
            file.kind
        );
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.register_indexed_file(
            file.path,
            file.kind,
            line_index,
            content_hash,
            source,
            Some(package),
        )
    }

    /// Register source whose caller retains the owned buffer. ASCII files need
    /// only their line index and content hash after collection; non-ASCII files
    /// retain one engine-owned copy for exact UTF-16 conversion.
    pub fn register_file_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        let line_index = SourceLineIndex::new(content);
        let content_hash = source_hash(content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(content.to_string())
        };
        self.register_indexed_file(path, kind, line_index, content_hash, source, None)
    }

    fn register_indexed_file(
        &mut self,
        path: PathBuf,
        kind: SourceKind,
        line_index: SourceLineIndex,
        content_hash: u64,
        source: Option<String>,
        library_package: Option<crate::core::LibraryPackageId>,
    ) -> SourceFileId {
        let id = self.sources.ids.get_or_insert(&path);
        self.sources.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source,
                line_index,
                content_hash,
                kind,
                library_package,
            },
        );
        id
    }

    pub fn replace_facts(
        &mut self,
        file_id: SourceFileId,
        facts: FileFacts,
        mode: ResolveMode,
    ) -> SemanticChange {
        let fingerprint = SemanticExportFingerprint::from_facts(&facts);
        let previous = self
            .semantic_export_fingerprints
            .insert(file_id, fingerprint);
        let change = SemanticChange::classify(previous, fingerprint);
        self.replace_facts_deferred(file_id, facts);
        match mode {
            ResolveMode::Immediate => self.resolve(),
            ResolveMode::Deferred => {}
        }
        change
    }

    pub fn resolve(&mut self) {
        let mut stats = ResolvePassStats::default();
        let graph_retry_started = Instant::now();
        self.retry_unresolved_graph_edges();
        stats.graph_retry_ns = elapsed_ns(graph_retry_started);
        self.resolve_constant_type_equations();
        stats.method_return_equation_solve_runs =
            usize::from(self.resolve_method_return_equations());
        self.resolve_reference_candidates(&mut stats);
        self.last_resolve_pass = stats;
    }

    /// Profiler evidence for the most recent full `resolve()` pass.
    pub fn last_resolve_stats(&self) -> &ResolvePassStats {
        &self.last_resolve_pass
    }

    pub fn resolve_file(&mut self, file_id: SourceFileId) {
        self.resolve_files(&[file_id]);
    }

    pub fn resolve_files(&mut self, file_ids: &[SourceFileId]) {
        let mut unique = HashSet::with_capacity(file_ids.len());
        for file_id in file_ids {
            self.assert_known_file_id(
                *file_id,
                "selected-file resolve references unknown source file id",
            );
            assert!(
                unique.insert(*file_id),
                "INVARIANT VIOLATED: selected-file semantic resolution contains duplicate file \
                 id {:?}. This is a bug because one staged resolution must replace each file's \
                 references and diagnostics exactly once. Fix: sort and deduplicate the selected \
                 file ids before calling AnalysisEngine::resolve_files.",
                file_id
            );
        }
        if file_ids.is_empty() {
            return;
        }
        self.retry_unresolved_graph_edges();
        self.resolve_constant_type_equations();
        self.resolve_method_return_equations();
        for file_id in file_ids {
            self.resolve_reference_candidates_in_file(*file_id);
        }
    }

    pub fn shrink_to_fit(&mut self) {
        self.sources.ids.shrink_to_fit();
        self.sources.files.shrink_to_fit();
        for file in self.sources.files.values_mut() {
            file.path.shrink_to_fit();
            if let Some(source) = &mut file.source {
                source.shrink_to_fit();
            }
            file.line_index.shrink_to_fit();
        }

        self.names.state.fqns.shrink_to_fit();
        self.names.state.const_lookups.shrink_to_fit();

        self.facts.definitions.symbols.shrink_to_fit();
        self.facts.definitions.methods.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.graph.shrink_to_fit();
        self.facts.references.candidates.shrink_to_fit();
        self.facts.references.resolved.shrink_to_fit();
        self.facts.diagnostics.candidates.shrink_to_fit();
        self.facts.diagnostics.resolved.shrink_to_fit();
        self.inference_by_file.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }

    pub fn query(&self) -> AnalysisQuery<'_> {
        AnalysisQuery::new(self)
    }

    pub fn stats(&self) -> AnalysisStats {
        let reference_candidate_stats = self.facts.references.candidates.stats();
        AnalysisStats {
            files: self.sources.files.len(),
            source_bytes: self
                .sources
                .files
                .values()
                .map(|file| file.line_index.len())
                .sum(),
            symbols: self.facts.definitions.symbols.fact_count(),
            methods: self.facts.definitions.methods.fact_count(),
            reference_candidates: self.facts.references.candidates.candidate_count(),
            constant_reference_candidates: reference_candidate_stats.constants,
            method_reference_candidates: reference_candidate_stats.methods,
            resolved_reference_candidates: reference_candidate_stats.resolved,
            references: self.facts.references.resolved.fact_count(),
            types: self.facts.types.fact_count(),
            diagnostic_candidates: self.facts.diagnostics.candidates.candidate_count(),
            diagnostics: self.facts.diagnostics.resolved.fact_count(),
            graph_nodes: self.graph.node_count(),
            graph_edges: self.graph.edge_count(),
            unresolved_graph_edges: self.graph.unresolved_edges().len(),
        }
    }

    pub fn estimated_memory_stats(&self) -> AnalysisMemoryStats {
        AnalysisMemoryStats {
            names: self.names.estimated_heap_bytes(),
            files: self.estimated_file_store_heap_bytes(),
            symbols: self.facts.definitions.symbols.estimated_heap_bytes(),
            methods: self.facts.definitions.methods.estimated_heap_bytes(),
            types: self.facts.types.estimated_heap_bytes(),
            reference_candidates: self.facts.references.candidates.estimated_heap_bytes(),
            references: self.facts.references.resolved.estimated_heap_bytes(),
            diagnostics: self.facts.diagnostics.resolved.estimated_heap_bytes(),
            diagnostic_candidates: self.facts.diagnostics.candidates.estimated_heap_bytes(),
            graph: self.graph.estimated_heap_bytes(),
            unresolved_graph_edges: self.graph.estimated_unresolved_heap_bytes(),
            query_caches: self.estimated_method_lookup_chain_cache_heap_bytes(),
        }
    }

    fn estimated_method_lookup_chain_cache_heap_bytes(&self) -> usize {
        let chain_bytes = |chain: &Vec<FullyQualifiedName>| {
            vec_payload_bytes(chain) + chain.iter().map(fqn_heap_bytes).sum::<usize>()
        };
        self.top_level_method_lookup_chain_cache
            .lock()
            .as_ref()
            .map(chain_bytes)
            .unwrap_or(0)
            + self
                .universal_object_method_lookup_chain_cache
                .lock()
                .as_ref()
                .map(chain_bytes)
                .unwrap_or(0)
    }

    fn estimated_file_store_heap_bytes(&self) -> usize {
        self.sources.ids.estimated_heap_bytes()
            + self.sources.files.capacity()
                * (size_of::<SourceFileId>() + size_of::<SourceFile>() + 1)
            + self
                .sources
                .files
                .values()
                .map(|file| {
                    file.path.as_os_str().len()
                        + file.source.as_ref().map(String::capacity).unwrap_or(0)
                        + vec_payload_bytes(&file.line_index.line_offsets)
                })
                .sum::<usize>()
            + self.semantic_export_fingerprints.capacity()
                * (size_of::<SourceFileId>() + size_of::<SemanticExportFingerprint>() + 1)
            + self.inference_by_file.capacity()
                * (size_of::<SourceFileId>() + size_of::<InferenceEvidence>() + 1)
            + self
                .inference_by_file
                .values()
                .map(InferenceEvidence::estimated_heap_bytes)
                .sum::<usize>()
            + self.call_expression_outcomes_by_file.capacity()
                * (size_of::<SourceFileId>()
                    + size_of::<Box<[(TextRange, StoredTypeInferenceOutcome)]>>()
                    + 1)
            + self
                .call_expression_outcomes_by_file
                .values()
                .map(|outcomes| {
                    outcomes.len() * size_of::<(TextRange, StoredTypeInferenceOutcome)>()
                })
                .sum::<usize>()
            + self.local_read_types_by_file.capacity()
                * (size_of::<SourceFileId>()
                    + size_of::<Box<[(TextRange, crate::core::type_store::RubyTypeId)]>>()
                    + 1)
            + self
                .local_read_types_by_file
                .values()
                .map(|reads| {
                    reads.len() * size_of::<(TextRange, crate::core::type_store::RubyTypeId)>()
                })
                .sum::<usize>()
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.sources.ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.sources.files.get(&id)
    }

    pub fn file_content_matches(&self, id: SourceFileId, content: &str) -> bool {
        self.file(id)
            .is_some_and(|file| file.content_hash == source_hash(content))
    }

    pub fn semantic_export_fingerprint(
        &self,
        file_id: SourceFileId,
    ) -> Option<SemanticExportFingerprint> {
        self.semantic_export_fingerprints.get(&file_id).copied()
    }

    /// Stable semantic identity for an immutable dependency seed.
    ///
    /// Physical paths and engine-local file IDs are deliberately excluded so
    /// equivalent runtime/core inputs can share one project-neutral producer.
    /// Source kind remains part of identity because definition precedence and
    /// edit/diagnostic policy differ between implementations, stubs, and
    /// signatures.
    pub fn semantic_context_fingerprint(&self) -> SemanticExportFingerprint {
        let mut file_fingerprints = self
            .semantic_export_fingerprints
            .iter()
            .map(|(file_id, fingerprint)| {
                let source = self.sources.files.get(file_id).expect(
                    "INVARIANT VIOLATED: semantic export fingerprint has no registered source file. This is a bug because replace_facts validates every file id before recording semantic state. Fix: remove fingerprints through the same file lifecycle as source registration.",
                );
                export_hash(|hasher| {
                    stable_source_kind(hasher, source.kind);
                    stable_u64(hasher, fingerprint.high);
                    stable_u64(hasher, fingerprint.low);
                })
            })
            .collect::<Vec<_>>();
        file_fingerprints.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        export_hash(|hasher| {
            stable_len(hasher, file_fingerprints.len());
            for fingerprint in &file_fingerprints {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }

    /// Stable, path-independent identity of every user-visible semantic fact,
    /// partitioned by its owning source file.
    pub fn semantic_result_file_fingerprints(
        &self,
    ) -> Vec<(SourceFileId, SemanticResultFingerprint)> {
        fn push_component(
            components: &mut HashMap<SourceFileId, Vec<SemanticExportFingerprint>>,
            file_id: SourceFileId,
            component: SemanticExportFingerprint,
        ) {
            components.get_mut(&file_id).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: semantic result fact belongs to unknown file id {:?}. This is a bug because every stored fact must be owned by one registered source. Fix: remove and replace facts through the same file lifecycle.",
                    file_id
                )
            }).push(component);
        }

        let mut components = self
            .sources
            .files
            .keys()
            .copied()
            .map(|file_id| (file_id, Vec::new()))
            .collect::<HashMap<_, _>>();

        for fact in self.all_symbol_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 1);
                    stable_fqn(hasher, &fact.fqn);
                    stable_symbol_kind(hasher, fact.kind);
                    stable_range_offsets(hasher, fact.name_range);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_method_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 2);
                    stable_fqn(hasher, &fact.fqn);
                    stable_fqn(hasher, &fact.owner);
                    stable_range_offsets(hasher, fact.name_range);
                    stable_range_offsets(hasher, fact.range);
                    stable_strings(hasher, &fact.params);
                    stable_len(hasher, fact.param_facts.len());
                    for parameter in &fact.param_facts {
                        stable_string(hasher, &parameter.name);
                        stable_method_param_kind(hasher, parameter.kind);
                        stable_optional_string(hasher, parameter.type_label.as_deref());
                        stable_optional_string(hasher, parameter.documentation.as_deref());
                    }
                    stable_optional_method(hasher, fact.delegate_receiver);
                    stable_method_visibility(hasher, fact.visibility);
                    stable_method_availability(hasher, &fact.availability);
                    stable_optional_string(hasher, fact.documentation.as_deref());
                    stable_optional_string(hasher, fact.return_type_label.as_deref());
                }),
            );
        }
        for fact in &self.method_visibility_overrides {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 3);
                    stable_fqn(hasher, &fact.owner);
                    stable_method(hasher, fact.method);
                    stable_method_visibility(hasher, fact.visibility);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.facts.types.all_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 4);
                    stable_type_subject(hasher, &fact.subject);
                    stable_ruby_type(hasher, &fact.ruby_type);
                    stable_type_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_graph_nodes() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 5);
                    stable_fqn(hasher, &fact.fqn);
                    stable_graph_node_kind(hasher, fact.kind);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.all_graph_edges() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 6);
                    stable_fqn(hasher, &fact.source);
                    stable_fqn(hasher, &fact.target);
                    stable_graph_edge_kind(hasher, fact.kind);
                    stable_graph_edge_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.unresolved_graph_edges() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 7);
                    stable_fqn(hasher, &fact.source);
                    stable_len(hasher, fact.target_parts.len());
                    for part in &fact.target_parts {
                        stable_string(hasher, part.as_str());
                    }
                    stable_bool(hasher, fact.absolute);
                    stable_fqn(hasher, &fact.context);
                    stable_graph_edge_kind(hasher, fact.kind);
                    stable_graph_edge_provenance(hasher, fact.provenance);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for (target, fact) in self.facts.references.resolved.iter_facts_with_targets() {
            let target = self.names.fqn(target).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: resolved reference target {:?} has no interned FQN. This is a bug because stored references must retain a valid semantic target. Fix: intern targets before resolving references and remove them only with their facts.",
                    target
                )
            });
            let caller = fact.caller.map(|caller| {
                self.names.fqn(caller).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: resolved reference caller {:?} has no interned FQN. This is a bug because caller provenance must remain valid while the reference exists. Fix: intern callers before resolving references and remove them only with their facts.",
                        caller
                    )
                })
            });
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 8);
                    stable_fqn(hasher, target);
                    stable_optional_fqn(hasher, caller);
                    stable_method_reference_access(hasher, fact.access);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for fact in self.facts.diagnostics.resolved.all_facts() {
            push_component(
                &mut components,
                fact.range.file_id,
                export_hash(|hasher| {
                    stable_u8(hasher, 9);
                    stable_diagnostic_severity(hasher, fact.severity);
                    stable_string(hasher, &fact.code);
                    stable_string(hasher, &fact.message);
                    stable_range_offsets(hasher, fact.range);
                }),
            );
        }
        for (file_id, contexts) in &self.execution_contexts {
            for context in contexts {
                push_component(
                    &mut components,
                    *file_id,
                    export_hash(|hasher| {
                        stable_u8(hasher, 10);
                        stable_fqn(hasher, &context.lexical_namespace);
                        stable_fqn(hasher, &context.implicit_receiver);
                        stable_fqn(hasher, &context.method_definition_owner);
                        stable_execution_scope_mode(hasher, context.lexical_scope);
                        stable_execution_scope_mode(hasher, context.local_scope);
                        stable_string(hasher, &context.extension_id);
                        stable_range_offsets(hasher, context.range);
                    }),
                );
            }
        }
        for (file_id, reads) in &self.local_read_types_by_file {
            for (range, ruby_type) in reads.as_ref() {
                let ruby_type = self.facts.types.ruby_type(*ruby_type);
                push_component(
                    &mut components,
                    *file_id,
                    export_hash(|hasher| {
                        stable_u8(hasher, 11);
                        stable_range_offsets(hasher, *range);
                        stable_ruby_type(hasher, ruby_type);
                    }),
                );
            }
        }

        components
            .into_iter()
            .map(|(file_id, mut facts)| {
                let source = self.sources.files.get(&file_id).expect(
                    "INVARIANT VIOLATED: semantic result component owner has no registered source file. This is a bug because the component map is seeded exclusively from registered sources. Fix: keep source removal and semantic fact removal atomic.",
                );
                facts.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
                (
                    file_id,
                    result_hash(|hasher| {
                        stable_source_kind(hasher, source.kind);
                        stable_len(hasher, facts.len());
                        for fact in &facts {
                            stable_u64(hasher, fact.high);
                            stable_u64(hasher, fact.low);
                        }
                    }),
                )
            })
            .collect()
    }

    /// Stable per-file fingerprints for the three resolution-owned result
    /// categories that are not already isolated by semantic export and
    /// diagnostic manifests: resolved references, framework execution
    /// contexts, and proven local-read types.
    pub fn semantic_resolution_file_fingerprints(
        &self,
    ) -> HashMap<SourceFileId, [SemanticResultFingerprint; 3]> {
        let category_hash = |tag: u8, mut components: Vec<SemanticExportFingerprint>| {
            components.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
            result_hash(|hasher| {
                stable_u8(hasher, tag);
                stable_len(hasher, components.len());
                for component in &components {
                    stable_u64(hasher, component.high);
                    stable_u64(hasher, component.low);
                }
            })
        };
        let mut components = self
            .sources
            .files
            .keys()
            .copied()
            .map(|file_id| (file_id, [Vec::new(), Vec::new(), Vec::new()]))
            .collect::<HashMap<_, _>>();

        for (target, fact) in self.facts.references.resolved.iter_facts_with_targets() {
            let target = self.names.fqn(target).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: per-file reference fingerprint target {:?} has no interned FQN. This is a bug because resolved references retain their target identity. Fix: remove references before removing interned names.",
                    target,
                )
            });
            let caller = fact.caller.map(|caller| {
                self.names.fqn(caller).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: per-file reference fingerprint caller {:?} has no interned FQN. This is a bug because resolved references retain caller provenance. Fix: remove references before removing interned names.",
                        caller,
                    )
                })
            });
            let component = export_hash(|hasher| {
                stable_fqn(hasher, target);
                stable_optional_fqn(hasher, caller);
                stable_method_reference_access(hasher, fact.access);
                stable_range_offsets(hasher, fact.range);
            });
            components
                .get_mut(&fact.range.file_id)
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: per-file reference fingerprint belongs to unknown file {:?}. This is a bug because resolved references cannot outlive their registered source. Fix: remove references before unregistering files.",
                        fact.range.file_id,
                    )
                })[0]
                .push(component);
        }
        for (file_id, contexts) in &self.execution_contexts {
            let output = &mut components
                .get_mut(file_id)
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: execution-context fingerprint belongs to unknown file {:?}. This is a bug because execution contexts cannot outlive their registered source. Fix: remove contexts before unregistering files.",
                        file_id,
                    )
                })[1];
            output.extend(contexts.iter().map(|context| {
                export_hash(|hasher| {
                    stable_fqn(hasher, &context.lexical_namespace);
                    stable_fqn(hasher, &context.implicit_receiver);
                    stable_fqn(hasher, &context.method_definition_owner);
                    stable_execution_scope_mode(hasher, context.lexical_scope);
                    stable_execution_scope_mode(hasher, context.local_scope);
                    stable_string(hasher, &context.extension_id);
                    stable_range_offsets(hasher, context.range);
                })
            }));
        }
        for (file_id, reads) in &self.local_read_types_by_file {
            let output = &mut components
                .get_mut(file_id)
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: local-read fingerprint belongs to unknown file {:?}. This is a bug because flow evidence cannot outlive its registered source. Fix: remove inference evidence before unregistering files.",
                        file_id,
                    )
                })[2];
            output.extend(reads.iter().map(|(range, ruby_type)| {
                export_hash(|hasher| {
                    stable_range_offsets(hasher, *range);
                    stable_ruby_type(hasher, self.facts.types.ruby_type(*ruby_type));
                })
            }));
        }

        components
            .into_iter()
            .map(|(file_id, [references, contexts, local_reads])| {
                (
                    file_id,
                    [
                        category_hash(1, references),
                        category_hash(2, contexts),
                        category_hash(3, local_reads),
                    ],
                )
            })
            .collect()
    }

    /// Stable, path-independent identity of every user-visible semantic fact.
    ///
    /// This is intended for cross-process correctness evidence, not query
    /// lookup. Each fact is reduced to an order-independent stable component;
    /// the final multiset preserves file ownership and source-kind precedence
    /// without retaining engine-local IDs or physical paths.
    pub fn semantic_result_fingerprint(&self) -> SemanticResultFingerprint {
        let mut file_fingerprints = self
            .semantic_result_file_fingerprints()
            .into_iter()
            .map(|(_, fingerprint)| fingerprint)
            .collect::<Vec<_>>();
        file_fingerprints.sort_unstable_by_key(|fingerprint| (fingerprint.high, fingerprint.low));
        result_hash(|hasher| {
            stable_len(hasher, file_fingerprints.len());
            for fingerprint in &file_fingerprints {
                stable_u64(hasher, fingerprint.high);
                stable_u64(hasher, fingerprint.low);
            }
        })
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.sources.files.values()
    }

    fn replace_facts_deferred(&mut self, file_id: SourceFileId, mut facts: FileFacts) {
        self.semantic_revision = self.semantic_revision.checked_add(1).expect(
            "INVARIANT VIOLATED: analysis engine semantic revision exhausted u64. \
             This is a bug because cached queries require monotonic invalidation. \
             Fix: widen the semantic revision before performing u64::MAX replacements.",
        );
        *self.top_level_method_lookup_chain_cache.get_mut() = None;
        *self.universal_object_method_lookup_chain_cache.get_mut() = None;
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        for (range, ruby_type) in facts.local_read_types.as_ref() {
            assert_eq!(
                range.file_id, file_id,
                "INVARIANT VIOLATED: compact local-read type belongs to a different file. This is a bug because inference evidence must be replaced atomically with its source. Fix: attach the registered SourceFileId while converting TypeTracker offsets."
            );
            assert!(
                *ruby_type != RubyType::Unknown,
                "INVARIANT VIOLATED: compact local-read evidence contains Unknown at {range:?}. This is a bug because only proven flow types may enter local_read_types. Fix: retain the failure in expression_unknown_reasons instead."
            );
        }
        for adjacent in facts.local_read_types.windows(2) {
            assert!(
                adjacent[0].0 < adjacent[1].0,
                "INVARIANT VIOLATED: compact local-read evidence is duplicated or unsorted. This is a bug because deterministic range queries require one result per AST read. Fix: sort and deduplicate TypeTracker results before engine replacement."
            );
        }
        let equations_changed = match self.inference_by_file.get(&file_id) {
            Some(previous) => {
                previous.method_return_equations != facts.inference.method_return_equations
            }
            None => !facts.inference.method_return_equations.is_empty(),
        };
        if !equations_changed {
            if let Some(previous) = self.inference_by_file.get(&file_id) {
                facts.inference.method_return_outcomes = previous.method_return_outcomes.clone();
                facts.inference.telemetry = previous.telemetry.clone();
                for fact in &mut facts.types {
                    if fact.provenance != TypeProvenance::Inferred {
                        continue;
                    }
                    let TypeSubject::MethodReturn(method) = &fact.subject else {
                        continue;
                    };
                    if let Some(outcome) = previous.method_return_outcomes.get(method) {
                        fact.ruby_type = outcome.clone().into_ruby_type();
                    }
                }
            }
        }
        let symbols = self.intern_symbol_facts(facts.symbols);
        self.facts
            .definitions
            .symbols
            .replace_file(file_id, symbols);
        let methods = self.intern_method_facts(facts.methods);
        self.facts
            .definitions
            .methods
            .replace_file(file_id, methods);
        self.method_visibility_overrides
            .retain(|fact| fact.range.file_id != file_id);
        self.method_visibility_overrides
            .extend(facts.method_visibility_overrides);
        self.facts.types.replace_file(file_id, facts.types);
        self.replace_execution_contexts(file_id, facts.execution_contexts);
        let graph_nodes = self.intern_graph_node_facts(facts.graph_nodes);
        let graph_edges = self.intern_graph_edge_facts(facts.graph_edges);
        let unresolved_graph_edges =
            self.intern_unresolved_graph_edge_facts(facts.unresolved_graph_edges);
        self.graph
            .replace_file(file_id, graph_nodes, graph_edges, unresolved_graph_edges);

        let reference_candidates = self.intern_reference_candidates(facts.reference_candidates);
        self.facts
            .references
            .candidates
            .replace_file(file_id, reference_candidates);
        self.facts
            .diagnostics
            .candidates
            .replace_file(file_id, facts.diagnostic_candidates);
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, facts.diagnostics);
        let call_expression_outcomes =
            std::mem::take(&mut facts.inference.call_expression_outcomes);
        self.inference_by_file.insert(file_id, facts.inference);
        if call_expression_outcomes.is_empty() {
            self.call_expression_outcomes_by_file.remove(&file_id);
        } else {
            let outcomes = call_expression_outcomes
                .into_iter()
                .map(|(range, outcome)| {
                    (
                        range,
                        StoredTypeInferenceOutcome::from_domain(&mut self.facts.types, outcome),
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.call_expression_outcomes_by_file
                .insert(file_id, outcomes);
        }
        if facts.local_read_types.is_empty() {
            self.local_read_types_by_file.remove(&file_id);
        } else {
            let local_read_types = facts
                .local_read_types
                .into_vec()
                .into_iter()
                .map(|(range, ruby_type)| (range, self.facts.types.intern_ruby_type(ruby_type)))
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.local_read_types_by_file
                .insert(file_id, local_read_types);
        }
        self.method_return_equations_dirty |= equations_changed;
        self.constant_type_equations_dirty = self.inference_by_file.values().any(|evidence| {
            !evidence.constant_type_equations.is_empty()
                || evidence
                    .method_return_equations
                    .iter()
                    .any(|equation| !equation.constant_dependencies().is_empty())
        });
    }

    fn resolve_constant_type_equations(&mut self) -> bool {
        if !self.constant_type_equations_dirty {
            return false;
        }
        let mut equations = self
            .inference_by_file
            .values()
            .flat_map(|evidence| evidence.constant_type_equations.iter().cloned())
            .collect::<Vec<ConstantTypeEquation>>();
        equations.sort();
        equations.dedup();
        if equations.is_empty() {
            if self.inference_by_file.values().any(|evidence| {
                evidence
                    .method_return_equations
                    .iter()
                    .any(|equation| !equation.constant_dependencies().is_empty())
            }) {
                self.method_return_equations_dirty = true;
            }
            self.constant_type_equations_dirty = false;
            return false;
        }

        let mut dependencies = equations
            .iter()
            .flat_map(|equation| equation.dependencies().iter().cloned())
            .collect::<Vec<ConstantTypeDependency>>();
        dependencies.extend(
            self.inference_by_file
                .values()
                .flat_map(|evidence| evidence.method_return_equations.iter())
                .flat_map(|equation| equation.constant_dependencies().iter().cloned()),
        );
        dependencies.sort();
        dependencies.dedup();
        let query = AnalysisQuery::new(self);
        let resolved_dependencies = dependencies
            .into_iter()
            .map(|dependency| {
                let resolved = resolve_constant_dependency(&query, &dependency);
                (dependency, resolved)
            })
            .collect::<BTreeMap<_, _>>();
        let constant_facts = self
            .facts
            .types
            .all_facts()
            .into_iter()
            .filter_map(|fact| {
                let TypeSubject::Constant(constant) = &fact.subject else {
                    return None;
                };
                Some(ConstantFactInput {
                    constant: constant.clone(),
                    target: ConstantTypeTarget::Fact {
                        subject: fact.subject.clone(),
                        range: fact.range,
                    },
                    ruby_type: fact.ruby_type,
                    order: (
                        fact.range.file_id.0,
                        fact.range.start_byte,
                        fact.range.end_byte,
                    ),
                })
            })
            .collect::<Vec<_>>();
        let outcomes =
            solve_constant_type_equations(&equations, &constant_facts, &resolved_dependencies);
        for (target, ruby_type) in outcomes {
            match target {
                ConstantTypeTarget::Fact { subject, range } => {
                    let updated = self
                        .facts
                        .types
                        .update_equation_target(&subject, range, ruby_type);
                    assert!(
                        updated > 0,
                        "INVARIANT VIOLATED: constant equation target {subject:?} has no matching type fact at {range:?}. This is a bug because equations and facts must be replaced atomically with their file. Fix: emit the equation from the same write path as its target TypeFact."
                    );
                }
                ConstantTypeTarget::LocalAssignment { name, range } => {
                    let updated = self
                        .facts
                        .types
                        .update_local_assignment_equation_target(&name, range, ruby_type);
                    assert!(
                        updated > 0,
                        "INVARIANT VIOLATED: local constructor equation target {name:?} has no matching assignment fact at {range:?}. This is a bug because the stable source assignment and its equation must be replaced atomically. Fix: emit the target fact and equation from the same local-write path."
                    );
                }
                ConstantTypeTarget::LocalRead(range) => {
                    self.update_constant_local_read(range, ruby_type);
                }
            }
        }
        if self.inference_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| !equation.constant_dependencies().is_empty())
        }) {
            self.method_return_equations_dirty = true;
        }
        self.constant_type_equations_dirty = false;
        true
    }

    fn update_constant_local_read(&mut self, range: TextRange, ruby_type: RubyType) {
        let mut reads = self
            .local_read_types_by_file
            .remove(&range.file_id)
            .unwrap_or_default()
            .into_vec();
        reads.retain(|(existing, _)| *existing != range);
        if ruby_type != RubyType::Unknown {
            let ruby_type = self.facts.types.intern_ruby_type(ruby_type);
            reads.push((range, ruby_type));
        }
        reads.sort_unstable_by_key(|(range, _)| *range);
        if !reads.is_empty() {
            self.local_read_types_by_file
                .insert(range.file_id, reads.into_boxed_slice());
        }
    }

    /// Solve the complete project-owned method-return equation graph once per
    /// equation change, before any call reference consumes method returns.
    ///
    /// Ordinary `resolve()` calls are O(1) when method bodies did not change.
    /// Equations contain no AST nodes and follow the same per-file replacement
    /// lifecycle as the inferred type facts they update.
    fn resolve_method_return_equations(&mut self) -> bool {
        if !self.method_return_equations_dirty {
            return false;
        }

        let mut file_ids = self
            .inference_by_file
            .iter()
            .filter_map(|(file_id, evidence)| {
                (!evidence.method_return_equations.is_empty()).then_some(*file_id)
            })
            .collect::<Vec<_>>();
        file_ids.sort_unstable();

        let has_constant_dependencies = file_ids.iter().any(|file_id| {
            self.inference_by_file
                .get(file_id)
                .expect(
                    "INVARIANT VIOLATED: a selected method-equation file disappeared while checking constant dependencies. This is a bug because resolution owns the engine write lock. Fix: keep equation selection and dependency inspection in one immutable phase.",
                )
                .method_return_equations
                .iter()
                .any(|equation| !equation.constant_dependencies().is_empty())
        });
        if file_ids.len() == 1
            && !self.method_return_solution_spans_files
            && !has_constant_dependencies
        {
            self.method_return_equations_dirty = false;
            return false;
        }

        let equations = file_ids
            .iter()
            .flat_map(|file_id| {
                self.inference_by_file
                    .get(file_id)
                    .expect(
                        "INVARIANT VIOLATED: a selected method-equation file disappeared during immutable collection. This is a bug because resolution owns the engine write lock. Fix: keep equation collection inside one resolution pass.",
                    )
                    .method_return_equations
                    .iter()
                    .map(|equation| {
                        let query = AnalysisQuery::new(self);
                        equation.with_resolved_constant_types(
                            equation.constant_dependencies().iter().map(|dependency| {
                                resolve_constant_dependency_type(&query, dependency)
                            }),
                        )
                    })
            })
            .collect::<Vec<_>>();

        if equations.is_empty() {
            self.method_return_equations_dirty = false;
            self.method_return_solution_spans_files = false;
            return false;
        }

        let solve_result = solve_method_return_equations_with_telemetry(&equations);
        for file_id in &file_ids {
            let methods = self
                .inference_by_file
                .get(file_id)
                .expect(
                    "INVARIANT VIOLATED: a method-equation owner disappeared before result projection. This is a bug because resolution owns the engine write lock. Fix: keep equation solving and projection atomic.",
                )
                .method_return_equations
                .iter()
                .map(|equation| equation.method().clone())
                .collect::<std::collections::BTreeSet<_>>();
            let outcomes = methods
                .iter()
                .map(|method| {
                    let outcome = solve_result.outcomes.get(method).unwrap_or_else(|| {
                        panic!(
                            "INVARIANT VIOLATED: method-return solver omitted equation `{method}`. This is a bug because every grouped method must produce exactly one proof outcome. Fix: keep SCC emission and result insertion exhaustive."
                        )
                    });
                    (method.clone(), outcome.clone())
                })
                .collect::<BTreeMap<_, _>>();
            self.facts
                .types
                .update_inferred_method_return_types_in_file(
                    *file_id,
                    outcomes
                        .iter()
                        .map(|(method, outcome)| (method, outcome.clone().into_ruby_type())),
                );
            let evidence = self.inference_by_file.get_mut(file_id).expect(
                "INVARIANT VIOLATED: a method-equation owner disappeared before evidence replacement. This is a bug because resolution owns the engine write lock. Fix: keep equation solving and evidence projection atomic.",
            );
            evidence.method_return_outcomes = outcomes;
            evidence.telemetry = InferenceTelemetry::default();
        }

        let telemetry_owner = *file_ids.first().expect(
            "INVARIANT VIOLATED: a non-empty method-equation solve has no file owner. This is a bug because equations are collected exclusively from sorted file evidence. Fix: preserve the owner while flattening equations.",
        );
        self.inference_by_file
            .get_mut(&telemetry_owner)
            .expect(
                "INVARIANT VIOLATED: the deterministic method-solver telemetry owner disappeared. This is a bug because resolution owns the engine write lock. Fix: assign telemetry before leaving the atomic solve pass.",
            )
            .telemetry = solve_result.telemetry;
        self.method_return_equations_dirty = false;
        self.method_return_solution_spans_files = file_ids.len() > 1;
        true
    }

    pub fn inference_telemetry(&self) -> InferenceTelemetry {
        let mut file_ids = self.inference_by_file.keys().copied().collect::<Vec<_>>();
        file_ids.sort_unstable();
        let mut aggregate = InferenceTelemetry::default();
        for file_id in file_ids {
            aggregate.merge(&self.inference_by_file.get(&file_id).expect(
                "INVARIANT VIOLATED: inference telemetry file key disappeared during immutable aggregation. This is a bug because engine queries hold a stable shared borrow. Fix: keep telemetry replacement behind the engine write lock.",
            ).telemetry);
        }
        aggregate
    }

    pub fn inference_telemetry_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&InferenceTelemetry> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| &evidence.telemetry)
    }

    pub fn method_return_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&BTreeMap<FullyQualifiedName, TypeInferenceOutcome>> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| &evidence.method_return_outcomes)
    }

    pub fn method_return_equations_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[crate::core::MethodReturnEquation]> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| evidence.method_return_equations.as_slice())
    }

    pub fn inference_evidence_in_file(&self, file_id: SourceFileId) -> Option<InferenceEvidence> {
        let mut evidence = self.inference_by_file.get(&file_id)?.clone();
        evidence.call_expression_outcomes = self
            .call_expression_outcomes_in_file(file_id)
            .unwrap_or_default();
        Some(evidence)
    }

    pub(crate) fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.inference_by_file.values().any(|evidence| {
            evidence
                .method_return_equations
                .iter()
                .any(|equation| equation.method() == method)
        })
    }

    pub(super) fn expression_unknown_reason(&self, range: TextRange) -> Option<UnknownReason> {
        self.call_expression_outcome_at(range)
            .and_then(|outcome| match outcome {
                TypeInferenceOutcomeRef::Proven(_) => None,
                TypeInferenceOutcomeRef::Unknown(reason) => Some(reason),
            })
            .or_else(|| {
                let evidence = self.inference_by_file.get(&range.file_id)?;
                evidence
                    .expression_unknown_reasons
                    .binary_search_by_key(&range, |(evidence_range, _)| *evidence_range)
                    .ok()
                    .map(|index| evidence.expression_unknown_reasons[index].1)
            })
    }

    pub(super) fn call_expression_outcomes_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, TypeInferenceOutcome)>> {
        self.call_expression_outcome_views_in_file(file_id)
            .map(|outcomes| {
                outcomes
                    .map(|(range, outcome)| {
                        let outcome = match outcome {
                            TypeInferenceOutcomeRef::Proven(ruby_type) => {
                                TypeInferenceOutcome::proven(ruby_type.clone())
                            }
                            TypeInferenceOutcomeRef::Unknown(reason) => {
                                TypeInferenceOutcome::unknown(reason)
                            }
                        };
                        (range, outcome)
                    })
                    .collect()
            })
    }

    pub(super) fn call_expression_outcome_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<impl Iterator<Item = (TextRange, TypeInferenceOutcomeRef<'_>)>> {
        self.call_expression_outcomes_by_file
            .get(&file_id)
            .map(|outcomes| {
                outcomes
                    .iter()
                    .map(|(range, outcome)| (*range, outcome.as_ref(&self.facts.types)))
            })
    }

    pub(super) fn call_expression_outcome_at(
        &self,
        range: TextRange,
    ) -> Option<TypeInferenceOutcomeRef<'_>> {
        let outcomes = self.call_expression_outcomes_by_file.get(&range.file_id)?;
        let index = outcomes
            .binary_search_by_key(&range, |(outcome_range, _)| *outcome_range)
            .ok()?;
        Some(outcomes[index].1.as_ref(&self.facts.types))
    }

    pub(super) fn local_read_types_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<Vec<(TextRange, RubyType)>> {
        self.local_read_type_views_in_file(file_id).map(|reads| {
            reads
                .map(|(range, ruby_type)| (range, ruby_type.clone()))
                .collect()
        })
    }

    pub(super) fn local_read_type_views_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<impl Iterator<Item = (TextRange, &RubyType)>> {
        self.inference_by_file.get(&file_id)?;
        let reads = self
            .local_read_types_by_file
            .get(&file_id)
            .map_or(&[][..], Box::as_ref);
        Some(
            reads
                .iter()
                .map(|(range, ruby_type)| (*range, self.facts.types.ruby_type(*ruby_type))),
        )
    }

    pub(super) fn local_read_type_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&RubyType> {
        self.inference_by_file.get(&file_id)?;
        let reads = self.local_read_types_by_file.get(&file_id)?;
        let upper = reads.partition_point(|(range, _)| range.start_byte <= byte_offset);
        reads[..upper]
            .iter()
            .rev()
            .find(|(range, _)| range.contains_offset(file_id, byte_offset))
            .map(|(_, ruby_type)| self.facts.types.ruby_type(*ruby_type))
    }

    pub(super) fn replace_resolved_call_expression_outcomes(
        &mut self,
        mut outcomes: HashMap<TextRange, TypeInferenceOutcome>,
    ) {
        // Sorting only the compact ranges avoids materializing a second
        // Vec<(TextRange, TypeInferenceOutcome)> while the resolve map and the
        // previous file-owned outcomes are both still live. Move each outcome
        // out of the map only when its file is merged.
        let mut ordered_ranges = outcomes.keys().copied().collect::<Vec<_>>();
        ordered_ranges.sort_unstable();
        let mut ordered_ranges = ordered_ranges.into_iter().peekable();
        while let Some(first_range) = ordered_ranges.next() {
            let file_id = first_range.file_id;
            let first_outcome = StoredTypeInferenceOutcome::from_domain(
                &mut self.facts.types,
                outcomes.remove(&first_range).expect(
                    "INVARIANT VIOLATED: sorted call-expression range has no resolved outcome. This is a bug because the range list is built directly from the owned outcome map. Fix: remove each map entry exactly once while grouping by file.",
                ),
            );
            let mut incoming = vec![(first_range, first_outcome)];
            while ordered_ranges
                .peek()
                .is_some_and(|range| range.file_id == file_id)
            {
                let range = ordered_ranges.next().expect(
                    "INVARIANT VIOLATED: a peeked call-expression range disappeared before consumption. This is a bug because the local iterator is not shared. Fix: keep grouping and consumption in one loop.",
                );
                let outcome = StoredTypeInferenceOutcome::from_domain(
                    &mut self.facts.types,
                    outcomes.remove(&range).expect(
                        "INVARIANT VIOLATED: grouped call-expression range has no resolved outcome. This is a bug because each sorted range must still own one map entry. Fix: remove each map entry exactly once while grouping by file.",
                    ),
                );
                incoming.push((range, outcome));
            }

            self.inference_by_file.get(&file_id).expect(
                "INVARIANT VIOLATED: resolved call outcome belongs to a file without inference evidence. This is a bug because file facts are installed before their method candidates resolve. Fix: replace inference evidence atomically with reference candidates.",
            );
            let mut existing = self
                .call_expression_outcomes_by_file
                .remove(&file_id)
                .unwrap_or_default()
                .into_vec()
                .into_iter()
                .peekable();
            let mut incoming = incoming.into_iter().peekable();
            let mut merged = Vec::with_capacity(existing.len() + incoming.len());
            loop {
                match (existing.peek(), incoming.peek()) {
                    (Some((existing_range, _)), Some((incoming_range, _))) => {
                        match existing_range.cmp(incoming_range) {
                            std::cmp::Ordering::Less => merged.push(existing.next().expect(
                                "INVARIANT VIOLATED: a peeked existing call outcome disappeared before merge consumption. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                            )),
                            std::cmp::Ordering::Equal => {
                                existing.next().expect(
                                    "INVARIANT VIOLATED: an equal existing call outcome disappeared before replacement. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                                );
                                merged.push(incoming.next().expect(
                                    "INVARIANT VIOLATED: an equal incoming call outcome disappeared before replacement. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                                ));
                            }
                            std::cmp::Ordering::Greater => merged.push(incoming.next().expect(
                                "INVARIANT VIOLATED: a peeked incoming call outcome disappeared before merge consumption. This is a bug because the local iterator is not shared. Fix: keep comparison and consumption atomic.",
                            )),
                        }
                    }
                    (Some(_), None) => {
                        merged.extend(existing);
                        break;
                    }
                    (None, Some(_)) => {
                        merged.extend(incoming);
                        break;
                    }
                    (None, None) => break,
                }
            }
            self.call_expression_outcomes_by_file
                .insert(file_id, merged.into_boxed_slice());
        }
        assert!(
            outcomes.is_empty(),
            "INVARIANT VIOLATED: resolved call-expression outcomes remained after the complete sorted merge. This is a bug because every map key was copied into the ordered range list. Fix: keep range collection and map ownership in the same merge operation."
        );
    }

    pub(super) fn expression_unknown_reasons_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Option<&[(TextRange, UnknownReason)]> {
        self.inference_by_file
            .get(&file_id)
            .map(|evidence| evidence.expression_unknown_reasons.as_slice())
    }

    pub(super) fn query_cache_identity(&self) -> (u64, u64) {
        (self.instance_id, self.semantic_revision)
    }

    pub(super) fn cached_top_level_method_lookup_chain(&self) -> Option<Vec<FullyQualifiedName>> {
        self.top_level_method_lookup_chain_cache.lock().clone()
    }

    pub(super) fn cache_top_level_method_lookup_chain(&self, chain: Vec<FullyQualifiedName>) {
        *self.top_level_method_lookup_chain_cache.lock() = Some(chain);
    }

    pub(super) fn cached_universal_object_method_lookup_chain(
        &self,
    ) -> Option<Vec<FullyQualifiedName>> {
        self.universal_object_method_lookup_chain_cache
            .lock()
            .clone()
    }

    pub(super) fn cache_universal_object_method_lookup_chain(
        &self,
        chain: Vec<FullyQualifiedName>,
    ) {
        *self.universal_object_method_lookup_chain_cache.lock() = Some(chain);
    }

    #[cfg(test)]
    fn valid_method_lookup_chain_cache_len_for_test(&self) -> usize {
        usize::from(self.top_level_method_lookup_chain_cache.lock().is_some())
            + usize::from(
                self.universal_object_method_lookup_chain_cache
                    .lock()
                    .is_some(),
            )
    }

    fn replace_execution_contexts(
        &mut self,
        file_id: SourceFileId,
        mut contexts: Vec<ExecutionContextFact>,
    ) {
        for context in &contexts {
            assert_eq!(
                context.range.file_id, file_id,
                "INVARIANT VIOLATED: execution context range belongs to a different file. This is a bug because FileFacts replacement must be file-local. Fix: construct execution context ranges from the owning RubyDocument."
            );
            assert!(
                context.lexical_namespace.namespace_kind().is_some()
                    && context.implicit_receiver.namespace_kind().is_some()
                    && context.method_definition_owner.namespace_kind().is_some(),
                "INVARIANT VIOLATED: execution context contains a non-namespace semantic target. This is a bug because receiver and definition ownership require namespace identities. Fix: validate and convert extension targets before engine ingestion."
            );
            assert!(
                !context.extension_id.is_empty(),
                "INVARIANT VIOLATED: execution context has empty extension provenance. This is a bug because generated runtime semantics must remain attributable. Fix: retain the validated manifest ID in ExecutionContextFact."
            );
        }
        contexts.sort_by_key(|context| {
            (
                context.range.start_byte,
                std::cmp::Reverse(context.range.end_byte),
                context.extension_id.clone(),
            )
        });
        for pair in contexts.windows(2) {
            assert!(
                pair[0].range != pair[1].range,
                "INVARIANT VIOLATED: multiple execution contexts own the same block range. This is a bug because extension context conflicts must be resolved before engine ingestion. Fix: deterministically reject incompatible contexts at the host boundary."
            );
        }
        if contexts.is_empty() {
            self.execution_contexts.remove(&file_id);
        } else {
            self.execution_contexts.insert(file_id, contexts);
        }
    }

    pub fn execution_context_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&ExecutionContextFact> {
        self.execution_contexts
            .get(&file_id)?
            .iter()
            .filter(|context| context.range.contains_offset(file_id, byte_offset))
            .min_by_key(|context| context.range.end_byte - context.range.start_byte)
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.types.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .symbols
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        let Some(target_id) = self.names.fqn_id(target) else {
            return &[];
        };
        self.facts.references.resolved.facts_for(target_id)
    }

    pub fn fqn_for_id(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.names.fqn(id)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .methods
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_facts_matching_owner(
        &self,
        owner: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts = self
            .facts
            .definitions
            .methods
            .facts_matching_owner(owner_id, partial)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect::<Vec<_>>();
        retain_effective_method_availability(&mut facts);
        facts
    }

    pub fn method_facts_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts = self
            .facts
            .definitions
            .methods
            .facts_matching_owner_name(owner_id, method)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect::<Vec<_>>();
        retain_effective_method_availability(&mut facts);
        facts
    }

    pub(super) fn method_absence_contract_matches_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> bool {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return false;
        };
        self.facts
            .definitions
            .methods
            .facts_matching_owner_name(owner_id, method)
            .iter()
            .any(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
    }

    pub(super) fn effective_method_fact_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> EffectiveMethodFactMatch {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return EffectiveMethodFactMatch::Missing;
        };
        self.effective_method_fact_matching_owner_id(owner_id, method)
    }

    pub(super) fn effective_method_fact_matching_owner_id(
        &self,
        owner_id: FqnId,
        method: &crate::core::RubyMethod,
    ) -> EffectiveMethodFactMatch {
        match self
            .facts
            .definitions
            .methods
            .effective_fact_matching_owner_name(owner_id, method)
        {
            StoredMethodFactMatch::Missing => EffectiveMethodFactMatch::Missing,
            StoredMethodFactMatch::Unique(fact) => {
                EffectiveMethodFactMatch::Unique(self.expand_method_fact(fact.clone()))
            }
            StoredMethodFactMatch::Ambiguous => EffectiveMethodFactMatch::Ambiguous,
        }
    }

    pub fn method_visibility_overrides_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides
            .iter()
            .filter(|fact| {
                fact.method == *method
                    && fact.owner.namespace_parts() == owner.namespace_parts()
                    && fact.owner.namespace_kind() == owner.namespace_kind()
            })
            .cloned()
            .collect()
    }

    pub fn method_visibility_overrides_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides
            .iter()
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn all_method_visibility_overrides(&self) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides.clone()
    }

    pub fn method_names_for_owner(&self, owner: &FullyQualifiedName) -> Vec<&'static str> {
        let mut names = self
            .method_facts_matching_owner(owner, "")
            .into_iter()
            .map(|fact| effective_method_name(&fact).as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        names
    }

    pub(super) fn ruby_method_names_for_owner_id(&self, owner: FqnId) -> Vec<RubyMethod> {
        self.facts
            .definitions
            .methods
            .ruby_method_names_for_owner(owner)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.graph
            .nodes_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(source_id) = self.names.fqn_id(source) else {
            return Vec::new();
        };
        self.graph
            .edges_from(source_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    /// Returns the one superclass that is statically proven for `source`.
    /// Explicit declarations outrank per-declaration implicit `Object` facts,
    /// but two distinct explicit targets or any unresolved explicit target
    /// make the superclass unknown. Duplicate declarations of the same target
    /// remain one semantic proof while retaining every file-owned fact.
    pub fn proven_superclass_edge(&self, source: &FullyQualifiedName) -> Option<GraphEdgeFact> {
        if self.superclass_source_has_unresolved_explicit_edge(source) {
            return None;
        }
        let source_id = self.names.fqn_id(source)?;
        match self.graph.superclass_resolution(source_id) {
            StoredSuperclassResolution::Unique(edge) => Some(self.expand_graph_edge_fact(edge)),
            StoredSuperclassResolution::Missing | StoredSuperclassResolution::Ambiguous => None,
        }
    }

    pub fn superclass_is_ambiguous(&self, source: &FullyQualifiedName) -> bool {
        self.names.fqn_id(source).is_some_and(|source_id| {
            self.graph.superclass_resolution(source_id) == StoredSuperclassResolution::Ambiguous
        })
    }

    fn superclass_source_has_unresolved_explicit_edge(&self, source: &FullyQualifiedName) -> bool {
        let instance_source = match source.namespace_kind() {
            Some(NamespaceKind::Singleton) => source.to_instance_namespace().expect(
                "INVARIANT VIOLATED: singleton superclass source cannot produce an instance namespace. This is a bug because graph superclass sources are Namespace FQNs. Fix: preserve Namespace identity for class graph nodes.",
            ),
            Some(NamespaceKind::Instance) => source.clone(),
            None => return false,
        };
        let Some(source_id) = self.names.fqn_id(&instance_source) else {
            return false;
        };
        self.graph.has_unresolved_explicit_superclass(source_id)
    }

    pub fn graph_edges_to(&self, target: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        let Some(target_id) = self.names.fqn_id(target) else {
            return Vec::new();
        };
        self.graph
            .edges_to(target_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.graph
            .nodes_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.graph
            .edges_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn all_graph_nodes(&self) -> Vec<GraphNodeFact> {
        self.graph
            .all_nodes()
            .into_iter()
            .map(|fact| self.expand_graph_node_fact(fact))
            .collect()
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.graph
            .all_edges()
            .into_iter()
            .map(|fact| self.expand_graph_edge_fact(fact))
            .collect()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.facts.diagnostics.resolved.facts_in_file(file_id)
    }

    /// Replace only `unresolved-require` diagnostics for one file.
    ///
    /// Keeps every other resolved diagnostic fact, candidates, and semantic
    /// stores intact. Used when dependency require roots become available after
    /// project files were already indexed with incomplete load-path context.
    pub fn replace_unresolved_require_diagnostics(
        &mut self,
        file_id: SourceFileId,
        require_diagnostics: Vec<DiagnosticFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "unresolved-require refresh references unknown source file id",
        );
        for fact in &require_diagnostics {
            assert_eq!(
                fact.range.file_id, file_id,
                "INVARIANT VIOLATED: unresolved-require diagnostic belongs to a different file. \
                 This is a bug because require diagnostic refresh is file-local. \
                 Fix: construct DiagnosticFact ranges from the owning SourceFileId."
            );
            assert_eq!(
                fact.code, "unresolved-require",
                "INVARIANT VIOLATED: replace_unresolved_require_diagnostics received code `{}`. \
                 This is a bug because this API only swaps unresolved-require facts. \
                 Fix: filter non-require diagnostics before calling this method.",
                fact.code
            );
        }
        self.semantic_revision = self.semantic_revision.checked_add(1).expect(
            "INVARIANT VIOLATED: analysis engine semantic revision exhausted u64. \
             This is a bug because cached queries require monotonic invalidation. \
             Fix: widen the semantic revision before performing u64::MAX replacements.",
        );
        let mut diagnostics = self
            .facts
            .diagnostics
            .resolved
            .facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.code != "unresolved-require")
            .collect::<Vec<_>>();
        diagnostics.extend(require_diagnostics);
        self.facts
            .diagnostics
            .resolved
            .replace_file(file_id, diagnostics);
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.facts.diagnostics.resolved.all_facts()
    }

    pub fn unresolved_graph_edges(&self) -> Vec<UnresolvedGraphEdgeFact> {
        self.graph
            .unresolved_edges()
            .into_iter()
            .map(|edge| self.expand_unresolved_graph_edge_fact(edge))
            .collect()
    }

    pub fn reference_store(&self) -> &ReferenceStore {
        &self.facts.references.resolved
    }

    pub fn method_store(&self) -> &MethodStore {
        &self.facts.definitions.methods
    }

    pub fn symbol_store(&self) -> &SymbolStore {
        &self.facts.definitions.symbols
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.facts.types
    }

    pub fn diagnostic_store(&self) -> &DiagnosticStore {
        &self.facts.diagnostics.resolved
    }

    pub fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.facts.references.candidates
    }

    pub fn file_count(&self) -> usize {
        self.sources.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.sources.files.contains_key(&file_id),
            "INVARIANT VIOLATED: {message}. \
             This is a bug because analysis facts and ranges must only reference registered files. \
             Fix: call AnalysisEngine::register_file before adding file facts."
        );
    }

    fn intern_reference_candidates(
        &mut self,
        candidates: Vec<ReferenceCandidate>,
    ) -> Vec<StoredReferenceCandidate> {
        candidates
            .into_iter()
            .map(|candidate| match candidate.kind {
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                } => {
                    let context = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(current_namespace));
                    let lookup = self
                        .names
                        .intern_const_lookup(ConstLookup::new(parts, false, context));
                    StoredReferenceCandidate::constant(candidate.range, lookup)
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range,
                    diagnostics,
                } => {
                    let root = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                    let owner = self
                        .names
                        .intern_const_lookup(ConstLookup::new(owner, true, root));
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::method(
                        candidate.range,
                        owner,
                        owner_kind,
                        method,
                        is_super,
                        access,
                        caller,
                        call_expression_range,
                        preferred_definition_range,
                        diagnostics,
                    )
                }
                ReferenceCandidateKind::Resolved { target, caller } => {
                    let target = self.names.intern_fqn(target);
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::resolved(candidate.range, target, caller)
                }
            })
            .collect()
    }

    fn intern_symbol_facts(&mut self, facts: Vec<SymbolFact>) -> Vec<StoredSymbolFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredSymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
            })
            .collect()
    }

    fn intern_method_facts(&mut self, facts: Vec<MethodFact>) -> Vec<StoredMethodFact> {
        facts
            .into_iter()
            .map(|fact| {
                let method = match &fact.fqn {
                    FullyQualifiedName::Method(_, method) => Some(*method),
                    FullyQualifiedName::Namespace(_, _)
                    | FullyQualifiedName::Constant(_)
                    | FullyQualifiedName::LocalVariable(_)
                    | FullyQualifiedName::InstanceVariable(_)
                    | FullyQualifiedName::ClassVariable(_)
                    | FullyQualifiedName::GlobalVariable(_) => None,
                };
                let fqn = self.names.intern_fqn(fact.fqn);
                let owner = self.names.intern_fqn(fact.owner);
                StoredMethodFact {
                    fqn,
                    owner,
                    method,
                    range: fact.range,
                    name_range: fact.name_range,
                    params: fact.params,
                    param_facts: fact.param_facts,
                    parameter_shape_complete: fact.parameter_shape_complete,
                    delegate_receiver: fact.delegate_receiver,
                    visibility: fact.visibility,
                    availability: fact.availability,
                    documentation: fact.documentation,
                    return_type_label: fact.return_type_label,
                }
            })
            .collect()
    }

    fn intern_graph_node_facts(&mut self, facts: Vec<GraphNodeFact>) -> Vec<StoredGraphNodeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredGraphNodeFact::new(fqn, fact.kind, fact.range)
            })
            .collect()
    }

    fn intern_graph_edge_facts(&mut self, facts: Vec<GraphEdgeFact>) -> Vec<StoredGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let target = self.names.intern_fqn(fact.target);
                StoredGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    fn intern_unresolved_graph_edge_facts(
        &mut self,
        facts: Vec<UnresolvedGraphEdgeFact>,
    ) -> Vec<StoredUnresolvedGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let context = self.names.intern_fqn(fact.context);
                let target = self.names.intern_const_lookup(ConstLookup::new(
                    ConstantPath::from_vec(fact.target_parts),
                    fact.absolute,
                    context,
                ));
                StoredUnresolvedGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    fn expand_symbol_fact(&self, fact: StoredSymbolFact) -> SymbolFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: symbol fact points to missing FQN id. \
                 This is a bug because symbol facts must only store interned FQN ids. \
                 Fix: intern symbol FQNs before inserting facts.",
            )
            .clone();
        SymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
    }

    fn expand_method_fact(&self, fact: StoredMethodFact) -> MethodFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing FQN id. \
                 This is a bug because method facts must only store interned FQN ids. \
                 Fix: intern method FQNs before inserting facts.",
            )
            .clone();
        let owner = self
            .names
            .fqn(fact.owner)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing owner FQN id. \
                 This is a bug because method facts must only store interned owner FQN ids. \
                 Fix: intern method owners before inserting facts.",
            )
            .clone();
        MethodFact {
            fqn,
            owner,
            range: fact.range,
            name_range: fact.name_range,
            params: fact.params,
            param_facts: fact.param_facts,
            parameter_shape_complete: fact.parameter_shape_complete,
            delegate_receiver: fact.delegate_receiver,
            visibility: fact.visibility,
            availability: fact.availability,
            documentation: fact.documentation,
            return_type_label: fact.return_type_label,
        }
    }

    fn expand_graph_node_fact(&self, fact: StoredGraphNodeFact) -> GraphNodeFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: graph node points to missing FQN id. \
                 This is a bug because graph nodes must only store interned FQN ids. \
                 Fix: intern graph node FQNs before inserting facts.",
            )
            .clone();
        GraphNodeFact::new(fqn, fact.kind, fact.range)
    }

    fn expand_graph_edge_fact(&self, fact: StoredGraphEdgeFact) -> GraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect(
                "INVARIANT VIOLATED: graph edge points to missing source FQN id. \
                 This is a bug because graph edges must only store interned source FQN ids. \
                 Fix: intern graph edge source FQNs before inserting facts.",
            )
            .clone();
        let target = self
            .names
            .fqn(fact.target)
            .expect(
                "INVARIANT VIOLATED: graph edge points to missing target FQN id. \
                 This is a bug because graph edges must only store interned target FQN ids. \
                 Fix: intern graph edge target FQNs before inserting facts.",
            )
            .clone();
        GraphEdgeFact::new(source, target, fact.kind, fact.range).with_provenance(fact.provenance)
    }

    fn expand_unresolved_graph_edge_fact(
        &self,
        fact: StoredUnresolvedGraphEdgeFact,
    ) -> UnresolvedGraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge points to missing source FQN id. \
                 This is a bug because unresolved graph edges must only store interned source FQN ids. \
                 Fix: intern unresolved graph edge sources before inserting facts.",
            )
            .clone();
        let lookup = self.names.const_lookup(fact.target).expect(
            "INVARIANT VIOLATED: unresolved graph edge points to missing constant lookup id. \
             This is a bug because unresolved graph edges must only store interned constant lookup ids. \
             Fix: intern unresolved graph edge targets before inserting facts.",
        );
        let context = self
            .names
            .fqn(lookup.context)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge lookup points to missing context FQN id. \
                 This is a bug because constant lookups must only store interned context FQN ids. \
                 Fix: intern constant lookup contexts before inserting facts.",
            )
            .clone();
        UnresolvedGraphEdgeFact::new(
            source,
            lookup.path.to_vec(),
            lookup.absolute,
            context,
            fact.kind,
            fact.range,
        )
        .with_provenance(fact.provenance)
    }

    fn retry_unresolved_graph_edges(&mut self) {
        if self.graph.unresolved_edges().is_empty() {
            return;
        }

        let pending = self.graph.take_unresolved_edges();
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                let singleton_superclass =
                    self.resolved_singleton_superclass_companion(&unresolved, &target);
                let target = self.names.intern_fqn(target);
                self.graph.add_edge(
                    StoredGraphEdgeFact::new(
                        unresolved.source,
                        target,
                        unresolved.kind,
                        unresolved.range,
                    )
                    .with_provenance(unresolved.provenance),
                );
                if let Some((source, target)) = singleton_superclass {
                    let source = self.names.intern_fqn(source);
                    let target = self.names.intern_fqn(target);
                    self.graph.add_edge(
                        StoredGraphEdgeFact::new(
                            source,
                            target,
                            GraphEdgeKind::Superclass,
                            unresolved.range,
                        )
                        .with_provenance(unresolved.provenance),
                    );
                }
            } else {
                self.graph.add_unresolved_edge(unresolved);
            }
        }
    }

    /// Ruby class-method inheritance follows the singleton classes of the
    /// ordinary superclass chain. The collector can emit both edges when the
    /// target is already known, but a cross-file superclass starts as one
    /// unresolved instance edge. Materialize its exact singleton companion at
    /// the same resolution boundary so indexing order cannot change class
    /// method lookup.
    fn resolved_singleton_superclass_companion(
        &self,
        unresolved: &StoredUnresolvedGraphEdgeFact,
        target: &FullyQualifiedName,
    ) -> Option<(FullyQualifiedName, FullyQualifiedName)> {
        if unresolved.kind != GraphEdgeKind::Superclass
            || target.namespace_kind() != Some(NamespaceKind::Instance)
        {
            return None;
        }
        let source = self.names.fqn(unresolved.source).expect(
            "INVARIANT VIOLATED: unresolved superclass edge points to a missing source FQN. This is a bug because graph edges retain interned sources for their full lifetime. Fix: retain source FQNs until unresolved edges are removed.",
        );
        if source.namespace_kind() != Some(NamespaceKind::Instance)
            || !self
                .graph_nodes_for(source)
                .iter()
                .any(|fact| fact.kind == GraphNodeKind::Class)
            || !self
                .graph_nodes_for(target)
                .iter()
                .any(|fact| fact.kind == GraphNodeKind::Class)
        {
            return None;
        }
        Some((
            source.to_singleton_namespace().expect(
                "INVARIANT VIOLATED: a class instance namespace cannot produce its singleton namespace. This is a bug because class declarations always use Namespace FQNs. Fix: keep class graph nodes namespace-owned.",
            ),
            target.to_singleton_namespace().expect(
                "INVARIANT VIOLATED: a class superclass cannot produce its singleton namespace. This is a bug because resolved superclass targets are class Namespace FQNs. Fix: validate graph node kinds before materializing class inheritance.",
            ),
        ))
    }

    fn resolve_unresolved_graph_target(
        &self,
        unresolved: &StoredUnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let lookup = self.names.const_lookup(unresolved.target).expect(
            "INVARIANT VIOLATED: unresolved graph edge points to missing constant lookup id. \
             This is a bug because unresolved graph edges must only store interned constant lookup ids. \
             Fix: intern unresolved graph edge targets before inserting facts.",
        );
        let context = self.names.fqn(lookup.context).expect(
            "INVARIANT VIOLATED: unresolved graph edge lookup points to missing context FQN id. \
             This is a bug because constant lookups must only store interned context FQN ids. \
             Fix: intern constant lookup contexts before inserting facts.",
        );
        let mut search_namespaces = if lookup.absolute {
            Vec::new()
        } else {
            context.namespace_parts()
        };

        loop {
            let mut probe = search_namespaces.clone();
            probe.extend(lookup.path.iter().cloned());
            let namespace_fqn = FullyQualifiedName::namespace(probe);
            if !self.graph_nodes_for(&namespace_fqn).is_empty() {
                return Some(namespace_fqn);
            }

            if lookup.absolute || search_namespaces.is_empty() {
                break;
            }
            search_namespaces.pop();
        }

        None
    }
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn retain_effective_method_availability(facts: &mut Vec<MethodFact>) {
    let absent = facts
        .iter()
        .filter(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
        .map(effective_method_name)
        .collect::<HashSet<_>>();
    let unavailable = facts
        .iter()
        .filter(|fact| matches!(fact.availability, MethodAvailability::Unavailable { .. }))
        .map(effective_method_name)
        .collect::<HashSet<_>>();
    facts.retain(|fact| {
        let method = effective_method_name(fact);
        if absent.contains(&method) {
            return false;
        }
        if unavailable.contains(&method) {
            return matches!(fact.availability, MethodAvailability::Unavailable { .. });
        }
        true
    });
}

fn effective_method_name(fact: &MethodFact) -> crate::core::RubyMethod {
    match &fact.fqn {
        FullyQualifiedName::Method(_, method) => *method,
        FullyQualifiedName::Namespace(_, _)
        | FullyQualifiedName::Constant(_)
        | FullyQualifiedName::LocalVariable(_)
        | FullyQualifiedName::InstanceVariable(_)
        | FullyQualifiedName::ClassVariable(_)
        | FullyQualifiedName::GlobalVariable(_) => panic!(
            "INVARIANT VIOLATED: method store returned a non-method FQN. \
             This is a bug because availability composition is defined only for method identities. \
             Fix: construct MethodFact with FullyQualifiedName::Method before engine insertion."
        ),
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
