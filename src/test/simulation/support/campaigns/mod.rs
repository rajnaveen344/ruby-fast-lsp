//! Explicit scale and read-only corpus acceptance campaigns.
mod corpus;
mod scale;
use std::{path::PathBuf, time::Duration};

pub async fn run_scale_lsp() {
    scale::run_lsp().await;
}
pub fn run_scale_engine() {
    scale::run_engine();
}
pub async fn run_corpus(root: PathBuf) {
    corpus::run(root).await;
}

fn assert_elapsed_under_env_budget(
    env_name: &str,
    default_budget: Duration,
    elapsed: Duration,
    label: &str,
) {
    let budget = std::env::var(env_name)
        .ok()
        .map(|value| {
            let millis = value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: {} `{}` is not a u64 millisecond budget: {}. \
                     This is a bug because perf budgets must be numeric. \
                     Fix: pass {}=<milliseconds>.",
                    env_name, value, err, env_name
                )
            });
            Duration::from_millis(millis)
        })
        .unwrap_or(default_budget);

    assert!(
        elapsed <= budget,
        "{} exceeded perf budget {:?}: elapsed {:?}. Override with {}=<milliseconds> if this machine is intentionally slower.",
        label,
        budget,
        elapsed,
        env_name
    );
}
