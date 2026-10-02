//! Configuration changes and extension watch registration.

use super::watched_files::rebuild_runtime_owned_project_state;
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::version::parse_ruby_family;
use crate::invariant::ExpectInvariant;
use crate::server::RubyLanguageServer;
use log::{info, warn};
use std::sync::atomic::Ordering;
use tower_lsp::lsp_types::*;

pub async fn handle_did_change_configuration(
    server: &RubyLanguageServer,
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
                let previous_config = server.config.lock().clone();
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
                if let Err(error) = server
                    .extensions
                    .registry()
                    .configure_from_config_and_workspace_roots_governed(
                        &config,
                        &server.workspace_root_paths(),
                        server.indexing.resources().clone(),
                    )
                    .await
                {
                    warn!("Extension settings reconfiguration worker failed: {error:#}");
                    return;
                }
                refresh_extension_watch_registration(server).await;

                *server.config.lock() = config.clone();

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

pub(super) async fn refresh_extension_watch_registration(server: &RubyLanguageServer) {
    if !server
        .extensions
        .dynamic_registration()
        .load(Ordering::Acquire)
    {
        return;
    }
    let Some(client) = &server.client else {
        return;
    };

    let desired = server.extensions.registry().watcher_globs();
    let mut current = server.extensions.registration().lock().await;
    if *current == desired {
        return;
    }

    if !current.is_empty() {
        let unregistration = Unregistration {
            id: "ruby-fast-lsp-extension-watchers".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
        };
        if let Err(err) = client.unregister_capability(vec![unregistration]).await {
            warn!("Failed to unregister extension file watchers: {:?}", err);
            return;
        }
        current.clear();
    }

    if desired.is_empty() {
        return;
    }
    let registration = extension_watch_registration(&desired);
    match client.register_capability(vec![registration]).await {
        Ok(()) => {
            info!(
                "Registered {} extension watched-file glob(s)",
                desired.len()
            );
            *current = desired;
        }
        Err(err) => warn!("Failed to register extension file watchers: {:?}", err),
    }
}

pub(super) fn extension_watch_registration(globs: &[String]) -> Registration {
    let options = DidChangeWatchedFilesRegistrationOptions {
        watchers: globs
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|pattern| FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern),
                kind: None,
            })
            .collect(),
    };
    Registration {
        id: "ruby-fast-lsp-extension-watchers".to_string(),
        method: "workspace/didChangeWatchedFiles".to_string(),
        register_options: Some(serde_json::to_value(options).expect_invariant(
            "typed watched-file registration options failed to serialize",
            "lsp-types registration values must serialize",
            "preserve serializable watcher option fields",
        )),
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
