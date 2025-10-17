//! Save Artifact Tool - Built-in tool for persisting task outputs
//!
//! This tool allows agents to persist important outputs and automatically
//! emits TaskArtifactUpdate events for A2A protocol compliance.

use a2a_types::{Artifact, Part};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::tools::{BaseTool, FunctionDeclaration, ToolArtifactAccess, ToolContext, ToolResult};

/// Built-in tool for saving artifacts
///
/// This tool integrates with the task management system and emits A2A-compliant
/// TaskArtifactUpdate events. It should not appear in exported configurations.
#[derive(Debug, Clone)]
pub struct SaveArtifactTool;

impl SaveArtifactTool {
    /// Create a new SaveArtifactTool
    pub fn new() -> Self {
        Self
    }
}

impl Default for SaveArtifactTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseTool for SaveArtifactTool {
    fn name(&self) -> &str {
        "save_artifact"
    }

    fn description(&self) -> &str {
        "Save an artifact (file, data, result) from the current task. Use this to persist important outputs."
    }

    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
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
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("name is required".to_string()),
        };

        let content = match args.get("content") {
            Some(c) => c,
            None => return ToolResult::error("content is required".to_string()),
        };

        let artifact_type = args.get("type").and_then(|v| v.as_str()).unwrap_or("data");

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Create content part based on the content type
        let content_part = if let Some(text) = content.as_str() {
            Part::Text {
                text: text.to_string(),
                metadata: None,
            }
        } else {
            // For non-text content, convert to string representation
            Part::Text {
                text: content.to_string(),
                metadata: None,
            }
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
                meta.insert("context_id".to_string(), json!(context.context_id()));
                meta.insert("task_id".to_string(), json!(context.task_id()));
                meta.insert(
                    "content_type".to_string(),
                    json!(match artifact_type {
                        "file" => "application/octet-stream",
                        "data" => "application/json",
                        "result" => "application/json",
                        "log" => "text/plain",
                        "image" => "image/png",
                        "document" => "text/plain",
                        _ => "application/json",
                    }),
                );
                meta
            }),
        };

        // Emit artifact saved event via ToolContext capabilities
        let _ = context.save_artifact(artifact).await;

        ToolResult::success(json!({
            "name": name,
            "type": artifact_type,
            "message": "Artifact emission requested"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_artifact_tool_properties() {
        let tool = SaveArtifactTool::new();
        assert_eq!(tool.name(), "save_artifact");
        assert!(!tool.description().is_empty());
        assert!(tool.get_declaration().is_some());
    }
}
