use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "extension")]
#[command(about = "Validate Ruby Fast LSP extension packages")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Validate {
        package: PathBuf,
    },
    Smoke {
        package: PathBuf,
    },
    /// Print the `[build] source_sha256` of a prebuilt mruby package.
    Sources {
        package: PathBuf,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let cli = Cli::parse();
    let package = match cli.command {
        Command::Validate { package } | Command::Smoke { package } => package,
        Command::Sources { package } => {
            let root = devtools::workspace_root();
            match devtools::package_sources::mruby_package_source_sha256(&root, &root.join(package))
            {
                Ok(sha256) => println!("{sha256}"),
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(1);
                }
            }
            return;
        }
    };

    match ruby_fast_lsp::environment::extensions::validate_extension_package(&package) {
        Ok(report) => {
            println!(
                "ok id={} status={} calls=[{}]",
                report.id,
                report.status,
                report.indexed_call_names.join(",")
            );
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
