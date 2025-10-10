//! Agent Executor - Immutable execution engine for agents
//!
//! This module contains the core execution logic that handles conversation
//! loops, tool calling, and event processing. It's designed to be immutable
//! after creation and efficiently shareable via Arc.

use crate::errors::{AgentError, AgentResult};
use crate::events::{EventProcessor, ExecutionContext};
use crate::models::{
    BaseLlm, LlmRequest,
    content::{Content, FunctionResponseParams},
};
use crate::observability::utils as obs_utils;
use crate::sessions::{QueryService, SessionService};
use crate::tools::BaseToolset;
use a2a_types::{
    Message, MessageRole, MessageSendParams, SendStreamingMessageResult, Task, TaskState,
    TaskStatus,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing;
use uuid::Uuid;

use super::config::AgentConfig;

/// Immutable execution engine that handles all agent execution logic
/// This can be Arc::cloned for efficient sharing without creating dummy agents
#[derive(Clone)]
pub struct AgentExecutor {
    /// The LLM model for generating responses
    pub(crate) model: Arc<dyn BaseLlm>,

    /// Optional toolset for function calling
    pub(crate) toolset: Option<Arc<dyn BaseToolset>>,

    /// Session service for persistence
    pub(crate) session_service: Arc<dyn SessionService>,

    /// Query service for read operations
    pub(crate) query_service: Arc<QueryService>,

    /// Event processor for business logic and event handling
    pub(crate) event_processor: Arc<EventProcessor>,

    /// Agent configuration
    pub(crate) config: AgentConfig,

    /// System instruction for the agent
    pub(crate) instruction: String,
}

impl AgentExecutor {
    /// Get the agent's LLM model
    pub fn model(&self) -> &Arc<dyn BaseLlm> {
        &self.model
    }

    /// Get the agent's toolset
    pub fn toolset(&self) -> Option<&Arc<dyn BaseToolset>> {
        self.toolset.as_ref()
    }

    /// Get access to the session service
    pub fn session_service(&self) -> &Arc<dyn SessionService> {
        &self.session_service
    }

    /// Get access to the query service
    pub fn query_service(&self) -> &Arc<QueryService> {
        &self.query_service
    }

    /// Get access to the event processor
    pub fn event_processor(&self) -> &Arc<EventProcessor> {
        &self.event_processor
    }

    /// Get the agent's configuration
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the agent's instruction
    pub fn instruction(&self) -> &str {
        &self.instruction
    }

    // ===== Core Execution Methods =====

    /// Execute a conversation (unified for streaming/non-streaming)
    pub async fn execute_conversation(
        &self,
        app_name: &str,
        user_id: &str,
        params: MessageSendParams,
        capture_all_events: Option<mpsc::UnboundedSender<crate::sessions::SessionEvent>>,
        capture_a2a: Option<mpsc::UnboundedSender<SendStreamingMessageResult>>,
        stream_tx: Option<mpsc::Sender<SendStreamingMessageResult>>,
    ) -> AgentResult<ExecutionResult> {
        // Handle context_id from message
        let session = self
            .get_or_create_session(app_name, user_id, &params.message.context_id)
            .await?;

        // Get task
        let mut continuous_task = false;
        let existing_task = self
            .get_task(app_name, user_id, &params.message.task_id)
            .await?;
        let task_id = if let Some(existing_task) = existing_task {
            continuous_task = true;
            existing_task.id.clone()
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        // Setup event forwarding for streaming and capture
        if capture_all_events.is_some() || capture_a2a.is_some() || stream_tx.is_some() {
            self.setup_event_forwarding(&task_id, capture_all_events, capture_a2a, stream_tx)
                .await?;
        }

        // Setup execution context
        let context = ExecutionContext::new(
            session.id.to_string(),
            task_id.to_string(),
            app_name.to_string(),
            user_id.to_string(),
            self.event_processor.clone(),
            self.query_service.clone(),
        );

        if continuous_task == false {
            let task = Task {
                kind: "task".to_string(),
                id: context.task_id.clone(),
                context_id: context.context_id.clone(),
                status: TaskStatus {
                    state: TaskState::Submitted,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    message: None,
                },
                history: vec![params.message.clone()],
                artifacts: vec![],
                metadata: None,
            };
            context
                .emit_task_status_update(TaskState::Submitted, None, Some(task))
                .await?;
        }

        // Run conversation loop
        self.run_conversation_loop(params.message, &context).await?;

        // Get final task
        let final_task = context
            .get_task()
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: context.task_id.clone(),
            })?;

        // unless the task is already in a terminal state, mark it as completed
        if matches!(
            final_task.status.state,
            TaskState::Completed | TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        ) {
            tracing::debug!(
                "Task {} is already in terminal state, not marking as completed",
                context.task_id
            );
        } else {
            context
                .emit_task_status_update(TaskState::Completed, None, None)
                .await?;
        }

        // Clean up all subscriptions for this task
        let event_bus = self.event_processor.event_bus();
        let _ = event_bus.unsubscribe(context.task_id.clone()).await;

        Ok(ExecutionResult {
            task: Some(final_task),
            context_id: context.context_id.clone(),
        })
    }

    /// Get or create session based on context_id presence
    async fn get_or_create_session(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: &Option<String>,
    ) -> AgentResult<crate::sessions::Session> {
        if let Some(context_id) = context_id {
            // context_id provided - must find existing session or error
            match self
                .session_service
                .get_session(app_name, user_id, context_id)
                .await?
            {
                Some(existing_session) => Ok(existing_session),
                None => Err(AgentError::SessionNotFound {
                    session_id: context_id.clone(),
                    app_name: app_name.to_string(),
                    user_id: user_id.to_string(),
                }),
            }
        } else {
            // No context_id provided - create new session with auto-generated ID
            self.session_service
                .create_session(app_name.to_string(), user_id.to_string())
                .await
        }
    }

    /// Get existing task or generate new task ID
    async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &Option<String>,
    ) -> AgentResult<Option<Task>> {
        if let Some(task_id) = task_id {
            // Continue existing task - check if it exists and is not terminal
            match self
                .query_service
                .get_task(app_name, user_id, task_id)
                .await?
            {
                Some(task) => {
                    if matches!(
                        task.status.state,
                        a2a_types::TaskState::Completed
                            | a2a_types::TaskState::Failed
                            | a2a_types::TaskState::Canceled
                            | a2a_types::TaskState::Rejected
                    ) {
                        return Err(AgentError::InvalidTaskStateTransition {
                            from: format!("{:?}", task.status.state),
                            to: "Working".to_string(),
                        });
                    }
                    Ok(Some(task))
                }
                None => Err(AgentError::TaskNotFound {
                    task_id: task_id.clone(),
                }),
            }
        } else {
            Ok(None)
        }
    }

    /// Set up event forwarding for streaming and capture
    async fn setup_event_forwarding(
        &self,
        task_id: &str,
        capture_all_events: Option<mpsc::UnboundedSender<crate::sessions::SessionEvent>>,
        capture_a2a: Option<mpsc::UnboundedSender<SendStreamingMessageResult>>,
        stream_a2a: Option<mpsc::Sender<SendStreamingMessageResult>>,
    ) -> AgentResult<()> {
        let event_bus = self.event_processor.event_bus();

        // Subscribe to all events for this task
        let mut event_receiver = event_bus.subscribe(task_id.to_string()).await?;

        // Spawn task to forward events to appropriate channels
        tokio::spawn(async move {
            while let Some(event) = event_receiver.recv().await {
                // Forward to all events capture if requested
                if let Some(ref all_events_tx) = capture_all_events {
                    let _ = all_events_tx.send(event.clone());
                }

                // Convert and forward A2A events
                if event.is_a2a_compatible() {
                    if let Some(a2a_result) = event.to_a2a_streaming_result() {
                        // Send to capture channel if requested
                        if let Some(ref a2a_tx) = capture_a2a {
                            let _ = a2a_tx.send(a2a_result.clone());
                        }

                        // Send to streaming channel if requested
                        if let Some(ref stream_tx) = stream_a2a {
                            let _ = stream_tx.send(a2a_result).await;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Run the main conversation loop with tool calling
    #[tracing::instrument(
        name = "radkit.conversation.execute_core",
        skip(self, initial_message, context),
        fields(
            task.id = %context.task_id,
            iteration.count = tracing::field::Empty,
            otel.kind = "internal",
        )
    )]
    async fn run_conversation_loop(
        &self,
        mut initial_message: Message,
        context: &ExecutionContext,
    ) -> AgentResult<()> {
        let max_iterations = self.config.max_iterations;
        let mut iteration = 0;

        // Emit the initial user message now that the task exists
        initial_message.context_id = Some(context.context_id.clone());
        initial_message.task_id = Some(context.task_id.clone());
        let initial_content = Content::from_message(
            initial_message,
            context.task_id.clone(),
            context.context_id.clone(),
        );
        context
            .emit_user_input(initial_content)
            .await
            .map_err(|e| {
                obs_utils::record_error(&e);
                e
            })?;

        loop {
            if iteration >= max_iterations {
                self.handle_iteration_limit_exceeded(context, max_iterations)
                    .await
                    .map_err(|e| {
                        obs_utils::record_error(&e);
                        e
                    })?;
                break;
            }

            iteration += 1;
            // Update iteration count in span
            tracing::Span::current().record("iteration.count", iteration);

            tracing::debug!(
                "Conversation iteration {} for task {}",
                iteration,
                context.task_id
            );

            // Get conversation history from EventProcessor for each iteration
            let content_messages = context.get_llm_conversation().await?;

            let llm_request = LlmRequest {
                messages: content_messages,
                current_task_id: context.task_id.clone(),
                context_id: context.context_id.clone(),
                system_instruction: Some(self.instruction.clone()),
                config: crate::models::GenerateContentConfig::default(),
                toolset: self.toolset.clone(),
                metadata: HashMap::new(),
            };

            // Generate response from LLM
            let response = self
                .model
                .generate_content(llm_request)
                .await
                .map_err(|e| AgentError::LlmError {
                    source: Box::new(e),
                })?;

            // Convert LLM response to Content and emit
            let response_content = Content::from_llm_response(
                response,
                context.task_id.clone(),
                context.context_id.clone(),
            );

            context.emit_message(response_content.clone()).await?;

            // Process tool calls if any
            let has_function_calls = self.process_tool_calls(&response_content, context).await?;

            if !has_function_calls {
                // No more tool calls, conversation complete
                // Task state will remain in its current state (likely Working or whatever tools set it to)
                // Tools are responsible for marking tasks as Completed/Failed using update_status
                break;
            }

            // Continue loop for next iteration
        }

        obs_utils::record_success();
        Ok(())
    }

    /// Handle iteration limit exceeded scenario
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ExecutionContext,
        _max_iterations: usize,
    ) -> AgentResult<()> {
        // Emit to persistent storage
        // Todo: add message here says "Iteration limit exceeded"
        context
            .emit_task_status_update(a2a_types::TaskState::Failed, None, None)
            .await
    }

    /// Process tool calls in a message and emit function responses
    #[tracing::instrument(
        name = "radkit.tools.process_calls",
        skip(self, content, context),
        fields(
            task.id = %context.task_id,
            tools.count = tracing::field::Empty,
            tools.total_duration_ms = tracing::field::Empty,
            otel.kind = "internal",
        )
    )]
    async fn process_tool_calls(
        &self,
        content: &Content,
        context: &ExecutionContext,
    ) -> AgentResult<bool> {
        let mut has_function_calls = false;
        let mut tool_count = 0;
        let mut total_duration = 0u64;

        for part in &content.parts {
            if let crate::models::content::ContentPart::FunctionCall {
                name,
                arguments,
                tool_use_id,
                ..
            } = part
            {
                has_function_calls = true;
                tool_count += 1;

                // Create individual span for each tool execution
                let tool_span = tracing::info_span!(
                    "radkit.tool.execute_call",
                    tool.name = %name,
                    tool.success = tracing::field::Empty,
                    tool.duration_ms = tracing::field::Empty,
                );

                let _guard = tool_span.enter();
                let start_time = std::time::Instant::now();

                // Execute the tool
                let tool_result = self.execute_tool_call(name, arguments, context).await;
                let duration_ms = start_time.elapsed().as_millis() as u64;
                total_duration += duration_ms;

                // Record tool metrics
                tool_span.record("tool.success", tool_result.success);
                tool_span.record("tool.duration_ms", duration_ms);
                obs_utils::record_tool_execution_metric(name, tool_result.success);

                // Create and emit function response as UserMessage (it's input back to the agent)
                let mut response_content = Content::new(
                    context.task_id.clone(),
                    context.context_id.clone(),
                    Uuid::new_v4().to_string(),
                    MessageRole::User,
                );

                response_content.add_function_response(FunctionResponseParams {
                    name: name.clone(),
                    success: tool_result.success,
                    result: tool_result.data.clone(),
                    error_message: tool_result.error_message.clone(),
                    tool_use_id: tool_use_id.clone(),
                    duration_ms: Some(duration_ms),
                    metadata: None,
                });

                // Function responses are UserMessage events (input to agent from tools)
                context.emit_user_input(response_content).await?;
            }
        }

        // Record aggregate metrics on parent span
        let span = tracing::Span::current();
        span.record("tools.count", tool_count);
        span.record("tools.total_duration_ms", total_duration);

        Ok(has_function_calls)
    }

    /// Execute a tool call and return the result
    async fn execute_tool_call(
        &self,
        name: &str,
        args: &serde_json::Value,
        context: &ExecutionContext,
    ) -> crate::tools::base_tool::ToolResult {
        if let Some(toolset) = &self.toolset {
            let available_tools = toolset.get_tools().await;

            if let Some(tool) = available_tools.iter().find(|t| t.name() == name) {
                // Convert serde_json::Value to HashMap<String, Value>
                let args_map: HashMap<String, serde_json::Value> =
                    if let serde_json::Value::Object(map) = args.clone() {
                        map.into_iter().collect()
                    } else {
                        HashMap::new()
                    };

                let tool_context = crate::tools::ToolContext::from_execution_context(context);
                tool.run_async(args_map, &tool_context).await
            } else {
                crate::tools::base_tool::ToolResult::error(format!("Tool '{name}' not found"))
            }
        } else {
            crate::tools::base_tool::ToolResult::error("No toolset available".to_string())
        }
    }
}

/// Result of conversation execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub task: Option<Task>,
    pub context_id: String,
}

impl std::fmt::Debug for AgentExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentExecutor")
            .field("instruction", &self.instruction)
            .field("config", &self.config)
            .field("has_toolset", &self.toolset.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::InMemoryEventBus;
    use crate::models::mock_llm::MockLlm;
    use crate::sessions::InMemorySessionService;
    use crate::tools::SimpleToolset;

    fn create_test_executor() -> AgentExecutor {
        let model = Arc::new(MockLlm::new("test-model".to_string()));
        let session_service = Arc::new(InMemorySessionService::new());
        let query_service = Arc::new(QueryService::new(session_service.clone()));
        let event_bus = Arc::new(InMemoryEventBus::new());
        let event_processor = Arc::new(EventProcessor::new(session_service.clone(), event_bus));

        AgentExecutor {
            model,
            toolset: None,
            session_service,
            query_service,
            event_processor,
            config: AgentConfig::default(),
            instruction: "Test instruction".to_string(),
        }
    }

    #[test]
    fn test_executor_creation() {
        let executor = create_test_executor();
        assert_eq!(executor.instruction(), "Test instruction");
        assert!(executor.toolset().is_none());
    }

    #[test]
    fn test_executor_with_toolset() {
        let mut executor = create_test_executor();
        let toolset = Arc::new(SimpleToolset::new(vec![]));
        executor.toolset = Some(toolset);

        assert!(executor.toolset().is_some());
    }

    #[test]
    fn test_executor_clone() {
        let executor = create_test_executor();
        let cloned = executor.clone();

        // Should have same instruction and config
        assert_eq!(executor.instruction(), cloned.instruction());
        assert_eq!(
            executor.config().max_iterations,
            cloned.config().max_iterations
        );

        // Arc clones should point to same underlying data
        assert!(Arc::ptr_eq(&executor.model, &cloned.model));
        assert!(Arc::ptr_eq(
            &executor.session_service,
            &cloned.session_service
        ));
    }

    #[test]
    fn test_executor_debug() {
        let executor = create_test_executor();
        let debug_str = format!("{:?}", executor);
        assert!(debug_str.contains("AgentExecutor"));
        assert!(debug_str.contains("Test instruction"));
    }
}
