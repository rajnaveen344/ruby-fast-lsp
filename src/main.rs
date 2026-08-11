use ruby_fast_lsp::check::{render_report, CheckOutputFormat, CheckSession};
use ruby_fast_lsp::server::RubyLanguageServer;
use std::path::PathBuf;
use std::process::exit;

use anyhow::{anyhow, Result};
use log::{error, info};
use tower_lsp::{LspService, Server};

#[cfg(all(feature = "jemalloc", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL_ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> Result<()> {
    if let Some(exit_code) = run_cli_command_if_requested().await? {
        if exit_code != 0 {
            exit(exit_code);
        }
        return Ok(());
    }

    // Initialize the logger.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .format(|buf, record| {
            use std::io::Write;
            let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
            writeln!(
                buf,
                "[{} {} {}] {}",
                ts,
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    // Start with info level - can be changed at runtime via configuration
    log::set_max_level(log::LevelFilter::Info);

    info!("Starting Ruby Fast LSP server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| {
        RubyLanguageServer::new(client).unwrap_or_else(|e| {
            error!("Failed to initialize Ruby LSP server: {}", e);
            exit(1)
        })
    })
    .custom_method(
        "ruby/namespaceTree",
        RubyLanguageServer::handle_namespace_tree_request,
    )
    // Debug commands for custom LSP clients
    .custom_method("$/listCommands", RubyLanguageServer::handle_list_commands)
    .custom_method(
        "ruby-fast-lsp/debug/lookup",
        RubyLanguageServer::handle_debug_lookup,
    )
    .custom_method(
        "ruby-fast-lsp/debug/stats",
        RubyLanguageServer::handle_debug_stats,
    )
    .custom_method(
        "ruby-fast-lsp/debug/ancestors",
        RubyLanguageServer::handle_debug_ancestors,
    )
    .custom_method(
        "ruby-fast-lsp/debug/methods",
        RubyLanguageServer::handle_debug_methods,
    )
    .custom_method(
        "ruby-fast-lsp/debug/inference-stats",
        RubyLanguageServer::handle_debug_inference_stats,
    )
    .custom_method("ruby/exportGraph", RubyLanguageServer::handle_export_graph)
    .custom_method(
        "ruby-fast-lsp/extensions/status",
        RubyLanguageServer::handle_extension_status,
    )
    .custom_method(
        "ruby-fast-lsp/runtime/discover",
        RubyLanguageServer::handle_runtime_discover,
    )
    .custom_method(
        "ruby-fast-lsp/runtime/status",
        RubyLanguageServer::handle_runtime_status,
    )
    .custom_method(
        "ruby-fast-lsp/indexing/status",
        RubyLanguageServer::handle_indexing_status,
    )
    .finish();

    info!("Ruby LSP server initialized, waiting for client connections");

    Server::new(stdin, stdout, socket)
        // tower-lsp defaults to 4 concurrent request handlers. Completed handlers
        // that are waiting on a backpressured stdout response slot still occupy
        // those slots, so a small limit turns client IO stalls into multi-second
        // goto/hover delays. Keep CPU-heavy work off the reactor; this only
        // bounds how many LSP futures may be in flight.
        .concurrency_level(64)
        .serve(service)
        .await;

    Ok(())
}

async fn run_cli_command_if_requested() -> Result<Option<i32>> {
    let mut arguments = std::env::args().skip(1);
    let Some(command) = arguments.next() else {
        return Ok(None);
    };
    match command.as_str() {
        "cache" => run_cache_command(arguments).map(|()| Some(0)),
        "check" => run_check_command(arguments).await.map(Some),
        "--stdio" => {
            if let Some(unexpected) = arguments.next() {
                return Err(anyhow!(
                    "LSP --stdio mode received unexpected argument `{unexpected}`"
                ));
            }
            Ok(None)
        }
        _ => Err(anyhow!(
            "unknown command `{command}`; expected `check`, `cache`, `--stdio`, or no command for LSP mode"
        )),
    }
}

fn run_cache_command(mut arguments: impl Iterator<Item = String>) -> Result<()> {
    let operation = arguments
        .next()
        .ok_or_else(|| anyhow!("cache command requires `show` or `clear`"))?;
    if let Some(unexpected) = arguments.next() {
        return Err(anyhow!(
            "cache command received unexpected argument `{unexpected}`"
        ));
    }
    let cache = ruby_fast_lsp::persistent_cache::PersistentDerivedProductCache::new(
        ruby_fast_lsp::utils::ruby_fast_lsp_user_cache_root()?,
    );
    let (action, summary) = match operation.as_str() {
        "show" => ("show", cache.summary()?),
        "clear" => ("clear", cache.clear()?),
        "cache" | "show-cache" | "clear-cache" => {
            return Err(anyhow!(
                "unknown cache operation `{operation}`; expected `show` or `clear`"
            ));
        }
        _ => {
            return Err(anyhow!(
                "unknown cache operation `{operation}`; expected `show` or `clear`"
            ));
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "action": action,
            "root": summary.root,
            "entries": summary.entries,
            "bytes": summary.bytes,
        }))?
    );
    Ok(())
}

async fn run_check_command(mut arguments: impl Iterator<Item = String>) -> Result<i32> {
    let mut path = None;
    let mut format = CheckOutputFormat::Human;

    while let Some(argument) = arguments.next() {
        if argument == "--format" {
            let value = arguments
                .next()
                .ok_or_else(|| anyhow!("check --format requires `human` or `json`"))?;
            format = CheckOutputFormat::parse(&value)?;
            continue;
        }
        if let Some(value) = argument.strip_prefix("--format=") {
            format = CheckOutputFormat::parse(value)?;
            continue;
        }
        if argument.starts_with('-') {
            return Err(anyhow!(
                "unknown check option `{argument}`; supported option: --format human|json"
            ));
        }
        if let Some(previous) = path.replace(PathBuf::from(&argument)) {
            return Err(anyhow!(
                "check accepts one file or project path; received both `{}` and `{argument}`",
                previous.display()
            ));
        }
    }

    let path = path.unwrap_or(std::env::current_dir()?);
    let report = CheckSession::default().check_path(&path).await?;
    println!("{}", render_report(&report, format)?);
    Ok(i32::from(report.has_failures()))
}
