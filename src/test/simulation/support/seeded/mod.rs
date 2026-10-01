//! Seeded generation of replayable simulation projects and edit scripts.

mod artifacts;
mod large_scale;
mod names;
mod rng;
mod scenario;
mod script;
mod seed_selection;

pub use artifacts::write_seed_artifact;
pub use large_scale::large_scale_project;
#[cfg(test)]
pub(crate) use scenario::add_module_dispatch_scenario;
pub use scenario::seeded_project;
pub use script::seeded_script;
pub use seed_selection::simulation_seeds_from_env;

use super::project::{self, SyntheticProject};
use super::OracleState;

const SIM_GENERATOR_VERSION: u32 = 41;
const FIXED_SEEDS: &[u64] = &[1, 42, 20_260_524];
const REGRESSION_SEEDS_TEXT: &str = include_str!("../regression_seeds.txt");
pub const LARGE_SCALE_RUBY_FILES: usize = 2_284;
pub const LARGE_SCALE_METHOD_DEFS: usize = 23_328;
pub const LARGE_SCALE_MIN_GRAPH_EDGES: usize = 18_000;

#[derive(Debug, Clone)]
pub struct SeededScript {
    pub seed: u64,
    pub project: SyntheticProject,
    pub initial_open_files: Vec<String>,
    pub steps: Vec<SeededStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeededStep {
    CheckDefinitions,
    CheckReferences,
    CheckHover,
    CheckTypes,
    ApplyEdit { index: usize },
    CloseReopen { file: String },
    OpenFile { file: String },
    CloseFile { file: String },
}

#[derive(Debug, Clone)]
struct SeededNames {
    trackable: String,
    trackable_hook_module: String,
    trackable_hook_include_module: String,
    trackable_hook_class_eval_module: String,
    trackable_concern_class_methods_module: String,
    visibility_hidden_mixin: String,
    visibility_hidden_user: String,
    visibility_public_mixin: String,
    visibility_public_user: String,
    gateway: String,
    fallback_gateway: String,
    base_invoice: String,
    account: String,
    invoice: String,
    sku: String,
    item: String,
    dynamic_record: String,
    summary: String,
    invoice_file: String,
    account_reopen_file: String,
    level_constant: String,
    level_constant_fqn: String,
    provider_constant: String,
    provider_constant_fqn: String,
    base_status_constant: String,
    base_status_constant_fqn: String,
    currency_constant: String,
    currency_constant_fqn: String,
    prefix_constant: String,
    prefix_constant_fqn: String,
    audit_method: String,
    hook_status_method: String,
    hook_render_method: String,
    hook_api_method: String,
    concern_lookup_method: String,
    visibility_hidden_method: String,
    visibility_public_method: String,
    record_method: String,
    tagged_method: String,
    capture_method: String,
    refund_method: String,
    const_get_method: String,
    default_method: String,
    void_method: String,
    private_method: String,
    private_probe_method: String,
    provider_method: String,
    queue_method: String,
    normalize_method: String,
    super_method: String,
    base_status_method: String,
    gateway_method: String,
    backup_gateway_method: String,
    account_reopen_method: String,
    charge_method: String,
    delegated_capture_method: String,
    block_scoped_method: String,
    chain_charge_method: String,
    constructor_charge_method: String,
    format_method: String,
    audit_sku_method: String,
    sku_method: String,
    publish_method: String,
    render_method: String,
    capture_total_method: String,
    dynamic_virtual_method: String,
    gateway_local: String,
    gateway_ivar: String,
    item_local: String,
    account_local: String,
    block_item_local: String,
    yield_item_local: String,
    visibility_hidden_local: String,
    visibility_public_local: String,
    account_reopen_local: String,
}
