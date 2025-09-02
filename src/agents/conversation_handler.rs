//! Unified Conversation Handler
//!
//! This module contains the unified conversation handling logic that uses
//! SessionService directly for persistence and EventBus for streaming.

use crate::a2a::{
    Message, MessageRole, Part, SendStreamingMessageResult, Task, TaskState, TaskStatus,
    TaskStatusUpdateEvent,
};
use crate::errors::{AgentError, AgentResult};
use crate::events::ExecutionContext;
use crate::models::{
    LlmRequest,
    content::{Content, FunctionResponseParams},
};

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing;
use uuid::Uuid;

/// Result of processing tool calls in a message
#[derive(Debug)]
pub struct ToolProcessingResult {
    pub has_function_calls: bool,
    pub response_parts: Vec<Part>,
}

/// Unified trait for handling different conversation modes
#[async_trait]
pub trait ConversationHandler {
    /// Handle iteration limit exceeded scenario
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ExecutionContext,
        max_iterations: usize,
    ) -> AgentResult<()>;
}

/// Unified non-streaming conversation handler
pub struct StandardConversationHandler<'a> {
    pub agent: &'a super::Agent,
}

#[async_trait]
impl<'a> ConversationHandler for StandardConversationHandler<'a> {
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ExecutionContext,
        _max_iterations: usize,
    ) -> AgentResult<()> {
        // Get current task state and emit failure
        let task = self
            .agent
            .event_processor()
            .get_task(&context.app_name, &context.user_id, &context.task_id)
            .await?;

        let old_state = task
            .map(|t| t.status.state)
            .unwrap_or(crate::a2a::TaskState::Working);

        context
            .emit_task_status_update(old_state, crate::a2a::TaskState::Failed, None)
            .await
    }
}

/// Unified streaming conversation handler
pub struct StreamingConversationHandler<'a> {
    pub agent: &'a super::Agent,
    pub stream_tx: mpsc::Sender<SendStreamingMessageResult>,
}

#[async_trait]
impl<'a> ConversationHandler for StreamingConversationHandler<'a> {
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ExecutionContext,
        _max_iterations: usize,
    ) -> AgentResult<()> {
        // Get current task state and emit failure
        let task = self
            .agent
            .event_processor()
            .get_task(&context.app_name, &context.user_id, &context.task_id)
            .await?;

        let old_state = task
            .map(|t| t.status.state)
            .unwrap_or(crate::a2a::TaskState::Working);

        let status_event = TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            status: crate::a2a::TaskStatus {
                state: crate::a2a::TaskState::Failed,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: None,
            },
            is_final: true,
            metadata: None,
        };

        // Send to both streaming and persistent storage
        let _ = self
            .stream_tx
            .send(SendStreamingMessageResult::TaskStatusUpdate(status_event))
            .await;
        context
            .emit_task_status_update(old_state, crate::a2a::TaskState::Failed, None)
            .await
    }
}

/// Unified conversation executor that handles the core conversation loop
pub struct ConversationExecutor<'a> {
    agent: &'a super::Agent,
}

impl<'a> ConversationExecutor<'a> {
    pub fn new(agent: &'a super::Agent) -> Self {
        Self { agent }
    }

    pub async fn execute_conversation_core<H: ConversationHandler>(
        &self,
        handler: &H,
        context: &ExecutionContext,
        _initial_message: &Message,
    ) -> AgentResult<()> {
        let max_iterations = self.agent.config().max_iterations;
        let mut iteration = 0;

        // Emit TaskCreated event at the start of conversation
        let task = Task {
            kind: "task".to_string(),
            id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            status: TaskStatus {
                state: TaskState::Submitted,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: None,
            },
            history: vec![],
            artifacts: vec![],
            metadata: None,
        };
        context.emit_task_created(task).await?;

        // Emit the initial user message now that the task exists
        let mut initial_msg = _initial_message.clone();
        initial_msg.context_id = Some(context.context_id.clone());
        initial_msg.task_id = Some(context.task_id.clone());
        context.emit_user_input(initial_msg).await?;

        loop {
            if iteration >= max_iterations {
                handler
                    .handle_iteration_limit_exceeded(context, max_iterations)
                    .await?;
                break;
            }

            iteration += 1;
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
                system_instruction: Some(self.agent.instruction().to_string()),
                config: crate::models::GenerateContentConfig::default(),
                toolset: self.agent.toolset().cloned(),
                metadata: context.current_params.metadata.clone().unwrap_or_default(),
            };

            // Generate response from LLM
            let response = self
                .agent
                .model()
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
            let tool_result = self.process_tool_calls(&response_content, context).await?;

            if !tool_result.has_function_calls {
                // No more tool calls, conversation complete
                // Task state will remain in its current state (likely Working or whatever tools set it to)
                // Tools are responsible for marking tasks as Completed/Failed using update_status
                break;
            }

            // Continue loop for next iteration
        }

        Ok(())
    }

    /// Process tool calls in a message and emit function responses
    async fn process_tool_calls(
        &self,
        content: &Content,
        context: &ExecutionContext,
    ) -> AgentResult<ToolProcessingResult> {
        let mut has_function_calls = false;

        for part in &content.parts {
            if let crate::models::content::ContentPart::FunctionCall {
                name,
                arguments,
                tool_use_id,
                ..
            } = part
            {
                has_function_calls = true;
                let start_time = std::time::Instant::now();

                // Execute the tool
                let tool_result = self.execute_tool_call(name, arguments, context).await;
                let duration_ms = start_time.elapsed().as_millis() as u64;

                // Create and emit function response
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

                context.emit_message(response_content).await?;
            }
        }

        Ok(ToolProcessingResult {
            has_function_calls,
            response_parts: Vec::new(), // Not used in unified system
        })
    }

    /// Execute a tool call and return the result
    async fn execute_tool_call(
        &self,
        name: &str,
        args: &serde_json::Value,
        context: &ExecutionContext,
    ) -> crate::tools::base_tool::ToolResult {
        if let Some(toolset) = self.agent.toolset() {
            let available_tools = toolset.get_tools().await;

            if let Some(tool) = available_tools.iter().find(|t| t.name() == name) {
                // Convert serde_json::Value to HashMap<String, Value>
                let args_map: HashMap<String, serde_json::Value> =
                    if let serde_json::Value::Object(map) = args.clone() {
                        map.into_iter().collect()
                    } else {
                        HashMap::new()
                    };

                tool.run_async(args_map, context).await
            } else {
                crate::tools::base_tool::ToolResult::error(format!("Tool '{name}' not found"))
            }
        } else {
            crate::tools::base_tool::ToolResult::error("No toolset available".to_string())
        }
    }
}
