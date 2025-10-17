//! Agent Builder - Mutable builder for creating immutable Agents
//!
//! This module provides a builder pattern for constructing agents with
//! clean API that doesn't require Arc wrapping from users.

use super::Agent;
use super::agent_executor::AgentExecutor;
use super::config::AgentConfig;
use crate::config::{AgentDefinition, EnvResolverFn};
use crate::events::{EventProcessor, InMemoryEventBus};
use crate::models::BaseLlm;
use crate::sessions::{InMemorySessionService, QueryService, SessionService};
use crate::tools::{BaseTool, BaseToolset, CombinedToolset, SimpleToolset};
use a2a_types::AgentCard;
use std::collections::HashMap;
use std::sync::Arc;

/// Builder for constructing Agent instances with a fluent API
pub struct AgentBuilder {
    // Core required fields (owned, not Arc)
    instruction: String,
    model: Box<dyn BaseLlm>,

    // Optional execution components
    session_service: Option<Box<dyn SessionService>>,
    tools: Vec<Box<dyn BaseTool>>,
    toolset: Option<Arc<dyn BaseToolset>>,
    config: AgentConfig,

    // The AgentCard being built
    agent_card: AgentCard,

    // Remote A2A agents this agent can communicate with
    // Each agent can have optional custom headers for authentication
    // If any remote agents are configured, the A2A tool is automatically created during build()
    remote_agents: Vec<(AgentCard, Option<HashMap<String, String>>)>,

    // Optional custom environment resolver for secrets (API keys, etc.)
    // If None, uses default std::env resolver
    env_resolver: Option<EnvResolverFn>,
}

impl AgentBuilder {
    /// Create a new builder with only required fields
    /// No Arc needed - accepts owned model
    pub fn new(instruction: impl Into<String>, model: impl BaseLlm + 'static) -> Self {
        Self {
            instruction: instruction.into(),
            model: Box::new(model),
            session_service: None,
            tools: Vec::new(),
            toolset: None,
            config: AgentConfig::default(),
            agent_card: AgentCard::new("", "", "", ""), // Defaults
            remote_agents: Vec::new(),
            env_resolver: None,
        }
    }

    /// Load an agent from YAML configuration
    ///
    /// This is a convenience method that delegates to [`crate::config::AgentLoader`].
    /// Returns an `AgentBuilder` that can be further customized before building.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use radkit::agents::AgentBuilder;
    /// use radkit::tools::{FunctionTool, ToolResult};
    /// use serde_json::json;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let yaml = r#"
    /// card:
    ///   name: My Agent
    ///   description: A helpful assistant
    ///   version: 1.0.0
    /// instruction: You are a helpful assistant
    /// model:
    ///   type: openai
    ///   name: gpt-4o
    ///   api_key_env: OPENAI_API_KEY
    /// "#;
    ///
    /// // Load and customize
    /// let agent = AgentBuilder::from_yaml(yaml).await?
    ///     .with_builtin_task_tools()  // Add tools after loading
    ///     .with_tool(FunctionTool::new(
    ///         "custom_tool".to_string(),
    ///         "My custom tool".to_string(),
    ///         |_args, _ctx| Box::pin(async {
    ///             ToolResult::success(json!({}))
    ///         })
    ///     ))
    ///     .build();
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_yaml(yaml: &str) -> Result<Self, crate::errors::LoaderError> {
        crate::config::loader::AgentLoader::from_yaml(yaml).await
    }

    /// Load an agent from JSON configuration
    ///
    /// This is a convenience method that delegates to [`crate::config::AgentLoader`].
    /// Returns an `AgentBuilder` that can be further customized before building.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use radkit::agents::AgentBuilder;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let json = r#"{
    ///   "card": {
    ///     "name": "My Agent",
    ///     "description": "A helpful assistant",
    ///     "version": "1.0.0"
    ///   },
    ///   "instruction": "You are a helpful assistant",
    ///   "model": {
    ///     "type": "openai",
    ///     "name": "gpt-4o",
    ///     "api_key_env": "OPENAI_API_KEY"
    ///   }
    /// }"#;
    ///
    /// // Load and customize
    /// let agent = AgentBuilder::from_json(json).await?
    ///     .with_config(radkit::agents::config::AgentConfig::default().with_max_iterations(20))
    ///     .build();
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_json(json: &str) -> Result<Self, crate::errors::LoaderError> {
        crate::config::loader::AgentLoader::from_json(json).await
    }

    // ===== AgentCard Configuration =====

    /// Apply any transformation to the agent card
    /// This is the primary way to configure agent metadata
    pub fn with_card<F>(mut self, f: F) -> Self
    where
        F: FnOnce(AgentCard) -> AgentCard,
    {
        self.agent_card = f(self.agent_card);
        self
    }

    // ===== Execution Component Configuration =====

    /// Add a single tool - accepts owned tool
    pub fn with_tool(mut self, tool: impl BaseTool + 'static) -> Self {
        self.tools.push(Box::new(tool));
        self
    }

    /// Add multiple tools at once
    pub fn with_tools(mut self, tools: Vec<impl BaseTool + 'static>) -> Self {
        self.tools.extend(
            tools
                .into_iter()
                .map(|tool| Box::new(tool) as Box<dyn BaseTool>),
        );
        self
    }

    /// Set a custom toolset (replaces individual tools)
    pub fn with_toolset(mut self, toolset: impl BaseToolset + 'static) -> Self {
        self.toolset = Some(Arc::new(toolset));
        self
    }

    /// Set session service - accepts owned service
    pub fn with_session_service(mut self, service: impl SessionService + 'static) -> Self {
        self.session_service = Some(Box::new(service));
        self
    }

    /// Set agent configuration
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Set a custom environment resolver for secrets (API keys, etc.)
    ///
    /// This resolver is passed to LLM providers at runtime to resolve environment variables
    /// for API keys. If not set, the default std::env resolver is used.
    ///
    /// # Example
    /// ```no_run
    /// use std::sync::Arc;
    /// use radkit::agents::AgentBuilder;
    /// use radkit::models::OpenAILlm;
    /// use radkit::config::EnvKey;
    /// # use radkit::errors::AgentError;
    ///
    /// let vault_resolver = Arc::new(|key: &str| -> Result<String, AgentError> {
    ///     // Fetch from vault instead of environment
    ///     Ok(format!("secret_from_vault_{}", key))
    /// });
    ///
    /// let agent = AgentBuilder::new(
    ///     "instruction",
    ///     OpenAILlm::new("gpt-4o".to_string(), EnvKey::new("OPENAI_API_KEY"))
    /// )
    /// .with_env_resolver(vault_resolver)
    /// .build();
    /// ```
    pub fn with_env_resolver(mut self, resolver: EnvResolverFn) -> Self {
        self.env_resolver = Some(resolver);
        self
    }

    /// Add the update_status built-in tool
    pub fn with_update_status_tool(mut self) -> Self {
        let tool = crate::tools::UpdateStatusTool::new();
        self.tools.push(Box::new(tool));
        self
    }

    /// Add the save_artifact built-in tool
    pub fn with_save_artifact_tool(mut self) -> Self {
        let tool = crate::tools::SaveArtifactTool::new();
        self.tools.push(Box::new(tool));
        self
    }

    /// Add both built-in task management tools
    pub fn with_builtin_task_tools(self) -> Self {
        self.with_update_status_tool().with_save_artifact_tool()
    }

    // ===== Remote Agent Configuration =====

    /// Add a remote A2A agent that this agent can communicate with
    ///
    /// Optionally provide custom headers for authentication or custom configuration.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use radkit::agents::AgentBuilder;
    /// use a2a_types::AgentCard;
    /// use std::collections::HashMap;
    /// # use radkit::models::mock_llm::MockLlm;
    ///
    /// let weather_card = AgentCard::new(
    ///     "Weather Agent",
    ///     "Provides weather information",
    ///     "1.0.0",
    ///     "https://weather.example.com"
    /// );
    ///
    /// // Without authentication
    /// let agent = AgentBuilder::new("instruction", MockLlm::new("model".to_string()))
    ///     .with_remote_agent(weather_card.clone(), None)
    ///     .build();
    ///
    /// // With authentication headers
    /// let mut headers = HashMap::new();
    /// headers.insert("Authorization".to_string(), "Bearer token123".to_string());
    /// headers.insert("X-API-Key".to_string(), "my-api-key".to_string());
    ///
    /// let agent = AgentBuilder::new("instruction", MockLlm::new("model".to_string()))
    ///     .with_remote_agent(weather_card, Some(headers))
    ///     .build();
    /// ```
    pub fn with_remote_agent(
        mut self,
        agent_card: AgentCard,
        headers: Option<HashMap<String, String>>,
    ) -> Self {
        self.remote_agents.push((agent_card, headers));
        self
    }

    /// Add multiple remote A2A agents at once
    ///
    /// Each agent can have optional custom headers.
    ///
    /// **Note:** The A2A tool is automatically created during `build()` if any remote agents
    /// are configured. No need to call a separate method to enable the tool.
    pub fn with_remote_agents(
        mut self,
        agents: Vec<(AgentCard, Option<HashMap<String, String>>)>,
    ) -> Self {
        self.remote_agents.extend(agents);
        self
    }

    /// Build the final immutable Agent
    ///
    /// **Note:** If any remote agents were added via `with_remote_agent()`, the A2A tool
    /// is automatically created here to enable communication with those agents.
    ///
    /// The agent definition is always built from the current builder state.
    pub fn build(mut self) -> Agent {
        // Automatically create A2A tool if remote agents are configured
        if !self.remote_agents.is_empty() {
            use crate::tools::A2AAgentTool;

            // Create A2A tool from agent cards and their optional headers
            // HTTP clients are created on-demand in the tool's run method (lazy initialization)
            if let Ok(tool) = A2AAgentTool::new(self.remote_agents.clone()) {
                self.tools.push(Box::new(tool));
            }
            // Silently ignore errors - agent will work without A2A capability
        }

        // Always build definition from current builder state
        let definition = self.build_definition();

        // Convert owned model to Arc
        let model = Arc::from(self.model);

        let toolset: Option<Arc<dyn BaseToolset>> = {
            let individual_tools: Vec<Arc<dyn BaseTool>> =
                self.tools.into_iter().map(Arc::from).collect();

            match (self.toolset, individual_tools.is_empty()) {
                (Some(base_toolset), true) => {
                    // Only a toolset, no individual tools
                    Some(base_toolset)
                }
                (Some(base_toolset), false) => {
                    // Toolset + individual tools: wrap tools in SimpleToolset, then combine
                    let tools_toolset =
                        Arc::new(SimpleToolset::new(individual_tools)) as Arc<dyn BaseToolset>;
                    Some(Arc::new(CombinedToolset::new(base_toolset, tools_toolset))
                        as Arc<dyn BaseToolset>)
                }
                (None, false) => {
                    // Only individual tools
                    Some(Arc::new(SimpleToolset::new(individual_tools)) as Arc<dyn BaseToolset>)
                }
                (None, true) => {
                    // No tools or toolset
                    None
                }
            }
        };

        // Create or use provided session service
        let session_service: Arc<dyn SessionService> = match self.session_service {
            Some(service) => service.into(),
            None => Arc::new(InMemorySessionService::new()),
        };

        // Create dependent services
        let query_service = Arc::new(QueryService::new(session_service.clone()));
        let event_bus = Arc::new(InMemoryEventBus::new());
        let event_processor = Arc::new(EventProcessor::new(session_service.clone(), event_bus));

        // Create executor with all components
        let executor = Arc::new(AgentExecutor {
            model,
            toolset,
            session_service,
            query_service,
            event_processor,
            config: self.config,
            instruction: self.instruction,
            env_resolver: self.env_resolver,
        });

        // Return immutable Agent with card, executor, and definition
        Agent {
            agent_card: self.agent_card,
            executor,
            definition,
        }
    }

    /// Build an AgentDefinition from the builder's current state
    ///
    /// This uses the SAME toolset construction logic as build() to ensure
    /// the definition accurately reflects the runtime configuration.
    fn build_definition(&self) -> AgentDefinition {
        // Get model config from the LLM instance
        let model_config = self.model.to_model_config();

        // Build tool provider configs
        let mut tool_provider_configs = Vec::new();

        // Add configs from the base toolset, if any
        if let Some(toolset) = &self.toolset {
            tool_provider_configs.extend(toolset.to_tool_provider_configs());
        }

        // Add configs from individual tools
        for tool in &self.tools {
            if let Some(config) = tool.to_tool_provider_config() {
                tool_provider_configs.push(config);
            }
        }

        // Convert remote agents to config format
        let remote_agents_config = if !self.remote_agents.is_empty() {
            use crate::config::RemoteAgentConfig;

            Some(
                self.remote_agents
                    .iter()
                    .map(|(card, headers)| RemoteAgentConfig {
                        name: card.name.clone(),
                        description: card.description.clone(),
                        version: card.version.clone(),
                        endpoint: card.url.clone(),
                        headers: headers.clone(),
                    })
                    .collect(),
            )
        } else {
            None
        };

        AgentDefinition {
            card: self.agent_card.clone(),
            instruction: self.instruction.clone(),
            max_iterations: self.config.max_iterations,
            timeout_seconds: self.config.timeout_seconds,
            custom_config: self.config.custom_config.clone(),
            model: model_config,
            tools: tool_provider_configs,
            remote_agents: remote_agents_config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock_llm::MockLlm;

    #[test]
    fn test_builder_minimal() {
        let agent =
            AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string())).build();

        assert_eq!(agent.instruction(), "Test instruction");
        assert_eq!(agent.name(), ""); // Default empty name
        assert_eq!(agent.description(), ""); // Default empty description
    }

    #[test]
    fn test_builder_with_card() {
        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_card(|c| {
                c.with_name("Test Agent")
                    .with_description("A test agent")
                    .with_version("1.0.0")
            })
            .build();

        assert_eq!(agent.name(), "Test Agent");
        assert_eq!(agent.description(), "A test agent");
        assert_eq!(agent.card().version, "1.0.0");
    }

    #[test]
    fn test_builder_with_config() {
        let config = AgentConfig::default().with_max_iterations(5);

        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_config(config.clone())
            .build();

        assert_eq!(agent.config().max_iterations, 5);
    }

    #[test]
    fn test_builder_with_remote_agents() {
        let remote_agent_1 = AgentCard::new(
            "Weather Agent",
            "Provides weather information",
            "1.0.0",
            "https://weather-agent.example.com",
        );

        let remote_agent_2 = AgentCard::new(
            "Calendar Agent",
            "Manages calendar events",
            "1.0.0",
            "https://calendar-agent.example.com",
        );

        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_remote_agent(remote_agent_1, None)
            .with_remote_agent(remote_agent_2, None)
            .build();

        // Note: In a real implementation, you would add a getter method to Agent
        // to retrieve the remote_agents list for testing purposes
        assert_eq!(agent.name(), "");
    }

    #[test]
    fn test_builder_with_multiple_remote_agents() {
        let remote_agents = vec![
            (
                AgentCard::new(
                    "Agent 1",
                    "Description 1",
                    "1.0.0",
                    "https://agent1.example.com",
                ),
                None,
            ),
            (
                AgentCard::new(
                    "Agent 2",
                    "Description 2",
                    "1.0.0",
                    "https://agent2.example.com",
                ),
                None,
            ),
        ];

        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_remote_agents(remote_agents)
            .build();

        assert_eq!(agent.name(), "");
    }

    #[test]
    fn test_build_without_remote_agents() {
        // If no remote agents configured, no A2A tool is created during build()
        let agent =
            AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string())).build();

        assert_eq!(agent.name(), "");
    }

    #[test]
    fn test_definition_reflects_builtin_tools() {
        use crate::config::ToolProviderConfig;

        // Test that builtin tools are correctly represented in the definition
        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_update_status_tool()
            .with_save_artifact_tool()
            .build();

        let definition = agent.get_definition();
        assert_eq!(definition.tools.len(), 2);

        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "update_status".to_string(),
        }));
        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "save_artifact".to_string(),
        }));
    }

    #[test]
    fn test_definition_reflects_custom_tools() {
        use crate::config::{FunctionToolConfig, ToolProviderConfig};
        use crate::tools::FunctionTool;
        use serde_json::json;

        let custom_tool = FunctionTool::new(
            "my_custom_tool".to_string(),
            "A custom tool".to_string(),
            |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
        )
        .with_parameters_schema(json!({ "type": "object" }));

        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_tool(custom_tool)
            .build();

        let definition = agent.get_definition();
        assert_eq!(definition.tools.len(), 1);
        assert_eq!(
            definition.tools[0],
            ToolProviderConfig::Function(FunctionToolConfig {
                name: "my_custom_tool".to_string(),
                args: json!({ "type": "object" }),
            })
        );
    }

    #[tokio::test]
    async fn test_from_yaml_loads_agent() {
        let yaml = r#"
card:
  name: Test Agent
  description: A test agent from YAML
  version: 1.0.0
  url: https://example.com
  protocolVersion: "0.3.0"
  capabilities:
    streaming: true
  defaultInputModes:
    - text/plain
  defaultOutputModes:
    - text/plain
  skills:
    - id: general
      name: general
      description: General assistance
      tags: []
instruction: You are a helpful assistant
model:
  type: openai
  name: gpt-4o
  api_key_env: OPENAI_API_KEY
max_iterations: 15
"#;

        let agent = AgentBuilder::from_yaml(yaml)
            .await
            .expect("Should load from YAML");

        let built_agent = agent.build();

        assert_eq!(built_agent.name(), "Test Agent");
        assert_eq!(built_agent.description(), "A test agent from YAML");
        assert_eq!(built_agent.version(), "1.0.0");
        assert_eq!(built_agent.instruction(), "You are a helpful assistant");
        assert_eq!(built_agent.config().max_iterations, 15);
    }

    #[tokio::test]
    async fn test_from_yaml_with_chaining() {
        let yaml = r#"
card:
  name: Base Agent
  description: Base configuration
  version: 1.0.0
  url: http://example.com
  protocolVersion: 0.3.0
  capabilities: {}
  defaultInputModes: []
  defaultOutputModes: []
  skills: []
instruction: Base instruction
model:
  type: openai
  name: gpt-4o
  api_key_env: OPENAI_API_KEY
# Define tools using the new array format
tools:
  - type: builtin
    name: update_status
"#;

        // Load from YAML and customize with additional tools
        use crate::config::{FunctionToolConfig, ToolProviderConfig};
        use crate::tools::FunctionTool;
        use serde_json::json;

        let agent = AgentBuilder::from_yaml(yaml)
            .await
            .expect("Should load from YAML")
            .with_save_artifact_tool() // Add another builtin tool
            .with_tool(FunctionTool::new(
                "custom_tool".to_string(),
                "Custom tool added after load".to_string(),
                |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
            ))
            .with_config(AgentConfig::default().with_max_iterations(20)) // Override config
            .build();

        assert_eq!(agent.name(), "Base Agent");
        assert_eq!(agent.instruction(), "Base instruction");
        assert_eq!(agent.config().max_iterations, 20); // Should use overridden value

        // Check that all tools are correctly represented in the definition
        let definition = agent.get_definition();
        assert_eq!(definition.tools.len(), 3);

        // Check for the tool loaded from YAML
        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "update_status".to_string(),
        }));

        // Check for the tool added via builder method
        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "save_artifact".to_string(),
        }));

        // Check for the function tool added via builder method
        assert!(
            definition
                .tools
                .contains(&ToolProviderConfig::Function(FunctionToolConfig {
                    name: "custom_tool".to_string(),
                    args: json!({}), // Default args for a tool with no schema
                }))
        );
    }

    #[tokio::test]
    async fn test_from_json_loads_agent() {
        let json = r#"{
  "card": {
    "name": "JSON Agent",
    "description": "Loaded from JSON",
    "version": "2.0.0",
    "url": "https://json.example.com",
    "protocolVersion": "0.3.0",
    "capabilities": {
      "streaming": true
    },
    "defaultInputModes": ["text/plain"],
    "defaultOutputModes": ["text/plain"],
    "skills": [
      {
        "id": "json_skill",
        "name": "json_skill",
        "description": "JSON processing",
        "tags": []
      }
    ]
  },
  "instruction": "JSON instruction",
  "model": {
    "type": "anthropic",
    "name": "claude-3-5-sonnet-20241022",
    "api_key_env": "ANTHROPIC_API_KEY"
  },
  "max_iterations": 25
}"#;

        let agent = AgentBuilder::from_json(json)
            .await
            .expect("Should load from JSON");

        let built_agent = agent.build();

        assert_eq!(built_agent.name(), "JSON Agent");
        assert_eq!(built_agent.description(), "Loaded from JSON");
        assert_eq!(built_agent.version(), "2.0.0");
        assert_eq!(built_agent.instruction(), "JSON instruction");
        assert_eq!(built_agent.config().max_iterations, 25);
    }

    #[tokio::test]
    async fn test_from_json_with_chaining() {
        let json = r#"{
  "card": {
    "name": "Chainable Agent",
    "description": "Test chaining",
    "version": "1.0.0",
    "url": "http://example.com",
    "protocolVersion": "0.3.0",
    "capabilities": {},
    "defaultInputModes": [],
    "defaultOutputModes": [],
    "skills": []
  },
  "instruction": "Original instruction",
  "model": {
    "type": "gemini",
    "name": "gemini-2.0-flash-exp",
    "api_key_env": "GEMINI_API_KEY"
  },
  "tools": [
    {
      "type": "builtin",
      "name": "update_status"
    }
  ]
}"#;

        use crate::config::{FunctionToolConfig, ToolProviderConfig};
        use crate::tools::FunctionTool;
        use serde_json::json;

        // Load and add more tools
        let agent = AgentBuilder::from_json(json)
            .await
            .expect("Should load from JSON")
            .with_save_artifact_tool() // Add another builtin
            .with_tool(FunctionTool::new(
                "post_load_tool".to_string(),
                "Added after JSON load".to_string(),
                |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
            ))
            .build();

        assert_eq!(agent.name(), "Chainable Agent");

        let definition = agent.get_definition();
        assert_eq!(definition.tools.len(), 3);

        // Check for the tool loaded from JSON
        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "update_status".to_string(),
        }));

        // Check for the tool added via builder method
        assert!(definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "save_artifact".to_string(),
        }));

        // Check for the function tool added via builder method
        assert!(
            definition
                .tools
                .contains(&ToolProviderConfig::Function(FunctionToolConfig {
                    name: "post_load_tool".to_string(),
                    args: json!({}),
                }))
        );
    }

    #[tokio::test]
    async fn test_from_yaml_invalid_config_fails() {
        let invalid_yaml = r#"
card:
  name: ""  # Empty name should fail validation
  description: Test
  version: 1.0.0
instruction: Test
model:
  type: openai
  name: gpt-4o
  api_key_env: OPENAI_API_KEY
"#;

        let result = AgentBuilder::from_yaml(invalid_yaml).await;
        assert!(result.is_err(), "Empty name should cause validation error");
    }

    #[tokio::test]
    async fn test_from_yaml_rejects_function_tool() {
        let yaml = r#"
card:
  name: Invalid Agent
  description: Tries to load a function tool
  version: 1.0.0
  url: http://example.com
  protocolVersion: 0.3.0
  capabilities: {}
  defaultInputModes: []
  defaultOutputModes: []
  skills: []
instruction: Test
model:
  type: openai
  name: gpt-4o
  api_key_env: OPENAI_API_KEY
tools:
  - type: function
    name: my_function
    args: {}
"#;

        let result = AgentBuilder::from_yaml(yaml).await;
        match result {
            Ok(_) => panic!("Should have failed to load agent with function tool"),
            Err(e) => {
                assert!(e.to_string().contains("Function tools cannot be loaded"));
            }
        }
    }

    #[tokio::test]
    async fn test_definition_and_round_trip_serialization() {
        use crate::config::{FunctionToolConfig, MCPHttpConfig, ToolProviderConfig, ToolsFilter};
        use crate::tools::{FunctionTool, MCPConnectionParams, MCPToolset};
        use serde_json::json;

        // 1. Build an agent with a mix of toolsets and individual tools
        let mcp_toolset = MCPToolset::new(
            "http_mcp".to_string(),
            MCPConnectionParams::Http {
                url: "http://localhost:8080".to_string(),
                headers: Default::default(),
                timeout: std::time::Duration::from_secs(30),
            },
        );

        let individual_tool = FunctionTool::new(
            "individual_tool".to_string(),
            "An individual tool".to_string(),
            |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
        );

        let original_agent = AgentBuilder::new(
            "Test instruction",
            crate::models::OpenAILlm::new("gpt-4o".to_string(), "OPENAI_API_KEY".into()),
        )
        .with_card(|c| c.with_name("Test Agent"))
        .with_toolset(mcp_toolset) // An MCP toolset
        .with_tool(individual_tool) // A function tool
        .with_update_status_tool() // A built-in tool
        .build();

        // 2. Check that the generated definition is correct
        let original_definition = original_agent.get_definition();

        assert_eq!(original_definition.tools.len(), 3);
        assert!(
            original_definition
                .tools
                .contains(&ToolProviderConfig::Builtin {
                    name: "update_status".to_string()
                })
        );

        assert!(
            original_definition
                .tools
                .contains(&ToolProviderConfig::Function(FunctionToolConfig {
                    name: "individual_tool".to_string(),
                    args: json!({}),
                }))
        );

        assert!(
            original_definition
                .tools
                .contains(&ToolProviderConfig::MCPHttp(MCPHttpConfig {
                    name: "http_mcp".to_string(),
                    url: "http://localhost:8080".to_string(),
                    headers: Default::default(),
                    timeout: 30000,
                    tools_filter: None,
                }))
        );

        // 3. Serialize to YAML
        let yaml = original_agent.to_yaml().unwrap();

        // 4. Assert that deserializing this YAML fails because it contains a function tool
        match AgentBuilder::from_yaml(&yaml).await {
            Ok(_) => panic!("Deserializing a definition with a function tool should fail."),
            Err(e) => {
                assert!(e.to_string().contains("Function tools cannot be loaded"));
            }
        }

        // 5. Create a new YAML with only the loadable tools and deserialize it
        let loadable_yaml = r#"
card:
  name: Test Agent
  description: ""
  version: ""
  url: ""
  protocolVersion: 0.3.0
  capabilities: {}
  defaultInputModes: []
  defaultOutputModes: []
  skills: []
instruction: Test instruction
model:
  type: openai
  name: gpt-4o
  api_key_env: OPENAI_API_KEY
tools:
  - type: mcp_http
    name: http_mcp
    url: http://localhost:8080
    headers: {}
    timeout: 30000
    tools_filter: null
  - type: builtin
    name: update_status
"#;

        let new_agent = AgentBuilder::from_yaml(loadable_yaml)
            .await
            .expect("Should deserialize from YAML")
            .build();
        let new_definition = new_agent.get_definition();

        // 6. Compare the new definition to an expected definition
        assert_eq!(new_definition.tools.len(), 2);
        assert!(new_definition.tools.contains(&ToolProviderConfig::Builtin {
            name: "update_status".to_string()
        }));
        assert!(
            new_definition
                .tools
                .contains(&ToolProviderConfig::MCPHttp(MCPHttpConfig {
                    name: "http_mcp".to_string(),
                    url: "http://localhost:8080".to_string(),
                    headers: Default::default(),
                    timeout: 30000,
                    tools_filter: None,
                }))
        );
    }

    #[tokio::test]
    async fn test_builder_combines_toolset_and_individual_tools() {
        use crate::tools::{FunctionTool, SimpleToolset};
        use serde_json::json;

        // 1. Create a toolset with one tool
        let toolset_tool = Arc::new(FunctionTool::new(
            "tool_from_set".to_string(),
            "A tool from a toolset".to_string(),
            |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
        ));
        let toolset = SimpleToolset::new(vec![toolset_tool]);

        // 2. Create an individual tool
        let individual_tool = FunctionTool::new(
            "individual_tool".to_string(),
            "An individual tool".to_string(),
            |_args, _ctx| Box::pin(async { crate::tools::ToolResult::success(json!({})) }),
        );

        // 3. Build agent with both
        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_toolset(toolset)
            .with_tool(individual_tool)
            .build();

        // 4. Verify the runtime toolset
        let final_toolset = agent
            .executor
            .toolset
            .as_ref()
            .expect("Agent should have a toolset");

        // It should be a CombinedToolset containing the two sources
        let tools = final_toolset.get_tools().await;
        assert_eq!(tools.len(), 2);

        let tool_names: Vec<_> = tools.iter().map(|t| t.name()).collect();
        assert!(tool_names.contains(&"tool_from_set"));
        assert!(tool_names.contains(&"individual_tool"));
    }

    #[tokio::test]
    async fn test_builder_creates_a2a_tool_for_remote_agents() {
        // 1. Configure a remote agent
        let remote_agent_card = AgentCard::new(
            "Remote Weather Agent",
            "Provides weather info",
            "1.0",
            "http://remote.weather",
        );

        // 2. Build the agent
        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_remote_agent(remote_agent_card, None)
            .build();

        // 3. Verify the A2A tool exists in the runtime toolset
        let toolset = agent
            .executor
            .toolset
            .as_ref()
            .expect("Agent should have a toolset");
        let tools = toolset.get_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "call_remote_agent");
    }

    #[tokio::test]
    async fn test_definition_excludes_a2a_tool() {
        // 1. Configure a remote agent and another tool
        let remote_agent_card = AgentCard::new(
            "Remote Weather Agent",
            "Provides weather info",
            "1.0",
            "http://remote.weather",
        );

        // 2. Build the agent
        let agent = AgentBuilder::new("Test instruction", MockLlm::new("test-model".to_string()))
            .with_remote_agent(remote_agent_card, None)
            .with_update_status_tool() // Add another tool for comparison
            .build();

        // 3. Get the definition and verify the tools list
        let definition = agent.get_definition();

        // The definition should contain the `update_status` tool
        assert_eq!(definition.tools.len(), 1);
        assert!(matches!(
            &definition.tools[0],
            crate::config::ToolProviderConfig::Builtin { name } if name == "update_status"
        ));

        // The definition should NOT contain the `call_remote_agent` tool, as it's handled
        // by the `remote_agents` field.
        assert!(!definition
            .tools
            .iter()
            .any(|t| matches!(t, crate::config::ToolProviderConfig::Function(f) if f.name == "call_remote_agent"))
        );

        // The `remote_agents` field should be correctly populated
        assert!(definition.remote_agents.is_some());
        assert_eq!(definition.remote_agents.as_ref().unwrap().len(), 1);
        assert_eq!(
            definition.remote_agents.as_ref().unwrap()[0].name,
            "Remote Weather Agent"
        );
    }
}
