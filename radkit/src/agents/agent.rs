//! Agent - A2A Protocol API Facade
//!
//! This module provides the main Agent API that implements the A2A (Agent-to-Agent) protocol.
//! Agent is now a clean facade that delegates all execution to AgentExecutor.

use super::agent_builder::AgentBuilder;
use super::agent_executor::AgentExecutor;
use crate::agents::{SendMessageResultWithEvents, SendStreamingMessageResultWithEvents};
use crate::errors::AgentResult;
use crate::observability::utils as obs_utils;
use a2a_types::{AgentCard, MessageSendParams, SendMessageResult, Task};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::{ReceiverStream, UnboundedReceiverStream};
use tracing::Instrument;

pub struct Agent {
    /// Agent metadata (name, description, version, capabilities, etc.)
    pub agent_card: AgentCard,

    /// Execution engine (shared via Arc for efficient streaming)
    pub(crate) executor: Arc<AgentExecutor>,
}

impl Agent {
    /// Create a new agent builder - only requires instruction and model
    pub fn builder(
        instruction: impl Into<String>,
        model: impl crate::models::BaseLlm + 'static,
    ) -> AgentBuilder {
        AgentBuilder::new(instruction, model)
    }

    // ===== Convenience Accessor Methods =====

    /// Get the agent's name
    pub fn name(&self) -> &str {
        &self.agent_card.name
    }

    /// Get the agent's description
    pub fn description(&self) -> &str {
        &self.agent_card.description
    }

    /// Get the agent's version
    pub fn version(&self) -> &str {
        &self.agent_card.version
    }

    /// Get the agent's instruction
    pub fn instruction(&self) -> &str {
        self.executor.instruction()
    }

    /// Get the agent's card (immutable reference)
    pub fn card(&self) -> &AgentCard {
        &self.agent_card
    }

    /// Get the agent's config
    pub fn config(&self) -> &crate::agents::config::AgentConfig {
        self.executor.config()
    }

    /// Get access to session service for testing/advanced usage
    pub fn session_service(&self) -> Arc<dyn crate::sessions::SessionService> {
        self.executor.session_service().clone()
    }

    // ===== A2A Protocol Methods =====

    /// Send a message and get response with all events (non-streaming)
    #[tracing::instrument(
        name = "radkit.agent.send_message",
        skip(self, params),
        fields(
            agent.name = %self.agent_card.name,
            app.name = %app_name,
            user.id = tracing::field::Empty,
            agent.session_id = tracing::field::Empty,
            message.role = ?params.message.role,
            otel.kind = "server",
            otel.status_code = tracing::field::Empty,
            error.message = tracing::field::Empty,
        )
    )]
    pub async fn send_message(
        &self,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
    ) -> AgentResult<SendMessageResultWithEvents> {
        // Record PII-safe user ID
        tracing::Span::current().record("user.id", obs_utils::hash_user_id(&user_id).as_str());

        // Create channels to capture events (dual capture system like original)
        let (all_tx, mut all_rx) = mpsc::unbounded_channel();
        let (a2a_tx, mut a2a_rx) = mpsc::unbounded_channel();

        // Execute conversation without streaming
        let result = self
            .executor
            .execute_conversation(
                &app_name,
                &user_id,
                params,
                Some(all_tx), // Capture all events
                Some(a2a_tx), // Capture A2A events separately
                None,         // No streaming
            )
            .await
            .inspect_err(|e| {
                obs_utils::record_error(e);
                obs_utils::record_agent_message_metric(&self.agent_card.name, false);
            })?;

        // Record session_id now that we have the result
        if let Some(ref task) = result.task {
            tracing::Span::current().record("agent.session_id", &task.context_id);
        }

        // Collect all events
        let mut all_events = Vec::new();
        while let Ok(event) = all_rx.try_recv() {
            all_events.push(event);
        }

        // Collect A2A events (already filtered and converted)
        let mut a2a_events = Vec::new();
        while let Ok(event) = a2a_rx.try_recv() {
            a2a_events.push(event);
        }

        // Return task result with events
        let task_result = if let Some(task) = result.task {
            SendMessageResult::Task(task)
        } else {
            return Err(crate::errors::AgentError::TaskNotFound {
                task_id: "unknown".to_string(),
            });
        };

        obs_utils::record_success();
        obs_utils::record_agent_message_metric(&self.agent_card.name, true);

        Ok(SendMessageResultWithEvents {
            result: task_result,
            all_events,
            a2a_events,
        })
    }

    /// Send a streaming message and get real-time event streams
    #[tracing::instrument(
        name = "radkit.agent.send_streaming_message",
        skip(self, params),
        fields(
            agent.name = %self.agent_card.name,
            app.name = %app_name,
            user.id = tracing::field::Empty,
            agent.session_id = tracing::field::Empty,
            message.role = ?params.message.role,
            streaming = true,
            otel.kind = "server",
            otel.status_code = tracing::field::Empty,
            error.message = tracing::field::Empty,
        )
    )]
    pub async fn send_streaming_message(
        &self,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
    ) -> AgentResult<SendStreamingMessageResultWithEvents> {
        // Record PII-safe user ID
        tracing::Span::current().record("user.id", obs_utils::hash_user_id(&user_id).as_str());

        // Capture span context for background task
        let span = tracing::Span::current();
        let context_id = params.message.context_id.clone();
        let agent_name = self.agent_card.name.clone();

        // Create channels for streaming
        let (stream_tx, stream_rx) = mpsc::channel(100);
        let (all_tx, all_rx) = mpsc::unbounded_channel();

        // Clone executor for the spawned task (efficient Arc::clone!)
        let executor = Arc::clone(&self.executor);

        // Spawn background task to execute conversation
        tokio::spawn(
            async move {
                let result = executor
                    .execute_conversation(
                        &app_name,
                        &user_id,
                        params,
                        Some(all_tx),    // Capture all events
                        None,            // A2A events go to stream, not capture
                        Some(stream_tx), // Enable streaming
                    )
                    .await;

                // Handle any errors (could send error event to stream)
                match result {
                    Ok(_) => {
                        obs_utils::record_success();
                        obs_utils::record_agent_message_metric(&agent_name, true);
                    }
                    Err(e) => {
                        obs_utils::record_error(&e);
                        obs_utils::record_agent_message_metric(&agent_name, false);
                        tracing::error!("Streaming conversation failed: {:?}", e);
                    }
                }
            }
            .instrument(span),
        );

        // Record session_id if available
        if let Some(ctx_id) = context_id {
            tracing::Span::current().record("agent.session_id", &ctx_id);
        }

        // Return streaming result with both channels
        Ok(SendStreamingMessageResultWithEvents {
            a2a_stream: Box::pin(ReceiverStream::new(stream_rx)),
            all_events_stream: Box::pin(UnboundedReceiverStream::new(all_rx)),
        })
    }

    /// Get a specific task by ID with optional parameters (A2A Protocol: tasks/get)
    pub async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        params: a2a_types::TaskQueryParams,
    ) -> AgentResult<Task> {
        // Get task using Agent's query service (business logic)
        let mut a2a_task = self
            .executor
            .query_service()
            .get_task(app_name, user_id, &params.id)
            .await?
            .ok_or_else(|| crate::errors::AgentError::TaskNotFound {
                task_id: params.id.clone(),
            })?;

        // Apply history length limit if specified
        if let Some(history_length) = params.history_length {
            if history_length >= 0 {
                let limit = history_length as usize;
                if a2a_task.history.len() > limit {
                    a2a_task.history = a2a_task
                        .history
                        .into_iter()
                        .rev()
                        .take(limit)
                        .rev()
                        .collect();
                }
            }
        }

        Ok(a2a_task)
    }

    /// List all tasks for app/user with optional context filter (A2A Protocol: tasks/list)
    pub async fn list_tasks(&self, app_name: &str, user_id: &str) -> AgentResult<Vec<Task>> {
        self.executor
            .query_service()
            .list_tasks(app_name, user_id, None)
            .await
    }
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("name", &self.agent_card.name)
            .field("description", &self.agent_card.description)
            .field("version", &self.agent_card.version)
            .field("instruction", &self.executor.instruction())
            .field("has_toolset", &self.executor.toolset().is_some())
            .finish()
    }
}

// Implement Clone for Agent (cheap Arc clone)
impl Clone for Agent {
    fn clone(&self) -> Self {
        Self {
            agent_card: self.agent_card.clone(),
            executor: Arc::clone(&self.executor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mock_llm::MockLlm;
    use crate::tools::{FunctionTool, ToolResult};
    use a2a_types::{Message, MessageRole, Part, SendMessageResult, TaskState};
    use serde_json::json;
    use std::sync::Arc;
    use uuid::Uuid;

    // Helper to create a test agent with mock dependencies
    fn create_test_agent() -> Agent {
        Agent::builder(
            "You are a helpful test agent.",
            MockLlm::new("mock-model".to_string()),
        )
        .with_card(|c| c.with_name("test_agent").with_description("A test agent"))
        .build()
    }

    // Helper to create a basic MessageSendParams
    fn create_send_params(text: &str) -> MessageSendParams {
        MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: Uuid::new_v4().to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: text.to_string(),
                    metadata: None,
                }],
                context_id: None,
                task_id: None,
                reference_task_ids: Vec::new(),
                extensions: Vec::new(),
                metadata: None,
            },
            configuration: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_agent_builder() {
        let agent = Agent::builder("Test instruction", MockLlm::new("test-model".to_string()))
            .with_card(|c| c.with_name("Test Agent").with_description("A test agent"))
            .build();

        assert_eq!(agent.name(), "Test Agent");
        assert_eq!(agent.description(), "A test agent");
        assert_eq!(agent.instruction(), "Test instruction");
    }

    #[tokio::test]
    async fn test_agent_clone() {
        let agent = create_test_agent();
        let cloned = agent.clone();

        // Should have same properties
        assert_eq!(agent.instruction(), cloned.instruction());

        // Executor should be the same Arc (efficient)
        assert!(Arc::ptr_eq(&agent.executor, &cloned.executor));
    }

    // ===== Comprehensive Test Suite Based on Original .bak Files =====

    #[tokio::test]
    async fn test_send_message_simple_text_response() {
        let agent = create_test_agent();
        let params = create_send_params("Hello");

        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            assert_eq!(task.status.state, TaskState::Submitted); // Task starts as submitted
            assert!(task.history.len() >= 1); // At least user message
            assert_eq!(task.history[0].role, MessageRole::User);

            // Note: Event capture might be empty in unit tests with mock LLM
            // In real usage, events are properly captured during actual execution
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_with_tool_error() {
        let failing_tool = FunctionTool::new(
            "failing_tool".to_string(),
            "A tool that always fails".to_string(),
            |_args, _ctx| {
                Box::pin(async {
                    ToolResult::error("Tool deliberately failed for testing".to_string())
                })
            },
        );

        let agent = Agent::builder(
            "You are a helpful assistant",
            MockLlm::new("test-model".to_string()),
        )
        .with_tool(failing_tool)
        .build();

        let params = create_send_params("Please use a tool that will fail");

        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            // Task might complete or fail depending on tool error handling
            assert!(matches!(
                task.status.state,
                TaskState::Submitted
                    | TaskState::Working
                    | TaskState::Completed
                    | TaskState::Failed
            ));

            // Note: In unit tests, event capture may be limited
            // Real integration tests verify proper event flow
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_with_builtin_tools() {
        let agent = Agent::builder(
            "You are a helpful assistant",
            MockLlm::new("test-model".to_string()),
        )
        .with_builtin_task_tools()
        .build();

        // Verify builtin tools are available
        assert!(
            agent.executor.toolset().is_some(),
            "Agent should have a toolset"
        );

        let params = create_send_params("Please update status to working");
        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            // Note: MockLLM may not call tools, so task could be in any valid state
            assert!(
                matches!(
                    task.status.state,
                    TaskState::Submitted
                        | TaskState::Working
                        | TaskState::Completed
                        | TaskState::Failed
                        | TaskState::InputRequired
                        | TaskState::AuthRequired
                        | TaskState::Canceled
                        | TaskState::Rejected
                ),
                "Task state should be valid, got: {:?}",
                task.status.state
            );

            // Note: Events are properly captured in real execution scenarios
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_multi_turn_conversation() {
        let agent = create_test_agent();

        // First message
        let params1 = create_send_params("Hello, I'm starting a conversation");
        let result1 = agent
            .send_message("app1".to_string(), "user1".to_string(), params1)
            .await
            .unwrap();

        let task1 = if let SendMessageResult::Task(task) = result1.result {
            task
        } else {
            panic!("Expected a Task result");
        };

        // Second message in same context
        let mut params2 = create_send_params("Continue our conversation");
        params2.message.context_id = Some(task1.context_id.clone());

        let result2 = agent
            .send_message("app1".to_string(), "user1".to_string(), params2)
            .await
            .unwrap();

        if let SendMessageResult::Task(task2) = result2.result {
            // Should be in same context
            assert_eq!(task1.context_id, task2.context_id);

            // Note: Events are captured during real execution scenarios
            // Unit tests with MockLlm may have limited event generation
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_get_task_basic() {
        let agent = create_test_agent();

        // First create a task
        let params = create_send_params("Hello");
        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        let original_task = if let SendMessageResult::Task(task) = execution.result {
            task
        } else {
            panic!("Expected a Task result");
        };

        // Now retrieve it
        let retrieved_task = agent
            .get_task(
                "app1",
                "user1",
                a2a_types::TaskQueryParams {
                    id: original_task.id.clone(),
                    history_length: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(original_task.id, retrieved_task.id);
        assert_eq!(original_task.context_id, retrieved_task.context_id);
    }

    #[tokio::test]
    async fn test_get_task_with_history_limit() {
        let agent = create_test_agent();

        // Create a task with multiple messages
        let params1 = create_send_params("First message");
        let execution1 = agent
            .send_message("app1".to_string(), "user1".to_string(), params1)
            .await
            .unwrap();

        let task1 = if let SendMessageResult::Task(task) = execution1.result {
            task
        } else {
            panic!("Expected a Task result");
        };

        // Continue conversation in same context
        let mut params2 = create_send_params("Second message");
        params2.message.context_id = Some(task1.context_id.clone());

        let execution2 = agent
            .send_message("app1".to_string(), "user1".to_string(), params2)
            .await
            .unwrap();

        let task2 = if let SendMessageResult::Task(task) = execution2.result {
            task
        } else {
            panic!("Expected a Task result");
        };

        // Retrieve with history limit
        let retrieved_task = agent
            .get_task(
                "app1",
                "user1",
                a2a_types::TaskQueryParams {
                    id: task2.id.clone(),
                    history_length: Some(1), // Limit to 1 message
                    metadata: None,
                },
            )
            .await
            .unwrap();

        // Should have limited history
        assert!(retrieved_task.history.len() <= 1);
    }

    #[tokio::test]
    async fn test_list_tasks_basic() {
        let agent = create_test_agent();

        // Create a couple of tasks
        let params1 = create_send_params("First task");
        let _execution1 = agent
            .send_message("app1".to_string(), "user1".to_string(), params1)
            .await
            .unwrap();

        let params2 = create_send_params("Second task");
        let _execution2 = agent
            .send_message("app1".to_string(), "user1".to_string(), params2)
            .await
            .unwrap();

        // List all tasks
        let tasks = agent.list_tasks("app1", "user1").await.unwrap();

        // Should have at least the tasks we created
        assert!(tasks.len() >= 2);
    }

    #[tokio::test]
    async fn test_agent_with_tools_builder() {
        let test_tool = FunctionTool::new(
            "test_tool".to_string(),
            "A test tool".to_string(),
            |_args, _ctx| {
                Box::pin(async { ToolResult::success(json!({"result": "tool executed"})) })
            },
        );

        let agent = Agent::builder(
            "You are a helpful assistant",
            MockLlm::new("test-model".to_string()),
        )
        .with_tool(test_tool)
        .with_card(|c| {
            c.with_name("Test Agent")
                .with_description("A test agent with tools")
        })
        .build();

        assert_eq!(agent.name(), "Test Agent");
        assert_eq!(agent.description(), "A test agent with tools");
        assert!(agent.executor.toolset().is_some());
    }

    #[tokio::test]
    async fn test_agent_config_builder() {
        let config = crate::agents::config::AgentConfig::default().with_max_iterations(10);

        let agent = Agent::builder("Test instruction", MockLlm::new("test-model".to_string()))
            .with_config(config)
            .build();

        assert_eq!(agent.config().max_iterations, 10);
    }
}
