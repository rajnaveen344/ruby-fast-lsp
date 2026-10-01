use std::cmp::Ordering;
use std::collections::HashSet;

use crate::core::callables::callable_signature::CallableSignature;
use crate::core::callables::callable_signature::CallableTypeTemplate;
use crate::core::callables::callable_signature::DirectYieldCall;
use crate::core::callables::callable_signature::ForwardedBlockCall;
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::file_owned::arena::{FileArena, FileIndex, RowId};
use crate::core::storage::file_owned::FileRow;
use crate::core::storage::memory_estimate::{
    ruby_type_heap_bytes, string_heap_bytes, vec_payload_bytes,
};
use crate::core::{FullyQualifiedName, RubyMethod, SourceFileId, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MethodParamKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodAvailability {
    Available,
    Unavailable { reason: String },
    Absent { reason: String },
}

impl Default for MethodAvailability {
    fn default() -> Self {
        Self::Available
    }
}

impl MethodAvailability {
    pub fn reason(&self) -> Option<&String> {
        match self {
            Self::Available => None,
            Self::Unavailable { reason } => Some(reason),
            Self::Absent { reason } => Some(reason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParamFact {
    pub name: String,
    pub kind: MethodParamKind,
    pub type_label: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct HigherOrderMethodMetadata {
    pub(crate) callable_signatures: Vec<CallableSignature>,
    pub(crate) forwarded_block_call: Option<ForwardedBlockCall>,
    pub(crate) direct_yield_call: Option<DirectYieldCall>,
}

impl HigherOrderMethodMetadata {
    fn is_empty(&self) -> bool {
        self.callable_signatures.is_empty()
            && self.forwarded_block_call.is_none()
            && self.direct_yield_call.is_none()
    }
}

impl MethodParamFact {
    pub fn new(name: impl Into<String>, kind: MethodParamKind) -> Self {
        let name = name.into();
        invariant!(
            !name.is_empty(),
            what = "method parameter fact name is empty",
            why = "parameter facts must identify a Ruby parameter",
            fix = "skip anonymous parameters or assign a valid generated name before inserting",
        );
        Self {
            name,
            kind,
            type_label: None,
            documentation: None,
        }
    }

    pub fn with_signature_metadata(
        mut self,
        type_label: Option<String>,
        documentation: Option<String>,
    ) -> Self {
        self.type_label = type_label;
        self.documentation = documentation;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFact {
    pub fqn: FullyQualifiedName,
    pub owner: FullyQualifiedName,
    pub range: TextRange,
    pub name_range: TextRange,
    pub params: Vec<String>,
    pub param_facts: Vec<MethodParamFact>,
    pub(crate) parameter_shape_complete: bool,
    pub delegate_receiver: Option<RubyMethod>,
    pub visibility: MethodVisibility,
    pub availability: MethodAvailability,
    pub documentation: Option<String>,
    pub return_type_label: Option<String>,
    pub(crate) higher_order: Option<Box<HigherOrderMethodMetadata>>,
}

impl MethodFact {
    /// Construct a method declaration whose parameter shape is unavailable.
    ///
    /// Use `with_params` or `with_param_facts`, including with an empty
    /// vector, when the source proves the complete parameter shape.
    pub fn new(fqn: FullyQualifiedName, owner: FullyQualifiedName, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            range,
            name_range: range,
            params: Vec::new(),
            param_facts: Vec::new(),
            parameter_shape_complete: false,
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            availability: MethodAvailability::Available,
            documentation: None,
            return_type_label: None,
            higher_order: None,
        }
    }

    pub fn with_params(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        params: Vec<String>,
    ) -> Self {
        let param_facts = params
            .iter()
            .map(|name| MethodParamFact::new(name.clone(), MethodParamKind::Required))
            .collect();
        Self::with_param_facts(fqn, owner, range, param_facts)
    }

    pub fn with_param_facts(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        param_facts: Vec<MethodParamFact>,
    ) -> Self {
        let params = param_facts.iter().map(|param| param.name.clone()).collect();
        Self {
            fqn,
            owner,
            range,
            name_range: range,
            params,
            param_facts,
            parameter_shape_complete: true,
            delegate_receiver: None,
            visibility: MethodVisibility::Public,
            availability: MethodAvailability::Available,
            documentation: None,
            return_type_label: None,
            higher_order: None,
        }
    }

    pub fn with_delegate_receiver(
        fqn: FullyQualifiedName,
        owner: FullyQualifiedName,
        range: TextRange,
        delegate_receiver: RubyMethod,
    ) -> Self {
        Self {
            fqn,
            owner,
            range,
            name_range: range,
            params: Vec::new(),
            param_facts: Vec::new(),
            parameter_shape_complete: false,
            delegate_receiver: Some(delegate_receiver),
            visibility: MethodVisibility::Public,
            availability: MethodAvailability::Available,
            documentation: None,
            return_type_label: None,
            higher_order: None,
        }
    }

    pub fn with_visibility(mut self, visibility: MethodVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn has_complete_parameter_shape(&self) -> bool {
        self.parameter_shape_complete
    }

    pub fn with_availability(mut self, availability: MethodAvailability) -> Self {
        if let MethodAvailability::Unavailable { reason } | MethodAvailability::Absent { reason } =
            &availability
        {
            invariant!(
                !reason.trim().is_empty(),
                what = "unavailable method fact has an empty reason",
                why = "unsupported-runtime-api diagnostics must explain the runtime limitation",
                fix = "provide a non-empty @unavailable reason in the owning stub declaration",
            );
        }
        self.availability = availability;
        self
    }

    pub fn with_name_range(mut self, name_range: TextRange) -> Self {
        invariant!(
            name_range.file_id == self.range.file_id,
            what = "method name range belongs to a different file than its declaration",
            why = "declaration edits must stay within their source file",
            fix = "derive both ranges from the same registered source document",
        );
        invariant!(
            self.range.start_byte <= name_range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            what = "method name range is outside its declaration range",
            why = "rename requires the exact declaration token",
            fix = "use Prism's method name location inside the enclosing declaration location",
        );
        self.name_range = name_range;
        self
    }

    pub fn with_signature_metadata(
        mut self,
        documentation: Option<String>,
        return_type_label: Option<String>,
    ) -> Self {
        self.documentation = documentation;
        self.return_type_label = return_type_label;
        self
    }

    pub(crate) fn with_callable_signatures(
        mut self,
        callable_signatures: Vec<CallableSignature>,
    ) -> Self {
        if callable_signatures.is_empty() {
            if let Some(metadata) = self.higher_order.as_mut() {
                metadata.callable_signatures.clear();
            }
            self.clear_empty_higher_order();
            return self;
        }
        self.higher_order_metadata_mut().callable_signatures = callable_signatures;
        self
    }

    pub(crate) fn with_forwarded_block_call(
        mut self,
        forwarded_block_call: Option<ForwardedBlockCall>,
    ) -> Self {
        self.set_forwarded_block_call(forwarded_block_call);
        self
    }

    pub(crate) fn with_direct_yield_call(
        mut self,
        direct_yield_call: Option<DirectYieldCall>,
    ) -> Self {
        self.set_direct_yield_call(direct_yield_call);
        self
    }

    pub(crate) fn set_forwarded_block_call(
        &mut self,
        forwarded_block_call: Option<ForwardedBlockCall>,
    ) {
        if let Some(forwarded_block_call) = forwarded_block_call {
            self.higher_order_metadata_mut().forwarded_block_call = Some(forwarded_block_call);
        } else {
            if let Some(metadata) = self.higher_order.as_mut() {
                metadata.forwarded_block_call = None;
            }
            self.clear_empty_higher_order();
        }
    }

    pub(crate) fn set_direct_yield_call(&mut self, direct_yield_call: Option<DirectYieldCall>) {
        if let Some(direct_yield_call) = direct_yield_call {
            self.higher_order_metadata_mut().direct_yield_call = Some(direct_yield_call);
        } else {
            if let Some(metadata) = self.higher_order.as_mut() {
                metadata.direct_yield_call = None;
            }
            self.clear_empty_higher_order();
        }
    }

    pub(crate) fn callable_signatures(&self) -> &[CallableSignature] {
        self.higher_order
            .as_deref()
            .map(|metadata| metadata.callable_signatures.as_slice())
            .unwrap_or_default()
    }

    pub(crate) fn forwarded_block_call(&self) -> Option<&ForwardedBlockCall> {
        self.higher_order
            .as_deref()
            .and_then(|metadata| metadata.forwarded_block_call.as_ref())
    }

    pub(crate) fn direct_yield_call(&self) -> Option<&DirectYieldCall> {
        self.higher_order
            .as_deref()
            .and_then(|metadata| metadata.direct_yield_call.as_ref())
    }

    fn higher_order_metadata_mut(&mut self) -> &mut HigherOrderMethodMetadata {
        self.higher_order
            .get_or_insert_with(|| Box::new(HigherOrderMethodMetadata::default()))
    }

    fn clear_empty_higher_order(&mut self) {
        if self
            .higher_order
            .as_deref()
            .is_some_and(HigherOrderMethodMetadata::is_empty)
        {
            self.higher_order = None;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodVisibilityOverrideFact {
    pub owner: FullyQualifiedName,
    pub method: RubyMethod,
    pub visibility: MethodVisibility,
    pub range: TextRange,
}

impl MethodVisibilityOverrideFact {
    pub fn new(
        owner: FullyQualifiedName,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) -> Self {
        Self {
            owner,
            method,
            visibility,
            range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMethodFact {
    pub fqn: FqnId,
    pub owner: FqnId,
    pub method: Option<RubyMethod>,
    pub range: TextRange,
    pub name_range: TextRange,
    pub params: Vec<String>,
    pub param_facts: Vec<MethodParamFact>,
    pub(crate) parameter_shape_complete: bool,
    pub delegate_receiver: Option<RubyMethod>,
    pub visibility: MethodVisibility,
    pub availability: MethodAvailability,
    pub documentation: Option<String>,
    pub return_type_label: Option<String>,
    pub(crate) higher_order: Option<Box<HigherOrderMethodMetadata>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredMethodFactMatch<'a> {
    Missing,
    Unique(&'a StoredMethodFact),
    Ambiguous,
}

#[derive(Debug, Clone, Default)]
pub struct MethodStore {
    facts: FileArena<StoredMethodFact>,
    facts_by_fqn: FileIndex<FqnId>,
    facts_by_owner: FileIndex<FqnId>,
    facts_by_owner_name: FileIndex<(FqnId, RubyMethod)>,
}

impl FileRow for StoredMethodFact {
    fn file_id(&self) -> SourceFileId {
        self.range.file_id
    }
}

impl MethodStore {
    pub fn facts_for(&self, fqn: FqnId) -> Vec<StoredMethodFact> {
        self.clone_facts(self.facts_by_fqn.get(&fqn))
    }

    pub fn all_facts(&self) -> Vec<StoredMethodFact> {
        self.facts.iter().cloned().collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.len()
    }

    pub fn facts_matching_owner(&self, owner: FqnId, partial: &str) -> Vec<StoredMethodFact> {
        self.indexed(self.facts_by_owner.get(&owner))
            .filter(|fact| {
                fact.method
                    .is_some_and(|method| method.get_name().starts_with(partial))
            })
            .cloned()
            .collect()
    }

    pub fn facts_matching_owner_name(
        &self,
        owner: FqnId,
        method: &RubyMethod,
    ) -> Vec<StoredMethodFact> {
        self.clone_facts(self.facts_by_owner_name.get(&(owner, *method)))
    }

    /// Select the effective exact owner/name fact without cloning or expanding
    /// the indexed candidates.
    ///
    /// The owner/name bucket is already kept in deterministic file/range/FQN
    /// order. Exact duplicates are therefore adjacent, matching the previous
    /// sort-and-dedup behavior after expansion. Runtime `Absent` facts mask the
    /// method completely; otherwise `Unavailable` facts override available
    /// declarations for the same exact method identity.
    pub(crate) fn effective_fact_matching_owner_name(
        &self,
        owner: FqnId,
        method: &RubyMethod,
    ) -> StoredMethodFactMatch<'_> {
        let ids = self.facts_by_owner_name.get(&(owner, *method));
        if self
            .indexed(ids)
            .any(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
        {
            return StoredMethodFactMatch::Missing;
        }
        let unavailable_wins = self
            .indexed(ids)
            .any(|fact| matches!(fact.availability, MethodAvailability::Unavailable { .. }));
        let mut effective = self.indexed(ids).filter(|fact| {
            !unavailable_wins || matches!(fact.availability, MethodAvailability::Unavailable { .. })
        });
        let Some(first) = effective.next() else {
            return StoredMethodFactMatch::Missing;
        };
        for fact in effective {
            if fact != first {
                return StoredMethodFactMatch::Ambiguous;
            }
        }
        StoredMethodFactMatch::Unique(first)
    }

    pub(crate) fn ruby_method_names_for_owner(&self, owner: FqnId) -> Vec<RubyMethod> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        for fact in self.indexed(self.facts_by_owner.get(&owner)) {
            let Some(method) = fact.method else {
                continue;
            };
            if seen.insert(method) {
                names.push(method);
            }
        }
        names
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<StoredMethodFact> {
        self.facts.rows_in_file(file_id).cloned().collect()
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = StoredMethodFact>,
    ) {
        self.facts.remove_file(file_id, |arena, stale| {
            self.facts_by_fqn.unlink(stale.fqn, file_id, arena);
            self.facts_by_owner.unlink(stale.owner, file_id, arena);
            if let Some(method) = stale.method {
                self.facts_by_owner_name
                    .unlink((stale.owner, method), file_id, arena);
            }
        });
        let ids = self.facts.insert_file(file_id, facts, by_range);
        let facts = &self.facts;
        self.facts_by_fqn.link(
            file_id,
            ids.iter().map(|id| (facts.get(*id).fqn, *id)),
            facts,
            |left, right| by_range(left, right).then(left.owner.cmp(&right.owner)),
        );
        self.facts_by_owner.link(
            file_id,
            ids.iter().map(|id| (facts.get(*id).owner, *id)),
            facts,
            by_range_then_fqn,
        );
        self.facts_by_owner_name.link(
            file_id,
            ids.iter().filter_map(|id| {
                let fact = facts.get(*id);
                fact.method.map(|method| ((fact.owner, method), *id))
            }),
            facts,
            by_range_then_fqn,
        );
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.facts.estimated_heap_bytes()
            + self.facts.iter().map(method_fact_heap_bytes).sum::<usize>()
            + self.facts_by_fqn.estimated_heap_bytes()
            + self.facts_by_owner.estimated_heap_bytes()
            + self.facts_by_owner_name.estimated_heap_bytes()
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.facts_by_fqn.shrink_to_fit();
        self.facts_by_owner.shrink_to_fit();
        self.facts_by_owner_name.shrink_to_fit();
    }

    fn indexed<'a>(&'a self, ids: &'a [RowId]) -> impl Iterator<Item = &'a StoredMethodFact> {
        ids.iter().map(|id| self.facts.get(*id))
    }

    fn clone_facts(&self, ids: &[RowId]) -> Vec<StoredMethodFact> {
        self.indexed(ids).cloned().collect()
    }
}

fn by_range(left: &StoredMethodFact, right: &StoredMethodFact) -> Ordering {
    (left.range.start_byte, left.range.end_byte)
        .cmp(&(right.range.start_byte, right.range.end_byte))
}

fn by_range_then_fqn(left: &StoredMethodFact, right: &StoredMethodFact) -> Ordering {
    by_range(left, right).then(left.fqn.cmp(&right.fqn))
}

fn method_fact_heap_bytes(fact: &StoredMethodFact) -> usize {
    vec_payload_bytes(&fact.params)
        + fact.params.iter().map(string_heap_bytes).sum::<usize>()
        + vec_payload_bytes(&fact.param_facts)
        + fact
            .param_facts
            .iter()
            .map(|param| {
                string_heap_bytes(&param.name)
                    + param
                        .type_label
                        .as_ref()
                        .map(string_heap_bytes)
                        .unwrap_or(0)
                    + param
                        .documentation
                        .as_ref()
                        .map(string_heap_bytes)
                        .unwrap_or(0)
            })
            .sum::<usize>()
        + fact
            .availability
            .reason()
            .map(string_heap_bytes)
            .unwrap_or(0)
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
        + fact
            .higher_order
            .as_deref()
            .map(higher_order_method_metadata_heap_bytes)
            .unwrap_or(0)
}

fn higher_order_method_metadata_heap_bytes(metadata: &HigherOrderMethodMetadata) -> usize {
    std::mem::size_of::<HigherOrderMethodMetadata>()
        + vec_payload_bytes(&metadata.callable_signatures)
        + metadata
            .callable_signatures
            .iter()
            .map(callable_signature_heap_bytes)
            .sum::<usize>()
        + metadata
            .forwarded_block_call
            .as_ref()
            .map(|forwarded| string_heap_bytes(&forwarded.receiver_parameter))
            .unwrap_or(0)
        + metadata
            .direct_yield_call
            .as_ref()
            .map(|direct| {
                vec_payload_bytes(&direct.parameter_names)
                    + direct
                        .parameter_names
                        .iter()
                        .map(string_heap_bytes)
                        .sum::<usize>()
            })
            .unwrap_or(0)
}

fn callable_signature_heap_bytes(signature: &CallableSignature) -> usize {
    vec_payload_bytes(&signature.receiver_type_parameters)
        + signature
            .receiver_type_parameters
            .iter()
            .map(string_heap_bytes)
            .sum::<usize>()
        + vec_payload_bytes(&signature.type_parameters)
        + signature
            .type_parameters
            .iter()
            .map(string_heap_bytes)
            .sum::<usize>()
        + vec_payload_bytes(&signature.parameters)
        + signature
            .parameters
            .iter()
            .map(|parameter| callable_template_heap_bytes(&parameter.ruby_type))
            .sum::<usize>()
        + vec_payload_bytes(&signature.block.parameters)
        + signature
            .block
            .parameters
            .iter()
            .map(callable_template_heap_bytes)
            .sum::<usize>()
        + callable_template_heap_bytes(&signature.block.return_type)
        + callable_template_heap_bytes(&signature.return_type)
}

fn callable_template_heap_bytes(template: &CallableTypeTemplate) -> usize {
    match template {
        CallableTypeTemplate::Concrete(ruby_type) => ruby_type_heap_bytes(ruby_type),
        CallableTypeTemplate::Receiver => 0,
        CallableTypeTemplate::Variable(name) => string_heap_bytes(name),
        CallableTypeTemplate::Array(element) => callable_template_heap_bytes(element),
        CallableTypeTemplate::Hash(key, value) => {
            callable_template_heap_bytes(key) + callable_template_heap_bytes(value)
        }
        CallableTypeTemplate::Union(members) => {
            vec_payload_bytes(members)
                + members
                    .iter()
                    .map(callable_template_heap_bytes)
                    .sum::<usize>()
        }
        CallableTypeTemplate::Unconstrained => 0,
    }
}

#[cfg(test)]
mod tests;
