use std::collections::{HashMap, HashSet};

use super::memory_estimate::{
    map_table_bytes, ruby_type_heap_bytes, string_heap_bytes, vec_payload_bytes,
};
use crate::{
    CallableSignature, CallableTypeTemplate, DirectYieldCall, ForwardedBlockCall, FqnId,
    FullyQualifiedName, RubyMethod, SourceFileId, TextRange,
};

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
        assert!(
            !name.is_empty(),
            "INVARIANT VIOLATED: method parameter fact name is empty. \
             This is a bug because parameter facts must identify a Ruby parameter. \
             Fix: skip anonymous parameters or assign a valid generated name before inserting."
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
            assert!(
                !reason.trim().is_empty(),
                "INVARIANT VIOLATED: unavailable method fact has an empty reason. \
                 This is a bug because unsupported-runtime-api diagnostics must explain the runtime limitation. \
                 Fix: provide a non-empty @unavailable reason in the owning stub declaration."
            );
        }
        self.availability = availability;
        self
    }

    pub fn with_name_range(mut self, name_range: TextRange) -> Self {
        assert!(
            name_range.file_id == self.range.file_id,
            "INVARIANT VIOLATED: method name range belongs to a different file than its declaration. This is a bug because declaration edits must stay within their source file. Fix: derive both ranges from the same registered source document."
        );
        assert!(
            self.range.start_byte <= name_range.start_byte
                && name_range.end_byte <= self.range.end_byte,
            "INVARIANT VIOLATED: method name range is outside its declaration range. This is a bug because rename requires the exact declaration token. Fix: use Prism's method name location inside the enclosing declaration location."
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

impl StoredMethodFact {
    pub fn new(fqn: FqnId, owner: FqnId, method: Option<RubyMethod>, range: TextRange) -> Self {
        Self {
            fqn,
            owner,
            method,
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

    pub fn with_param_facts(
        fqn: FqnId,
        owner: FqnId,
        method: Option<RubyMethod>,
        range: TextRange,
        param_facts: Vec<MethodParamFact>,
    ) -> Self {
        let params = param_facts.iter().map(|param| param.name.clone()).collect();
        Self {
            fqn,
            owner,
            method,
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
}

#[derive(Debug, Clone, Default)]
pub struct MethodStore {
    facts: Vec<Option<StoredMethodFact>>,
    free_facts: Vec<MethodFactId>,
    facts_by_fqn: HashMap<FqnId, Vec<MethodFactId>>,
    facts_by_owner: HashMap<FqnId, Vec<MethodFactId>>,
    facts_by_owner_name: HashMap<(FqnId, RubyMethod), Vec<MethodFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<MethodFactId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MethodFactId(u32);

impl MethodFactId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: method fact arena exceeded u32 ids. This is a bug because \
             retained method indexes use bounded compact ids. Fix: widen MethodFactId and \
             every stored method index together before retaining more than u32::MAX facts.",
        ))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl MethodStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: StoredMethodFact) {
        let method_name = fact.method;
        let file_id = fact.range.file_id;
        let fqn = fact.fqn;
        let owner = fact.owner;
        let id = self.insert_fact(fact);
        self.facts_by_fqn.entry(fqn).or_default().push(id);
        sort_method_ids_by_fqn(&self.facts, self.facts_by_fqn.get_mut(&fqn).unwrap());
        self.facts_by_owner.entry(owner).or_default().push(id);
        sort_method_ids_by_owner(&self.facts, self.facts_by_owner.get_mut(&owner).unwrap());
        if let Some(method_name) = method_name {
            let key = (owner, method_name);
            self.facts_by_owner_name.entry(key).or_default().push(id);
            sort_method_ids_by_owner(&self.facts, self.facts_by_owner_name.get_mut(&key).unwrap());
        }
        self.facts_by_file.entry(file_id).or_default().push(id);
        sort_method_ids_by_file(&self.facts, self.facts_by_file.get_mut(&file_id).unwrap());
    }

    pub fn facts_for(&self, fqn: FqnId) -> Vec<StoredMethodFact> {
        self.facts_by_fqn
            .get(&fqn)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn all_facts(&self) -> Vec<StoredMethodFact> {
        self.facts.iter().filter_map(|fact| fact.clone()).collect()
    }

    pub fn fact_count(&self) -> usize {
        self.facts.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn facts_matching_owner(&self, owner: FqnId, partial: &str) -> Vec<StoredMethodFact> {
        self.facts_by_owner
            .get(&owner)
            .into_iter()
            .flat_map(|ids| ids.iter().filter_map(|id| self.fact(*id)))
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
        self.facts_by_owner_name
            .get(&(owner, *method))
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
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
        let Some(ids) = self.facts_by_owner_name.get(&(owner, *method)) else {
            return StoredMethodFactMatch::Missing;
        };
        let indexed_fact = |id: &MethodFactId| {
            self.fact(*id).expect(
                "INVARIANT VIOLATED: method owner/name index points to a missing fact. \
                 This is a bug because indexes must be cleared before arena facts. \
                 Fix: remove every MethodFactId from owner/name indexes before freeing it.",
            )
        };

        if ids
            .iter()
            .map(indexed_fact)
            .any(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
        {
            return StoredMethodFactMatch::Missing;
        }
        let unavailable_wins = ids
            .iter()
            .map(indexed_fact)
            .any(|fact| matches!(fact.availability, MethodAvailability::Unavailable { .. }));
        let mut effective = ids.iter().map(indexed_fact).filter(|fact| {
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

    pub fn method_names_for_owner(&self, owner: FqnId) -> Vec<&'static str> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let Some(facts) = self.facts_by_owner.get(&owner) else {
            return names;
        };
        for fact in facts.iter().filter_map(|id| self.fact(*id)) {
            let Some(method) = fact.method else {
                continue;
            };
            let name = method.as_str();
            if seen.insert(name) {
                names.push(name);
            }
        }
        names
    }

    pub(crate) fn ruby_method_names_for_owner(&self, owner: FqnId) -> Vec<RubyMethod> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        let Some(facts) = self.facts_by_owner.get(&owner) else {
            return names;
        };
        for fact in facts.iter().filter_map(|id| self.fact(*id)) {
            let Some(method) = fact.method else {
                continue;
            };
            if seen.insert(method) {
                names.push(method);
            }
        }
        names
    }

    pub fn facts_in_file(&self, file_id: crate::SourceFileId) -> Vec<StoredMethodFact> {
        self.facts_by_file
            .get(&file_id)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    pub fn remove_file(&mut self, file_id: crate::SourceFileId) {
        let Some(stale_facts) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_facts {
            let Some(stale) = self.take_fact(stale_id) else {
                continue;
            };
            self.free_facts.push(stale_id);
            if let Some(ids) = self.facts_by_fqn.get_mut(&stale.fqn) {
                ids.retain(|id| *id != stale_id);
                if ids.is_empty() {
                    self.facts_by_fqn.remove(&stale.fqn);
                }
            }
            if let Some(ids) = self.facts_by_owner.get_mut(&stale.owner) {
                ids.retain(|id| *id != stale_id);
                if ids.is_empty() {
                    self.facts_by_owner.remove(&stale.owner);
                }
            }
            if let Some(method) = stale.method {
                let key = (stale.owner, method);
                if let Some(ids) = self.facts_by_owner_name.get_mut(&key) {
                    ids.retain(|id| *id != stale_id);
                    if ids.is_empty() {
                        self.facts_by_owner_name.remove(&key);
                    }
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: crate::SourceFileId,
        facts: impl IntoIterator<Item = StoredMethodFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_fqns = HashSet::new();
        let mut touched_owners = HashSet::new();
        let mut touched_owner_names = HashSet::new();
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement method fact belongs to a different file id. \
                 This is a bug because MethodStore::replace_file must only receive facts for the target file. \
                 Fix: partition method facts by SourceFileId before replacing."
            );
            let fqn = fact.fqn;
            let owner = fact.owner;
            let method_name = fact.method;
            let id = self.insert_fact(fact);
            touched_fqns.insert(fqn);
            touched_owners.insert(owner);
            self.facts_by_fqn.entry(fqn).or_default().push(id);
            self.facts_by_owner.entry(owner).or_default().push(id);
            if let Some(method) = method_name {
                let key = (owner, method);
                touched_owner_names.insert(key);
                self.facts_by_owner_name.entry(key).or_default().push(id);
            }
            self.facts_by_file.entry(file_id).or_default().push(id);
        }
        for fqn in touched_fqns {
            if let Some(ids) = self.facts_by_fqn.get_mut(&fqn) {
                sort_method_ids_by_fqn(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        for owner in touched_owners {
            if let Some(ids) = self.facts_by_owner.get_mut(&owner) {
                sort_method_ids_by_owner(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        for key in touched_owner_names {
            if let Some(ids) = self.facts_by_owner_name.get_mut(&key) {
                sort_method_ids_by_owner(&self.facts, ids);
                ids.shrink_to_fit();
            }
        }
        if let Some(ids) = self.facts_by_file.get_mut(&file_id) {
            sort_method_ids_by_file(&self.facts, ids);
            ids.shrink_to_fit();
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.facts)
            + vec_payload_bytes(&self.free_facts)
            + self
                .facts
                .iter()
                .filter_map(|fact| fact.as_ref())
                .map(method_fact_heap_bytes)
                .sum::<usize>()
            + map_table_bytes(&self.facts_by_fqn)
            + map_table_bytes(&self.facts_by_owner)
            + map_table_bytes(&self.facts_by_owner_name)
            + map_table_bytes(&self.facts_by_file)
            + self
                .facts_by_fqn
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_owner
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_owner_name
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.free_facts.shrink_to_fit();
        self.facts_by_fqn.shrink_to_fit();
        self.facts_by_owner.shrink_to_fit();
        self.facts_by_owner_name.shrink_to_fit();
        self.facts_by_file.shrink_to_fit();
        for ids in self.facts_by_fqn.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_owner.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_owner_name.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_file.values_mut() {
            ids.shrink_to_fit();
        }
    }

    fn insert_fact(&mut self, fact: StoredMethodFact) -> MethodFactId {
        if let Some(id) = self.free_facts.pop() {
            let slot = self.facts.get_mut(id.index()).expect(
                "INVARIANT VIOLATED: method free list points outside fact arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by MethodStore::take_fact.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: method free list points to occupied fact slot. \
                 This is a bug because free ids must only reference removed facts. \
                 Fix: push each removed method id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = MethodFactId::from_index(self.facts.len());
        self.facts.push(Some(fact));
        id
    }

    fn fact(&self, id: MethodFactId) -> Option<&StoredMethodFact> {
        self.facts.get(id.index()).and_then(Option::as_ref)
    }

    fn take_fact(&mut self, id: MethodFactId) -> Option<StoredMethodFact> {
        self.facts.get_mut(id.index()).and_then(Option::take)
    }

    fn clone_facts(&self, ids: &[MethodFactId]) -> Vec<StoredMethodFact> {
        ids.iter()
            .filter_map(|id| self.fact(*id).cloned())
            .collect()
    }
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

fn sort_method_ids_by_fqn(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
            "INVARIANT VIOLATED: method index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.owner,
        )
    });
}

fn sort_method_ids_by_owner(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
            "INVARIANT VIOLATED: method owner index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            fact.fqn,
        )
    });
}

fn sort_method_ids_by_file(facts: &[Option<StoredMethodFact>], ids: &mut [MethodFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
            "INVARIANT VIOLATED: method file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every MethodStore index.",
        );
        (fact.range.start_byte, fact.range.end_byte)
    });
}

#[cfg(test)]
mod tests {
    use crate::{FqnId, FullyQualifiedName, RubyMethod, SourceFileId, TextRange};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    #[test]
    fn ordinary_method_has_no_higher_order_payload_and_empty_replacement_clears_it() {
        let method = RubyMethod::new("transform").unwrap();
        let ordinary = MethodFact::new(
            FullyQualifiedName::method(Vec::new(), method),
            FullyQualifiedName::namespace(Vec::new()),
            TextRange::new(file(), 0, 8),
        );
        assert!(ordinary.higher_order.is_none());

        let signature = CallableSignature {
            receiver_type_parameters: Vec::new(),
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            block: crate::CallableBlockTemplate {
                parameters: Vec::new(),
                return_type: CallableTypeTemplate::Unconstrained,
                required: true,
            },
            return_type: CallableTypeTemplate::Unconstrained,
        };
        let with_signature = ordinary.with_callable_signatures(vec![signature]);
        assert_eq!(with_signature.callable_signatures().len(), 1);
        assert!(with_signature.higher_order.is_some());

        let cleared = with_signature.with_callable_signatures(Vec::new());
        assert!(cleared.callable_signatures().is_empty());
        assert!(cleared.higher_order.is_none());
    }

    #[test]
    fn replace_file_removes_stale_method_facts_for_same_file_only() {
        let fqn = FqnId(1);
        let other_fqn = FqnId(2);
        let owner = FqnId(3);
        let name = RubyMethod::new("name").unwrap();
        let email = RubyMethod::new("email").unwrap();
        let mut store = MethodStore::new();
        store.add(StoredMethodFact::new(
            fqn,
            owner,
            Some(name),
            TextRange::new(file(), 0, 8),
        ));
        store.add(StoredMethodFact::new(
            other_fqn,
            owner,
            Some(email),
            TextRange::new(SourceFileId(2), 0, 8),
        ));

        store.replace_file(
            file(),
            [StoredMethodFact::new(
                fqn,
                owner,
                Some(name),
                TextRange::new(file(), 10, 18),
            )],
        );

        assert_eq!(store.facts_for(fqn).len(), 1);
        assert_eq!(store.facts_for(fqn)[0].range.start_byte, 10);
        assert_eq!(store.facts_for(other_fqn).len(), 1);
    }

    #[test]
    fn exact_owner_name_match_borrows_one_effective_fact_and_deduplicates() {
        let fqn = FqnId(1);
        let owner = FqnId(2);
        let method = RubyMethod::new("call").unwrap();
        let fact = StoredMethodFact::new(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(3), 4, 12),
        );
        let mut store = MethodStore::new();
        store.add(fact.clone());
        store.add(fact);

        let match_result = store.effective_fact_matching_owner_name(owner, &method);
        let StoredMethodFactMatch::Unique(selected) = match_result else {
            panic!("identical stored method facts must collapse to one borrowed match")
        };
        let first_id = store.facts_by_owner_name[&(owner, method)][0];
        assert!(std::ptr::eq(
            selected,
            store
                .fact(first_id)
                .expect("the indexed method fact must remain in the arena")
        ));
    }

    #[test]
    fn exact_owner_name_match_preserves_availability_and_ambiguity() {
        let fqn = FqnId(1);
        let owner = FqnId(2);
        let method = RubyMethod::new("call").unwrap();
        let mut available = StoredMethodFact::new(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(1), 0, 8),
        );
        let mut unavailable = StoredMethodFact::new(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(2), 0, 8),
        );
        unavailable.availability = MethodAvailability::Unavailable {
            reason: "JRuby runtime API".to_string(),
        };

        let mut store = MethodStore::new();
        store.add(available.clone());
        store.add(unavailable);
        assert!(matches!(
            store.effective_fact_matching_owner_name(owner, &method),
            StoredMethodFactMatch::Unique(fact)
                if matches!(fact.availability, MethodAvailability::Unavailable { .. })
        ));

        available.range = TextRange::new(SourceFileId(3), 0, 8);
        available.availability = MethodAvailability::Absent {
            reason: "not defined by this runtime".to_string(),
        };
        store.add(available);
        assert!(matches!(
            store.effective_fact_matching_owner_name(owner, &method),
            StoredMethodFactMatch::Missing
        ));

        let mut ambiguous = MethodStore::new();
        ambiguous.add(StoredMethodFact::new(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(1), 0, 8),
        ));
        ambiguous.add(StoredMethodFact::new(
            fqn,
            owner,
            Some(method),
            TextRange::new(SourceFileId(2), 0, 8),
        ));
        assert!(matches!(
            ambiguous.effective_fact_matching_owner_name(owner, &method),
            StoredMethodFactMatch::Ambiguous
        ));
    }
}
