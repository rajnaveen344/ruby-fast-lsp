use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencySummary {
    pub samples: usize,
    pub min: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub max: Duration,
}

impl LatencySummary {
    pub fn from_samples(samples: &[Duration]) -> Self {
        assert!(
            !samples.is_empty(),
            "INVARIANT VIOLATED: latency samples are empty. This is a bug because a percentile cannot be computed without observations. Fix: record at least one measured operation before building a summary."
        );

        let mut sorted = samples.to_vec();
        sorted.sort_unstable();

        Self {
            samples: sorted.len(),
            min: sorted[0],
            p50: nearest_rank(&sorted, 50),
            p95: nearest_rank(&sorted, 95),
            max: sorted[sorted.len() - 1],
        }
    }
}

fn nearest_rank(sorted: &[Duration], percentile: usize) -> Duration {
    assert!(
        !sorted.is_empty(),
        "INVARIANT VIOLATED: sorted latency samples are empty. This is a bug because nearest-rank selection requires observations. Fix: validate samples before selecting a percentile."
    );
    assert!(
        (1..=100).contains(&percentile),
        "INVARIANT VIOLATED: percentile {percentile} is outside 1..=100. This is a bug because nearest-rank percentiles are defined only in that range. Fix: request a percentile from 1 through 100."
    );

    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted[rank - 1]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionBudget {
    pub cold_indexing: Duration,
    pub edit: Duration,
    pub completion: Duration,
    pub hover: Duration,
    pub definition: Duration,
    pub references: Duration,
    pub diagnostics: Duration,
    pub engine_heap_bytes: usize,
}

impl Default for ProductionBudget {
    fn default() -> Self {
        Self {
            cold_indexing: Duration::from_secs(2),
            edit: Duration::from_millis(100),
            completion: Duration::from_millis(50),
            hover: Duration::from_millis(25),
            definition: Duration::from_millis(25),
            references: Duration::from_millis(50),
            diagnostics: Duration::from_millis(25),
            engine_heap_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionMeasurements {
    pub cold_indexing: Duration,
    pub edit: LatencySummary,
    pub completion: LatencySummary,
    pub hover: LatencySummary,
    pub definition: LatencySummary,
    pub references: LatencySummary,
    pub diagnostics: LatencySummary,
    pub engine_heap_bytes: usize,
}

impl ProductionBudget {
    pub fn exceeded_by(&self, measurements: &ProductionMeasurements) -> Vec<&'static str> {
        let mut exceeded = Vec::new();
        if measurements.cold_indexing > self.cold_indexing {
            exceeded.push("cold_indexing");
        }
        if measurements.edit.p95 > self.edit {
            exceeded.push("edit");
        }
        if measurements.completion.p95 > self.completion {
            exceeded.push("completion");
        }
        if measurements.hover.p95 > self.hover {
            exceeded.push("hover");
        }
        if measurements.definition.p95 > self.definition {
            exceeded.push("definition");
        }
        if measurements.references.p95 > self.references {
            exceeded.push("references");
        }
        if measurements.diagnostics.p95 > self.diagnostics {
            exceeded.push("diagnostics");
        }
        if measurements.engine_heap_bytes > self.engine_heap_bytes {
            exceeded.push("engine_heap");
        }
        exceeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_summary_uses_nearest_rank_percentiles() {
        let samples = (1..=100).map(Duration::from_millis).collect::<Vec<_>>();

        let summary = LatencySummary::from_samples(&samples);

        assert_eq!(summary.samples, 100);
        assert_eq!(summary.min, Duration::from_millis(1));
        assert_eq!(summary.p50, Duration::from_millis(50));
        assert_eq!(summary.p95, Duration::from_millis(95));
        assert_eq!(summary.max, Duration::from_millis(100));
    }

    #[test]
    #[should_panic(expected = "latency samples are empty")]
    fn latency_summary_rejects_empty_samples() {
        LatencySummary::from_samples(&[]);
    }

    #[test]
    fn production_budget_reports_only_exceeded_measurements() {
        let budget = ProductionBudget::default();
        let measurements = ProductionMeasurements {
            cold_indexing: budget.cold_indexing,
            edit: summary_with_p95(budget.edit + Duration::from_millis(1)),
            completion: summary_with_p95(budget.completion),
            hover: summary_with_p95(budget.hover),
            definition: summary_with_p95(budget.definition),
            references: summary_with_p95(budget.references),
            diagnostics: summary_with_p95(budget.diagnostics),
            engine_heap_bytes: budget.engine_heap_bytes + 1,
        };

        assert_eq!(
            budget.exceeded_by(&measurements),
            vec!["edit", "engine_heap"]
        );
    }

    fn summary_with_p95(p95: Duration) -> LatencySummary {
        LatencySummary {
            samples: 1,
            min: p95,
            p50: p95,
            p95,
            max: p95,
        }
    }
}
