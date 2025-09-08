use super::MCPSessionManager;
use crate::errors::AgentResult;
use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult};
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParam, Content, JsonObject};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Individual MCP tool that uses session manager for execution
pub struct MCPTool {
    name: String,
    description: String,
    input_schema: Value,
    session_manager: Arc<MCPSessionManager>,
}

impl MCPTool {
    /// Create a new MCPTool
    pub fn new(
        name: String,
        description: String,
        input_schema: Value,
        session_manager: Arc<MCPSessionManager>,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            session_manager,
        }
    }

    /// Execute the tool call with retry logic for connection failures
    async fn execute_tool_call(
        &self,
        args: HashMap<String, Value>,
        _context: &ToolContext<'_>,
    ) -> AgentResult<ToolResult> {
        const MAX_ATTEMPTS: u32 = 2;
        let mut attempts = 0;

        while attempts < MAX_ATTEMPTS {
            attempts += 1;
            debug!(
                "Attempting MCP tool call: {} (attempt {})",
                self.name, attempts
            );

            match self.try_execute_once(args.clone()).await {
                Ok(result) => {
                    debug!("MCP tool call succeeded: {}", self.name);
                    return Ok(result);
                }
                Err(e) if Self::is_retryable_mcp_error(&e) && attempts < MAX_ATTEMPTS => {
                    warn!(
                        "MCP tool call failed due to connection error, retrying: {} (attempt {})",
                        e, attempts
                    );
                    continue;
                }
                Err(e) => {
                    error!("MCP tool call failed: {} - {}", self.name, e);
                    return Ok(ToolResult::error(format!("MCP tool error: {e}")));
                }
            }
        }

        Ok(ToolResult::error(
            "Max retry attempts exceeded for MCP tool call".to_string(),
        ))
    }

    /// Try to execute the tool call once
    async fn try_execute_once(
        &self,
        args: HashMap<String, Value>,
    ) -> Result<ToolResult, rmcp::service::ServiceError> {
        // Get session (will create or reuse)
        let session = self
            .session_manager
            .create_session(None)
            .await
            .map_err(|_| rmcp::service::ServiceError::TransportClosed)?;

        // Convert args to MCP object format
        let mcp_args = if args.is_empty() {
            None
        } else {
            // Convert HashMap to JsonObject (serde_json::Map)
            let json_map: JsonObject = args.into_iter().collect();
            Some(json_map)
        };

        // Call the tool
        let result = session
            .call_tool(CallToolRequestParam {
                name: self.name.clone().into(),
                arguments: mcp_args,
            })
            .await?;

        // Convert MCP result to Radkit ToolResult
        if result.is_error.unwrap_or(false) {
            let error_msg = Self::extract_error_message(&result.content);
            Ok(ToolResult::error(error_msg))
        } else {
            let content = Self::extract_content_as_json(&result.content);
            Ok(ToolResult::success(content))
        }
    }

    /// Check if an rmcp error should trigger retry
    fn is_retryable_mcp_error(error: &rmcp::service::ServiceError) -> bool {
        use rmcp::service::ServiceError;

        match error {
            // Transport-related errors are retryable
            ServiceError::TransportClosed => true,
            ServiceError::TransportSend(_) => true,

            // Timeout cancellations are retryable, others are not
            ServiceError::Cancelled { reason } => reason
                .as_ref()
                .map(|r| r.to_lowercase().contains("timeout"))
                .unwrap_or(false),

            // Protocol and other errors are not retryable
            ServiceError::McpError(_) => false,
            ServiceError::UnexpectedResponse => false,

            // Conservative default - don't retry unknown errors
            _ => false,
        }
    }

    /// Extract error message from MCP content
    fn extract_error_message(content: &[Content]) -> String {
        if content.is_empty() {
            return "Unknown MCP error".to_string();
        }

        let mut messages = Vec::new();
        for item in content {
            if let Some(text_content) = item.as_text() {
                messages.push(text_content.text.clone());
            }
        }

        if messages.is_empty() {
            "Non-text error content".to_string()
        } else {
            messages.join(" ")
        }
    }

    /// Extract content from MCP response and convert to JSON
    fn extract_content_as_json(content: &[Content]) -> Value {
        if content.is_empty() {
            return Value::Null;
        }

        // If single text content, try to parse as JSON, fallback to string
        if content.len() == 1 {
            if let Some(text_content) = content[0].as_text() {
                // Try to parse as JSON first
                if let Ok(json_value) = serde_json::from_str::<Value>(&text_content.text) {
                    return json_value;
                } else {
                    return Value::String(text_content.text.clone());
                }
            }
        }

        // Multiple content items - collect text parts
        let mut text_parts = Vec::new();
        for item in content {
            if let Some(text_content) = item.as_text() {
                text_parts.push(text_content.text.clone());
            }
        }

        if text_parts.is_empty() {
            serde_json::json!({"message": "No text content available"})
        } else {
            serde_json::json!({"text": text_parts})
        }
    }
}

#[async_trait]
impl BaseTool for MCPTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_long_running(&self) -> bool {
        // MCP tools can potentially be long-running
        // This could be made configurable in the future
        true
    }

    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.input_schema.clone(),
        })
    }

    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        match self.execute_tool_call(args, context).await {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to execute MCP tool {}: {}", self.name, e);
                ToolResult::error(format!("Tool execution failed: {e}"))
            }
        }
    }
}

impl std::fmt::Debug for MCPTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MCPTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_rmcp_error() {
        use rmcp::service::ServiceError;

        // Test retryable errors - we can't easily create TransportSend in tests
        // since DynamicTransportError is complex, but TransportClosed and timeout are key cases
        assert!(MCPTool::is_retryable_mcp_error(
            &ServiceError::TransportClosed
        ));
        assert!(MCPTool::is_retryable_mcp_error(&ServiceError::Cancelled {
            reason: Some("timeout occurred".to_string())
        }));

        // Test non-retryable errors
        assert!(!MCPTool::is_retryable_mcp_error(
            &ServiceError::UnexpectedResponse
        ));
        assert!(!MCPTool::is_retryable_mcp_error(&ServiceError::Cancelled {
            reason: Some("user cancelled".to_string())
        }));
        assert!(!MCPTool::is_retryable_mcp_error(&ServiceError::Cancelled {
            reason: None
        }));
    }

    #[test]
    fn test_extract_error_message() {
        use rmcp::model::Content;

        // Empty content
        let content = vec![];
        assert_eq!(
            MCPTool::extract_error_message(&content),
            "Unknown MCP error"
        );

        // Single text content
        let content = vec![Content::text("Error occurred")];
        assert_eq!(MCPTool::extract_error_message(&content), "Error occurred");

        // Multiple text contents
        let content = vec![
            Content::text("Error:"),
            Content::text("Something went wrong"),
        ];
        assert_eq!(
            MCPTool::extract_error_message(&content),
            "Error: Something went wrong"
        );
    }

    #[test]
    fn test_extract_content_as_json() {
        use rmcp::model::Content;

        // Empty content
        let content = vec![];
        assert_eq!(MCPTool::extract_content_as_json(&content), Value::Null);

        // Single text content (valid JSON)
        let content = vec![Content::text(r#"{"result": "success"}"#)];
        let expected = serde_json::json!({"result": "success"});
        assert_eq!(MCPTool::extract_content_as_json(&content), expected);

        // Single text content (plain text)
        let content = vec![Content::text("Simple text response")];
        assert_eq!(
            MCPTool::extract_content_as_json(&content),
            Value::String("Simple text response".to_string())
        );

        // Multiple text contents
        let content = vec![Content::text("First part"), Content::text("Second part")];
        let result = MCPTool::extract_content_as_json(&content);
        let expected = serde_json::json!({
            "text": ["First part", "Second part"]
        });
        assert_eq!(result, expected);
    }
}
