use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::tools::tool_context::ToolContext;

/// Function declaration that describes a tool's interface to the LLM.
/// Similar to Google's FunctionDeclaration but simplified for our use case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema for parameters
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: Value,
    pub error_message: Option<String>,
}

impl ToolResult {
    pub fn success(data: Value) -> Self {
        Self {
            success: true,
            data,
            error_message: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: Value::Null,
            error_message: Some(message),
        }
    }
}

/// Core trait for all tools in the system.
/// Tools provide functionality that agents can use during conversations.
#[async_trait]
pub trait BaseTool: Send + Sync {
    /// The name of the tool - must be unique within an agent
    fn name(&self) -> &str;

    /// Human-readable description of what this tool does
    fn description(&self) -> &str;

    /// Whether this tool represents a long-running operation
    fn is_long_running(&self) -> bool {
        false
    }

    /// Gets the function declaration for this tool.
    /// This describes the tool's interface to the LLM.
    /// Returns None if this tool doesn't need to be exposed to the LLM
    /// (e.g., for built-in tools that are handled specially by the LLM provider).
    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        None
    }

    /// Convert this tool to a ToolProviderConfig for agent definition.
    ///
    /// Returns `None` if the tool should not be included in the serialized definition
    /// (e.g., for tools that are part of a larger toolset like MCP).
    fn to_tool_provider_config(&self) -> Option<crate::config::ToolProviderConfig> {
        None
    }

    /// Executes the tool with the given arguments and context.
    /// This is the main entry point for tool execution.
    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult;
}
