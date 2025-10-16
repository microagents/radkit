use crate::a2a::AgentCard;
use crate::agents::AgentBuilder;
use crate::config::schema::validate_config;
use crate::config::{
    AgentDefinition, EnvKey, MCPHttpConfig, MCPStdioConfig, ModelConfig, ToolProviderConfig,
};
use crate::errors::LoaderError;
use crate::tools::{MCPConnectionParams, MCPToolset};
use serde_json::Value;

/// Main agent loader for loading agents from YAML/JSON configuration files
///
/// **Note on Environment Resolution:**
/// The loader does not resolve environment variables during loading.
/// API keys in configuration files are stored as environment variable names (e.g., "OPENAI_API_KEY").
/// Use `AgentBuilder::with_env_resolver()` on the returned builder if you need custom
/// resolution (vault, secrets manager, etc.). Otherwise, the default std::env resolver is used.
pub struct AgentLoader;

impl AgentLoader {
    /// Create a new loader
    pub fn new() -> Self {
        Self
    }

    /// Load agent from YAML string
    pub async fn from_yaml(yaml: &str) -> Result<AgentBuilder, LoaderError> {
        Self::new().load_from_yaml(yaml).await
    }

    /// Load agent from JSON string
    pub async fn from_json(json: &str) -> Result<AgentBuilder, LoaderError> {
        Self::new().load_from_json(json).await
    }

    /// Load agent from YAML string using this loader's resolver
    pub async fn load_from_yaml(&self, yaml: &str) -> Result<AgentBuilder, LoaderError> {
        self.load_from_str(yaml, true).await
    }

    /// Load agent from JSON string using this loader's resolver
    pub async fn load_from_json(&self, json: &str) -> Result<AgentBuilder, LoaderError> {
        self.load_from_str(json, false).await
    }

    async fn load_from_str(
        &self,
        content: &str,
        is_yaml: bool,
    ) -> Result<AgentBuilder, LoaderError> {
        // Parse content to Value for schema validation
        let value: Value = if is_yaml {
            serde_yaml::from_str(content)?
        } else {
            serde_json::from_str(content)
                .map_err(|e| LoaderError::validation(format!("Failed to parse JSON: {e}")))?
        };
        // Validate against JSON schema
        validate_config(&value)?;

        // Parse to typed config
        let config: AgentDefinition = if is_yaml {
            serde_yaml::from_str(content)?
        } else {
            serde_json::from_str(content)
                .map_err(|e| LoaderError::validation(format!("Failed to parse JSON: {e}")))?
        };

        Self::validate_business_logic(&config)?;

        let mut builder = self.create_agent_builder(&config).await?;

        // Add tools if configured
        if !config.tools.is_empty() {
            builder = Self::add_tools(builder, &config.tools).await?;
        }

        // Add remote agents if configured
        if let Some(remote_agents_config) = &config.remote_agents {
            builder = self.add_remote_agents(builder, remote_agents_config)?;
        }

        // Add agent card
        builder = Self::add_agent_card(builder, config.card.clone()).await?;

        Ok(builder)
    }

    async fn create_agent_builder(
        &self,
        config: &AgentDefinition,
    ) -> Result<AgentBuilder, LoaderError> {
        // Create LLM with EnvKey - resolution happens at runtime in LLM generate_content
        let builder = match &config.model {
            ModelConfig::Anthropic { name, api_key_env } => {
                let model =
                    crate::models::AnthropicLlm::new(name.clone(), EnvKey::new(api_key_env));
                AgentBuilder::new(config.instruction.clone(), model)
            }
            ModelConfig::OpenAI { name, api_key_env } => {
                let model = crate::models::OpenAILlm::new(name.clone(), EnvKey::new(api_key_env));
                AgentBuilder::new(config.instruction.clone(), model)
            }
            ModelConfig::Gemini { name, api_key_env } => {
                let model = crate::models::GeminiLlm::new(name.clone(), EnvKey::new(api_key_env));
                AgentBuilder::new(config.instruction.clone(), model)
            }
        };

        // Use the complete config from definition (includes max_iterations, timeout, custom_config)
        Ok(builder.with_config(config.to_agent_config()))
    }

    fn validate_business_logic(config: &AgentDefinition) -> Result<(), LoaderError> {
        if config.card.name.is_empty() {
            return Err(LoaderError::validation("Agent name cannot be empty"));
        }

        if config.instruction.is_empty() {
            return Err(LoaderError::validation("Instruction cannot be empty"));
        }

        if config.max_iterations == 0 {
            return Err(LoaderError::validation(
                "max_iterations must be greater than 0",
            ));
        }

        // Reject configurations with function tools
        for tool_provider in &config.tools {
            if let ToolProviderConfig::Function(_) = tool_provider {
                return Err(LoaderError::validation(
                    "Function tools cannot be loaded from configuration files. \
                    Function implementations (closures) are not serializable. \
                    Please add function tools programmatically using AgentBuilder::with_tool(). \
                    The 'function' type in configuration files is export-only for documentation purposes.",
                ));
            }
        }

        Ok(())
    }

    async fn add_tools(
        mut builder: AgentBuilder,
        tool_providers: &[ToolProviderConfig],
    ) -> Result<AgentBuilder, LoaderError> {
        for provider_config in tool_providers {
            match provider_config {
                ToolProviderConfig::Builtin { name } => {
                    match name.as_str() {
                        "update_status" => builder = builder.with_update_status_tool(),
                        "save_artifact" => builder = builder.with_save_artifact_tool(),
                        _ => {
                            // Silently ignore unknown built-in tools
                        }
                    }
                }
                ToolProviderConfig::MCPHttp(mcp_config) => {
                    let toolset = Self::create_mcp_http_toolset(mcp_config).await?;
                    builder = builder.with_toolset(toolset);
                }
                ToolProviderConfig::MCPStdio(mcp_config) => {
                    let toolset = Self::create_mcp_stdio_toolset(mcp_config).await?;
                    builder = builder.with_toolset(toolset);
                }
                ToolProviderConfig::Function(_) => {
                    // This is already validated in `validate_business_logic`,
                    // but we ignore it here as a safeguard.
                }
            }
        }

        Ok(builder)
    }

    async fn create_mcp_http_toolset(config: &MCPHttpConfig) -> Result<MCPToolset, LoaderError> {
        let toolset = MCPToolset::new(
            config.name.clone(),
            MCPConnectionParams::Http {
                url: config.url.clone(),
                headers: config.headers.clone(),
                timeout: std::time::Duration::from_millis(config.timeout),
            },
        );

        Ok(toolset)
    }

    async fn create_mcp_stdio_toolset(config: &MCPStdioConfig) -> Result<MCPToolset, LoaderError> {
        let toolset = MCPToolset::new(
            config.name.clone(),
            MCPConnectionParams::Stdio {
                command: config.command.clone(),
                args: config.args.clone(),
                env: config.env.clone(),
                timeout: std::time::Duration::from_millis(config.timeout),
            },
        );

        Ok(toolset)
    }

    /// Add agent card to the builder
    async fn add_agent_card(
        mut builder: AgentBuilder,
        card: AgentCard,
    ) -> Result<AgentBuilder, LoaderError> {
        builder = builder.with_card(|_| card);
        Ok(builder)
    }

    /// Add remote agents to the builder
    ///
    /// Converts RemoteAgentConfig to AgentCard tuples.
    /// Header values are passed through as-is (not resolved at load time).
    fn add_remote_agents(
        &self,
        mut builder: AgentBuilder,
        remote_agents: &[crate::config::RemoteAgentConfig],
    ) -> Result<AgentBuilder, LoaderError> {
        for remote_config in remote_agents {
            // Create AgentCard from config
            let card = AgentCard::new(
                &remote_config.name,
                &remote_config.description,
                &remote_config.version,
                &remote_config.endpoint,
            );

            // Pass headers through as-is (no resolution at load time)
            // If headers contain env var references, they should be resolved at runtime
            builder = builder.with_remote_agent(card, remote_config.headers.clone());
        }

        Ok(builder)
    }
}

impl Default for AgentLoader {
    fn default() -> Self {
        Self
    }
}
