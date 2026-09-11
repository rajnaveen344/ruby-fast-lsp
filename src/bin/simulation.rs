//! Opt-in acceptance campaigns; ordinary regressions remain in `cargo test`.
use clap::{Parser, Subcommand};
use ruby_fast_lsp::simulation::campaigns;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Run explicit Ruby Fast LSP simulation acceptance campaigns")]
struct Args {
    #[command(subcommand)]
    campaign: Campaign,
}

#[derive(Subcommand)]
enum Campaign {
    /// Check sampled LSP features on a generated large project.
    ScaleLsp,
    /// Check every modeled definition and reference on a generated large project.
    ScaleEngine,
    /// Check a deliberately selected, read-only Ruby corpus.
    Corpus {
        #[arg(long)]
        root: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let name = match Args::parse().campaign {
        Campaign::ScaleLsp => {
            campaigns::run_scale_lsp().await;
            "scale-lsp"
        }
        Campaign::ScaleEngine => {
            campaigns::run_scale_engine();
            "scale-engine"
        }
        Campaign::Corpus { root } => {
            campaigns::run_corpus(root).await;
            "corpus"
        }
    };
    println!(
        "{}",
        serde_json::json!({"simulation_campaign": name, "status": "passed"})
    );
}
