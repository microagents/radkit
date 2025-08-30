//! Conversation Handler
//!
//! This module contains the conversation handling logic that was extracted from agent.rs
//! to eliminate code duplication between streaming and non-streaming conversation loops.

use crate::a2a::{Message, MessageRole, Part, SendStreamingMessageResult, TaskStatusUpdateEvent};
use crate::errors::{AgentError, AgentResult};
use crate::events::ProjectedExecutionContext;
use crate::models::{
    GenerateContentConfig, LlmRequest,
    content::{Content, FunctionResponseParams},
};
use crate::tools::SimpleToolset;

use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing;
use uuid::Uuid;

/// Result of processing tool calls in a message
#[derive(Debug)]
pub struct ToolProcessingResult {
    pub has_function_calls: bool,
    pub response_parts: Vec<Part>,
}

/// Trait for handling different conversation modes (streaming vs non-streaming)
#[async_trait]
pub trait ConversationHandler {
    /// Handle iteration limit exceeded scenario
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ProjectedExecutionContext,
        max_iterations: usize,
    ) -> AgentResult<()>;
}

/// Non-streaming conversation handler
pub struct StandardConversationHandler<'a> {
    pub agent: &'a super::Agent,
}

#[async_trait]
impl<'a> ConversationHandler for StandardConversationHandler<'a> {
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ProjectedExecutionContext,
        _max_iterations: usize,
    ) -> AgentResult<()> {
        // Emit task failure event - let the agent/tools decide final state
        let old_state = self
            .agent
            .task_manager()
            .get_task(&context.app_name, &context.user_id, &context.task_id)
            .await
            .ok()
            .flatten()
            .map(|t| t.status.state)
            .unwrap_or(crate::a2a::TaskState::Working);

        context
            .emit_task_status_update(old_state, crate::a2a::TaskState::Failed, None)
            .await
    }
}

/// Streaming conversation handler  
pub struct StreamingConversationHandler<'a> {
    pub agent: &'a super::Agent,
    pub stream_tx: mpsc::Sender<SendStreamingMessageResult>,
}

#[async_trait]
impl<'a> ConversationHandler for StreamingConversationHandler<'a> {
    async fn handle_iteration_limit_exceeded(
        &self,
        context: &ProjectedExecutionContext,
        max_iterations: usize,
    ) -> AgentResult<()> {
        // Fail the task
        let task_status = crate::a2a::TaskStatus {
            state: crate::a2a::TaskState::Failed,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            message: Some(Message {
                kind: "message".to_string(),
                message_id: Uuid::new_v4().to_string(),
                role: MessageRole::Agent,
                parts: vec![Part::Text {
                    text: format!("Maximum iterations ({max_iterations}) exceeded"),
                    metadata: None,
                }],
                context_id: Some(context.context_id.clone()),
                task_id: Some(context.task_id.clone()),
                reference_task_ids: Vec::new(),
                extensions: Vec::new(),
                metadata: None,
            }),
        };

        // Send failure status
        let failure_event = TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            status: task_status.clone(),
            is_final: true,
            metadata: None,
        };

        // Send failure event with better error handling
        if let Err(e) = self
            .stream_tx
            .send(SendStreamingMessageResult::TaskStatusUpdate(failure_event))
            .await
        {
            tracing::warn!("Failed to stream failure event: {}", e);
            // Continue to persist the failure via storage
        }

        Ok(())
    }
}

/// Core conversation execution logic
pub struct ConversationExecutor<'a> {
    pub agent: &'a super::Agent,
}

impl<'a> ConversationExecutor<'a> {
    pub fn new(agent: &'a super::Agent) -> Self {
        Self { agent }
    }

    /// Execute the core conversation loop with the given handler
    pub async fn execute_conversation_core<H: ConversationHandler>(
        &self,
        mut context: ProjectedExecutionContext,
        handler: H,
    ) -> AgentResult<ProjectedExecutionContext> {
        let max_iterations = self.agent.config().max_iterations;
        let mut iteration = 0;

        // Set up metadata with system instruction
        self.setup_context_metadata(&mut context);

        loop {
            iteration += 1;
            if iteration > max_iterations {
                handler
                    .handle_iteration_limit_exceeded(&context, max_iterations)
                    .await?;
                break;
            }

            // Get task messages and create LlmRequest
            let llm_request = self.create_llm_request(&context).await?;

            // Process through LLM using proper LlmRequest/LlmResponse pattern
            let llm_response = self.agent.model().generate_content(llm_request).await?;

            // Content from LlmResponse
            let response_content = llm_response.message;

            // Create model info from usage metadata
            let model_info =
                llm_response
                    .usage_metadata
                    .as_ref()
                    .map(|u| crate::events::internal::ModelInfo {
                        model_name: self.agent.model().model_name().to_string(),
                        prompt_tokens: u.input_tokens.map(|t| t as usize),
                        response_tokens: u.output_tokens.map(|t| t as usize),
                        cost_estimate: u.cost_cents.map(|c| c as f64 / 100.0),
                    });

            // Emit message received event directly with Content
            context
                .emit_message(response_content.clone(), model_info)
                .await?;

            // Process tool calls from Content
            let tool_results = self
                .process_tool_calls_from_content(&response_content, &context)
                .await?;

            if tool_results.has_function_calls {
                // Model made function calls - continue loop to process results
                // The function results will be available in the next iteration
                continue;
            } else {
                // No function calls - conversation is complete
                // The response has already been stored via emit_message above
                break;
            }
        }

        Ok(context)
    }

    /// Set up context metadata with system instruction
    fn setup_context_metadata(&self, context: &mut ProjectedExecutionContext) {
        if context.current_params.metadata.is_none() {
            context.current_params.metadata = Some(HashMap::new());
        }
        if let Some(ref mut metadata) = context.current_params.metadata {
            metadata.insert(
                "system_instruction".to_string(),
                json!(self.agent.instruction()),
            );
            metadata.insert("task_id".to_string(), json!(&context.task_id));
        }
    }

    /// Create LlmRequest from session events converted to ExtendedMessages
    async fn create_llm_request(
        &self,
        context: &ProjectedExecutionContext,
    ) -> AgentResult<LlmRequest> {
        // Get the full session with ALL events across ALL tasks
        let session = self
            .agent
            .session_service()
            .get_session(&context.app_name, &context.user_id, &context.context_id)
            .await?
            .ok_or_else(|| AgentError::SessionNotFound {
                session_id: context.context_id.clone(),
                app_name: context.app_name.clone(),
                user_id: context.user_id.clone(),
            })?;

        // Debug: Check what tools are available
        if let Some(toolset) = self.agent.toolset() {
            let _available_tools = toolset.get_tools().await;
        }

        // Convert session events to Content messages
        let content_messages = self
            .convert_events_to_content_messages(&session.events, &context.task_id)
            .await?;

        // Create LlmRequest with Content messages (includes function calls/responses)
        Ok(LlmRequest::with_messages(
            content_messages,               // All content from session
            context.task_id.clone(),        // Current task for highlighting
            context.context_id.clone(),     // Session context ID
        )
        .with_system_instruction(format!(
            "{}\n\nSession Context:\n- Current Task: {}\n- Session ID: {}\n- Previous tasks are marked with [Task: <id>]",
            self.agent.instruction(),
            &context.task_id,
            &context.context_id,
        ))
        .with_config(GenerateContentConfig::default())
        .with_toolset(
            self.agent.toolset()
                .cloned()
                .unwrap_or_else(|| Arc::new(SimpleToolset::new(vec![]))),
        ))
    }

    /// Convert session events to Content messages for LLM processing
    ///
    /// This method reconstructs the conversation flow from MessageReceived events:
    /// - MessageReceived events with User role → User Content
    /// - MessageReceived events with Agent role → Assistant Content (includes function calls/responses)
    ///
    /// Content preserves all function call/response data for LLM context while
    /// A2A Message conversion filters it out for clean task storage.
    async fn convert_events_to_content_messages(
        &self,
        events: &[crate::events::InternalEvent],
        _current_task_id: &str,
    ) -> AgentResult<Vec<Content>> {
        use crate::events::InternalEvent;

        let mut content_messages = Vec::new();

        for event in events {
            match event {
                InternalEvent::MessageReceived { content, .. } => {
                    // Add Content directly - it already contains all function calls/responses
                    // and provides clean A2A Message conversion via to_a2a_message()
                    content_messages.push(content.clone());
                }

                _ => {
                    // Other events (StateChange) don't affect conversation flow
                }
            }
        }

        Ok(content_messages)
    }

    /// Process tool calls from Content
    pub async fn process_tool_calls_from_content(
        &self,
        content: &Content,
        context: &ProjectedExecutionContext,
    ) -> AgentResult<ToolProcessingResult> {
        use std::time::Instant;

        let mut has_function_calls = false;

        // Process function calls from Content
        for function_call_part in content.get_function_calls() {
            if let crate::models::content::ContentPart::FunctionCall {
                name,
                arguments,
                tool_use_id,
                metadata: _,
            } = function_call_part
            {
                has_function_calls = true;

                // Execute the tool
                let start_time = Instant::now();
                let tool_result = self.execute_tool_call(name, arguments, context).await;
                let duration_ms = start_time.elapsed().as_millis() as u64;

                // Check if tool was found
                let is_tool_not_available = tool_result.error_message.as_ref().is_some_and(|msg| {
                    msg.contains("not found") || msg.contains("No toolset available")
                });

                if is_tool_not_available {
                    has_function_calls = false;
                }

                // Create and store function response message
                let mut response_content = Content::new(
                    context.task_id.clone(),
                    context.context_id.clone(),
                    uuid::Uuid::new_v4().to_string(),
                    crate::a2a::MessageRole::User,
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

                // Emit function response as separate message
                context.emit_message(response_content, None).await?;
            }
        }

        Ok(ToolProcessingResult {
            has_function_calls,
            response_parts: Vec::new(), // Empty - we don't create fake messages anymore
        })
    }

    /// Execute a tool call and return the result
    async fn execute_tool_call(
        &self,
        name: &str,
        args: &serde_json::Value,
        context: &ProjectedExecutionContext,
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
