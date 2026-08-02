use ruby_fast_lsp::server::RubyLanguageServer;
use std::process::exit;

use anyhow::{anyhow, Result};
use log::{error, info};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() -> Result<()> {
    if run_cache_command_if_requested()? {
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

fn run_cache_command_if_requested() -> Result<bool> {
    let mut arguments = std::env::args().skip(1);
    let Some(command) = arguments.next() else {
        return Ok(false);
    };
    if command != "cache" {
        return Ok(false);
    }
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
    Ok(true)
}
