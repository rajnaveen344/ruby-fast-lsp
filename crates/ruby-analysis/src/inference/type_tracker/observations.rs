//! Variable snapshots and exact local-read evidence consumed by the collector.

use crate::core::{ConstantTypeDependency, RubyType, UnknownReason};
use crate::inference::type_tracker::TypeTracker;
use crate::invariant::ExpectInvariant;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Default)]
pub(in crate::inference::type_tracker) struct TypeObservations {
    /// Variable types at each offset (for queries)
    /// Key = offset where state was recorded, Value = all variables and their types
    pub(in crate::inference::type_tracker) snapshots: BTreeMap<usize, HashMap<String, RubyType>>,
    /// Exact local-read results requested by FactCollector. Appending during
    /// traversal keeps the interactive path allocation-light; extraction
    /// sorts and collapses repeated bounded-loop visits to their final result.
    pub(in crate::inference::type_tracker) local_reads: Vec<LocalReadType>,
    pub(in crate::inference::type_tracker) record_local_reads: bool,
    /// Set on the first branch, rescue, or loop in the current method. Exact
    /// read evidence is only useful inside or after control flow; straight-line
    /// reads are already represented by the scope's assignment facts.
    pub(in crate::inference::type_tracker) has_seen_control_flow: bool,
    /// Proven types of locals passed directly as call arguments, keyed by the
    /// read's start offset. Only reads visited exactly once with the exact
    /// reaching environment are recorded; see `observe_argument_local_reads`.
    pub(in crate::inference::type_tracker) argument_reads: Vec<(usize, RubyType)>,
    /// Depth of bodies that may run more than once or later than their
    /// position (loops and blocks). One forward visit of such a body does not
    /// see every value a captured local can hold there.
    pub(in crate::inference::type_tracker) repeatable_body_depth: usize,
}

/// One exact local-variable read solved by the forward flow tracker.
///
/// Offsets remain parser-native until FactCollector attaches the owning
/// `SourceFileId`. This keeps the reusable inference layer independent of LSP
/// positions and of workspace file registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalReadType {
    pub start_offset: usize,
    pub end_offset: usize,
    pub name: String,
    pub ruby_type: RubyType,
    pub unknown_reason: Option<UnknownReason>,
    pub constant_dependencies: BTreeSet<ConstantTypeDependency>,
}

impl TypeTracker {
    pub(crate) fn max_live_shape_aliases(&self) -> usize {
        self.environment.max_live_shape_aliases
    }

    /// Variable snapshots by offset, for tests that inspect flow state.
    #[cfg(test)]
    pub fn into_var_types(self) -> BTreeMap<usize, HashMap<String, RubyType>> {
        self.observations.snapshots
    }

    pub(crate) fn with_local_read_types(mut self) -> Self {
        self.observations.record_local_reads = true;
        self
    }

    pub(crate) fn take_local_read_types(&mut self) -> Vec<LocalReadType> {
        let mut reads = std::mem::take(&mut self.observations.local_reads);
        reads.sort_by_key(|read| (read.start_offset, read.end_offset));

        let mut deduplicated: Vec<LocalReadType> = Vec::with_capacity(reads.len());
        for read in reads {
            if deduplicated.last().is_some_and(|previous| {
                previous.start_offset == read.start_offset && previous.end_offset == read.end_offset
            }) {
                *deduplicated.last_mut().expect_invariant(
                    "the final local-read entry disappeared after it was checked",
                    "no mutation occurs between the check and replacement",
                    "keep repeated-read collapse atomic",
                ) = read;
            } else {
                deduplicated.push(read);
            }
        }
        deduplicated
    }

    /// Proven argument-local types, sorted by read offset.
    pub(crate) fn take_argument_local_reads(&mut self) -> Vec<(usize, RubyType)> {
        let mut reads = std::mem::take(&mut self.observations.argument_reads);
        reads.sort_by_key(|(offset, _)| *offset);
        reads.dedup_by_key(|(offset, _)| *offset);
        reads
    }

    /// Track a body that may run repeatedly or after later statements.
    pub(in crate::inference::type_tracker) fn track_repeatable_body<T>(
        &mut self,
        track: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.observations.repeatable_body_depth += 1;
        let result = track(self);
        self.observations.repeatable_body_depth -= 1;
        result
    }

    /// Record the type of each local passed directly to `call`.
    ///
    /// The environment is exact here only for a single forward visit, so
    /// reads inside loops and blocks are skipped. A local whose value is
    /// Unknown, carries an Unknown reason, or depends on a constant equation
    /// that resolves later is left unrecorded: no record means no proof.
    /// So is a local whose value came from a method or constant lookup: that
    /// type is a snapshot of other files, and an edit there would leave the
    /// recorded proof stale.
    pub(in crate::inference::type_tracker) fn observe_argument_local_reads(
        &mut self,
        call: &ruby_prism::CallNode,
    ) {
        if !self.observations.record_local_reads || self.observations.repeatable_body_depth > 0 {
            return;
        }
        let Some(arguments) = call.arguments() else {
            return;
        };
        for argument in arguments.arguments().iter() {
            if let Some(keywords) = argument.as_keyword_hash_node() {
                for element in keywords.elements().iter() {
                    if let Some(assoc) = element.as_assoc_node() {
                        self.observe_argument_local_read(&assoc.value());
                    }
                }
            } else {
                self.observe_argument_local_read(&argument);
            }
        }
    }

    fn observe_argument_local_read(&mut self, argument: &ruby_prism::Node<'_>) {
        let Some(read) = argument.as_local_variable_read_node() else {
            return;
        };
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        let Some(ruby_type) = self.environment.types.get(&name) else {
            return;
        };
        if RubyType::contains_unknown(ruby_type)
            || self.environment.unknown_reason(&name).is_some()
            || !self.environment.dependencies(&name).is_empty()
            || !self.environment.is_self_evident(&name)
        {
            return;
        }
        self.observations
            .argument_reads
            .push((read.location().start_offset(), ruby_type.clone()));
    }

    /// Record current variable state at an offset
    pub(in crate::inference::type_tracker) fn record_state(&mut self, offset: usize) {
        // Only record if there are variables to track
        if !self.environment.types.is_empty() {
            self.observations
                .snapshots
                .insert(offset, self.environment.types.clone());
        }
    }
}

/// Read the latest variable snapshot at or before an offset.
#[cfg(test)]
pub fn get_var_type_at(
    var_types: &BTreeMap<usize, HashMap<String, RubyType>>,
    offset: usize,
    var_name: &str,
) -> Option<RubyType> {
    var_types
        .range(..=offset)
        .next_back()
        .and_then(|(_, vars)| vars.get(var_name).cloned())
}

impl TypeTracker {
    /// Whether `value` has a type this file alone decides: a scalar literal,
    /// a constructor call on a constant, a read of a local written from one
    /// of those, or a two-way choice between such values.
    pub(in crate::inference::type_tracker) fn value_is_self_evident(
        &self,
        value: &ruby_prism::Node<'_>,
    ) -> bool {
        if let Some(read) = value.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.environment.is_self_evident(&name);
        }
        if let Some(choice) = value.as_if_node() {
            let otherwise = choice
                .subsequent()
                .and_then(|subsequent| subsequent.as_else_node())
                .and_then(|otherwise| only_statement(otherwise.statements()));
            return match (only_statement(choice.statements()), otherwise) {
                (Some(then), Some(otherwise)) => {
                    self.value_is_self_evident(&then) && self.value_is_self_evident(&otherwise)
                }
                _ => false,
            };
        }
        if let Some(call) = value.as_call_node() {
            return call.name().as_slice() == b"new"
                && call.block().is_none()
                && call.receiver().is_some_and(|receiver| {
                    receiver.as_constant_read_node().is_some()
                        || receiver.as_constant_path_node().is_some()
                });
        }
        value.as_string_node().is_some()
            || value.as_interpolated_string_node().is_some()
            || value.as_symbol_node().is_some()
            || value.as_interpolated_symbol_node().is_some()
            || value.as_integer_node().is_some()
            || value.as_float_node().is_some()
            || value.as_true_node().is_some()
            || value.as_false_node().is_some()
            || value.as_regular_expression_node().is_some()
            || value.as_interpolated_regular_expression_node().is_some()
    }
}

fn only_statement<'pr>(
    statements: Option<ruby_prism::StatementsNode<'pr>>,
) -> Option<ruby_prism::Node<'pr>> {
    let mut body = statements?.body().iter();
    let first = body.next()?;
    body.next().is_none().then_some(first)
}
