//! One registry shape for observational statistics.
//!
//! A statistics source declares its named values once with [`stat_set!`]. It
//! records them either into a [`StatsSnapshot`] it owns exclusively (for
//! example one engine resolve pass) or into a shared [`StatsRegistry`] when
//! several threads record at once (caches, extension hosts, call hosts). Every
//! consumer (profiler reports, budgets, logs, protocol telemetry, and tests)
//! reads the same detached [`StatsSnapshot`] type, merges snapshots by each
//! statistic's declared [`Merge`] rule, and serializes it as a `name -> u64`
//! map.
//!
//! Statistics are observational: they never participate in semantic results,
//! fingerprints, or admission decisions.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// How one statistic combines when snapshots are merged or recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Merge {
    /// Counts, byte totals, and accumulated durations add.
    Sum,
    /// High-water marks keep the larger value.
    Max,
}

/// A closed set of named statistics declared by [`stat_set!`].
pub trait Stat: Copy + Eq + fmt::Debug + 'static {
    /// Every statistic in declaration order; `ALL[s.index()] == s`.
    const ALL: &'static [Self];
    /// Stable report name used by serialization and logs.
    fn name(self) -> &'static str;
    fn merge(self) -> Merge;
    fn index(self) -> usize;
}

/// Declare a statistics set: a fieldless enum whose variants carry a stable
/// report name and an optional `max` merge rule (default: sum).
///
/// ```
/// use ruby_analysis::stat_set;
/// use ruby_analysis::stats::{Merge, Stat};
///
/// stat_set! {
///     pub enum CacheStat {
///         Hits = "hits",
///         MaxWaitNs = "max_wait_ns": max,
///     }
/// }
///
/// assert_eq!(CacheStat::ALL, &[CacheStat::Hits, CacheStat::MaxWaitNs]);
/// assert_eq!(CacheStat::MaxWaitNs.name(), "max_wait_ns");
/// assert_eq!(CacheStat::Hits.merge(), Merge::Sum);
/// assert_eq!(CacheStat::MaxWaitNs.merge(), Merge::Max);
/// ```
#[macro_export]
macro_rules! stat_set {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $label:literal $(: $merge:ident)?),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        $vis enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl $crate::stats::Stat for $name {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            fn merge(self) -> $crate::stats::Merge {
                match self {
                    $(Self::$variant => $crate::stat_set!(@merge $($merge)?)),+
                }
            }

            fn index(self) -> usize {
                self as usize
            }
        }
    };
    (@merge) => { $crate::stats::Merge::Sum };
    (@merge max) => { $crate::stats::Merge::Max };
}

/// Convert a measured duration to nanoseconds.
pub fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or_else(|_| {
        unreachable_invariant!(
            what = "a measured duration of {:?} overflowed u64 nanoseconds",
            why = "one in-process measurement cannot exceed u64::MAX nanoseconds",
            fix = "inspect a hung or corrupted timer before recording it",
            duration,
        )
    })
}

/// Convert an in-memory count to a statistic value.
pub fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or_else(|_| {
        unreachable_invariant!(
            what = "an in-memory count of {} does not fit u64",
            why = "usize is at most 64 bits on every supported target",
            fix = "widen statistic values before supporting a wider target",
            value,
        )
    })
}

/// Detached values for one statistics set, indexed by [`Stat::index`].
#[derive(Clone, PartialEq, Eq)]
pub struct StatsSnapshot<S: Stat> {
    values: Box<[u64]>,
    stats: PhantomData<S>,
}

impl<S: Stat> Default for StatsSnapshot<S> {
    fn default() -> Self {
        Self {
            values: vec![0; S::ALL.len()].into_boxed_slice(),
            stats: PhantomData,
        }
    }
}

impl<S: Stat> StatsSnapshot<S> {
    pub fn get(&self, stat: S) -> u64 {
        self.values[stat.index()]
    }

    /// Overwrite one value, for gauges observed at snapshot time.
    pub fn set(&mut self, stat: S, value: u64) {
        self.values[stat.index()] = value;
    }

    /// Record one observation by the statistic's merge rule.
    pub fn record(&mut self, stat: S, value: u64) {
        let slot = &mut self.values[stat.index()];
        *slot = match stat.merge() {
            Merge::Sum => slot.saturating_add(value),
            Merge::Max => (*slot).max(value),
        };
    }

    pub fn increment(&mut self, stat: S) {
        self.record(stat, 1);
    }

    pub fn record_duration(&mut self, stat: S, duration: Duration) {
        self.record(stat, duration_ns(duration));
    }

    /// Combine another snapshot of the same set into this one.
    pub fn merge(&mut self, other: &Self) {
        for &stat in S::ALL {
            self.record(stat, other.get(stat));
        }
    }

    /// Saturating sum of every value, for sets whose values share one unit.
    pub fn total(&self) -> u64 {
        self.values
            .iter()
            .fold(0u64, |total, value| total.saturating_add(*value))
    }

    /// Every statistic with its name and value, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (S, &'static str, u64)> + '_ {
        S::ALL
            .iter()
            .map(|&stat| (stat, stat.name(), self.get(stat)))
    }
}

impl<S: Stat> fmt::Debug for StatsSnapshot<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.iter().map(|(_, name, value)| (name, value)))
            .finish()
    }
}

impl<S: Stat> Serialize for StatsSnapshot<S> {
    fn serialize<Z: Serializer>(&self, serializer: Z) -> Result<Z::Ok, Z::Error> {
        let mut map = serializer.serialize_map(Some(S::ALL.len()))?;
        for (_, name, value) in self.iter() {
            map.serialize_entry(name, &value)?;
        }
        map.end()
    }
}

impl<'de, S: Stat> Deserialize<'de> for StatsSnapshot<S> {
    /// Missing names read as zero and unknown names are ignored, so readers
    /// accept reports from servers that add or retire statistics.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SnapshotVisitor<S>(PhantomData<S>);

        impl<'de, S: Stat> Visitor<'de> for SnapshotVisitor<S> {
            type Value = StatsSnapshot<S>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of statistic names to unsigned integers")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut snapshot = StatsSnapshot::default();
                while let Some(name) = map.next_key::<String>()? {
                    match S::ALL.iter().find(|stat| stat.name() == name) {
                        Some(&stat) => snapshot.set(stat, map.next_value()?),
                        None => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(snapshot)
            }
        }

        deserializer.deserialize_map(SnapshotVisitor(PhantomData))
    }
}

/// Lock-free recorder for one statistics set shared across threads.
pub struct StatsRegistry<S: Stat> {
    values: Box<[AtomicU64]>,
    stats: PhantomData<S>,
}

impl<S: Stat> Default for StatsRegistry<S> {
    fn default() -> Self {
        Self {
            values: S::ALL.iter().map(|_| AtomicU64::new(0)).collect(),
            stats: PhantomData,
        }
    }
}

impl<S: Stat> fmt::Debug for StatsRegistry<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.snapshot().fmt(formatter)
    }
}

impl<S: Stat> StatsRegistry<S> {
    /// Record one observation by the statistic's merge rule. Sums saturate at
    /// `u64::MAX` instead of wrapping.
    pub fn record(&self, stat: S, value: u64) {
        let slot = &self.values[stat.index()];
        match stat.merge() {
            Merge::Sum => {
                let previous = slot.fetch_add(value, Ordering::Relaxed);
                if previous.checked_add(value).is_none() {
                    slot.store(u64::MAX, Ordering::Relaxed);
                }
            }
            Merge::Max => {
                slot.fetch_max(value, Ordering::Relaxed);
            }
        }
    }

    pub fn increment(&self, stat: S) {
        self.record(stat, 1);
    }

    pub fn record_duration(&self, stat: S, duration: Duration) {
        self.record(stat, duration_ns(duration));
    }

    pub fn get(&self, stat: S) -> u64 {
        self.values[stat.index()].load(Ordering::Relaxed)
    }

    /// Read every value. Each value is read independently; concurrent
    /// recording may land between reads.
    pub fn snapshot(&self) -> StatsSnapshot<S> {
        let mut snapshot = StatsSnapshot::default();
        for &stat in S::ALL {
            snapshot.set(stat, self.get(stat));
        }
        snapshot
    }

    pub fn reset(&self) {
        for value in self.values.iter() {
            value.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Merge, Stat, StatsRegistry, StatsSnapshot};
    use std::time::Duration;

    crate::stat_set! {
        enum Example {
            Hits = "hits",
            WaitNs = "wait_ns",
            MaxWaitNs = "max_wait_ns": max,
        }
    }

    #[test]
    fn declaration_order_matches_indices() {
        for (index, stat) in Example::ALL.iter().enumerate() {
            assert_eq!(stat.index(), index);
        }
        assert_eq!(Example::MaxWaitNs.merge(), Merge::Max);
        assert_eq!(Example::Hits.merge(), Merge::Sum);
    }

    #[test]
    fn registry_records_by_merge_rule_and_saturates() {
        let registry = StatsRegistry::<Example>::default();
        registry.increment(Example::Hits);
        registry.increment(Example::Hits);
        registry.record_duration(Example::WaitNs, Duration::from_nanos(7));
        registry.record_duration(Example::MaxWaitNs, Duration::from_nanos(7));
        registry.record_duration(Example::MaxWaitNs, Duration::from_nanos(3));
        let snapshot = registry.snapshot();
        assert_eq!(snapshot.get(Example::Hits), 2);
        assert_eq!(snapshot.get(Example::WaitNs), 7);
        assert_eq!(snapshot.get(Example::MaxWaitNs), 7);

        registry.record(Example::Hits, u64::MAX);
        assert_eq!(registry.get(Example::Hits), u64::MAX);
        registry.reset();
        assert_eq!(registry.snapshot(), StatsSnapshot::default());
    }

    #[test]
    fn snapshots_merge_and_round_trip_as_named_maps() {
        let mut left = StatsSnapshot::<Example>::default();
        left.record(Example::Hits, 2);
        left.record(Example::MaxWaitNs, 9);
        let mut right = StatsSnapshot::<Example>::default();
        right.record(Example::Hits, 3);
        right.record(Example::MaxWaitNs, 4);
        left.merge(&right);
        assert_eq!(left.get(Example::Hits), 5);
        assert_eq!(left.get(Example::MaxWaitNs), 9);

        let json = serde_json::to_value(&left).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"hits": 5, "wait_ns": 0, "max_wait_ns": 9})
        );
        let decoded: StatsSnapshot<Example> =
            serde_json::from_value(serde_json::json!({"hits": 5, "max_wait_ns": 9, "retired": 1}))
                .unwrap();
        assert_eq!(decoded, left);
    }
}
