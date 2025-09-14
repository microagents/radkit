//! Agent Builder - Mutable builder for creating immutable Agents
//!
//! This module provides a builder pattern for constructing agents with
//! clean API that doesn't require Arc wrapping from users.

use crate::events::{EventProcessor, InMemoryEventBus};
use crate::models::BaseLlm;
use crate::sessions::{InMemorySessionService, QueryService, SessionService};
use crate::tools::{BaseTool, BaseToolset, SimpleToolset};
use a2a_types::AgentCard;
use std::sync::Arc;

use super::Agent;
use super::agent_executor::AgentExecutor;
use super::config::AgentConfig;

/// Builder for constructing Agent instances with a fluent API
pub struct AgentBuilder {
    // Core required fields (owned, not Arc)
    instruction: String,
    model: Box<dyn BaseLlm>,

    // Optional execution components
    session_service: Option<Arc<dyn SessionService>>,
    tools: Vec<Arc<dyn BaseTool>>,
    toolset: Option<Arc<dyn BaseToolset>>,
    config: AgentConfig,

    // The AgentCard being built
    agent_card: AgentCard,
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

    /// Add a single tool - accepts owned tool, wraps in Arc
    pub fn with_tool(mut self, tool: impl BaseTool + 'static) -> Self {
        self.tools.push(Arc::new(tool));
        self.toolset = None; // Clear custom toolset if adding individual tools
        self
    }

    /// Add multiple tools at once
    pub fn with_tools(mut self, tools: Vec<Arc<dyn BaseTool>>) -> Self {
        self.tools.extend(tools);
        self.toolset = None;
        self
    }

    /// Set a custom toolset (replaces individual tools)
    pub fn with_toolset(mut self, toolset: impl BaseToolset + 'static) -> Self {
        self.toolset = Some(Arc::new(toolset));
        self.tools.clear(); // Clear individual tools
        self
    }

    /// Set session service - takes Arc for shared ownership
    /// Use Arc::clone() if you need to retain access to the session service
    pub fn with_session_service(mut self, service: Arc<dyn SessionService>) -> Self {
        self.session_service = Some(service);
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
        self.tools.push(Arc::new(tool));
        self
    }

    /// Add the save_artifact built-in tool
    pub fn with_save_artifact_tool(mut self) -> Self {
        let tool = crate::tools::builtin_tools::create_save_artifact_tool();
        self.tools.push(Arc::new(tool));
        self
    }

    /// Add both built-in task management tools
    pub fn with_builtin_task_tools(self) -> Self {
        self.with_update_status_tool().with_save_artifact_tool()
    }

    /// Build the final immutable Agent
    pub fn build(self) -> Agent {
        // Convert owned model to Arc
        let model = Arc::from(self.model);

        // Create toolset - either custom or from individual tools
        let toolset = if let Some(custom_toolset) = self.toolset {
            Some(custom_toolset)
        } else if !self.tools.is_empty() {
            // Tools are already Arc<dyn BaseTool>, no conversion needed
            Some(Arc::new(SimpleToolset::new(self.tools)) as Arc<dyn BaseToolset>)
        } else {
            None
        };

        // Create or use provided session service
        let session_service = self
            .session_service
            .unwrap_or_else(|| Arc::new(InMemorySessionService::new()));

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
}
