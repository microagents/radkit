//! Agent Executor - Immutable execution engine for agents
//!
//! This module contains the core execution logic that handles conversation
//! loops, tool calling, and event processing. It's designed to be immutable
//! after creation and efficiently shareable via Arc.

use crate::config::EnvResolverFn;
use crate::errors::{AgentError, AgentResult};
use crate::events::{EventProcessor, ExecutionContext};
use crate::models::{BaseLlm, LlmRequest, content::Content};
use crate::observability::utils as obs_utils;
use crate::sessions::{QueryService, SessionService};
use crate::tools::BaseToolset;
use a2a_types::{
    MessageRole, MessageSendParams, SendStreamingMessageResult, Task, TaskState, TaskStatus,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{self, Instrument};
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

    /// Optional custom environment resolver for secrets (API keys, etc.)
    /// If None, LLM uses default std::env resolver
    pub(crate) env_resolver: Option<EnvResolverFn>,
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
    #[tracing::instrument(
        name = "radkit.agent.execute_conversation",
        skip(self, params, capture_all_events, capture_a2a_events, stream_tx),
        fields(
            app_name = %app_name,
            user_id = obs_utils::hash_user_id(user_id),
            task.id = tracing::field::Empty,
            context.id = tracing::field::Empty,
            otel.kind = "internal",
        )
    )]
    pub async fn execute_conversation(
        &self,
        app_name: &str,
        user_id: &str,
        params: MessageSendParams,
        capture_all_events: Option<mpsc::UnboundedSender<crate::sessions::SessionEvent>>,
        capture_a2a_events: Option<mpsc::UnboundedSender<SendStreamingMessageResult>>,
        stream_tx: Option<mpsc::Sender<SendStreamingMessageResult>>,
    ) -> AgentResult<ExecutionResult> {
        use tokio::sync::oneshot;
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
        // Clone the channels to use later for final Task emission
        let capture_a2a_clone = capture_a2a_events.clone();
        let stream_tx_clone = stream_tx.clone();

        if capture_all_events.is_some() || capture_a2a_events.is_some() || stream_tx.is_some() {
            self.setup_event_forwarding(
                &task_id,
                capture_all_events,
                capture_a2a_events,
                stream_tx,
            )
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

        // Record task.id and context.id on parent span
        let span = tracing::Span::current();
        span.record("task.id", context.task_id.as_str());
        span.record("context.id", context.context_id.as_str());

        if !continuous_task {
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

        // Create oneshot channel for completion notification
        let (completion_tx, completion_rx) = oneshot::channel();

        // Setup tool completion handler for resumption BEFORE emitting any events
        // This ensures the handler is listening before any tool completions can fire
        self.setup_tool_completion_handler(&context, completion_tx)
            .await?;

        // Emit initial user message
        let mut initial_message = params.message.clone();
        initial_message.context_id = Some(context.context_id.clone());
        initial_message.task_id = Some(context.task_id.clone());
        let initial_content = Content::from_message(
            initial_message,
            context.task_id.clone(),
            context.context_id.clone(),
        );
        context.emit_user_input(initial_content).await?;

        // Execute first turn (non-blocking)
        self.execute_turn(&context).await?;

        // Wait for completion notification from handler
        // Handler will send when execution reaches terminal state
        completion_rx.await.map_err(|_| AgentError::Internal {
            component: "agent_executor".to_string(),
            reason: "Completion notification channel closed unexpectedly".to_string(),
        })?;

        // Get final task
        let final_task = context
            .get_task()
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: context.task_id.clone(),
            })?;

        // NOTE: Do NOT automatically mark task as completed
        // Tasks should remain in their current state (Submitted, Working, etc.)
        // unless tools explicitly changed the state using update_status
        // This matches the original behavior before event-driven execution

        // Emit final Task object directly to A2A streams
        // (TaskStatusUpdate events don't convert to Task results, so we send manually)
        // Send this AFTER other events have propagated so clients receive them in order
        let final_task_result = SendStreamingMessageResult::Task(final_task.clone());

        if let Some(ref capture_tx) = capture_a2a_clone {
            let _ = capture_tx.send(final_task_result.clone());
        }

        if let Some(ref stream) = stream_tx_clone {
            let _ = stream.send(final_task_result).await;
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
    #[tracing::instrument(
        name = "radkit.session.get_or_create",
        skip(self),
        fields(
            app_name = %app_name,
            user_id = obs_utils::hash_user_id(user_id),
            context.id = tracing::field::Empty,
            otel.kind = "internal",
        )
    )]
    async fn get_or_create_session(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: &Option<String>,
    ) -> AgentResult<crate::sessions::Session> {
        let session = if let Some(context_id) = context_id {
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
        }?;

        // Record context.id on span
        let span = tracing::Span::current();
        span.record("context.id", session.id.as_str());

        Ok(session)
    }

    /// Get existing task or generate new task ID
    #[tracing::instrument(
        name = "radkit.task.get",
        skip(self),
        fields(
            app_name = %app_name,
            user_id = obs_utils::hash_user_id(user_id),
            task.id = tracing::field::Empty,
            task.continuous = false,
            otel.kind = "internal",
        )
    )]
    async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &Option<String>,
    ) -> AgentResult<Option<Task>> {
        let result = if let Some(task_id) = task_id {
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

                    // Record task.id and mark as continuous
                    let span = tracing::Span::current();
                    span.record("task.id", task.id.as_str());
                    span.record("task.continuous", true);

                    Ok(Some(task))
                }
                None => Err(AgentError::TaskNotFound {
                    task_id: task_id.clone(),
                }),
            }
        } else {
            Ok(None)
        }?;

        Ok(result)
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

        // Capture current span for trace propagation
        let forward_span = tracing::Span::current();

        // Spawn task to forward events to appropriate channels
        tokio::spawn(
            async move {
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
            }
            .instrument(forward_span),
        );

        Ok(())
    }

    // ===== Event-Driven Turn-Based Execution =====

    /// Execute a single turn of conversation (non-blocking)
    /// Returns true if more turns are needed, false if complete
    #[tracing::instrument(
        name = "radkit.conversation.execute_turn",
        skip(self, context),
        fields(
            task.id = %context.task_id,
            otel.kind = "internal",
        )
    )]
    async fn execute_turn(&self, context: &ExecutionContext) -> AgentResult<bool> {
        // Get conversation history for LLM
        let history = context.get_llm_conversation().await?;

        // Build LLM request
        let llm_request = LlmRequest {
            messages: history,
            current_task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            system_instruction: Some(self.instruction.clone()),
            config: crate::models::GenerateContentConfig::default(),
            toolset: self.toolset.clone(),
            metadata: HashMap::new(),
        };

        // Call LLM (async but we await here - LLM calls are typically fast)
        let response = self
            .model
            .generate_content(llm_request, self.env_resolver.clone())
            .await
            .map_err(|e| AgentError::LlmError {
                source: Box::new(e),
            })?;

        // Convert response to content
        let response_content = Content::from_llm_response(
            response,
            context.task_id.clone(),
            context.context_id.clone(),
        );

        // Emit agent message
        context.emit_message(response_content.clone()).await?;

        // Check for function calls
        let has_function_calls = response_content.has_function_calls();

        if has_function_calls {
            // Dispatch tools asynchronously
            // When tools complete, they emit UserMessage with FunctionResponse
            // The completion handler will call execute_turn again
            self.dispatch_tool_calls(&response_content, context).await?;
            Ok(true)
        } else {
            // No more function calls - conversation complete
            // NOTE: Do NOT change task state here - leave it in current state
            // Tools are responsible for updating task state if needed
            Ok(false)
        }
    }

    /// Dispatch tool calls asynchronously (non-blocking)
    #[tracing::instrument(
        name = "radkit.tools.dispatch_calls",
        skip(self, content, context),
        fields(
            task.id = %context.task_id,
            tools.count = tracing::field::Empty,
            otel.kind = "internal",
        )
    )]
    async fn dispatch_tool_calls(
        &self,
        content: &Content,
        context: &ExecutionContext,
    ) -> AgentResult<()> {
        use crate::models::content::ContentPart;

        let function_calls = content.get_function_calls();
        let tool_count = function_calls.len();

        // Record tool count on parent span
        let span = tracing::Span::current();
        span.record("tools.count", tool_count);

        for part in function_calls {
            if let ContentPart::FunctionCall {
                name,
                arguments,
                tool_use_id,
                ..
            } = part
            {
                let tool_use_id = tool_use_id
                    .clone()
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                // Spawn async tool execution (fire and forget)
                let executor = self.clone();
                let name = name.clone();
                let arguments = arguments.clone();
                let context_clone = context.clone();

                tokio::spawn(async move {
                    // Create individual span for this tool execution
                    let tool_span = tracing::info_span!(
                        "radkit.tool.execute_call",
                        tool.name = %name,
                        tool.success = tracing::field::Empty,
                        tool.duration_ms = tracing::field::Empty,
                        otel.kind = "internal",
                    );
                    let _guard = tool_span.enter();

                    let start_time = std::time::Instant::now();

                    // Execute tool
                    let tool_result = executor
                        .execute_tool_call(&name, &arguments, &context_clone)
                        .await;
                    let duration_ms = start_time.elapsed().as_millis() as u64;

                    // Record tool metrics on span
                    tool_span.record("tool.success", tool_result.success);
                    tool_span.record("tool.duration_ms", duration_ms);
                    obs_utils::record_tool_execution_metric(&name, tool_result.success);

                    // Create and emit function response as UserMessage (input back to agent)
                    let mut response_content = Content::new(
                        context_clone.task_id.clone(),
                        context_clone.context_id.clone(),
                        Uuid::new_v4().to_string(),
                        MessageRole::User,
                    );

                    // Create function response part
                    let function_response = ContentPart::FunctionResponse {
                        name: name.clone(),
                        success: tool_result.success,
                        result: tool_result.data.clone(),
                        error_message: tool_result.error_message.clone(),
                        tool_use_id: Some(tool_use_id.clone()),
                        duration_ms: Some(duration_ms),
                        metadata: None,
                    };

                    response_content.parts.push(function_response.clone());

                    // Emit function response as UserMessage
                    // This will trigger the completion handler to execute next turn
                    let _ = context_clone.emit_user_input(response_content).await;
                });
            }
        }

        Ok(())
    }

    /// Setup handler for tool completion events and automatic resumption
    #[tracing::instrument(
        name = "radkit.conversation.setup_handler",
        skip(self, context, completion_tx),
        fields(
            task.id = %context.task_id,
            otel.kind = "internal",
        )
    )]
    async fn setup_tool_completion_handler(
        &self,
        context: &ExecutionContext,
        completion_tx: tokio::sync::oneshot::Sender<()>,
    ) -> AgentResult<()> {
        let event_bus = self.event_processor.event_bus();
        let mut event_receiver = event_bus.subscribe(context.task_id.clone()).await?;

        let executor = self.clone();
        let context_clone = context.clone();
        let task_id = context.task_id.clone();
        let max_iterations = self.config.max_iterations;

        tokio::spawn(async move {
            let handler_span = tracing::info_span!(
                "radkit.conversation.handler_loop",
                task.id = %task_id,
                max_iterations = max_iterations,
                otel.kind = "internal",
            );
            let _guard = handler_span.enter();

            let mut turn_count = 1; // Initial turn already executed before handler setup

            while let Some(event) = event_receiver.recv().await {
                // Listen for UserMessage events containing FunctionResponse parts
                if let crate::sessions::SessionEventType::UserMessage { content } =
                    &event.event_type
                {
                    // If this message has function responses, execute next turn
                    if content.has_function_responses() {
                        turn_count += 1;

                        if turn_count > max_iterations {
                            // Iteration limit exceeded
                            tracing::warn!(
                                turn_count = turn_count,
                                max_iterations = max_iterations,
                                "Iteration limit exceeded"
                            );

                            // Emit task failed status
                            let error_msg = format!(
                                "Maximum iteration limit ({}) exceeded. The agent made {} turns without completing.",
                                max_iterations,
                                turn_count - 1
                            );
                            let _ = context_clone
                                .emit_task_status_update(TaskState::Failed, Some(error_msg), None)
                                .await;

                            // Signal completion
                            let _ = completion_tx.send(());
                            break;
                        }

                        tracing::debug!(
                            turn_count = turn_count,
                            max_iterations = max_iterations,
                            "Function response received, executing next turn"
                        );
                        let _ = executor.execute_turn(&context_clone).await;
                    }
                }

                // Check if conversation is complete (no more function calls in last agent message)
                if let crate::sessions::SessionEventType::AgentMessage { content } =
                    &event.event_type
                {
                    if !content.has_function_calls() {
                        // No function calls - conversation complete
                        tracing::info!(
                            turn_count = turn_count,
                            "Conversation complete, no more function calls"
                        );
                        let _ = completion_tx.send(());
                        break;
                    }
                }
            }
        });

        Ok(())
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
            env_resolver: None,
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
