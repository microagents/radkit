use crate::a2a::AgentCard;
use crate::errors::LoaderError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete agent configuration from YAML/JSON
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDefinition {
    /// Agent card with metadata
    pub card: AgentCard,

    /// System instruction for the agent
    pub instruction: String,

    /// Maximum iterations for tool calling
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,

    /// Timeout for the entire execution in seconds
    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    /// Additional custom configuration
    #[serde(default)]
    pub custom_config: HashMap<String, serde_json::Value>,

    /// Model configuration
    pub model: ModelConfig,

    /// Optional tools configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolProviderConfig>,

    /// Optional remote A2A agents configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_agents: Option<Vec<RemoteAgentConfig>>,
}

/// Model configuration variants
///
/// **Important**: The `api_key_env` field contains the **environment variable name**, not the actual API key.
/// For example: "OPENAI_API_KEY" not "sk-abc123..."
///
/// At runtime, the LLM will resolve this environment variable using either:
/// - The default `std::env` resolver
/// - A custom resolver provided via `AgentBuilder::with_env_resolver()`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ModelConfig {
    #[serde(rename = "anthropic")]
    Anthropic {
        name: String,
        #[serde(rename = "api_key_env")]
        api_key_env: String,
    },
    #[serde(rename = "openai")]
    OpenAI {
        name: String,
        #[serde(rename = "api_key_env")]
        api_key_env: String,
    },
    #[serde(rename = "gemini")]
    Gemini {
        name: String,
        #[serde(rename = "api_key_env")]
        api_key_env: String,
    },
}

/// Unified configuration for any tool provider.
///
/// This enum represents the configuration for any "provider" of tools,
/// whether it's a single built-in tool, a function, or a connection
/// that discovers multiple tools (like MCP).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ToolProviderConfig {
    /// Configuration for a single built-in tool.
    #[serde(rename = "builtin")]
    Builtin {
        /// The name of the built-in tool (e.g., "update_status").
        name: String,
    },

    /// Configuration for an MCP-over-HTTP tool provider.
    #[serde(rename = "mcp_http")]
    MCPHttp(MCPHttpConfig),

    /// Configuration for an MCP-over-Stdio tool provider.
    #[serde(rename = "mcp_stdio")]
    MCPStdio(MCPStdioConfig),

    /// Configuration for a single function tool.
    ///
    /// **IMPORTANT: Export-Only**
    /// This variant is used for serializing the schema of a `FunctionTool` for
    /// documentation or inspection. It cannot be loaded back, as the function's
    /// implementation (code) is not serializable.
    #[serde(rename = "function")]
    Function(FunctionToolConfig),
}

/// Configuration for a function tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionToolConfig {
    /// Function name
    pub name: String,

    /// Function arguments schema
    pub args: serde_json::Value,
}

/// Configuration for remote A2A agents
///
/// Remote agents enable delegation to external A2A-compliant agent services.
/// Authentication is handled via environment variables to avoid exposing secrets in configs.
///
/// **Authentication Headers:**
/// - Use environment variable references like `${API_TOKEN}` for sensitive values
/// - The loader resolves these at runtime using `EnvResolver`
/// - Example: `"Authorization": "${REMOTE_AGENT_TOKEN}"`
///
/// **Example Configuration:**
/// ```yaml
/// remote_agents:
///   - name: Weather Agent
///     description: Provides weather information
///     version: 1.0.0
///     endpoint: https://weather.example.com
///     headers:
///       Authorization: ${WEATHER_API_TOKEN}
///       X-API-Key: ${WEATHER_API_KEY}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAgentConfig {
    /// Name of the remote agent
    pub name: String,

    /// Description of the agent's capabilities
    pub description: String,

    /// Version of the agent
    pub version: String,

    /// Agent endpoint URL
    pub endpoint: String,

    /// Optional authentication headers (values can be env var references)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MCPStdioConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    pub tools_filter: Option<ToolsFilter>,
}

/// MCP HTTP connection configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MCPHttpConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    pub tools_filter: Option<ToolsFilter>,
}

/// Tool filter configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolsFilter {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

fn default_max_iterations() -> usize {
    10
}

fn default_timeout() -> u64 {
    30000
}

impl AgentDefinition {
    /// Create an AgentConfig from this definition
    pub fn to_agent_config(&self) -> crate::agents::config::AgentConfig {
        crate::agents::config::AgentConfig {
            max_iterations: self.max_iterations,
            timeout_seconds: self.timeout_seconds,
            custom_config: self.custom_config.clone(),
        }
    }

    /// Export this definition to YAML format
    pub fn to_yaml(&self) -> Result<String, LoaderError> {
        serde_yaml::to_string(self)
            .map_err(|e| LoaderError::validation(format!("Failed to serialize to YAML: {e}")))
    }

    /// Export this definition to JSON format (pretty-printed)
    pub fn to_json(&self) -> Result<String, LoaderError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| LoaderError::validation(format!("Failed to serialize to JSON: {e}")))
    }

    /// Export this definition to compact JSON format
    pub fn to_json_compact(&self) -> Result<String, LoaderError> {
        serde_json::to_string(self)
            .map_err(|e| LoaderError::validation(format!("Failed to serialize to JSON: {e}")))
    }
}
