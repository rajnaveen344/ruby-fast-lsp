//! Configuration changes.

use super::watched_files::rebuild_runtime_owned_project_state;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::version::parse_ruby_family;
use crate::server::Server;
use log::{info, warn};
use tower_lsp::lsp_types::*;

pub async fn handle_did_change_configuration(
    server: &Server,
    params: DidChangeConfigurationParams,
) {
    info!("Configuration change received");

    if let Some(settings) = params.settings.as_object() {
        if let Some(ruby_fast_lsp_settings) = settings.get("rubyFastLsp") {
            if let Ok(mut config) =
                serde_json::from_value::<RubyFastLspConfig>(ruby_fast_lsp_settings.clone())
            {
                if let Err(error) = config.validate_runtime_configuration() {
                    warn!("Rejected invalid runtime configuration update: {error}");
                    return;
                }
                let previous_config = server.configuration_snapshot();
                preserve_initialization_only_config(
                    &mut config,
                    &previous_config,
                    ruby_fast_lsp_settings,
                );
                let runtime_changed_workspaces = server
                    .list_workspaces()
                    .into_iter()
                    .filter(|workspace| {
                        let root = workspace.root_path.to_string_lossy();
                        previous_config
                            .runtime
                            .selection_for_project(&root, &previous_config.ruby_version)
                            != config
                                .runtime
                                .selection_for_project(&root, &config.ruby_version)
                            || previous_config.jruby.project_config(&root)
                                != config.jruby.project_config(&root)
                    })
                    .collect::<Vec<_>>();
                info!("Updated configuration: {:?}", config);

                // Apply log level immediately (works without restart)
                config.apply_log_level();
                if let Err(error) = server.reconfigure_extensions(&config).await {
                    warn!("Extension settings reconfiguration worker failed: {error:#}");
                    return;
                }
                server.refresh_extension_watch_registration().await;

                server.replace_configuration(config.clone());

                if let Some(version) = parse_ruby_family(&config.ruby_version) {
                    info!("Using configured Ruby compatibility version: {version:?}");
                } else {
                    info!(
                        "Ruby runtime and compatibility will be resolved independently per project"
                    );
                }

                for workspace in runtime_changed_workspaces {
                    rebuild_runtime_owned_project_state(server, workspace).await;
                }
            } else {
                warn!("Failed to parse configuration from settings");
            }
        }
    }
}

fn preserve_initialization_only_config(
    config: &mut RubyFastLspConfig,
    current: &RubyFastLspConfig,
    settings: &serde_json::Value,
) {
    let Some(settings) = settings.as_object() else {
        return;
    };

    if !settings.contains_key("extensionPath") {
        config.extension_path = current.extension_path.clone();
    }
    if !settings.contains_key("extensionPackages") {
        config.extension_packages = current.extension_packages.clone();
    }
    if !settings.contains_key("extensionDirs") {
        config.extension_dirs = current.extension_dirs.clone();
    }
    if !settings.contains_key("extensionSettings") {
        config.extension_settings = current.extension_settings.clone();
    }
    if !settings.contains_key("workspaceTrusted") {
        config.workspace_trusted = current.workspace_trusted;
    }
    if !settings.contains_key("projectExtensionsEnabled") {
        config.project_extensions_enabled = current.project_extensions_enabled;
    }
}
