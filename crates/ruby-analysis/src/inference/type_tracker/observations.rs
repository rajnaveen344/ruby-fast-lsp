//! Variable snapshots and exact local-read evidence consumed by the collector.

use crate::core::{ConstantTypeDependency, RubyType, UnknownReason};
use crate::inference::type_tracker::TypeTracker;
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

    /// Get variable types map (for storing in RubyDocument)
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
                *deduplicated.last_mut().expect(
                    "INVARIANT VIOLATED: the final local-read entry disappeared after it was checked. This is a bug because no mutation occurs between the check and replacement. Fix: keep repeated-read collapse atomic.",
                ) = read;
            } else {
                deduplicated.push(read);
            }
        }
        deduplicated
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
