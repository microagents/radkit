use crate::errors::{AgentError, AgentResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

use crate::tools::tool_context::ToolContext;
use a2a_types::MessageSendParams;

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

    /// Executes the tool with the given arguments and context.
    /// This is the main entry point for tool execution.
    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult;

    /// Processes the outgoing LLM request for this tool.
    /// Use cases:
    /// - Most common: adding this tool's declaration to the LLM request
    /// - Advanced: preprocessing the LLM request before it's sent
    async fn process_llm_request(
        &self,
        _context: &ToolContext<'_>,
        _llm_request: &mut MessageSendParams,
    ) -> AgentResult<()> {
        // Default implementation: add function declaration to message metadata if available
        // In A2A-native architecture, tools are passed via message data parts
        Ok(())
    }

    /// Creates a tool instance from configuration.
    /// Tools that support config-based creation should override this.
    fn from_config(_config: &ToolArgsConfig, _config_path: &Path) -> AgentResult<Box<dyn BaseTool>>
    where
        Self: Sized,
    {
        Err(AgentError::NotImplemented {
            feature: format!(
                "Configuration-based creation for tool: {}",
                std::any::type_name::<Self>()
            ),
        })
    }
}

/// Configuration for creating tools from config files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub args: Option<HashMap<String, Value>>,
}

/// Arguments configuration for tools - allows arbitrary key-value pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolArgsConfig {
    #[serde(flatten)]
    pub args: HashMap<String, Value>,
}
