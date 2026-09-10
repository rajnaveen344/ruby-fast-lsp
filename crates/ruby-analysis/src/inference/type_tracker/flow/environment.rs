use crate::core::{ConstantTypeDependency, LiteralKey, RubyType, UnknownReason, MAX_SHAPE_ALIASES};
use crate::inference::type_tracker::flow::shapes::identities::{
    ArrayShapeAliases, ShapeContainment, ShapeIdentity, ShapeIdentityState,
};
use crate::inference::type_tracker::flow::shapes::values::{
    non_shape_alternatives, shape_alternatives,
};
use crate::inference::type_tracker::TypeTracker;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default)]
pub(in crate::inference::type_tracker) struct FlowEnvironment {
    pub(in crate::inference::type_tracker) types: HashMap<String, RubyType>,
    pub(in crate::inference::type_tracker) constant_dependencies:
        HashMap<String, BTreeSet<ConstantTypeDependency>>,
    pub(in crate::inference::type_tracker) shape_bindings: HashMap<String, BTreeSet<ShapeIdentity>>,
    pub(in crate::inference::type_tracker) shape_states: HashMap<ShapeIdentity, ShapeIdentityState>,
    pub(in crate::inference::type_tracker) shape_containments: BTreeSet<ShapeContainment>,
    pub(in crate::inference::type_tracker) array_shape_aliases: HashMap<String, ArrayShapeAliases>,
    pub(in crate::inference::type_tracker) unknown_reasons: HashMap<String, UnknownReason>,
    pub(in crate::inference::type_tracker) callables:
        HashMap<String, crate::inference::higher_order::KnownProcType>,
    pub(in crate::inference::type_tracker) max_live_shape_aliases: usize,
}

impl FlowEnvironment {
    pub(in crate::inference::type_tracker) fn insert(
        &mut self,
        name: String,
        ruby_type: RubyType,
    ) -> Option<RubyType> {
        self.constant_dependencies.remove(&name);
        self.shape_bindings.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.unknown_reasons.remove(&name);
        self.types.insert(name, ruby_type)
    }

    pub(in crate::inference::type_tracker) fn insert_unknown(
        &mut self,
        name: String,
        reason: UnknownReason,
    ) -> Option<RubyType> {
        self.constant_dependencies.remove(&name);
        self.shape_bindings.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.unknown_reasons.insert(name.clone(), reason);
        self.types.insert(name, RubyType::Unknown)
    }

    pub(in crate::inference::type_tracker) fn bind_shape_identities(
        &mut self,
        name: String,
        ruby_type: RubyType,
        identities: BTreeSet<ShapeIdentity>,
    ) {
        assert!(
            !identities.is_empty(),
            "INVARIANT VIOLATED: a shape binding was installed without an abstract identity. This is a bug because aliases can only synchronize through a concrete flow-local identity. Fix: allocate or copy at least one identity before binding a shape local."
        );
        self.constant_dependencies.remove(&name);
        self.unknown_reasons.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.types.insert(name.clone(), ruby_type);
        self.shape_bindings.insert(name, identities);
        self.enforce_alias_bound();
    }

    pub(in crate::inference::type_tracker) fn shape_identities(
        &self,
        name: &str,
    ) -> BTreeSet<ShapeIdentity> {
        self.shape_bindings.get(name).cloned().unwrap_or_default()
    }

    pub(in crate::inference::type_tracker) fn unknown_reason(
        &self,
        name: &str,
    ) -> Option<UnknownReason> {
        self.unknown_reasons.get(name).copied()
    }

    pub(in crate::inference::type_tracker) fn invalidate_identities(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        reason: UnknownReason,
    ) {
        let mut affected = identities.clone();
        loop {
            let before = affected.len();
            for link in &self.shape_containments {
                if affected.contains(&link.parent) || affected.contains(&link.child) {
                    affected.insert(link.parent);
                    affected.insert(link.child);
                }
            }
            if affected.len() == before {
                break;
            }
        }
        for identity in &affected {
            assert!(
                self.shape_states.contains_key(identity),
                "INVARIANT VIOLATED: shape invalidation targeted an unknown abstract identity {:?}. This is a bug because a local cannot reference an identity absent from its environment. Fix: merge identity bindings and states atomically.",
                identity
            );
            self.shape_states
                .insert(*identity, ShapeIdentityState::Invalidated(reason));
        }
        self.synchronize_shape_aliases();
    }

    pub(in crate::inference::type_tracker) fn clear(&mut self) {
        self.types.clear();
        self.constant_dependencies.clear();
        self.shape_bindings.clear();
        self.shape_states.clear();
        self.shape_containments.clear();
        self.array_shape_aliases.clear();
        self.unknown_reasons.clear();
        self.callables.clear();
        self.max_live_shape_aliases = 0;
    }

    pub(in crate::inference::type_tracker) fn bind_array_shape_aliases(
        &mut self,
        name: String,
        aliases: ArrayShapeAliases,
    ) {
        if aliases.contained.is_empty() && aliases.unknown_reason.is_none() {
            self.array_shape_aliases.remove(&name);
        } else {
            self.array_shape_aliases.insert(name, aliases);
        }
        self.synchronize_shape_aliases();
    }

    pub(in crate::inference::type_tracker) fn contained_child(
        &self,
        parent: ShapeIdentity,
        key: &LiteralKey,
    ) -> Option<ShapeIdentity> {
        let mut matches = self
            .shape_containments
            .iter()
            .filter(|link| link.parent == parent && &link.key == key)
            .map(|link| link.child);
        let child = matches.next()?;
        assert!(
            matches.next().is_none(),
            "INVARIANT VIOLATED: one parent Hash field points at multiple abstract child identities. This is a bug because a precise required field contains one Ruby object on one flow path. Fix: invalidate ambiguous branch containment before installing the merged environment."
        );
        Some(child)
    }

    pub(in crate::inference::type_tracker) fn link_contained_shape(
        &mut self,
        parent: ShapeIdentity,
        key: LiteralKey,
        child: ShapeIdentity,
    ) {
        assert!(
            parent != child,
            "INVARIANT VIOLATED: a Hash identity was linked as its own statically proven child. This is a bug because bounded literal construction cannot create a recursive Ruby Hash. Fix: treat runtime-created cycles as an unsupported mutation boundary."
        );
        assert!(
            self.shape_states.contains_key(&parent) && self.shape_states.contains_key(&child),
            "INVARIANT VIOLATED: a containment edge references an absent Hash identity. This is a bug because parent and child states must exist before their relationship is installed. Fix: allocate both identities before calling link_contained_shape."
        );
        if let Some(existing) = self.contained_child(parent, &key) {
            assert_eq!(
                existing, child,
                "INVARIANT VIOLATED: one parent Hash field was rebound without detaching its prior child identity. This is a bug because Ruby assignment replaces the contained object. Fix: detach the exact field before installing its replacement containment edge."
            );
            return;
        }
        self.shape_containments
            .insert(ShapeContainment { parent, key, child });
    }

    pub(in crate::inference::type_tracker) fn detach_contained_shapes(
        &mut self,
        parents: &BTreeSet<ShapeIdentity>,
        key: Option<&LiteralKey>,
    ) {
        self.shape_containments.retain(|link| {
            !parents.contains(&link.parent) || key.is_some_and(|key| key != &link.key)
        });
    }

    pub(in crate::inference::type_tracker) fn synchronize_shape_aliases(&mut self) {
        let names = self.shape_bindings.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let identities = self.shape_identities(&name);
            let current_type = self.types.get(&name).cloned().unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape alias `{name}` has no local type. This is a bug because bindings and type entries must be installed atomically. Fix: use FlowEnvironment::bind_shape_identities for shape locals."
                )
            });
            match self.type_for_shape_identities(&current_type, &identities) {
                Ok(ruby_type) => {
                    self.types.insert(name.clone(), ruby_type);
                    self.unknown_reasons.remove(&name);
                }
                Err(reason) => {
                    self.types.insert(name.clone(), RubyType::Unknown);
                    self.unknown_reasons.insert(name, reason);
                }
            }
        }

        let array_names = self.array_shape_aliases.keys().cloned().collect::<Vec<_>>();
        for name in array_names {
            let aliases = self
                .array_shape_aliases
                .get(&name)
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: Array alias `{name}` disappeared during synchronization. This is a bug because the alias-name snapshot and map are not mutated by type projection. Fix: keep Array alias removal outside synchronize_shape_aliases."
                    )
                });
            let current_type = self.types.get(&name).cloned().unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: Array alias `{name}` has no local type. This is a bug because positional evidence and the Array type must be installed atomically. Fix: bind Array aliases only after inserting the assignment type."
                )
            });
            match self.type_for_array_shape_aliases(&current_type, &aliases) {
                Ok(ruby_type) => {
                    self.types.insert(name.clone(), ruby_type);
                    self.unknown_reasons.remove(&name);
                }
                Err(reason) => {
                    self.types
                        .insert(name.clone(), RubyType::Array(vec![RubyType::Unknown]));
                    self.unknown_reasons.insert(name, reason);
                }
            }
        }
    }

    pub(in crate::inference::type_tracker) fn type_for_array_shape_aliases(
        &self,
        current_type: &RubyType,
        aliases: &ArrayShapeAliases,
    ) -> Result<RubyType, UnknownReason> {
        if let Some(reason) = aliases.unknown_reason {
            return Err(reason);
        }
        let mut elements = array_element_alternatives(current_type)
            .iter()
            .flat_map(non_shape_alternatives)
            .collect::<Vec<_>>();
        for identity in &aliases.contained {
            match self.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: Array alias references absent contained identity {:?}. This is a bug because Array summaries and shape states must merge atomically. Fix: preserve every contained identity state while retaining positional evidence.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => {
                    elements.extend(shape_alternatives(ruby_type)?);
                }
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            }
        }
        assert!(
            !elements.is_empty(),
            "INVARIANT VIOLATED: a proven Array shape summary produced no element alternatives. This is a bug because a retained summary must contain at least one shape identity or an explicit unknown reason. Fix: remove empty Array summaries in bind_array_shape_aliases."
        );
        Ok(RubyType::Array(RubyType::canonical_union_members(elements)))
    }

    pub(in crate::inference::type_tracker) fn type_for_shape_identities(
        &self,
        current_type: &RubyType,
        identities: &BTreeSet<ShapeIdentity>,
    ) -> Result<RubyType, UnknownReason> {
        let mut alternatives = non_shape_alternatives(current_type);
        for identity in identities {
            match self.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape binding references absent identity {:?}. This is a bug because branch joins must merge identity states before synchronizing aliases. Fix: keep shape_bindings and shape_states in one FlowEnvironment.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => {
                    alternatives.extend(shape_alternatives(ruby_type)?);
                }
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            }
        }
        let joined = RubyType::union(alternatives);
        if joined == RubyType::Unknown {
            return Err(UnknownReason::ShapeBoundExceeded);
        }
        Ok(joined)
    }

    pub(in crate::inference::type_tracker) fn enforce_alias_bound(&mut self) {
        let identities = self.shape_states.keys().copied().collect::<Vec<_>>();
        let mut exceeded = BTreeSet::new();
        for identity in identities {
            let alias_count = self
                .shape_bindings
                .values()
                .filter(|binding| binding.contains(&identity))
                .count()
                + self
                    .shape_containments
                    .iter()
                    .filter(|link| link.child == identity)
                    .count();
            self.max_live_shape_aliases = self.max_live_shape_aliases.max(alias_count);
            if alias_count > MAX_SHAPE_ALIASES {
                exceeded.insert(identity);
            }
        }
        if !exceeded.is_empty() {
            self.invalidate_identities(&exceeded, UnknownReason::ShapeBoundExceeded);
        }
    }

    pub(in crate::inference::type_tracker) fn set_constant_dependencies(
        &mut self,
        name: String,
        dependencies: BTreeSet<ConstantTypeDependency>,
    ) {
        if dependencies.is_empty() {
            self.constant_dependencies.remove(&name);
        } else {
            self.constant_dependencies.insert(name, dependencies);
        }
    }

    pub(in crate::inference::type_tracker) fn dependencies(
        &self,
        name: &str,
    ) -> BTreeSet<ConstantTypeDependency> {
        self.constant_dependencies
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
}

impl TypeTracker {
    /// Merge another environment into this one
    ///
    /// Used at control flow join points (after if/case/while).
    /// Variables with different types are merged into unions.
    ///
    /// If `no_else_branch` is true, variables that only exist in one branch
    /// are assumed to be nil in the other branch.
    pub(in crate::inference::type_tracker) fn merge_env(
        &mut self,
        other_env: &FlowEnvironment,
        no_else_branch: bool,
    ) {
        let this_env = self.environment.clone();
        // For each variable in other environment
        for (var, other_ty) in &other_env.types {
            if let Some(this_ty) = self.environment.types.get(var) {
                // Variable exists in both - create union if types differ
                if this_ty != other_ty {
                    let union = RubyType::union(vec![this_ty.clone(), other_ty.clone()]);
                    self.environment.insert(var.clone(), union);
                }
            } else {
                // Variable only in other environment
                if no_else_branch {
                    // No else branch: variable might not be defined
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.environment.insert(var.clone(), union);
                } else {
                    // Has else branch: variable was defined in else but not then
                    // Add with nil union
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.environment.insert(var.clone(), union);
                }
            }
        }

        // Handle variables only in this environment (they might be nil in other)
        if no_else_branch {
            // If there's no else branch, variables in then branch might be undefined
            // when the condition is false
            for (var, this_ty) in self.environment.types.clone() {
                if !other_env.types.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.environment.insert(var, union);
                }
            }
        } else {
            // Has else branch: variables in then but not else get nil union
            for (var, this_ty) in self.environment.types.clone() {
                if !other_env.types.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.environment.insert(var, union);
                }
            }
        }

        let mut names = this_env
            .types
            .keys()
            .chain(other_env.types.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_dependencies = HashMap::new();
        for name in std::mem::take(&mut names) {
            let this_dependencies = this_env.dependencies(&name);
            let other_dependencies = other_env.dependencies(&name);
            // A dependency-only equation can represent this join only when
            // every reachable branch contributes a constant term. A literal,
            // parameter, unsupported Unknown, or implicit nil branch would
            // require a concrete base term; drop the equation rather than
            // publishing the known constant members as a partial union.
            if this_dependencies.is_empty() || other_dependencies.is_empty() {
                continue;
            }
            let dependencies = this_dependencies
                .into_iter()
                .chain(other_dependencies)
                .collect::<BTreeSet<_>>();
            if !dependencies.is_empty() {
                merged_dependencies.insert(name, dependencies);
            }
        }
        self.environment.constant_dependencies = merged_dependencies;

        // Merge abstract Hash identity state independently from displayed
        // local types. The same identity can be mutated differently on two
        // reachable branches; its joined state must then update every alias,
        // preserving complete correlated variants rather than field-wise
        // compression.
        let identity_keys = this_env
            .shape_states
            .keys()
            .chain(other_env.shape_states.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let mut merged_states = HashMap::new();
        for identity in identity_keys {
            let state = match (
                this_env.shape_states.get(&identity),
                other_env.shape_states.get(&identity),
            ) {
                (
                    Some(ShapeIdentityState::Invalidated(reason)),
                    Some(ShapeIdentityState::Invalidated(other_reason)),
                ) => ShapeIdentityState::Invalidated(if reason == other_reason {
                    *reason
                } else if *reason == UnknownReason::ShapeBoundExceeded
                    || *other_reason == UnknownReason::ShapeBoundExceeded
                {
                    UnknownReason::ShapeBoundExceeded
                } else {
                    UnknownReason::MutableShapeInvalidated
                }),
                (Some(ShapeIdentityState::Invalidated(reason)), Some(_))
                | (Some(_), Some(ShapeIdentityState::Invalidated(reason))) => {
                    ShapeIdentityState::Invalidated(*reason)
                }
                (
                    Some(ShapeIdentityState::Proven(left)),
                    Some(ShapeIdentityState::Proven(right)),
                ) => {
                    let joined = RubyType::union([left.clone(), right.clone()]);
                    if joined == RubyType::Unknown {
                        ShapeIdentityState::Invalidated(UnknownReason::ShapeBoundExceeded)
                    } else {
                        ShapeIdentityState::Proven(joined)
                    }
                }
                (Some(state), None) | (None, Some(state)) => state.clone(),
                (None, None) => panic!(
                    "INVARIANT VIOLATED: merged shape identity {:?} is absent from both branch environments. This is a bug because identity_keys is derived from those exact maps. Fix: keep key collection and state lookup in one immutable merge.",
                    identity
                ),
            };
            merged_states.insert(identity, state);
        }

        let binding_names = this_env
            .shape_bindings
            .keys()
            .chain(other_env.shape_bindings.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_bindings = HashMap::new();
        for name in binding_names {
            let identities = this_env
                .shape_identities(&name)
                .into_iter()
                .chain(other_env.shape_identities(&name))
                .collect::<BTreeSet<_>>();
            if !identities.is_empty() {
                merged_bindings.insert(name, identities);
            }
        }

        let mut merged_unknown_reasons = HashMap::new();
        for name in this_env
            .unknown_reasons
            .keys()
            .chain(other_env.unknown_reasons.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            if self.environment.types.get(&name) != Some(&RubyType::Unknown) {
                continue;
            }
            let reason = match (
                this_env.unknown_reason(&name),
                other_env.unknown_reason(&name),
            ) {
                (Some(left), Some(right)) if left == right => left,
                (Some(UnknownReason::ShapeBoundExceeded), _)
                | (_, Some(UnknownReason::ShapeBoundExceeded)) => UnknownReason::ShapeBoundExceeded,
                (Some(UnknownReason::MutableShapeInvalidated), _)
                | (_, Some(UnknownReason::MutableShapeInvalidated)) => {
                    UnknownReason::MutableShapeInvalidated
                }
                (Some(reason), None) | (None, Some(reason)) => reason,
                (Some(_), Some(_)) | (None, None) => UnknownReason::UnresolvedAssignmentValue,
            };
            merged_unknown_reasons.insert(name, reason);
        }

        let merged_containments = this_env
            .shape_containments
            .intersection(&other_env.shape_containments)
            .cloned()
            .collect::<BTreeSet<_>>();
        for link in this_env
            .shape_containments
            .symmetric_difference(&other_env.shape_containments)
        {
            for identity in [link.parent, link.child] {
                if merged_states.contains_key(&identity) {
                    merged_states.insert(
                        identity,
                        ShapeIdentityState::Invalidated(UnknownReason::MutableShapeInvalidated),
                    );
                }
            }
        }
        let array_names = this_env
            .array_shape_aliases
            .keys()
            .chain(other_env.array_shape_aliases.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_array_shape_aliases = HashMap::new();
        let mut inconsistent_array_identities = BTreeSet::new();
        for name in array_names {
            match (
                this_env.array_shape_aliases.get(&name),
                other_env.array_shape_aliases.get(&name),
            ) {
                (Some(left), Some(right)) if left == right => {
                    merged_array_shape_aliases.insert(name, left.clone());
                }
                (Some(left), Some(right)) => {
                    inconsistent_array_identities.extend(&left.contained);
                    inconsistent_array_identities.extend(&right.contained);
                }
                (Some(aliases), None) | (None, Some(aliases)) => {
                    inconsistent_array_identities.extend(&aliases.contained);
                }
                (None, None) => panic!(
                    "INVARIANT VIOLATED: merged Array alias name `{name}` is absent from both branch environments. This is a bug because array_names is derived from those exact maps. Fix: keep key collection and lookup in one immutable merge."
                ),
            }
        }
        self.environment.shape_states = merged_states;
        self.environment.shape_bindings = merged_bindings;
        self.environment.shape_containments = merged_containments;
        self.environment.array_shape_aliases = merged_array_shape_aliases;
        self.environment.unknown_reasons = merged_unknown_reasons;
        self.environment.max_live_shape_aliases = this_env
            .max_live_shape_aliases
            .max(other_env.max_live_shape_aliases);
        let callable_names = this_env
            .callables
            .keys()
            .chain(other_env.callables.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_callables = HashMap::new();
        for name in callable_names {
            let merged = match (
                this_env.callables.get(&name),
                other_env.callables.get(&name),
            ) {
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
                    "INVARIANT VIOLATED: callable merge key `{name}` is absent from both branch environments. This is a bug because keys are derived from those exact maps. Fix: keep callable key collection and lookup atomic."
                ),
            };
            merged_callables.insert(name, merged);
        }
        self.environment.callables = merged_callables;
        if !inconsistent_array_identities.is_empty() {
            self.environment.invalidate_identities(
                &inconsistent_array_identities,
                UnknownReason::MutableShapeInvalidated,
            );
        }
        self.environment.enforce_alias_bound();
        self.environment.synchronize_shape_aliases();
    }
}
pub(in crate::inference::type_tracker) fn array_element_alternatives(
    ruby_type: &RubyType,
) -> Vec<RubyType> {
    match ruby_type {
        RubyType::Array(elements) => elements.clone(),
        RubyType::Union(members) => members
            .iter()
            .flat_map(array_element_alternatives)
            .collect(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => panic!(
            "INVARIANT VIOLATED: positional shape aliases are attached to non-Array type `{ruby_type}`. This is a bug because every reachable type for an Array identity must remain an Array. Fix: clear Array aliases whenever any branch rebinds the local to a non-Array value."
        ),
    }
}
