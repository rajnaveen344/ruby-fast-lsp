use super::rng::SeededRng;
use super::{FIXED_SEEDS, REGRESSION_SEEDS_TEXT};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn simulation_seeds_from_env() -> Vec<u64> {
    if let Ok(value) = std::env::var("SIM_SEED") {
        return vec![parse_seed("SIM_SEED", &value)];
    }

    if let Ok(value) = std::env::var("SIM_SEEDS") {
        let seeds = value
            .split(',')
            .map(str::trim)
            .filter(|seed| !seed.is_empty())
            .map(|seed| parse_seed("SIM_SEEDS", seed))
            .collect::<Vec<_>>();
        assert!(
            !seeds.is_empty(),
            "INVARIANT VIOLATED: SIM_SEEDS was set but contained no seeds. This is a bug because seeded replay needs at least one numeric seed. Fix: pass SIM_SEEDS=1,42."
        );
        return seeds;
    }

    let random_count = std::env::var("SIM_RANDOM_SEEDS")
        .ok()
        .map(|value| parse_seed_count(&value))
        .unwrap_or(0);
    let mut seeds = FIXED_SEEDS.to_vec();
    seeds.extend(regression_seeds());
    seeds.extend(random_seeds(random_count));
    seeds.sort_unstable();
    seeds.dedup();
    seeds
}

fn parse_seed(env_name: &str, value: &str) -> u64 {
    value.parse::<u64>().unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: {} `{}` is not a u64: {}. This is a bug because seeded simulation replay needs numeric seeds. Fix: pass {}=<u64>.",
            env_name, value, err, env_name
        )
    })
}

fn regression_seeds() -> Vec<u64> {
    REGRESSION_SEEDS_TEXT
        .lines()
        .enumerate()
        .filter_map(|(line_idx, line)| {
            let seed = line.split('#').next().unwrap_or("").trim();
            if seed.is_empty() {
                None
            } else {
                Some(parse_seed(
                    &format!(
                        "src/test/simulation/support/regression_seeds.txt:{}",
                        line_idx + 1
                    ),
                    seed,
                ))
            }
        })
        .collect()
}

fn parse_seed_count(value: &str) -> usize {
    let count = value.parse::<usize>().unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: SIM_RANDOM_SEEDS `{}` is not a usize: {}. This is a bug because random simulation needs a numeric count. Fix: pass SIM_RANDOM_SEEDS=<count>.",
            value, err
        )
    });
    assert!(
        count <= 100,
        "INVARIANT VIOLATED: SIM_RANDOM_SEEDS `{}` is too large. This is a bug because unit tests must stay bounded. Fix: run a smaller count or move soak testing to a dedicated command.",
        count
    );
    count
}

fn random_seeds(count: usize) -> Vec<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: system clock is before UNIX_EPOCH: {}. This is a bug because random simulation seeds need monotonic-ish entropy. Fix: check system clock.",
                err
            )
        });
    let mut rng = SeededRng::new(now.as_nanos() as u64 ^ now.as_secs().rotate_left(17));
    (0..count).map(|_| rng.next_u64()).collect()
}
