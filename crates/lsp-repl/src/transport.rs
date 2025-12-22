//! JSON-RPC transport layer for LSP communication.
//!
//! Handles the framing of LSP messages over stdio using the Content-Length header.

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

use crate::protocol::{JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Transport layer for LSP JSON-RPC communication over stdio.
pub struct Transport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Transport {
    /// Create a new transport from child process stdio handles.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
        }
    }

    /// Send a JSON-RPC request and wait for a response.
    pub async fn send_request(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        self.send_message(&serde_json::to_value(request)?).await?;

        // Read responses until we get the one matching our request ID
        loop {
            let message = self.read_message().await?;

            match message {
                JsonRpcMessage::Response(response) => {
                    if response.id == request.id {
                        return Ok(response);
                    }
                    // Response for a different request, keep reading
                }
                JsonRpcMessage::Notification(_) => {
                    // Server sent a notification, ignore and keep reading
                    continue;
                }
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(&mut self, notification: &JsonRpcNotification) -> Result<()> {
        self.send_message(&serde_json::to_value(notification)?)
            .await
    }

    /// Send a raw JSON-RPC message with Content-Length header.
    async fn send_message(&mut self, message: &serde_json::Value) -> Result<()> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;

        Ok(())
    }

    /// Read a JSON-RPC message from the server.
    pub async fn read_message(&mut self) -> Result<JsonRpcMessage> {
        // Read headers until we find Content-Length
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).await?;

            let line = line.trim();

            // Empty line signals end of headers
            if line.is_empty() {
                break;
            }

            // Parse Content-Length header
            if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                content_length = Some(len_str.parse().context("Invalid Content-Length")?);
            }
            // Ignore other headers (like Content-Type)
        }

        let content_length =
            content_length.ok_or_else(|| anyhow!("Missing Content-Length header"))?;

        // Read the body
        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await?;

        let body_str = String::from_utf8(body).context("Invalid UTF-8 in message body")?;

        // Parse the JSON-RPC message
        let message: JsonRpcMessage =
            serde_json::from_str(&body_str).context("Failed to parse JSON-RPC message")?;

        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_length_header_format() {
        // Verify the header format we generate
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#;
        let expected_header = format!("Content-Length: {}\r\n\r\n", body.len());
        assert_eq!(expected_header, "Content-Length: 40\r\n\r\n");
    }

    #[test]
    fn test_message_body_serialization() {
        let request = JsonRpcRequest::new(1, "textDocument/hover", None);
        let body = serde_json::to_string(&request).unwrap();
        // Verify the body is valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["method"], "textDocument/hover");
    }
}
