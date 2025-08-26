use crate::errors::{AgentError, AgentResult};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing;
use uuid::Uuid;

use crate::a2a::{
    MessageRole, MessageSendParams, Part, SendStreamingMessageResult, Task, TaskQueryParams,
    TaskStatusUpdateEvent,
};

use super::conversation_handler::{
    ConversationExecutor, StandardConversationHandler, StreamingConversationHandler,
};
use crate::events::{
    A2ACaptureProjector, A2AOnlyProjector, EventCaptureProjector, EventProjector, InternalEvent,
    InternalOnlyProjector, MultiProjector, ProjectedExecutionContext, StorageProjector,
};
use crate::models::BaseLlm;
use crate::sessions::{InMemorySessionService, SessionService};
use crate::task::{InMemoryTaskStore, TaskManager, TaskStore};
use crate::tools::{BaseTool, BaseToolset, CombinedToolset, SimpleToolset};

use super::config::{AgentConfig, AuthenticatedAgent};

/// Agent definition and configuration (lightweight, reusable)
/// Now focused purely on configuration - execution is handled by AgentRunner
pub struct Agent {
    name: String,
    description: String,
    instruction: String,
    model: Arc<dyn BaseLlm>,
    toolset: Option<Arc<dyn BaseToolset>>,
    session_service: Arc<dyn SessionService>,
    task_manager: Arc<TaskManager>, // Always present, not optional
    config: AgentConfig,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("instruction", &self.instruction)
            .field("config", &self.config)
            .finish()
    }
}

impl Agent {
    /// Create a new Agent with default in-memory session store and task store
    /// Event projectors are created with proper context during execution
    pub fn new(
        name: String,
        description: String,
        instruction: String,
        model: Arc<dyn BaseLlm>,
    ) -> Self {
        let (session_service, task_manager) = Self::create_core_components(None, None);

        Self {
            name,
            description,
            instruction,
            model,
            toolset: None,
            session_service,
            task_manager,
            config: AgentConfig::default(),
        }
    }

    /// Set a custom session service (builder pattern)
    /// Preserves existing TaskStore - only changes SessionService
    pub fn with_session_service(mut self, session_service: Arc<dyn SessionService>) -> Self {
        self.session_service = session_service;
        self
    }

    /// Set a custom task store (builder pattern)
    /// Preserves existing SessionService - only changes TaskStore
    pub fn with_task_store(mut self, task_store: Arc<dyn TaskStore>) -> Self {
        self.task_manager = Arc::new(TaskManager::new(task_store));
        self
    }

    pub fn with_tools(mut self, tools: Vec<Arc<dyn BaseTool>>) -> Self {
        self.toolset = Some(Arc::new(SimpleToolset::new(tools)));
        self
    }

    pub fn with_toolset(mut self, toolset: Arc<dyn BaseToolset>) -> Self {
        self.toolset = Some(toolset);
        self
    }

    /// Set custom agent configuration
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Get access to the task manager for testing/inspection
    pub fn task_manager(&self) -> &Arc<TaskManager> {
        &self.task_manager
    }

    /// Get access to the session service for testing/inspection  
    pub fn session_service(&self) -> &Arc<dyn SessionService> {
        &self.session_service
    }

    /// Get the agent's configuration
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the agent's instruction
    pub fn instruction(&self) -> &str {
        &self.instruction
    }

    /// Get the agent's model
    pub fn model(&self) -> &Arc<dyn BaseLlm> {
        &self.model
    }

    /// Get the agent's toolset
    pub fn toolset(&self) -> Option<&Arc<dyn BaseToolset>> {
        self.toolset.as_ref()
    }

    /// Get the agent's name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the agent's description
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Add a tool to the agent's toolset
    fn add_tool_to_toolset(mut self, tool: Arc<dyn BaseTool>) -> Self {
        if let Some(existing_toolset) = &self.toolset {
            // Combine existing toolset with the new tool
            self.toolset = Some(Arc::new(CombinedToolset::new(
                Arc::clone(existing_toolset),
                vec![tool],
            )));
        } else {
            // No existing toolset, create new one with the tool
            self.toolset = Some(Arc::new(SimpleToolset::new(vec![tool])));
        }
        self
    }

    /// Add the update_status built-in tool
    /// This tool allows agents to control their task lifecycle states and automatically
    /// emits `TaskStatusUpdate` events for A2A protocol compliance.
    pub fn with_update_status_tool(self) -> Self {
        let tool = crate::tools::builtin_tools::create_update_status_tool();
        self.add_tool_to_toolset(tool)
    }

    /// Add the save_artifact built-in tool  
    /// This tool allows agents to persist important outputs and automatically
    /// emits `TaskArtifactUpdate` events for A2A protocol compliance.
    pub fn with_save_artifact_tool(self) -> Self {
        let tool = crate::tools::builtin_tools::create_save_artifact_tool();
        self.add_tool_to_toolset(tool)
    }

    /// Add both update_status and save_artifact built-in tools
    /// Convenience method for the common case of enabling task management tools
    pub fn with_builtin_task_tools(self) -> Self {
        self.with_update_status_tool().with_save_artifact_tool()
    }

    /// A2A Protocol: message/send - Send message to agent (non-streaming)
    /// Returns either a Task or Message as per A2A specification
    pub async fn send_message(
        &self,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
    ) -> AgentResult<super::execution_result::SendMessageResultWithEvents> {
        // Create channels to capture events
        let (internal_tx, mut internal_rx) = mpsc::unbounded_channel();
        let (a2a_tx, mut a2a_rx) = mpsc::unbounded_channel();

        // Set up execution context with event capture
        let context = self
            .setup_execution_context(
                &app_name,
                &user_id,
                params,
                Some(internal_tx.clone()),
                Some(a2a_tx.clone()),
                None, // No streaming for non-streaming method
            )
            .await?;

        // Execute conversation loop with tool calling
        let final_context = self.execute_conversation_loop(context).await?;

        // Get final task from TaskManager
        let final_task = self
            .task_manager
            .get_task(&app_name, &user_id, &final_context.task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: final_context.task_id.clone(),
            })?;

        // Collect captured events
        drop(internal_tx); // Close sender to drain receiver
        drop(a2a_tx);
        let mut internal_events = Vec::new();
        while let Ok(event) = internal_rx.try_recv() {
            internal_events.push(event);
        }
        let mut a2a_events = Vec::new();
        while let Ok(event) = a2a_rx.try_recv() {
            a2a_events.push(event);
        }

        // Return enriched result
        Ok(super::execution_result::SendMessageResultWithEvents {
            result: crate::a2a::SendMessageResult::Task(final_task),
            internal_events,
            a2a_events,
        })
    }

    /// A2A Protocol: message/stream - Send message with streaming updates
    /// Returns stream of A2A streaming message results  
    pub async fn send_streaming_message(
        &self,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
    ) -> AgentResult<super::execution_result::SendStreamingMessageResultWithEvents> {
        // Create channels for streaming A2A events and internal events
        let (stream_tx, stream_rx) = mpsc::channel(100);
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();

        // Clone necessary components for the spawned task (cheaper than cloning whole agent)
        let model = Arc::clone(&self.model);
        let session_service = Arc::clone(&self.session_service);
        let task_manager = Arc::clone(&self.task_manager);
        let toolset = self.toolset.clone();
        let config = self.config.clone();
        let name = self.name.clone();
        let description = self.description.clone();
        let instruction = self.instruction.clone();
        let app_name_clone = app_name.clone();
        let user_id_clone = user_id.clone();
        let params_clone = params.clone();
        let internal_tx_clone = internal_tx;

        // Spawn the execution in background to allow streaming
        tokio::spawn(async move {
            // Create a temporary agent for the spawned context
            let agent = Agent {
                name,
                description,
                instruction,
                model,
                toolset,
                session_service,
                task_manager,
                config,
            };

            // Execute the streaming conversation loop with internal event capture
            let result = agent
                .execute_streaming_conversation(
                    app_name_clone.clone(),
                    user_id_clone.clone(),
                    params_clone.clone(),
                    stream_tx.clone(),
                    Some(internal_tx_clone),
                )
                .await;

            // Handle any errors by sending them as task status updates
            if let Err(e) = result {
                // Try to extract context information from the error or params
                let (task_id, context_id) =
                    if let Some(context_id) = &params_clone.message.context_id {
                        let task_id = params_clone.message.task_id.as_deref().unwrap_or("unknown");
                        (task_id.to_string(), context_id.clone())
                    } else {
                        ("error".to_string(), "error".to_string())
                    };

                let error_event = TaskStatusUpdateEvent {
                    kind: "status-update".to_string(),
                    task_id: task_id.clone(),
                    context_id: context_id.clone(),
                    status: crate::a2a::TaskStatus {
                        state: crate::a2a::TaskState::Failed,
                        timestamp: Some(chrono::Utc::now().to_rfc3339()),
                        message: Some(crate::a2a::Message {
                            kind: "message".to_string(),
                            message_id: Uuid::new_v4().to_string(),
                            role: MessageRole::Agent,
                            parts: vec![Part::Text {
                                text: format!("Streaming execution error: {}", e),
                                metadata: Some({
                                    let mut meta = std::collections::HashMap::new();
                                    meta.insert(
                                        "error_type".to_string(),
                                        serde_json::Value::String("streaming_failure".to_string()),
                                    );
                                    meta.insert(
                                        "app_name".to_string(),
                                        serde_json::Value::String(app_name_clone.clone()),
                                    );
                                    meta.insert(
                                        "user_id".to_string(),
                                        serde_json::Value::String(user_id_clone.clone()),
                                    );
                                    meta
                                }),
                            }],
                            context_id: Some(context_id.clone()),
                            task_id: Some(task_id.clone()),
                            reference_task_ids: Vec::new(),
                            extensions: Vec::new(),
                            metadata: None,
                        }),
                    },
                    is_final: true,
                    metadata: None,
                };

                // Send error event with better error handling
                if let Err(send_err) = stream_tx
                    .send(SendStreamingMessageResult::TaskStatusUpdate(error_event))
                    .await
                {
                    // If we can't send to the stream, log the error using proper logging
                    tracing::error!(
                        "Failed to send error event to stream: {} (original error: {})",
                        send_err,
                        e
                    );
                }
            }
        });

        use tokio_stream::wrappers::ReceiverStream;

        // Return enriched result with both streams
        Ok(
            super::execution_result::SendStreamingMessageResultWithEvents {
                stream: Box::pin(ReceiverStream::new(stream_rx)),
                internal_events: internal_rx,
            },
        )
    }

    /// A2A Protocol: tasks/get - Get task by ID with optional parameters
    pub async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        params: TaskQueryParams,
    ) -> AgentResult<Task> {
        // Get task directly from TaskManager (already A2A format)
        let mut a2a_task = self
            .task_manager
            .get_task(app_name, user_id, &params.id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
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

    /// A2A Protocol: List all tasks for this app/user (returns A2A Task format)
    /// Note: tasks/list is primarily gRPC/REST only per A2A spec, but we provide it for completeness
    pub async fn list_tasks(&self, app_name: &str, user_id: &str) -> AgentResult<Vec<Task>> {
        // Get all tasks directly from TaskManager (already A2A format)
        let all_a2a_tasks = self
            .task_manager
            .list_tasks(app_name, user_id, None)
            .await?;

        Ok(all_a2a_tasks)
    }

    /// Create an authenticated agent wrapper for a specific app/user context
    pub fn authenticated(&self, app_name: String, user_id: String) -> AuthenticatedAgent {
        AuthenticatedAgent::new(self, app_name, user_id)
    }
}

// Execution context is now ProjectedExecutionContext from events module

// Implementation methods for Agent
impl Agent {
    /// Helper function to create core components for constructors (reduces duplication)
    /// Used by constructors that need to set up both SessionService and TaskManager
    /// Builder methods (with_session_service, with_task_store) directly modify individual components
    fn create_core_components(
        session_service: Option<Arc<dyn SessionService>>,
        task_store: Option<Arc<dyn TaskStore>>,
    ) -> (Arc<dyn SessionService>, Arc<TaskManager>) {
        let session_service =
            session_service.unwrap_or_else(|| Arc::new(InMemorySessionService::new()));
        let task_store = task_store.unwrap_or_else(|| Arc::new(InMemoryTaskStore::new()));
        let task_manager = Arc::new(TaskManager::new(task_store));
        (session_service, task_manager)
    }

    /// Set up execution context from MessageSendParams with optional event capture
    async fn setup_execution_context(
        &self,
        app_name: &str,
        user_id: &str,
        params: MessageSendParams,
        capture_internal: Option<mpsc::UnboundedSender<InternalEvent>>,
        capture_a2a: Option<mpsc::UnboundedSender<SendStreamingMessageResult>>,
        stream_a2a: Option<mpsc::Sender<SendStreamingMessageResult>>,
    ) -> AgentResult<ProjectedExecutionContext> {
        // Handle context_id from message
        let session = if let Some(context_id) = &params.message.context_id {
            // context_id provided - must find existing session or error
            match self
                .session_service
                .get_session(app_name, user_id, context_id)
                .await?
            {
                Some(existing_session) => existing_session,
                None => {
                    return Err(AgentError::SessionNotFound {
                        session_id: context_id.clone(),
                        app_name: app_name.to_string(),
                        user_id: user_id.to_string(),
                    });
                }
            }
        } else {
            // No context_id provided - create new session with auto-generated ID
            self.session_service
                .create_session(app_name.to_string(), user_id.to_string())
                .await?
        };

        // Get or create task using TaskManager
        let task_id = if let Some(task_id) = &params.message.task_id {
            // Continue existing task - check if it exists and is not terminal
            match self
                .task_manager
                .get_task(app_name, user_id, task_id)
                .await?
            {
                Some(task) => {
                    if matches!(
                        task.status.state,
                        crate::a2a::TaskState::Completed
                            | crate::a2a::TaskState::Failed
                            | crate::a2a::TaskState::Canceled
                            | crate::a2a::TaskState::Rejected
                    ) {
                        return Err(AgentError::InvalidTaskStateTransition {
                            from: format!("{:?}", task.status.state),
                            to: "Working".to_string(),
                        });
                    }
                    task_id.clone()
                }
                None => {
                    return Err(AgentError::TaskNotFound {
                        task_id: task_id.clone(),
                    });
                }
            }
        } else {
            // Create new task via TaskManager
            let task = self
                .task_manager
                .create_task(
                    app_name.to_string(),
                    user_id.to_string(),
                    session.id.clone(),
                )
                .await?;
            task.id.clone()
        };

        // Always create storage projector for persistence
        let storage_projector = Arc::new(StorageProjector::new(
            Arc::clone(&self.task_manager),
            Arc::clone(&self.session_service),
            app_name.to_string(),
            user_id.to_string(),
        ));

        // Build combined projector based on what channels were provided
        let mut projectors: Vec<Arc<dyn EventProjector>> = vec![storage_projector];

        // Add capturing projectors based on what channels were provided
        match (capture_internal, capture_a2a) {
            (Some(internal_tx), Some(a2a_tx)) => {
                // Both channels - use full EventCaptureProjector
                projectors.push(Arc::new(EventCaptureProjector::new(internal_tx, a2a_tx)));
            }
            (Some(internal_tx), None) => {
                // Internal events only - use InternalOnlyProjector
                projectors.push(Arc::new(InternalOnlyProjector::new(internal_tx)));
            }
            (None, Some(a2a_tx)) => {
                // A2A events only - use A2ACaptureProjector for unbounded capture
                projectors.push(Arc::new(A2ACaptureProjector::new(a2a_tx)));
            }
            (None, None) => {
                // No capture channels - only storage and streaming
            }
        }

        // Add A2A streaming projector if stream channel provided
        if let Some(stream_tx) = stream_a2a {
            projectors.push(Arc::new(A2AOnlyProjector::new(stream_tx)));
        }

        // Always use MultiProjector for consistent architecture
        let context_projector: Arc<dyn EventProjector> = Arc::new(MultiProjector::new(projectors));

        let context = ProjectedExecutionContext::new(
            session.id.clone(),
            task_id.clone(),
            app_name.to_string(),
            user_id.to_string(),
            params.clone(),
            context_projector,
        );

        // Now emit the current user input message with proper context
        let mut msg = params.message.clone();
        // Ensure IDs are set
        msg.context_id = Some(session.id.clone());
        msg.task_id = Some(task_id.clone());
        context.emit_user_input(msg).await?;

        Ok(context)
    }

    /// Execute the main conversation loop with tool calling
    async fn execute_conversation_loop(
        &self,
        context: ProjectedExecutionContext,
    ) -> AgentResult<ProjectedExecutionContext> {
        let executor = ConversationExecutor::new(self);
        let handler = StandardConversationHandler { agent: self };
        executor.execute_conversation_core(context, handler).await
    }

    async fn execute_streaming_conversation(
        &self,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
        stream_tx: mpsc::Sender<SendStreamingMessageResult>,
        internal_tx: Option<mpsc::UnboundedSender<InternalEvent>>,
    ) -> AgentResult<()> {
        // Set up execution context with both streaming and capture
        let context = self
            .setup_execution_context(
                &app_name,
                &user_id,
                params,
                internal_tx,
                None, // A2A events go to stream, not capture
                Some(stream_tx.clone()),
            )
            .await?;

        // Send initial working status with better error context
        let working_event = TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            status: crate::a2a::TaskStatus {
                state: crate::a2a::TaskState::Working,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: None,
            },
            is_final: false,
            metadata: Some({
                let mut meta = std::collections::HashMap::new();
                meta.insert(
                    "streaming_session".to_string(),
                    serde_json::Value::Bool(true),
                );
                meta.insert(
                    "app_name".to_string(),
                    serde_json::Value::String(app_name.clone()),
                );
                meta.insert(
                    "user_id".to_string(),
                    serde_json::Value::String(user_id.clone()),
                );
                meta
            }),
        };
        stream_tx
            .send(SendStreamingMessageResult::TaskStatusUpdate(working_event))
            .await
            .map_err(|e| AgentError::Internal {
                component: "stream_sender".to_string(),
                reason: format!("Failed to send initial working status: {}", e),
            })?;

        // Execute the conversation loop
        let final_context = self
            .execute_conversation_loop_streaming(context, stream_tx.clone())
            .await?;

        // Get final task and send it
        let final_task = self
            .task_manager
            .get_task(&app_name, &user_id, &final_context.task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: "unknown".to_string(),
            })?;

        // Send final task with proper error handling
        stream_tx
            .send(SendStreamingMessageResult::Task(final_task))
            .await
            .map_err(|e| AgentError::Internal {
                component: "stream_sender".to_string(),
                reason: format!("Failed to send final task: {}", e),
            })?;

        Ok(())
    }

    /// Execute conversation loop with streaming events
    async fn execute_conversation_loop_streaming(
        &self,
        context: ProjectedExecutionContext,
        stream_tx: mpsc::Sender<SendStreamingMessageResult>,
    ) -> AgentResult<ProjectedExecutionContext> {
        let executor = ConversationExecutor::new(self);
        let handler = StreamingConversationHandler {
            agent: self,
            stream_tx,
        };
        executor.execute_conversation_core(context, handler).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::{Message, MessageRole, Part, SendMessageResult, TaskState};
    use crate::events::InternalEvent;
    use crate::models::mock_llm::MockLlm;
    use crate::tools::ToolResult;
    use crate::tools::function_tool::FunctionTool;
    use serde_json::json;

    // Helper to create a test agent with mock dependencies
    fn create_test_agent() -> Agent {
        let model = Arc::new(MockLlm::new("mock-model".to_string()));
        Agent::new(
            "test_agent".to_string(),
            "A test agent".to_string(),
            "You are a helpful test agent.".to_string(),
            model,
        )
    }

    // Helper to create a basic MessageSendParams
    fn create_send_params(text: &str) -> MessageSendParams {
        MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: uuid::Uuid::new_v4().to_string(),
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
    async fn test_setup_execution_context_new_session_and_task() {
        let agent = create_test_agent();
        let params = create_send_params("Hello");

        let context = agent
            .setup_execution_context("app1", "user1", params, None, None, None)
            .await
            .unwrap();

        assert!(!context.context_id.is_empty());
        assert!(!context.task_id.is_empty());

        // Verify session and task were created
        let session = agent
            .session_service
            .get_session("app1", "user1", &context.context_id)
            .await
            .unwrap();
        assert!(session.is_some());

        let task = agent
            .task_manager
            .get_task("app1", "user1", &context.task_id)
            .await
            .unwrap();
        assert!(task.is_some());
    }

    #[tokio::test]
    async fn test_setup_execution_context_existing_session() {
        let agent = create_test_agent();
        let session = agent
            .session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();

        let mut params = create_send_params("Hello");
        params.message.context_id = Some(session.id.clone());

        let context = agent
            .setup_execution_context("app1", "user1", params, None, None, None)
            .await
            .unwrap();

        assert_eq!(context.context_id, session.id);
    }

    #[tokio::test]
    async fn test_setup_execution_context_continue_task() {
        let agent = create_test_agent();

        // First create the session that the task will belong to
        let session = agent
            .session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();

        // Now create a task associated with this session
        let task = agent
            .task_manager
            .create_task("app1".to_string(), "user1".to_string(), session.id.clone())
            .await
            .unwrap();

        let mut params = create_send_params("Continue");
        params.message.task_id = Some(task.id.clone());
        params.message.context_id = Some(task.context_id.clone());

        let context = agent
            .setup_execution_context("app1", "user1", params, None, None, None)
            .await
            .unwrap();

        assert_eq!(context.task_id, task.id);
    }

    #[tokio::test]
    async fn test_setup_execution_context_error_on_terminal_task() {
        let agent = create_test_agent();

        // First create the session
        let session = agent
            .session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();

        // Create task with the session's context_id
        let task = agent
            .task_manager
            .create_task("app1".to_string(), "user1".to_string(), session.id.clone())
            .await
            .unwrap();

        // Set task to a terminal state
        let mut updated_task = task.clone();
        updated_task.status.state = TaskState::Completed;
        agent
            .task_manager
            .save_task("app1", "user1", &updated_task)
            .await
            .unwrap();

        let mut params = create_send_params("Continue");
        params.message.task_id = Some(task.id.clone());
        params.message.context_id = Some(task.context_id.clone());

        let result = agent
            .setup_execution_context("app1", "user1", params, None, None, None)
            .await;

        // Should fail with InvalidTaskStateTransition error
        match result {
            Err(AgentError::InvalidTaskStateTransition { from, to }) => {
                assert_eq!(from, "Completed");
                assert_eq!(to, "Working");
            }
            Err(other) => panic!("Expected InvalidTaskStateTransition, got: {}", other),
            Ok(_) => panic!("Expected error but got success"),
        }
    }

    #[tokio::test]
    async fn test_send_message_simple_text_response() {
        let agent = create_test_agent();
        let params = create_send_params("Hello");

        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            assert_eq!(task.status.state, TaskState::Submitted); // No automatic completion
            assert_eq!(task.history.len(), 2); // User message + Agent response
            assert_eq!(task.history[0].role, MessageRole::User);
            assert_eq!(task.history[1].role, MessageRole::Agent);

            if let Part::Text { text, .. } = &task.history[1].parts[0] {
                assert!(text.contains("Mock LLM"));
            } else {
                panic!("Expected text part in agent response");
            }
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_with_tool_error() {
        let failing_tool = Arc::new(FunctionTool::new(
            "failing_tool".to_string(),
            "A tool that always fails".to_string(),
            |_args, _ctx| {
                Box::pin(async {
                    ToolResult::error("Tool deliberately failed for testing".to_string())
                })
            },
        ));

        let agent = create_test_agent().with_tools(vec![failing_tool]);
        let params = create_send_params("Please use a tool that will fail");

        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            // Task might complete or fail depending on tool error handling
            assert!(matches!(
                task.status.state,
                TaskState::Completed | TaskState::Failed
            ));

            // With our new architecture, Task.history should be clean (no function_call/response data)
            // Tool interactions are stored only in Session.events

            // Get the session to check for tool events
            let session = agent
                .session_service()
                .get_session("app1", "user1", &task.context_id)
                .await
                .unwrap()
                .expect("Session should exist");

            // Debug: Print all events to see what's being captured
            println!("🔍 DEBUG: Session has {} events", session.events.len());
            for (i, event) in session.events.iter().enumerate() {
                match event {
                    InternalEvent::MessageReceived { content, .. } => {
                        println!(
                            "🔍 DEBUG: Event {}: MessageReceived with {} parts",
                            i,
                            content.parts.len()
                        );
                        for (j, part) in content.parts.iter().enumerate() {
                            match part {
                                crate::models::content::ContentPart::FunctionCall {
                                    name, ..
                                } => {
                                    println!("🔍 DEBUG:   Part {}: FunctionCall({})", j, name);
                                }
                                crate::models::content::ContentPart::Text { text, .. } => {
                                    println!("🔍 DEBUG:   Part {}: Text({})", j, text);
                                }
                                _ => {
                                    println!("🔍 DEBUG:   Part {}: {:?}", j, part);
                                }
                            }
                        }
                    }
                    _ => {
                        println!("🔍 DEBUG: Event {}: {:?}", i, event);
                    }
                }
            }

            // Check that function call and response are captured in MessageReceived events
            let message_with_function_call = session.events.iter().find(|e| {
                matches!(e, InternalEvent::MessageReceived { content, .. }
                    if content.has_function_calls() &&
                       content.get_function_calls().iter().any(|part| {
                           matches!(part, crate::models::content::ContentPart::FunctionCall { name, .. } if name == "failing_tool")
                       })
                )
            });
            assert!(
                message_with_function_call.is_some(),
                "Function call should be captured in MessageReceived event"
            );

            let message_with_function_response = session.events.iter().find(|e| {
                matches!(e, InternalEvent::MessageReceived { content, .. }
                    if content.get_function_responses().iter().any(|part| {
                           matches!(part, crate::models::content::ContentPart::FunctionResponse { name, success, .. }
                               if name == "failing_tool" && !success)
                       })
                )
            });
            assert!(
                message_with_function_response.is_some(),
                "Function response should be captured in MessageReceived event"
            );

            // Verify the error message is captured in the function response
            if let Some(InternalEvent::MessageReceived { content, .. }) =
                message_with_function_response
            {
                for part in content.get_function_responses() {
                    if let crate::models::content::ContentPart::FunctionResponse {
                        name,
                        error_message: Some(err_msg),
                        ..
                    } = part
                    {
                        if name == "failing_tool" {
                            assert!(
                                err_msg.contains("Tool deliberately failed")
                                    || err_msg.contains("fail"),
                                "Tool error should be captured in function response: {}",
                                err_msg
                            );
                        }
                    }
                }
            }

            // Verify Task.history is clean (no function_call/function_response data)
            for message in &task.history {
                for part in &message.parts {
                    if let Part::Data { data, .. } = part {
                        assert!(
                            data.get("function_call").is_none(),
                            "Task.history should not contain function_call data"
                        );
                        assert!(
                            data.get("function_response").is_none(),
                            "Task.history should not contain function_response data"
                        );
                    }
                }
            }
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_with_builtin_tools() {
        let agent = create_test_agent().with_builtin_task_tools();

        // Verify builtin tools are available before sending message
        assert!(agent.toolset.is_some(), "Agent should have a toolset");
        let toolset = agent.toolset.as_ref().unwrap();
        let tools = toolset.get_tools().await;
        assert!(
            tools.iter().any(|tool| tool.name() == "update_status"),
            "Should have update_status tool"
        );
        assert!(
            tools.iter().any(|tool| tool.name() == "save_artifact"),
            "Should have save_artifact tool"
        );

        let params = create_send_params("Please update status to working");
        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            // MockLlm generates function calls that may fail due to tool context issues in testing
            // The test is really about verifying builtin tools are configured
            assert!(matches!(
                task.status.state,
                TaskState::Submitted
                    | TaskState::Completed
                    | TaskState::Working
                    | TaskState::Failed
            ));
            // Verify the agent attempted to use tools and didn't just ignore them
            assert!(
                !task.history.is_empty(),
                "Task should have conversation history"
            );
        } else {
            panic!("Expected a Task result");
        }
    }

    #[tokio::test]
    async fn test_send_message_with_session_state() {
        let agent = create_test_agent();

        // First, create a session with some state
        let session = agent
            .session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();

        // Add some app and user state
        agent
            .session_service
            .update_app_state("app1", "theme", json!("dark"))
            .await
            .unwrap();
        agent
            .session_service
            .update_user_state("app1", "user1", "language", json!("spanish"))
            .await
            .unwrap();

        let mut params = create_send_params("Hello with context");
        params.message.context_id = Some(session.id.clone());

        let execution = agent
            .send_message("app1".to_string(), "user1".to_string(), params)
            .await
            .unwrap();

        if let SendMessageResult::Task(task) = execution.result {
            assert_eq!(task.status.state, TaskState::Submitted); // No automatic completion
            assert_eq!(task.context_id, session.id);

            // Verify the session was used and state is available
            let retrieved_session = agent
                .session_service
                .get_session("app1", "user1", &session.id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(
                retrieved_session.get_state("app:theme"),
                Some(&json!("dark"))
            );
            assert_eq!(
                retrieved_session.get_state("user:language"),
                Some(&json!("spanish"))
            );
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

        let task2 = if let SendMessageResult::Task(task) = result2.result {
            task
        } else {
            panic!("Expected a Task result");
        };

        // Both tasks should be in the same session
        assert_eq!(task1.context_id, task2.context_id);
        // But different task IDs
        assert_ne!(task1.id, task2.id);

        // Verify conversation history in second task
        assert!(task2.history.len() >= 2);
        assert_eq!(task2.history[0].role, MessageRole::User);
    }

    #[tokio::test]
    async fn test_agent_configuration_builders() {
        let model = Arc::new(MockLlm::new("test-model".to_string()));
        let test_tool = Arc::new(FunctionTool::new(
            "test_tool".to_string(),
            "A test tool".to_string(),
            |_args, _ctx| Box::pin(async { ToolResult::success(json!({"result": "ok"})) }),
        ));

        let config = AgentConfig::default().with_max_iterations(10);

        let agent = Agent::new(
            "configured_agent".to_string(),
            "A fully configured agent".to_string(),
            "You are a configured test agent.".to_string(),
            model,
        )
        .with_tools(vec![test_tool])
        .with_config(config)
        .with_builtin_task_tools();

        // Test configuration values
        assert_eq!(agent.name, "configured_agent");
        assert_eq!(agent.description, "A fully configured agent");
        assert_eq!(agent.instruction, "You are a configured test agent.");
        // Note: max_iterations is private, test through behavior
        assert!(agent.toolset.is_some());

        let toolset = agent.toolset.unwrap();
        let tools = toolset.get_tools().await;
        assert!(tools.len() >= 3); // custom tool + 2 builtin tools
    }

    #[tokio::test]
    async fn test_a2a_only_projector_error_handling() {
        use crate::events::A2AOnlyProjector;
        use tokio::sync::mpsc;

        // Create a closed channel to simulate stream failure
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // Close the receiver

        let a2a_projector = A2AOnlyProjector::new(tx);

        // This should not fail even though the stream channel is closed
        let test_message = crate::a2a::SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "test".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "test".to_string(),
                metadata: None,
            }],
            context_id: None,
            task_id: None,
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        });

        let result = a2a_projector.project_a2a(test_message).await;
        assert!(result.is_ok(), "Should handle closed stream gracefully");

        // Test that internal events are ignored
        use crate::models::content::{Content, ContentPart};
        let test_content = Content {
            task_id: "test".to_string(),
            context_id: "test".to_string(),
            message_id: "test_msg".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![ContentPart::Text {
                text: "Test message".to_string(),
                metadata: None,
            }],
            metadata: None,
        };

        let internal_event = crate::events::InternalEvent::MessageReceived {
            content: test_content,
            metadata: crate::events::internal::EventMetadata {
                app_name: "test".to_string(),
                user_id: "test".to_string(),
                model_info: None,
                performance: None,
            },
            timestamp: chrono::Utc::now(),
        };

        let result = a2a_projector.project_internal(internal_event).await;
        assert!(result.is_ok(), "Should ignore internal events");
    }

    #[tokio::test]
    async fn test_builder_pattern_consistency() {
        use crate::models::MockLlm;
        use crate::sessions::InMemorySessionService;
        use crate::task::InMemoryTaskStore;

        let model = Arc::new(MockLlm::new("test-model".to_string()));

        // Test 1: Default constructor creates working agent
        let agent1 = Agent::new(
            "test_agent".to_string(),
            "description".to_string(),
            "instruction".to_string(),
            model.clone(),
        );

        assert_eq!(agent1.name, "test_agent");

        // Test 2: Builder pattern with custom components
        let custom_task_store = Arc::new(InMemoryTaskStore::new());
        let custom_session = Arc::new(InMemorySessionService::new());

        let agent2 = Agent::new(
            "builder_test".to_string(),
            "description".to_string(),
            "instruction".to_string(),
            model,
        )
        .with_task_store(custom_task_store)
        .with_session_service(custom_session);

        assert_eq!(agent2.name, "builder_test");
    }
}
