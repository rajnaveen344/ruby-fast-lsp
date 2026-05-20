//! Debug capabilities for the LSP REPL.
//!
//! This module provides custom debug methods that can be invoked via the
//! `$/listCommands` protocol, allowing tools like `lsp-repl` to discover
//! and execute debug commands.

use log::debug;
pub use ruby_analysis_engine::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
use serde::{Deserialize, Serialize};

use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;

// ============================================================================
// Protocol Types
// ============================================================================

/// A parameter definition for a custom command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A custom command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub method: String,
    pub description: String,
    #[serde(default)]
    pub params: Vec<CommandParam>,
}

/// Response from `$/listCommands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCommandsResponse {
    pub commands: Vec<CommandDefinition>,
}

// ============================================================================
// Lookup Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupParams {
    /// The fully qualified name to look up (e.g., "User#find", "Foo::Bar")
    pub fqn: String,
}

// ============================================================================
// Stats Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/stats`.
/// Empty struct to satisfy tower-lsp custom method requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsParams {}

// ============================================================================
// Ancestors Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/ancestors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorsParams {
    /// The class/module name to get ancestors for
    pub class: String,
}

// ============================================================================
// Export Graph Types
// ============================================================================

/// Parameters for `ruby/exportGraph`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportGraphParams {}

// ============================================================================
// Handlers
// ============================================================================

/// Handle `$/listCommands` - return the list of available custom commands.
pub fn handle_list_commands() -> ListCommandsResponse {
    debug!("[DEBUG] Listing available commands");

    let commands = vec![
        CommandDefinition {
            name: "lookup".to_string(),
            method: "ruby-fast-lsp/debug/lookup".to_string(),
            description: "Query index for a fully qualified name (e.g., User#find, Foo::Bar)"
                .to_string(),
            params: vec![CommandParam {
                name: "fqn".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The FQN to look up".to_string()),
            }],
        },
        CommandDefinition {
            name: "stats".to_string(),
            method: "ruby-fast-lsp/debug/stats".to_string(),
            description: "Show index statistics".to_string(),
            params: vec![],
        },
        CommandDefinition {
            name: "ancestors".to_string(),
            method: "ruby-fast-lsp/debug/ancestors".to_string(),
            description: "Show inheritance and mixin chain for a class".to_string(),
            params: vec![CommandParam {
                name: "class".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The class name".to_string()),
            }],
        },
        CommandDefinition {
            name: "methods".to_string(),
            method: "ruby-fast-lsp/debug/methods".to_string(),
            description: "List all methods for a class".to_string(),
            params: vec![CommandParam {
                name: "class".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The class name".to_string()),
            }],
        },
        CommandDefinition {
            name: "inference-stats".to_string(),
            method: "ruby-fast-lsp/debug/inference-stats".to_string(),
            description: "Show type inference statistics and coverage".to_string(),
            params: vec![],
        },
    ];

    ListCommandsResponse { commands }
}

/// Handle `ruby-fast-lsp/debug/lookup` - query analysis state for an FQN.
pub fn handle_lookup(server: &RubyLanguageServer, params: LookupParams) -> LookupResponse {
    debug!("[DEBUG] Looking up FQN: {}", params.fqn);
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_lookup(&params.fqn)
}

/// Handle `ruby-fast-lsp/debug/stats` - return index statistics.
pub fn handle_stats(server: &RubyLanguageServer) -> StatsResponse {
    debug!("[DEBUG] Getting index stats");
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_stats(server.is_indexing_complete())
}

/// Handle `ruby-fast-lsp/debug/ancestors` - get inheritance chain for a class.
pub fn handle_ancestors(server: &RubyLanguageServer, params: AncestorsParams) -> AncestorsResponse {
    debug!("[DEBUG] Getting ancestors for: {}", params.class);
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_ancestors(&params.class)
}

/// Parameters for `ruby-fast-lsp/debug/methods`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodsParams {
    /// The class name to list methods for
    pub class: String,
}

/// Handle `ruby-fast-lsp/debug/methods` - list methods for a class.
pub fn handle_methods(server: &RubyLanguageServer, params: MethodsParams) -> MethodsResponse {
    debug!("[DEBUG] Getting methods for: {}", params.class);
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_methods(&params.class)
}

// ============================================================================
// Inference Stats Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/inference-stats`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InferenceStatsParams {}

/// Handle `ruby-fast-lsp/debug/inference-stats` - get type inference statistics.
pub fn handle_inference_stats(server: &RubyLanguageServer) -> InferenceStatsResponse {
    debug!("[DEBUG] Getting inference stats");
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_inference_stats()
}

/// Handle `ruby/exportGraph` - export the inheritance graph as JSON.
pub fn handle_export_graph(
    server: &RubyLanguageServer,
    _params: ExportGraphParams,
) -> ExportGraphResponse {
    debug!("[DEBUG] Exporting inheritance graph");
    let query = EngineQuery::with_engine(server.analysis_engine.clone());
    query.debug_export_graph()
}
