use crate::core::{LiteralKey, RubyType, UnknownReason, MAX_SHAPE_ALIASES};
use crate::inference::r#type::literal::literal_key;
use crate::inference::type_tracker::flow::shapes::identities::{
    ArrayShapeAliases, ShapeAlternativeTransition, ShapeContainment, ShapeIdentity,
    ShapeIdentityState,
};
use crate::inference::type_tracker::flow::shapes::values::{
    literal_array_index, literal_hash_keys, precise_contained_shape_field, shape_alternatives,
    shape_with_contained_transitions, type_is_shape_only,
};
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::{BTreeMap, BTreeSet};

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn allocate_shape_identity(
        &mut self,
        ruby_type: RubyType,
    ) -> ShapeIdentity {
        assert!(
            type_is_shape_only(&ruby_type),
            "INVARIANT VIOLATED: attempted to allocate a Hash identity for non-shape type `{ruby_type}`. This is a bug because only complete shape-producing expressions participate in alias tracking. Fix: guard allocation with type_is_shape_only."
        );
        let identity = ShapeIdentity(self.next_shape_identity);
        self.next_shape_identity = self.next_shape_identity.checked_add(1).expect(
            "INVARIANT VIOLATED: one flow traversal allocated more than u32::MAX abstract Hash identities. This is a bug because source size and fixed shape bounds make that impossible in a valid analysis pass. Fix: investigate repeated allocation or widen ShapeIdentity.",
        );
        let previous = self
            .environment
            .shape_states
            .insert(identity, ShapeIdentityState::Proven(ruby_type));
        assert!(
            previous.is_none(),
            "INVARIANT VIOLATED: flow-local Hash identity {:?} was allocated twice. This is a bug because the allocator must be monotonic across cloned branch environments. Fix: keep next_shape_identity on TypeTracker rather than FlowEnvironment.",
            identity
        );
        identity
    }

    pub(in crate::inference::type_tracker) fn shape_identities_for_alias_expression(
        &self,
        node: &Node<'_>,
    ) -> BTreeSet<ShapeIdentity> {
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.environment.shape_identities(name.as_ref());
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .map(|body| self.shape_identities_for_alias_expression(&body))
                .unwrap_or_default();
        }
        if let Some(call) = node.as_call_node() {
            let method_name = call.name().as_slice();
            let identity_returning_hash_call =
                method_name == b"freeze" || (method_name == b"to_h" && call.block().is_none());
            if identity_returning_hash_call && call.arguments().is_none() {
                return call
                    .receiver()
                    .map(|receiver| self.shape_identities_for_alias_expression(&receiver))
                    .unwrap_or_default();
            }
            if let Some(receiver) = call.receiver() {
                if let Some(local) = receiver.as_local_variable_read_node() {
                    let name = String::from_utf8_lossy(local.name().as_slice());
                    if let Some(aliases) = self.environment.array_shape_aliases.get(name.as_ref()) {
                        if call.arguments().is_none() {
                            return match method_name {
                                b"first" => aliases.identities_at(0),
                                b"last" => aliases.identities_at(-1),
                                _ => BTreeSet::new(),
                            };
                        }
                        if matches!(method_name, b"[]" | b"at" | b"fetch") {
                            let arguments = call.arguments().expect(
                                "INVARIANT VIOLATED: checked Array read arguments disappeared before use. This is a bug because Prism nodes are immutable. Fix: destructure call.arguments once.",
                            );
                            let mut arguments = arguments.arguments().iter();
                            let Some(argument) = arguments.next() else {
                                return BTreeSet::new();
                            };
                            if arguments.next().is_none() {
                                if let Some(index) = literal_array_index(&argument) {
                                    return aliases.identities_at(index);
                                }
                            }
                        }
                    }
                }
            }
        }
        BTreeSet::new()
    }

    pub(in crate::inference::type_tracker) fn array_shape_aliases_for_assignment(
        &mut self,
        node: &Node<'_>,
    ) -> Option<ArrayShapeAliases> {
        if let Some(local) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice());
            return self
                .environment
                .array_shape_aliases
                .get(name.as_ref())
                .cloned();
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .and_then(|body| self.array_shape_aliases_for_assignment(&body));
        }
        if let Some(call) = node.as_call_node() {
            if call.arguments().is_none()
                && call.block().is_none()
                && matches!(call.name().as_slice(), b"to_a" | b"freeze")
            {
                let receiver = call.receiver()?;
                let local = receiver.as_local_variable_read_node()?;
                let name = String::from_utf8_lossy(local.name().as_slice());
                return self
                    .environment
                    .array_shape_aliases
                    .get(name.as_ref())
                    .cloned();
            }
        }
        let array = node.as_array_node()?;
        let elements = array.elements().iter().collect::<Vec<_>>();
        let mut aliases = ArrayShapeAliases {
            length: elements.len(),
            ..ArrayShapeAliases::default()
        };
        for (index, element) in elements.iter().enumerate() {
            let mut identities = self.shape_identities_for_alias_expression(element);
            if identities.is_empty() {
                if let Some(local) = element.as_local_variable_read_node() {
                    let name = String::from_utf8_lossy(local.name().as_slice());
                    if let Some(nested) = self.environment.array_shape_aliases.get(name.as_ref()) {
                        identities.extend(&nested.contained);
                    }
                } else if element.as_hash_node().is_some() {
                    if let Some(Ok(ruby_type)) = self.infer_collection_literal_type(element) {
                        if type_is_shape_only(&ruby_type) {
                            let identity = self.allocate_shape_identity(ruby_type);
                            self.link_shape_literal_children(element, identity);
                            if let Err(reason) = self.materialize_direct_shape_children(identity) {
                                self.environment
                                    .invalidate_identities(&BTreeSet::from([identity]), reason);
                                aliases.positions.clear();
                                aliases.contained.clear();
                                aliases.unknown_reason = Some(reason);
                                return Some(aliases);
                            }
                            identities.insert(identity);
                        }
                    }
                }
            }
            if !identities.is_empty() {
                aliases.positions.insert(index, identities.clone());
            }
            aliases.contained.extend(identities);
            if aliases.positions.len() > MAX_SHAPE_ALIASES
                || aliases.contained.len() > MAX_SHAPE_ALIASES
            {
                self.environment
                    .invalidate_identities(&aliases.contained, UnknownReason::ShapeBoundExceeded);
                aliases.positions.clear();
                aliases.contained.clear();
                aliases.unknown_reason = Some(UnknownReason::ShapeBoundExceeded);
                return Some(aliases);
            }
        }
        Some(aliases)
    }

    pub(in crate::inference::type_tracker) fn shape_identities_for_contained_alias_expression(
        &mut self,
        node: &Node<'_>,
        inferred_type: &RubyType,
    ) -> BTreeSet<ShapeIdentity> {
        let Some((parent_name, path)) = shape_local_key_path(node) else {
            return BTreeSet::new();
        };
        let root_parents = self.environment.shape_identities(&parent_name);
        if root_parents.is_empty() {
            return BTreeSet::new();
        }

        let mut current_parents = root_parents.clone();
        for key in path {
            let mut children = BTreeSet::new();
            let mut pending = Vec::new();
            for parent in &current_parents {
                if let Some(child) = self.environment.contained_child(*parent, &key) {
                    children.insert(child);
                    continue;
                }
                let child_type = match self.environment.shape_states.get(parent).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: nested keyed read references absent parent identity {:?}. This is a bug because a local shape binding and its state must be installed atomically. Fix: preserve both through assignment and branch joins.",
                        parent
                    )
                }) {
                    ShapeIdentityState::Proven(parent_type) => {
                        precise_contained_shape_field(parent_type, &key)
                    }
                    ShapeIdentityState::Invalidated(_) => None,
                };
                let Some(child_type) = child_type else {
                    self.environment.invalidate_identities(
                        &root_parents,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return BTreeSet::new();
                };
                pending.push((*parent, child_type));
            }

            for (parent, child_type) in pending {
                let child = self.allocate_shape_identity(child_type);
                self.environment
                    .link_contained_shape(parent, key.clone(), child);
                children.insert(child);
            }
            current_parents = children;
        }

        if current_parents.is_empty() {
            return current_parents;
        }
        let projected = self
            .shape_identity_type(&current_parents)
            .unwrap_or(RubyType::Unknown);
        if projected != *inferred_type {
            self.environment
                .invalidate_identities(&root_parents, UnknownReason::MutableShapeInvalidated);
            return BTreeSet::new();
        }
        current_parents
    }

    pub(in crate::inference::type_tracker) fn link_shape_literal_children(
        &mut self,
        node: &Node<'_>,
        parent: ShapeIdentity,
    ) {
        if self.link_non_mutating_merge_children(node, parent) {
            return;
        }
        let hash = if let Some(hash) = node.as_hash_node() {
            Some(hash)
        } else if let Some(parentheses) = node.as_parentheses_node() {
            parentheses.body().and_then(|body| body.as_hash_node())
        } else {
            None
        };
        let Some(hash) = hash else {
            return;
        };

        let mut links = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for element in hash.elements().iter() {
            if let Some(splat) = element.as_assoc_splat_node() {
                let Some(value) = splat.value() else {
                    self.environment.invalidate_identities(
                        &BTreeSet::from([parent]),
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return;
                };
                let source_parents = self.shape_identities_for_alias_expression(&value);
                for source_parent in &source_parents {
                    if self
                        .materialize_direct_shape_children(*source_parent)
                        .is_err()
                    {
                        self.environment.invalidate_identities(
                            &BTreeSet::from([parent]),
                            UnknownReason::MutableShapeInvalidated,
                        );
                        return;
                    }
                }
                for link in self.environment.shape_containments.clone() {
                    if !source_parents.contains(&link.parent) {
                        continue;
                    }
                    if links
                        .insert(link.key.clone(), link.child)
                        .is_some_and(|previous| previous != link.child)
                    {
                        self.environment.invalidate_identities(
                            &BTreeSet::from([parent]),
                            UnknownReason::MutableShapeInvalidated,
                        );
                        return;
                    }
                }
                continue;
            }
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            let Some(key) = literal_key(&assoc.key()) else {
                continue;
            };
            links.remove(&key);
            let children = self.shape_identities_for_alias_expression(&assoc.value());
            if children.is_empty() {
                continue;
            }
            let Some(child) = children
                .iter()
                .copied()
                .next()
                .filter(|_| children.len() == 1)
            else {
                self.environment.invalidate_identities(
                    &BTreeSet::from([parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return;
            };
            links.insert(key, child);
        }

        for (key, child) in links {
            let parent_field = self
                .environment
                .shape_states
                .get(&parent)
                .and_then(|state| match state {
                    ShapeIdentityState::Proven(parent_type) => {
                        precise_contained_shape_field(parent_type, &key)
                    }
                    ShapeIdentityState::Invalidated(_) => None,
                });
            let child_type = self.shape_identity_type(&BTreeSet::from([child])).ok();
            if parent_field.is_none() || parent_field != child_type {
                self.environment.invalidate_identities(
                    &BTreeSet::from([parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return;
            }
            self.environment.link_contained_shape(parent, key, child);
        }
        self.environment.enforce_alias_bound();
    }

    pub(in crate::inference::type_tracker) fn link_non_mutating_merge_children(
        &mut self,
        node: &Node<'_>,
        new_parent: ShapeIdentity,
    ) -> bool {
        let Some(call) = node.as_call_node() else {
            return false;
        };
        if call.name().as_slice() != b"merge" {
            return false;
        }
        let Some(receiver) = call.receiver() else {
            return false;
        };
        let Some(receiver_local) = receiver.as_local_variable_read_node() else {
            return false;
        };
        let Some(arguments) = call.arguments() else {
            return false;
        };
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return false;
        }
        let right = &argument_nodes[0];
        let Some(right_hash) = right.as_hash_node() else {
            self.environment.invalidate_identities(
                &BTreeSet::from([new_parent]),
                UnknownReason::MutableShapeInvalidated,
            );
            return true;
        };
        let Some(overwritten_keys) = literal_hash_keys(&right_hash) else {
            self.environment.invalidate_identities(
                &BTreeSet::from([new_parent]),
                UnknownReason::MutableShapeInvalidated,
            );
            return true;
        };
        let receiver_name = String::from_utf8_lossy(receiver_local.name().as_slice());
        let source_parents = self.environment.shape_identities(receiver_name.as_ref());
        for source_parent in &source_parents {
            if self
                .materialize_direct_shape_children(*source_parent)
                .is_err()
            {
                self.environment.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
        }

        let mut copied = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for link in self.environment.shape_containments.clone() {
            if !source_parents.contains(&link.parent) || overwritten_keys.contains(&link.key) {
                continue;
            }
            if copied
                .insert(link.key.clone(), link.child)
                .is_some_and(|previous| previous != link.child)
            {
                self.environment.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
        }
        for (key, child) in copied {
            let child_type = self.shape_identity_type(&BTreeSet::from([child])).ok();
            let parent_field = self
                .environment
                .shape_states
                .get(&new_parent)
                .and_then(|state| match state {
                    ShapeIdentityState::Proven(parent_type) => {
                        precise_contained_shape_field(parent_type, &key)
                    }
                    ShapeIdentityState::Invalidated(_) => None,
                });
            if parent_field.is_none() || parent_field != child_type {
                self.environment.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
            self.environment
                .link_contained_shape(new_parent, key, child);
        }

        self.link_shape_literal_children(right, new_parent);
        true
    }

    pub(in crate::inference::type_tracker) fn link_mutating_merge_children(
        &mut self,
        receivers: &BTreeSet<ShapeIdentity>,
        right: &Node<'_>,
        overwritten_keys: &BTreeSet<LiteralKey>,
    ) -> Result<(), UnknownReason> {
        if right.as_hash_node().is_some() {
            for receiver in receivers {
                self.link_shape_literal_children(right, *receiver);
            }
            self.shape_identity_type(receivers).map(|_| ())?;
            return Ok(());
        }

        let source_parents = self.shape_identities_for_alias_expression(right);
        if source_parents.is_empty() {
            // There is no local alias that must remain synchronized. Create
            // child identities from the now-complete receiver shape so later
            // keyed reads within this flow still share one object identity.
            for receiver in receivers {
                self.materialize_direct_shape_children(*receiver)?;
            }
            return Ok(());
        }

        for source_parent in &source_parents {
            self.materialize_direct_shape_children(*source_parent)?;
        }
        let mut copied = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for link in self.environment.shape_containments.clone() {
            if !source_parents.contains(&link.parent) || !overwritten_keys.contains(&link.key) {
                continue;
            }
            if copied
                .insert(link.key.clone(), link.child)
                .is_some_and(|previous| previous != link.child)
            {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
        }

        for receiver in receivers {
            for (key, child) in &copied {
                let parent_field = self
                    .environment
                    .shape_states
                    .get(receiver)
                    .and_then(|state| match state {
                        ShapeIdentityState::Proven(parent_type) => {
                            precise_contained_shape_field(parent_type, key)
                        }
                        ShapeIdentityState::Invalidated(_) => None,
                    });
                let child_type = self.shape_identity_type(&BTreeSet::from([*child]))?;
                if parent_field.as_ref() != Some(&child_type) {
                    return Err(UnknownReason::MutableShapeInvalidated);
                }
                self.environment
                    .link_contained_shape(*receiver, key.clone(), *child);
            }
        }
        self.environment.enforce_alias_bound();
        self.shape_identity_type(receivers).map(|_| ())
    }

    pub(in crate::inference::type_tracker) fn materialize_direct_shape_children(
        &mut self,
        parent: ShapeIdentity,
    ) -> Result<(), UnknownReason> {
        let parent_type = match self.environment.shape_states.get(&parent).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: child materialization references absent parent identity {:?}. This is a bug because only allocated shape identities can own child fields. Fix: allocate the parent before materializing containment.",
                parent
            )
        }) {
            ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
            ShapeIdentityState::Invalidated(reason) => return Err(*reason),
        };
        let RubyType::Shape(shape) = parent_type else {
            return Err(UnknownReason::MutableShapeInvalidated);
        };
        let fields = shape
            .fields()
            .iter()
            .filter(|field| field.is_required() && type_is_shape_only(field.value()))
            .map(|field| (field.key().clone(), field.value().clone()))
            .collect::<Vec<_>>();
        for (key, child_type) in fields {
            if self.environment.contained_child(parent, &key).is_some() {
                continue;
            }
            let child = self.allocate_shape_identity(child_type);
            self.environment.link_contained_shape(parent, key, child);
        }
        self.environment.enforce_alias_bound();
        Ok(())
    }

    pub(in crate::inference::type_tracker) fn apply_array_shape_call_boundary(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
    ) {
        let Some(receiver) = call.receiver() else {
            return;
        };
        let Some(local) = receiver.as_local_variable_read_node() else {
            return;
        };
        let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
        let Some(aliases) = self.environment.array_shape_aliases.get(&name).cloned() else {
            return;
        };
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let preserves_positional_proof = match method_name {
            "first" | "last" | "to_a" | "freeze" => argument_nodes.is_empty(),
            "[]" | "at" | "fetch" => {
                argument_nodes.len() == 1 && literal_array_index(&argument_nodes[0]).is_some()
            }
            "length" | "size" | "empty?" | "include?" | "member?" | "index" | "rindex" | "join"
            | "inspect" | "hash" | "eql?" | "==" => true,
            _ => false,
        };
        if preserves_positional_proof {
            return;
        }

        self.environment
            .invalidate_identities(&aliases.contained, UnknownReason::MutableShapeInvalidated);
    }

    pub(in crate::inference::type_tracker) fn shape_identities_in_escape_expression(
        &self,
        node: &Node<'_>,
    ) -> BTreeSet<ShapeIdentity> {
        let direct = self.shape_identities_for_alias_expression(node);
        if !direct.is_empty() {
            return direct;
        }
        if let Some(local) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice());
            if let Some(aliases) = self.environment.array_shape_aliases.get(name.as_ref()) {
                return aliases.contained.clone();
            }
        }
        if let Some(array) = node.as_array_node() {
            return array
                .elements()
                .iter()
                .flat_map(|element| self.shape_identities_in_escape_expression(&element))
                .collect();
        }
        if let Some(hash) = node.as_hash_node() {
            let mut identities = BTreeSet::new();
            for element in hash.elements().iter() {
                if let Some(assoc) = element.as_assoc_node() {
                    identities.extend(self.shape_identities_in_escape_expression(&assoc.key()));
                    identities.extend(self.shape_identities_in_escape_expression(&assoc.value()));
                } else if let Some(splat) = element.as_assoc_splat_node() {
                    if let Some(value) = splat.value() {
                        identities.extend(self.shape_identities_in_escape_expression(&value));
                    }
                }
            }
            return identities;
        }
        BTreeSet::new()
    }

    pub(in crate::inference::type_tracker) fn shape_identity_type(
        &self,
        identities: &BTreeSet<ShapeIdentity>,
    ) -> Result<RubyType, UnknownReason> {
        let mut alternatives = Vec::new();
        for identity in identities {
            match self.environment.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: call receiver references absent shape identity {:?}. This is a bug because receiver bindings and identity states must be cloned and joined together. Fix: merge the complete FlowEnvironment.",
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
        (joined != RubyType::Unknown)
            .then_some(joined)
            .ok_or(UnknownReason::ShapeBoundExceeded)
    }

    pub(in crate::inference::type_tracker) fn transform_shape_identities(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        mut transform: impl FnMut(&RubyType) -> Result<RubyType, UnknownReason>,
    ) -> Result<RubyType, UnknownReason> {
        let mut transitions = BTreeMap::new();
        for identity in identities {
            let current = match self.environment.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape transform references absent identity {:?}. This is a bug because mutations may target only live aliases. Fix: preserve identity states through branch cloning and joins.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            };
            let mut identity_transitions = Vec::new();
            for before in shape_alternatives(&current)? {
                match transform(&before) {
                    Ok(after) if after != RubyType::Unknown => {
                        identity_transitions.push(ShapeAlternativeTransition { before, after });
                    }
                    Ok(_) => {
                        self.environment
                            .invalidate_identities(identities, UnknownReason::ShapeBoundExceeded);
                        return Err(UnknownReason::ShapeBoundExceeded);
                    }
                    Err(reason) => {
                        self.environment.invalidate_identities(identities, reason);
                        return Err(reason);
                    }
                }
            }
            let ruby_type = RubyType::union(
                identity_transitions
                    .iter()
                    .map(|transition| transition.after.clone()),
            );
            if ruby_type == RubyType::Unknown {
                self.environment
                    .invalidate_identities(identities, UnknownReason::ShapeBoundExceeded);
                return Err(UnknownReason::ShapeBoundExceeded);
            }
            self.environment
                .shape_states
                .insert(*identity, ShapeIdentityState::Proven(ruby_type));
            transitions.insert(*identity, identity_transitions);
        }
        self.propagate_contained_shape_updates(transitions)?;
        self.environment.synchronize_shape_aliases();
        self.shape_identity_type(identities)
    }

    pub(in crate::inference::type_tracker) fn propagate_contained_shape_updates(
        &mut self,
        mut changed: BTreeMap<ShapeIdentity, Vec<ShapeAlternativeTransition>>,
    ) -> Result<(), UnknownReason> {
        for _ in 0..crate::core::MAX_SHAPE_SOLVE_ITERATIONS {
            if changed.is_empty() {
                return Ok(());
            }
            let mut links_by_parent = BTreeMap::<ShapeIdentity, Vec<ShapeContainment>>::new();
            for link in &self.environment.shape_containments {
                if changed.contains_key(&link.child) {
                    links_by_parent
                        .entry(link.parent)
                        .or_default()
                        .push(link.clone());
                }
            }
            let mut changed_parents = BTreeMap::new();
            for (parent, links) in links_by_parent {
                let parent_type = match self.environment.shape_states.get(&parent).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: containment propagation references absent parent identity {:?}. This is a bug because an edge and both endpoint states must be cloned and joined atomically. Fix: preserve shape_containments with shape_states.",
                        parent
                    )
                }) {
                    ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
                    ShapeIdentityState::Invalidated(reason) => return Err(*reason),
                };
                let mut parent_transitions = Vec::new();
                for before in shape_alternatives(&parent_type)? {
                    let mut after = before.clone();
                    for link in &links {
                        let child_transitions = changed.get(&link.child).unwrap_or_else(|| {
                            panic!(
                                "INVARIANT VIOLATED: containment worklist lost child transition {:?}. This is a bug because links_by_parent was derived from the same changed map. Fix: keep one immutable worklist generation.",
                                link.child
                            )
                        });
                        after =
                            shape_with_contained_transitions(&after, &link.key, child_transitions)?;
                    }
                    parent_transitions.push(ShapeAlternativeTransition { before, after });
                }
                let updated = RubyType::union(
                    parent_transitions
                        .iter()
                        .map(|transition| transition.after.clone()),
                );
                if updated == RubyType::Unknown {
                    self.environment.invalidate_identities(
                        &BTreeSet::from([parent]),
                        UnknownReason::ShapeBoundExceeded,
                    );
                    return Err(UnknownReason::ShapeBoundExceeded);
                }
                self.environment
                    .shape_states
                    .insert(parent, ShapeIdentityState::Proven(updated));
                changed_parents.insert(parent, parent_transitions);
            }
            changed = changed_parents;
        }

        let remaining = changed.keys().copied().collect::<BTreeSet<_>>();
        self.environment
            .invalidate_identities(&remaining, UnknownReason::ShapeBoundExceeded);
        Err(UnknownReason::ShapeBoundExceeded)
    }

    pub(in crate::inference::type_tracker) fn link_assigned_shape_child(
        &mut self,
        parents: &BTreeSet<ShapeIdentity>,
        key: &LiteralKey,
        children: &BTreeSet<ShapeIdentity>,
        value_type: &RubyType,
    ) -> Result<(), UnknownReason> {
        let child = children
            .iter()
            .copied()
            .next()
            .filter(|_| children.len() == 1)
            .ok_or(UnknownReason::MutableShapeInvalidated)?;
        if parents.contains(&child) || !type_is_shape_only(value_type) {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        let child_type = self.shape_identity_type(children)?;
        if child_type != *value_type {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        for parent in parents {
            let parent_field = match self.environment.shape_states.get(parent).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape field assignment references absent parent identity {:?}. This is a bug because receiver bindings and states must be installed atomically. Fix: preserve both through mutation.",
                    parent
                )
            }) {
                ShapeIdentityState::Proven(parent_type) => {
                    precise_contained_shape_field(parent_type, key)
                }
                ShapeIdentityState::Invalidated(_) => None,
            };
            if parent_field.as_ref() != Some(value_type) {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
        }
        for parent in parents {
            self.environment
                .link_contained_shape(*parent, key.clone(), child);
        }
        self.environment.enforce_alias_bound();
        self.environment.synchronize_shape_aliases();
        Ok(())
    }
}
pub(in crate::inference::type_tracker) fn shape_local_key_path(
    node: &Node<'_>,
) -> Option<(String, Vec<LiteralKey>)> {
    if let Some(parentheses) = node.as_parentheses_node() {
        return parentheses
            .body()
            .and_then(|body| shape_local_key_path(&body));
    }
    let call = node.as_call_node()?;
    let receiver = call.receiver()?;
    let (name, mut path) = if let Some(local) = receiver.as_local_variable_read_node() {
        (
            String::from_utf8_lossy(local.name().as_slice()).to_string(),
            Vec::new(),
        )
    } else {
        shape_local_key_path(&receiver)?
    };
    let arguments = call.arguments()?;
    let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
    match call.name().as_slice() {
        b"[]" | b"fetch" if argument_nodes.len() == 1 => {
            path.push(literal_key(&argument_nodes[0])?);
        }
        b"dig" if !argument_nodes.is_empty() => {
            for argument in argument_nodes {
                path.push(literal_key(&argument)?);
            }
        }
        _ => return None,
    }
    Some((name, path))
}

pub(in crate::inference::type_tracker) fn shape_local_key_read(
    node: &Node<'_>,
) -> Option<(String, LiteralKey)> {
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"[]" {
        return None;
    }
    let receiver = call.receiver()?.as_local_variable_read_node()?;
    let arguments = call.arguments()?;
    let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
    if argument_nodes.len() != 1 {
        return None;
    }
    Some((
        String::from_utf8_lossy(receiver.name().as_slice()).to_string(),
        literal_key(&argument_nodes[0])?,
    ))
}
