//! A2A Agent Tool - Remote Agent Communication
//!
//! This module provides tools for Radkit agents to communicate with remote A2A-compliant agents.
//! The tool handles routing, context tracking, and multi-turn conversations with remote agents.

use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult};
use a2a_client::A2AClient;
use a2a_types::{
    AgentCard, Message, MessageRole, MessageSendParams, Part, SendMessageResponse,
    SendMessageResult, SendStreamingMessageResult, TaskState,
};
use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Tracks remote agent context across calls
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteContextInfo {
    /// Remote agent's `context_id` (A2A protocol)
    remote_context_id: Option<String>,
    /// Most recent remote `task_id`
    remote_task_id: Option<String>,
    /// When we last called this agent
    last_call: Option<String>,
    /// Number of messages exchanged
    message_count: u32,
    /// Remote endpoint for reference
    endpoint: String,
    /// When context was created
    created_at: String,
}

impl RemoteContextInfo {
    fn new(endpoint: String) -> Self {
        Self {
            remote_context_id: None,
            remote_task_id: None,
            last_call: None,
            message_count: 0,
            endpoint,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    fn update_from_response(&mut self, response: &SendMessageResponse) {
        // Extract context_id and task_id from response
        match response {
            SendMessageResponse::Success(success) => match &success.result {
                SendMessageResult::Task(task) => {
                    self.remote_context_id = Some(task.context_id.clone());
                    self.remote_task_id = Some(task.id.clone());
                }
                SendMessageResult::Message(msg) => {
                    if let Some(ctx) = &msg.context_id {
                        self.remote_context_id = Some(ctx.clone());
                    }
                    if let Some(task) = &msg.task_id {
                        self.remote_task_id = Some(task.clone());
                    }
                }
            },
            SendMessageResponse::Error(_) => {
                // Error response, don't update context
            }
        }
        self.last_call = Some(Utc::now().to_rfc3339());
        self.message_count += 1;
    }
}

/// Tool for calling remote A2A agents
pub struct A2AAgentTool {
    /// Map of `agent_name` -> `AgentCard` (for metadata and client creation)
    agent_cards: HashMap<String, AgentCard>,
    /// Map of `agent_name` -> Optional headers for authentication
    agent_headers: HashMap<String, Option<HashMap<String, String>>>,
}

impl std::fmt::Debug for A2AAgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("A2AAgentTool")
            .field("agent_names", &self.agent_cards.keys().collect::<Vec<_>>())
            .field("agent_cards", &self.agent_cards)
            .finish()
    }
}

impl A2AAgentTool {
    /// Create tool from agent cards with optional custom headers
    ///
    /// Each agent can have optional custom headers for authentication.
    /// HTTP clients are created on-demand during tool execution.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use radkit::tools::A2AAgentTool;
    /// use a2a_types::AgentCard;
    /// use std::collections::HashMap;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let weather_card = AgentCard::new(
    ///     "Weather Agent",
    ///     "Provides weather info",
    ///     "1.0.0",
    ///     "https://weather.example.com"
    /// );
    ///
    /// // Create authentication headers
    /// let mut headers = HashMap::new();
    /// headers.insert("Authorization".to_string(), "Bearer token123".to_string());
    /// headers.insert("X-API-Key".to_string(), "my-api-key".to_string());
    ///
    /// let tool = A2AAgentTool::new(vec![
    ///     (weather_card, Some(headers))
    /// ])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(agents: Vec<(AgentCard, Option<HashMap<String, String>>)>) -> Result<Self, String> {
        let mut cards = HashMap::new();
        let mut headers = HashMap::new();

        for (card, agent_headers) in agents {
            let name = normalize_agent_name(&card.name);

            // Validate agent card has URL
            if card.url.is_empty() {
                return Err(format!("Agent card '{name}' does not contain a valid URL"));
            }

            cards.insert(name.clone(), card);
            headers.insert(name, agent_headers);
        }

        if cards.is_empty() {
            return Err("No remote agents configured".to_string());
        }

        Ok(Self {
            agent_cards: cards,
            agent_headers: headers,
        })
    }

    /// Create A2A client on-demand for a specific agent
    fn create_client(&self, agent_name: &str) -> Result<A2AClient, String> {
        let card = self
            .agent_cards
            .get(agent_name)
            .ok_or_else(|| format!("Agent '{agent_name}' not found"))?;

        let headers = self.agent_headers.get(agent_name).and_then(|h| h.as_ref());

        match headers {
            Some(headers) => A2AClient::from_card_with_headers(card.clone(), headers.clone())
                .map_err(|e| format!("Failed to create A2A client for {agent_name}: {e}")),
            None => A2AClient::from_card(card.clone())
                .map_err(|e| format!("Failed to create A2A client for {agent_name}: {e}")),
        }
    }

    /// Get session state key for storing remote context
    fn context_state_key(agent_name: &str) -> String {
        format!("a2a_context:{agent_name}")
    }

    /// Get or create remote context for an agent
    fn get_or_create_remote_context(
        &self,
        agent_name: &str,
        context: &ToolContext<'_>,
    ) -> Result<RemoteContextInfo, String> {
        let state_key = Self::context_state_key(agent_name);

        // Try to get existing remote context from session state
        if let Some(existing) = context.state().get_state(&state_key) {
            if let Ok(info) = serde_json::from_value::<RemoteContextInfo>(existing) {
                return Ok(info);
            }
        }

        // No existing context - create new one
        let endpoint = self
            .agent_cards
            .get(agent_name)
            .map(|card| card.url.clone())
            .unwrap_or_default();

        Ok(RemoteContextInfo::new(endpoint))
    }

    /// Store remote context info in session state
    fn store_remote_context(
        &self,
        agent_name: &str,
        info: &RemoteContextInfo,
        context: &ToolContext<'_>,
    ) -> Result<(), String> {
        let state_key = Self::context_state_key(agent_name);
        let value = serde_json::to_value(info).map_err(|e| e.to_string())?;
        context.state().set_state(&state_key, value);
        Ok(())
    }

    /// Build A2A message with proper context
    fn build_a2a_message(
        &self,
        message_text: &str,
        remote_context: &RemoteContextInfo,
        continue_conversation: bool,
    ) -> Message {
        Message {
            kind: "message".to_string(),
            message_id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: message_text.to_string(),
                metadata: None,
            }],
            // Use remote context_id if continuing conversation
            context_id: if continue_conversation {
                remote_context.remote_context_id.clone()
            } else {
                None
            },
            task_id: None, // Let remote agent create task
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        }
    }

    /// Extract human-readable response from A2A response
    fn extract_response_content(&self, response: &SendMessageResponse) -> String {
        match response {
            SendMessageResponse::Success(success) => match &success.result {
                SendMessageResult::Task(task) => {
                    // Extract last agent message from history
                    task.history
                        .iter()
                        .rev()
                        .find(|msg| msg.role == MessageRole::Agent)
                        .and_then(|msg| {
                            msg.parts.first().and_then(|part| match part {
                                Part::Text { text, .. } => Some(text.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_else(|| format!("Task {} created", task.id))
                }
                SendMessageResult::Message(msg) => msg
                    .parts
                    .first()
                    .and_then(|part| match part {
                        Part::Text { text, .. } => Some(text.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "No text response".to_string()),
            },
            SendMessageResponse::Error(err) => {
                format!("Error: {}", err.error.message)
            }
        }
    }

    /// Handle streaming call to remote agent
    async fn call_with_streaming(
        &self,
        agent_name: &str,
        client: &A2AClient,
        params: MessageSendParams,
        remote_context: &mut RemoteContextInfo,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        // Call streaming endpoint
        let mut stream = match client.send_streaming_message(params).await {
            Ok(stream) => stream,
            Err(e) => {
                return ToolResult::error(format!(
                    "Failed to initiate streaming call to {agent_name}: {e}"
                ));
            }
        };

        // Accumulate messages and track state
        let mut accumulated_messages = Vec::new();
        let mut accumulated_artifacts = Vec::new();
        let mut terminal_state: Option<TaskState> = None;
        let mut status_message: Option<String> = None;

        // Process streaming events until terminal condition
        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => match event {
                    SendStreamingMessageResult::Message(msg) => {
                        // Accumulate message
                        if let Some(Part::Text { text, .. }) = msg.parts.first() {
                            accumulated_messages.push(text.clone());
                        }
                    }
                    SendStreamingMessageResult::TaskStatusUpdate(status_event) => {
                        // Update remote context with task_id
                        remote_context.remote_task_id = Some(status_event.task_id.clone());
                        remote_context.remote_context_id = Some(status_event.context_id.clone());

                        // Check for terminal states
                        let is_terminal = matches!(
                            status_event.status.state,
                            TaskState::InputRequired
                                | TaskState::Completed
                                | TaskState::Failed
                                | TaskState::Canceled
                                | TaskState::Rejected
                        );

                        if is_terminal {
                            terminal_state = Some(status_event.status.state.clone());

                            // Extract status message if available
                            if let Some(msg) = &status_event.status.message {
                                if let Some(Part::Text { text, .. }) = msg.parts.first() {
                                    status_message = Some(text.clone());
                                }
                            }
                            break;
                        }
                    }
                    SendStreamingMessageResult::TaskArtifactUpdate(artifact_event) => {
                        // Accumulate artifact
                        accumulated_artifacts.push(artifact_event.artifact.clone());

                        // Check for last chunk
                        if artifact_event.last_chunk == Some(true) {
                            break;
                        }
                    }
                    SendStreamingMessageResult::Task(task) => {
                        // Final task object - update context
                        remote_context.remote_task_id = Some(task.id.clone());
                        remote_context.remote_context_id = Some(task.context_id.clone());
                    }
                },
                Err(e) => {
                    return ToolResult::error(format!("Streaming error from {agent_name}: {e}"));
                }
            }
        }

        // Update remote context stats
        remote_context.last_call = Some(Utc::now().to_rfc3339());
        remote_context.message_count += 1;

        // Store updated context
        if let Err(e) = self.store_remote_context(agent_name, remote_context, context) {
            return ToolResult::error(format!("Failed to store remote context: {e}"));
        }

        // Build response based on terminal state and priority
        let response_text = match terminal_state {
            Some(TaskState::Completed) => {
                // Priority for Completed: artifacts first, then messages
                if !accumulated_artifacts.is_empty() {
                    // Send only artifacts
                    self.format_artifacts(&accumulated_artifacts)
                } else if !accumulated_messages.is_empty() {
                    accumulated_messages.join("\n")
                } else if let Some(msg) = status_message {
                    msg
                } else {
                    format!("Task completed by {agent_name}")
                }
            }
            Some(
                TaskState::Failed
                | TaskState::Rejected
                | TaskState::InputRequired
                | TaskState::Canceled,
            ) => {
                // Priority for error states: status message first, then accumulated messages
                if let Some(msg) = status_message {
                    msg
                } else if !accumulated_messages.is_empty() {
                    accumulated_messages.join("\n")
                } else {
                    format!("Task ended with state: {:?}", terminal_state.unwrap())
                }
            }
            _ => {
                // No terminal state detected (last_chunk or stream ended)
                if !accumulated_artifacts.is_empty() {
                    self.format_artifacts(&accumulated_artifacts)
                } else if !accumulated_messages.is_empty() {
                    accumulated_messages.join("\n")
                } else {
                    format!("Task submitted to {agent_name}")
                }
            }
        };

        ToolResult::success(json!(response_text))
    }

    /// Format artifacts for display
    fn format_artifacts(&self, artifacts: &[a2a_types::Artifact]) -> String {
        if artifacts.is_empty() {
            return String::from("No artifacts");
        }

        artifacts
            .iter()
            .map(|artifact| {
                let name = artifact.name.as_deref().unwrap_or("unnamed");

                // Extract text content from artifact parts
                let content = artifact
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        Part::Text { text, .. } => Some(text.clone()),
                        Part::Data { data, .. } => {
                            // Convert JSON value to string
                            if let Some(s) = data.as_str() {
                                Some(s.to_string())
                            } else {
                                Some(data.to_string())
                            }
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                if content.is_empty() {
                    format!("[Artifact: {name}] (no text content)")
                } else {
                    format!("[Artifact: {name}]\n{content}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Handle synchronous call to remote agent
    async fn call_synchronous(
        &self,
        agent_name: &str,
        client: &A2AClient,
        params: MessageSendParams,
        remote_context: &mut RemoteContextInfo,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        // Call synchronous endpoint
        let response = match client.send_message(params).await {
            Ok(resp) => resp,
            Err(e) => {
                return ToolResult::error(format!("Failed to call {agent_name}: {e}"));
            }
        };

        // Update remote context from response
        remote_context.update_from_response(&response);

        // Store updated context
        if let Err(e) = self.store_remote_context(agent_name, remote_context, context) {
            return ToolResult::error(format!("Failed to store remote context: {e}"));
        }

        // Extract and return response
        let response_text = self.extract_response_content(&response);
        ToolResult::success(json!(response_text))
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for A2AAgentTool {
    fn name(&self) -> &'static str {
        "call_remote_agent"
    }

    fn description(&self) -> &'static str {
        "Call a remote agent to delegate a task or ask a question."
    }

    fn declaration(&self) -> FunctionDeclaration {
        // Build enum of available agent names
        let agent_names: Vec<String> = self.agent_cards.keys().cloned().collect();

        // Build description with agent details
        let mut desc =
            "Call a remote agent to delegate a task or ask a question. Available agents:\n"
                .to_string();
        for (name, card) in &self.agent_cards {
            desc.push_str(&format!("- {}: {}\n", name, card.description));
        }

        FunctionDeclaration::new(
            "call_remote_agent",
            desc,
            json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "enum": agent_names,
                        "description": "Name of the remote agent to call"
                    },
                    "message": {
                        "type": "string",
                        "description": "The message or question to send to the remote agent"
                    },
                    "continue_conversation": {
                        "type": "boolean",
                        "description": "Whether to continue previous conversation with this agent (default: true)",
                        "default": true
                    }
                },
                "required": ["agent_name", "message"]
            }),
        )
    }

    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        // 1. Extract arguments
        let agent_name = match args.get("agent_name").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => return ToolResult::error("agent_name is required".to_string()),
        };

        let message_text = match args.get("message").and_then(|v| v.as_str()) {
            Some(msg) => msg,
            None => return ToolResult::error("message is required".to_string()),
        };

        let continue_conversation = args
            .get("continue_conversation")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        // 2. Create client on-demand for this agent
        let client = match self.create_client(agent_name) {
            Ok(c) => c,
            Err(e) => {
                let available = self
                    .agent_cards
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                return ToolResult::error(format!(
                    "Failed to create client for '{agent_name}': {e}. Available agents: {available}"
                ));
            }
        };

        // 3. Get or create remote context
        let mut remote_context = match self.get_or_create_remote_context(agent_name, context) {
            Ok(ctx) => ctx,
            Err(e) => return ToolResult::error(format!("Failed to get context: {e}")),
        };

        // 4. Build A2A message
        let message = self.build_a2a_message(message_text, &remote_context, continue_conversation);

        let params = MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        };

        // 5. Check if agent supports streaming
        let agent_card = self.agent_cards.get(agent_name).unwrap(); // Safe: already validated
        let supports_streaming = agent_card.capabilities.streaming.unwrap_or(false);

        if supports_streaming {
            // Use streaming path
            self.call_with_streaming(agent_name, &client, params, &mut remote_context, context)
                .await
        } else {
            // Use synchronous path
            self.call_synchronous(agent_name, &client, params, &mut remote_context, context)
                .await
        }
    }
}

/// Normalize agent name for tool usage
///
/// Converts "Weather Agent" -> "`weather_agent`"
/// This ensures consistent naming in the tool interface.
fn normalize_agent_name(name: &str) -> String {
    name.to_lowercase()
        .replace([' ', '-'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Builder for `A2AAgentTool`
pub struct A2AAgentToolBuilder {
    agents: Vec<(AgentCard, Option<HashMap<String, String>>)>,
}

impl A2AAgentToolBuilder {
    /// Create a new builder
    pub const fn new() -> Self {
        Self { agents: Vec::new() }
    }

    /// Add a remote agent card with optional headers
    pub fn add_agent(mut self, card: AgentCard, headers: Option<HashMap<String, String>>) -> Self {
        self.agents.push((card, headers));
        self
    }

    /// Add multiple remote agent cards with optional headers
    pub fn with_agents(
        mut self,
        agents: Vec<(AgentCard, Option<HashMap<String, String>>)>,
    ) -> Self {
        self.agents.extend(agents);
        self
    }

    /// Build the `A2AAgentTool`
    pub fn build(self) -> Result<A2AAgentTool, String> {
        A2AAgentTool::new(self.agents)
    }
}

impl Default for A2AAgentToolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_agent_name_replaces_separators() {
        assert_eq!(normalize_agent_name("Weather Agent"), "weather_agent");
        assert_eq!(normalize_agent_name("Agent-42"), "agent_42");
    }

    #[test]
    fn builder_creates_tool_with_agent_card() {
        let mut card = AgentCard::new("Test Agent", "Description", "1.0.0", "https://example.com");
        card.capabilities.streaming = Some(false);

        let tool = A2AAgentToolBuilder::new()
            .add_agent(card, None)
            .build()
            .expect("tool");

        assert!(tool.agent_cards.contains_key("test_agent"));
    }
}
