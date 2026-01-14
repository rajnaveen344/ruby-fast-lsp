//! Protocol types for the `$/listCommands` extension.
//!
//! This module defines the request/response types for the custom `$/listCommands`
//! method that allows LSP servers to advertise their available debug commands.

use serde::{Deserialize, Serialize};

/// A parameter definition for a custom command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandParam {
    /// The name of the parameter (e.g., "fqn", "line", "col")
    pub name: String,

    /// The type of the parameter (e.g., "string", "number", "boolean")
    #[serde(rename = "type")]
    pub param_type: String,

    /// Whether this parameter is required
    #[serde(default)]
    pub required: bool,

    /// Optional description of the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A custom command definition returned by `$/listCommands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    /// The short name used in the REPL (e.g., "lookup", "ancestors")
    pub name: String,

    /// The full LSP method name (e.g., "ruby-fast-lsp/debug/lookup")
    pub method: String,

    /// Human-readable description of what the command does
    pub description: String,

    /// The parameters this command accepts
    #[serde(default)]
    pub params: Vec<CommandParam>,
}

/// Response from the `$/listCommands` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCommandsResponse {
    /// The list of available custom commands
    pub commands: Vec<CommandDefinition>,
}

/// A JSON-RPC request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }
}

/// A JSON-RPC notification message (no id, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }
}

/// A JSON-RPC response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Any JSON-RPC message (for parsing incoming messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest::new(1, "textDocument/hover", None);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"textDocument/hover\""));
        assert!(!json.contains("params")); // skip_serializing_if = None
    }

    #[test]
    fn test_json_rpc_request_with_params() {
        let params = serde_json::json!({"line": 10, "col": 5});
        let request = JsonRpcRequest::new(42, "test/method", Some(params));
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"params\""));
        assert!(json.contains("\"line\":10"));
    }

    #[test]
    fn test_json_rpc_notification_serialization() {
        let notification = JsonRpcNotification::new("initialized", None);
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"initialized\""));
        assert!(!json.contains("\"id\"")); // notifications don't have id
    }

    #[test]
    fn test_json_rpc_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"value":"hello"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 1);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_error_response_deserialization() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, 1);
        assert!(response.result.is_none());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
        assert_eq!(error.message, "Method not found");
    }

    #[test]
    fn test_command_definition_serialization() {
        let cmd = CommandDefinition {
            name: "lookup".to_string(),
            method: "ruby-fast-lsp/debug/lookup".to_string(),
            description: "Query index".to_string(),
            params: vec![CommandParam {
                name: "fqn".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The FQN".to_string()),
            }],
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"name\":\"lookup\""));
        assert!(json.contains("\"method\":\"ruby-fast-lsp/debug/lookup\""));
        assert!(json.contains("\"type\":\"string\"")); // renamed from param_type
    }

    #[test]
    fn test_list_commands_response_deserialization() {
        let json = r#"{
            "commands": [
                {
                    "name": "stats",
                    "method": "ruby-fast-lsp/debug/stats",
                    "description": "Show stats",
                    "params": []
                }
            ]
        }"#;
        let response: ListCommandsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.commands.len(), 1);
        assert_eq!(response.commands[0].name, "stats");
    }

    #[test]
    fn test_json_rpc_message_untagged_enum() {
        // Response
        let json = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::Response(_)));

        // Notification
        let json = r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, JsonRpcMessage::Notification(_)));
    }
}
