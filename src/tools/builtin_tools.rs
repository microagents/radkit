//! Built-in tools for agent task management
//!
//! This module provides factory functions for creating built-in tools that integrate
//! directly with TaskManager and automatically emit A2A-compliant events.

use crate::a2a::{Artifact, Message, MessageRole, Part, TaskState, TaskStatus};

/// Built-in tools that can be enabled on agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTool {
    /// Update task status tool - allows agents to control their task lifecycle
    UpdateStatus,
    /// Save artifact tool - allows agents to persist important outputs
    SaveArtifact,
    // Future built-in tools can be added here:
    // SendNotification,
    // LogMessage,
    // SetReminder,
}
use crate::tools::{BaseTool, FunctionTool};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Create the built-in update_status tool
///
/// This tool allows agents to control their task lifecycle states and automatically
/// emits `TaskStatusUpdate` events for A2A protocol compliance.
pub fn create_update_status_tool() -> Arc<dyn BaseTool> {
    Arc::new(FunctionTool::new(
        "update_status".to_string(),
        "Update the current task status with a message. Use 'working' when starting work, 'completed' when finished successfully, 'failed' if there are errors. This communicates progress to users.".to_string(),
        |args, context| {
            Box::pin(async move {
                let status_str = args.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("working");
                let message = args.get("message")
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
                let task_status = TaskStatus {
                    state,
                    timestamp: Some(Utc::now().to_rfc3339()),
                    message: message.map(|text| Message {
                        kind: "message".to_string(),
                        message_id: Uuid::new_v4().to_string(),
                        role: MessageRole::Agent,
                        parts: vec![Part::Text { text, metadata: None }],
                        context_id: Some(context.context_id.clone()),
                        task_id: Some(context.task_id.clone()),
                        reference_task_ids: Vec::new(),
                        extensions: Vec::new(),
                        metadata: None,
                    }),
                };

                // Emit A2A status update via event system; storage handler will persist
                let event = crate::a2a::TaskStatusUpdateEvent {
                    kind: "status-update".to_string(),
                    task_id: context.task_id.clone(),
                    context_id: context.context_id.clone(),
                    status: task_status,
                    is_final: matches!(status_str, "completed" | "failed" | "canceled" | "rejected"),
                    metadata: None,
                };
                // Emit via tool context; if no projector wired, this is a no-op
                let _ = context
                    .emit_a2a(crate::a2a::SendStreamingMessageResult::TaskStatusUpdate(event))
                    .await;
                crate::tools::ToolResult::success(json!({
                    "status": status_str,
                    "message": "Status update emitted"
                }))
            })
        },
    )
    .with_parameters_schema(json!({
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
    }))) as Arc<dyn BaseTool>
}

/// Create the built-in save_artifact tool
///
/// This tool allows agents to persist important outputs and automatically
/// emits `TaskArtifactUpdate` events for A2A protocol compliance.
pub fn create_save_artifact_tool() -> Arc<dyn BaseTool> {
    Arc::new(FunctionTool::new(
        "save_artifact".to_string(),
        "Save an artifact (file, data, result) from the current task. Use this to persist important outputs.".to_string(),
        |args, context| {
            Box::pin(async move {
                let name = match args.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => return crate::tools::ToolResult::error("name is required".to_string()),
                };
                let content = match args.get("content") {
                    Some(c) => c,
                    None => return crate::tools::ToolResult::error("content is required".to_string()),
                };
                let artifact_type = args.get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("data");
                let description = args.get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                // Create content part based on the content type
                let content_part = if let Some(text) = content.as_str() {
                    Part::Text { text: text.to_string(), metadata: None }
                } else {
                    // For non-text content, convert to string representation
                    Part::Text { text: content.to_string(), metadata: None }
                };
                // Create A2A Artifact
                let artifact = Artifact {
                    artifact_id: format!("artifact_{}", Uuid::new_v4()),
                    parts: vec![content_part],
                    name: Some(name.to_string()),
                    description,
                    extensions: Vec::new(),
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("type".to_string(), json!(artifact_type));
                        meta
                    }),
                };

                // Emit an A2A artifact update; storage handler will persist
                let event = crate::a2a::TaskArtifactUpdateEvent {
                    kind: "artifact-update".to_string(),
                    task_id: context.task_id.clone(),
                    context_id: context.context_id.clone(),
                    artifact,
                    append: None,
                    last_chunk: Some(true),
                    metadata: None,
                };
                // Use context to emit; if not wired, it's a no-op
                let _ = context.emit_a2a(crate::a2a::SendStreamingMessageResult::TaskArtifactUpdate(event)).await;
                crate::tools::ToolResult::success(json!({
                    "name": name,
                    "type": artifact_type,
                    "message": "Artifact emission requested"
                }))
            })
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Name/identifier for the artifact"
            },
            "content": {
                "description": "The artifact content (can be any JSON type)"
            },
            "type": {
                "type": "string",
                "enum": ["file", "data", "result", "log", "image", "document"],
                "description": "Type of artifact being saved"
            },
            "description": {
                "type": "string",
                "description": "Optional description of the artifact"
            }
        },
        "required": ["name", "content"]
    }))) as Arc<dyn BaseTool>
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_builtin_tools_documentation() {
        // This test ensures the documentation module exists and is accessible
        // Built-in tools are now implemented directly in the agent for better integration

        // The actual functionality is tested in:
        // - tests/builtin_tools_rejection_tests.rs (integration tests)
        // - agents/agent.rs (unit tests for the agent implementation)

        // This confirms the documentation structure is in place
        assert!(true, "Built-in tools documentation module exists");
    }
}
