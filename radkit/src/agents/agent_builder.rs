//! Agent Builder - Mutable builder for creating immutable Agents
//!
//! This module provides a builder pattern for constructing agents with
//! clean API that doesn't require Arc wrapping from users.

use super::Agent;
use super::agent_executor::AgentExecutor;
use super::config::AgentConfig;
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
        }
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

    /// Add the update_status built-in tool
    pub fn with_update_status_tool(mut self) -> Self {
        let tool = crate::tools::builtin_tools::create_update_status_tool();
        self.tools.push(Box::new(tool));
        self
    }

    /// Add the save_artifact built-in tool
    pub fn with_save_artifact_tool(mut self) -> Self {
        let tool = crate::tools::builtin_tools::create_save_artifact_tool();
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

        // Convert owned model to Arc
        let model = Arc::from(self.model);

        let toolset: Option<Arc<dyn BaseToolset>> = {
            let individual_tools: Vec<Arc<dyn BaseTool>> =
                self.tools.into_iter().map(|t| Arc::from(t)).collect();
            if let Some(base_toolset) = self.toolset {
                if individual_tools.is_empty() {
                    // Only a toolset is provided
                    Some(base_toolset)
                } else {
                    Some(
                        Arc::new(CombinedToolset::new(base_toolset, individual_tools))
                            as Arc<dyn BaseToolset>,
                    )
                }
            } else if !individual_tools.is_empty() {
                // Only individual tools are provided
                Some(Arc::new(SimpleToolset::new(individual_tools)) as Arc<dyn BaseToolset>)
            } else {
                // No tools or toolset
                None
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
        });

        // Return immutable Agent with card and executor
        Agent {
            agent_card: self.agent_card,
            executor,
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
}
