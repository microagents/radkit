//! Update Status Tool - Built-in tool for task lifecycle management
//!
//! This tool allows agents to control their task lifecycle states and automatically
//! emits TaskStatusUpdate events for A2A protocol compliance.

use a2a_types::TaskState;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult, ToolTaskAccess};

/// Built-in tool for updating task status
///
/// This tool integrates with the task management system and emits A2A-compliant
/// TaskStatusUpdate events. It should not appear in exported configurations.
#[derive(Debug, Clone)]
pub struct UpdateStatusTool;

impl UpdateStatusTool {
    /// Create a new UpdateStatusTool
    pub fn new() -> Self {
        Self
    }
}

impl Default for UpdateStatusTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseTool for UpdateStatusTool {
    fn name(&self) -> &str {
        "update_status"
    }

    fn description(&self) -> &str {
        "Update the current task status with a message. Use 'working' when starting work, 'completed' when finished successfully, 'failed' if there are errors. This communicates progress to users."
    }

    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["submitted", "working", "input-required", "auth-required", "completed", "failed", "canceled", "rejected"],
                        "description": "The new task status: 'working' when starting work, 'completed' when task is finished successfully, 'failed' if errors occur, 'input-required' if user input needed"
                    },
                    "message": {
                        "type": "string",
                        "description": "Optional status message to provide context"
                    }
                },
                "required": ["status"]
            }),
        })
    }

    fn to_tool_provider_config(&self) -> Option<crate::config::ToolProviderConfig> {
        Some(crate::config::ToolProviderConfig::Builtin {
            name: self.name().to_string(),
        })
    }

    async fn run_async(
        &self,
        args: HashMap<String, serde_json::Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        let status_str = args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("working");

        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let state = match status_str {
            "submitted" => TaskState::Submitted,
            "working" => TaskState::Working,
            "input-required" => TaskState::InputRequired,
            "auth-required" => TaskState::AuthRequired,
            "completed" => TaskState::Completed,
            "failed" => TaskState::Failed,
            "canceled" => TaskState::Canceled,
            "rejected" => TaskState::Rejected,
            _ => TaskState::Working,
        };

        // Emit task status change event via ToolContext capabilities
        let _ = context.update_task_status(state, message).await;

        ToolResult::success(json!({
            "status": status_str,
            "message": "Status update emitted"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_status_tool_properties() {
        let tool = UpdateStatusTool::new();
        assert_eq!(tool.name(), "update_status");
        assert!(!tool.description().is_empty());
        assert!(tool.get_declaration().is_some());
    }
}
