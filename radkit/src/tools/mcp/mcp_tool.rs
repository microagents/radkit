use super::MCPSessionManager;
use crate::errors::AgentResult;
use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult};
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
    /// Create a new [`MCPTool`]
    #[must_use]
    pub const fn new(
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
            ServiceError::TransportClosed | ServiceError::TransportSend(_) => true,

            // Timeout cancellations are retryable, others are not
            ServiceError::Cancelled { reason } => reason
                .as_ref()
                .is_some_and(|r| r.to_lowercase().contains("timeout")),

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
    ///
    /// Handles all MCP content types (text, image, resource, audio) by serializing
    /// them to JSON. For text content, attempts to parse as JSON first.
    fn extract_content_as_json(content: &[Content]) -> Value {
        if content.is_empty() {
            return Value::Null;
        }

        // If single content item, extract it directly
        if content.len() == 1 {
            return Self::extract_single_content(&content[0]);
        }

        // Multiple content items - collect all as JSON array
        let mut content_items = Vec::new();
        for item in content {
            content_items.push(Self::extract_single_content(item));
        }

        serde_json::json!({"content": content_items})
    }

    /// Extract a single content item to JSON
    fn extract_single_content(item: &Content) -> Value {
        // Try text content first - attempt to parse as JSON
        if let Some(text_content) = item.as_text() {
            // Try to parse as JSON first, fallback to string
            return serde_json::from_str::<Value>(&text_content.text)
                .unwrap_or_else(|_| Value::String(text_content.text.clone()));
        }

        // For non-text content (image, resource, audio, etc.), serialize the entire
        // content object to preserve all data
        serde_json::to_value(item).unwrap_or_else(|e| {
            serde_json::json!({
                "error": "Failed to serialize content",
                "details": e.to_string()
            })
        })
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for MCPTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> FunctionDeclaration {
        FunctionDeclaration::new(
            self.name.clone(),
            self.description.clone(),
            self.input_schema.clone(),
        )
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
            .field("input_schema", &self.input_schema)
            .field("session_manager", &"<MCPSessionManager>")
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

        // Multiple text contents - now wrapped in content array
        let content = vec![Content::text("First part"), Content::text("Second part")];
        let result = MCPTool::extract_content_as_json(&content);
        let expected = serde_json::json!({
            "content": ["First part", "Second part"]
        });
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_single_content() {
        use rmcp::model::Content;

        // Text content that's valid JSON
        let item = Content::text(r#"{"key": "value"}"#);
        let result = MCPTool::extract_single_content(&item);
        assert_eq!(result, serde_json::json!({"key": "value"}));

        // Text content that's plain text
        let item = Content::text("plain text");
        let result = MCPTool::extract_single_content(&item);
        assert_eq!(result, Value::String("plain text".to_string()));

        // Non-text content should be serialized as-is
        // Note: We can't easily test image/resource content without more setup,
        // but the serde_json::to_value call will handle them properly
    }
}
