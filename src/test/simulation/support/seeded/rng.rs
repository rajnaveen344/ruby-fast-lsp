pub(super) struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub(super) fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    pub(super) fn range_usize(&mut self, upper: usize) -> usize {
        assert!(
            upper > 0,
            "INVARIANT VIOLATED: seeded RNG upper bound is zero. This is a bug because random ranges must have at least one value. Fix: pass a positive upper bound."
        );
        (self.next_u64() as usize) % upper
    }

    pub(super) fn shuffle<T>(&mut self, values: &mut [T]) {
        for idx in (1..values.len()).rev() {
            let swap_idx = self.range_usize(idx + 1);
            values.swap(idx, swap_idx);
        }
    }
}
