//! Debug capabilities for the LSP REPL.
//!
//! This module provides custom debug methods that can be invoked via the
//! `$/listCommands` protocol, allowing tools like `lsp-repl` to discover
//! and execute debug commands.

use std::collections::HashMap;

use log::debug;
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

/// An entry in the lookup response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupEntry {
    pub fqn: String,
    pub kind: String,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<String>>,
}

/// Response from `ruby-fast-lsp/debug/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    pub found: bool,
    pub entries: Vec<LookupEntry>,
}

// ============================================================================
// Stats Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/stats`.
/// Empty struct to satisfy tower-lsp custom method requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsParams {}

/// Response from `ruby-fast-lsp/debug/stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_definitions: usize,
    pub total_entries: usize,
    pub classes: usize,
    pub modules: usize,
    pub methods: usize,
    pub constants: usize,
    pub instance_variables: usize,
    pub files_indexed: usize,
    pub indexing_complete: bool,
}

// ============================================================================
// Ancestors Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/ancestors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorsParams {
    /// The class/module name to get ancestors for
    pub class: String,
}

/// An ancestor entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorEntry {
    pub name: String,
    pub kind: String, // "superclass", "include", "extend", "prepend"
}

/// Response from `ruby-fast-lsp/debug/ancestors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorsResponse {
    pub class: String,
    pub ancestors: Vec<AncestorEntry>,
}

// ============================================================================
// Export Graph Types
// ============================================================================

/// Parameters for `ruby/exportGraph`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportGraphParams {}

/// A snapshot of a single node in the inheritance graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNodeSnapshot {
    /// Node kind: "Class" or "Module"
    pub kind: String,
    /// Superclass FQN (for classes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superclass: Option<String>,
    /// Included modules (resolved FQNs)
    pub includes: Vec<String>,
    /// Prepended modules (resolved FQNs)
    pub prepends: Vec<String>,
    /// Classes/modules that include this module (reverse edge)
    pub included_by: Vec<String>,
    /// Classes/modules that prepend this module (reverse edge)
    pub prepended_by: Vec<String>,
    /// Direct subclasses (reverse edge, for classes)
    pub children: Vec<String>,
    /// Classes that ultimately include this module (traversing through modules)
    /// Only populated for modules, empty for classes.
    pub included_by_classes: Vec<String>,
    /// Method Resolution Order (computed from graph)
    pub mro: Vec<String>,
}

/// Response from `ruby/exportGraph`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportGraphResponse {
    /// Total number of nodes in the graph
    pub node_count: usize,
    /// All nodes indexed by FQN
    pub nodes: HashMap<String, GraphNodeSnapshot>,
}

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

/// Handle `ruby-fast-lsp/debug/lookup` - query the index for an FQN.
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

/// A method entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodEntry {
    pub name: String,
    pub kind: String,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

/// Response from `ruby-fast-lsp/debug/methods`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodsResponse {
    pub class: String,
    pub methods: Vec<MethodEntry>,
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

/// Response from `ruby-fast-lsp/debug/inference-stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStatsResponse {
    pub total_methods: usize,
    pub methods_with_return_type: usize,
    pub methods_without_return_type: usize,
    pub inference_coverage_percent: f64,
    pub top_files_by_method_count: Vec<FileMethodCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMethodCount {
    pub file: String,
    pub method_count: usize,
}

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
