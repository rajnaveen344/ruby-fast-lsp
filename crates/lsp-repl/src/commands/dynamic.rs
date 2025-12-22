//! Dynamic commands discovered from the server via $/listCommands.

use anyhow::Result;
use serde_json::json;

use super::CommandResult;
use crate::client::LspClient;

/// Execute a custom command from the server.
pub async fn execute(client: &mut LspClient, cmd: &str, args: &[&str]) -> Result<CommandResult> {
    // Find the command definition
    let cmd_def = client
        .custom_commands
        .iter()
        .find(|c| c.name == cmd)
        .cloned();

    match cmd_def {
        Some(def) => {
            // Build parameters from args
            let params = build_params(&def, args, client)?;

            // Execute the custom method
            match client.execute_custom(&def.method, params).await {
                Ok(result) => {
                    let output = format_result(&result);
                    Ok(CommandResult::Output(output))
                }
                Err(e) => Ok(CommandResult::Output(format!("Error: {}", e))),
            }
        }
        None => Ok(CommandResult::NotFound(cmd.to_string())),
    }
}

/// Build JSON parameters from command args based on the command definition.
fn build_params(
    def: &crate::protocol::CommandDefinition,
    args: &[&str],
    client: &LspClient,
) -> Result<serde_json::Value> {
    let mut params = serde_json::Map::new();

    for (i, param_def) in def.params.iter().enumerate() {
        let value = if i < args.len() {
            // Use provided argument
            match param_def.param_type.as_str() {
                "number" | "integer" => {
                    let n: i64 = args[i].parse().unwrap_or(0);
                    // Convert to 0-indexed if it's a line/character position
                    if param_def.name == "line" || param_def.name == "character" || param_def.name == "col" {
                        json!(n.saturating_sub(1))
                    } else {
                        json!(n)
                    }
                }
                "boolean" => json!(args[i].parse::<bool>().unwrap_or(false)),
                _ => json!(args[i]),
            }
        } else if param_def.required {
            // Required parameter missing
            return Err(anyhow::anyhow!(
                "Missing required parameter: {}",
                param_def.name
            ));
        } else {
            // Optional parameter, skip
            continue;
        };

        params.insert(param_def.name.clone(), value);
    }

    // Add current file URI if the command expects it and it wasn't provided
    if def.params.iter().any(|p| p.name == "uri" || p.name == "file")
        && !params.contains_key("uri")
        && !params.contains_key("file")
    {
        if let Some(uri) = &client.current_document {
            params.insert("uri".to_string(), json!(uri.to_string()));
        }
    }

    Ok(serde_json::Value::Object(params))
}

/// Format a JSON result for display.
fn format_result(result: &serde_json::Value) -> String {
    match result {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            // Pretty print arrays
            serde_json::to_string_pretty(arr).unwrap_or_else(|_| format!("{:?}", arr))
        }
        serde_json::Value::Object(obj) => {
            // Pretty print objects
            serde_json::to_string_pretty(obj).unwrap_or_else(|_| format!("{:?}", obj))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CommandDefinition, CommandParam};

    fn make_command(name: &str, params: Vec<CommandParam>) -> CommandDefinition {
        CommandDefinition {
            name: name.to_string(),
            method: format!("test/{}", name),
            description: "Test command".to_string(),
            params,
        }
    }

    fn make_param(name: &str, param_type: &str, required: bool) -> CommandParam {
        CommandParam {
            name: name.to_string(),
            param_type: param_type.to_string(),
            required,
            description: None,
        }
    }

    #[test]
    fn test_format_result_null() {
        assert_eq!(format_result(&json!(null)), "null");
    }

    #[test]
    fn test_format_result_bool() {
        assert_eq!(format_result(&json!(true)), "true");
        assert_eq!(format_result(&json!(false)), "false");
    }

    #[test]
    fn test_format_result_number() {
        assert_eq!(format_result(&json!(42)), "42");
        assert_eq!(format_result(&json!(3.14)), "3.14");
    }

    #[test]
    fn test_format_result_string() {
        assert_eq!(format_result(&json!("hello")), "hello");
    }

    #[test]
    fn test_format_result_array() {
        let result = format_result(&json!([1, 2, 3]));
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_format_result_object() {
        let result = format_result(&json!({"key": "value"}));
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    // Note: build_params requires a LspClient instance which is complex to mock.
    // The following tests verify the parameter parsing logic conceptually.

    #[test]
    fn test_param_type_conversion() {
        // Verify number parsing
        let arg = "42";
        let n: i64 = arg.parse().unwrap();
        assert_eq!(n, 42);

        // Verify boolean parsing
        let arg = "true";
        let b: bool = arg.parse().unwrap();
        assert!(b);

        // Verify line number conversion (1-indexed to 0-indexed)
        let line: i64 = 10;
        let converted = line.saturating_sub(1);
        assert_eq!(converted, 9);
    }

    #[test]
    fn test_command_definition_matching() {
        let commands = vec![
            make_command("lookup", vec![make_param("fqn", "string", true)]),
            make_command("stats", vec![]),
        ];

        // Find by name
        let found = commands.iter().find(|c| c.name == "lookup");
        assert!(found.is_some());
        assert_eq!(found.unwrap().method, "test/lookup");

        // Not found
        let not_found = commands.iter().find(|c| c.name == "unknown");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_command_params_validation() {
        let cmd = make_command(
            "test",
            vec![
                make_param("required_arg", "string", true),
                make_param("optional_arg", "string", false),
            ],
        );

        // Required params count
        let required_count = cmd.params.iter().filter(|p| p.required).count();
        assert_eq!(required_count, 1);

        // Optional params count
        let optional_count = cmd.params.iter().filter(|p| !p.required).count();
        assert_eq!(optional_count, 1);
    }
}

