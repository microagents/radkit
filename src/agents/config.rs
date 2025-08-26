//! Agent configuration types and utilities
//!
//! This module contains configuration structures and helper types for agent setup,
//! execution parameters, and authentication contexts.

use serde_json::Value;
use std::collections::HashMap;

use crate::a2a::{MessageSendParams, Task, TaskQueryParams};
use crate::agents::Agent;
use crate::errors::AgentResult;

/// Configuration for agent execution behavior
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of tool calling iterations to prevent infinite loops
    pub max_iterations: usize,
    /// Timeout for the entire execution in seconds
    pub timeout_seconds: Option<u64>,
    /// Additional custom configuration
    pub custom_config: HashMap<String, Value>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            timeout_seconds: Some(300), // 5 minutes
            custom_config: HashMap::new(),
        }
    }
}

impl AgentConfig {
    /// Create a new config with custom max iterations
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Create a new config with custom timeout
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    /// Create a new config without timeout
    pub fn without_timeout(mut self) -> Self {
        self.timeout_seconds = None;
        self
    }

    /// Add custom configuration value
    pub fn with_custom_config(mut self, key: String, value: Value) -> Self {
        self.custom_config.insert(key, value);
        self
    }
}

/// Authenticated agent wrapper that provides app_name and user_id context
/// This wrapper implements the A2A protocol's security model where all operations
/// require both application and user identification for proper isolation.
#[derive(Debug)]
pub struct AuthenticatedAgent<'a> {
    agent: &'a Agent,
    app_name: String,
    user_id: String,
}

impl<'a> AuthenticatedAgent<'a> {
    /// Create a new authenticated agent wrapper
    pub fn new(agent: &'a Agent, app_name: String, user_id: String) -> Self {
        Self {
            agent,
            app_name,
            user_id,
        }
    }

    /// A2A Protocol: message/send - Send message to agent (non-streaming)
    pub async fn send_message(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<super::execution_result::SendMessageResultWithEvents> {
        self.agent
            .send_message(self.app_name.clone(), self.user_id.clone(), params)
            .await
    }

    /// A2A Protocol: message/stream - Send message with streaming updates
    pub async fn send_streaming_message(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<super::execution_result::SendStreamingMessageResultWithEvents> {
        self.agent
            .send_streaming_message(self.app_name.clone(), self.user_id.clone(), params)
            .await
    }

    /// A2A Protocol: tasks/get - Get task by ID using bound authentication context
    pub async fn get_task(&self, params: TaskQueryParams) -> AgentResult<Task> {
        self.agent
            .get_task(&self.app_name, &self.user_id, params)
            .await
    }

    /// A2A Protocol: List all tasks using bound authentication context  
    pub async fn list_tasks(&self) -> AgentResult<Vec<Task>> {
        self.agent.list_tasks(&self.app_name, &self.user_id).await
    }

    /// Get the app name for this authenticated context
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Get the user ID for this authenticated context
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Get the underlying agent reference
    pub fn agent(&self) -> &Agent {
        self.agent
    }
}
