//! Task execution logic for the runtime.
//!
//! This module is only available on native targets with the `runtime` feature enabled.
//! It handles the execution of A2A protocol requests and coordinates with services.
//!
//! # Slot Handling
//!
//! When skills return `InputRequired`, they provide a `SkillSlot` that describes what
//! type of input is expected. This slot information is used for:
//! - Input validation when the user provides a response
//! - UI hints for clients about what to collect
//! - Type-safe multi-turn conversations
//!
//! Currently, `SkillSlot` is a stub implementation. When it's fully implemented,
//! slot data will be stored in `TaskContext` under the reserved key `__radkit_current_slot`.
//! This approach reuses the existing TaskContext persistence mechanism rather than
//! adding separate storage.
//!
//! **Future implementation:**
//! ```rust,ignore
//! // When InputRequired is returned:
//! task_context.save_data("__radkit_current_slot", &slot)?;
//!
//! // When continuing:
//! let slot: SkillSlot = task_context.load_data("__radkit_current_slot")?
//!     .ok_or(...)?;
//! // Use slot for input validation
//! ```

#![cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]

use crate::agent::{builder::SkillRegistration, AgentDefinition, OnInputResult, OnRequestResult};
use crate::errors::{AgentError, AgentResult};
use crate::models::{utils, Content, Role};
use crate::runtime::context::{AuthContext, Context, TaskContext};
use crate::runtime::core::{negotiator::NegotiationDecision, status_mapper, TaskEventReceiver};
use crate::runtime::task_manager::{Task, TaskEvent};
use crate::runtime::{DefaultRuntime, Runtime};
use a2a_types::{A2ARequestPayload, MessageSendParams, TaskIdParams, TaskQueryParams};
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

pub struct RequestExecutor<'a> {
    runtime: Arc<dyn Runtime>,
    agent_def: &'a AgentDefinition,
}

#[derive(Debug)]
pub(crate) struct TaskStream {
    pub(crate) task: a2a_types::Task,
    pub(crate) initial_events: Vec<TaskEvent>,
    pub(crate) receiver: Option<TaskEventReceiver>,
}

impl<'a> RequestExecutor<'a> {
    pub fn new(runtime: Arc<DefaultRuntime>, agent_def: &'a AgentDefinition) -> Self {
        Self { runtime, agent_def }
    }

    pub async fn execute(&self, payload: A2ARequestPayload) -> AgentResult<Response> {
        match payload {
            A2ARequestPayload::SendMessage { params } => {
                self.handle_send_message(params).await?;
                Ok(Json(json!({})).into_response())
            }
            A2ARequestPayload::GetTask { params } => {
                let task = self.handle_get_task(params).await?;
                Ok(Json(task).into_response())
            }
            A2ARequestPayload::CancelTask { params } => {
                let task = self.handle_cancel_task(params).await?;
                Ok(Json(task).into_response())
            }
            _ => Err(AgentError::NotImplemented {
                feature: "This RPC method".to_string(),
            }),
        }
    }

    async fn handle_send_message(&self, params: MessageSendParams) -> AgentResult<()> {
        let _ = self.prepare_task_stream(params).await?;
        Ok(())
    }

    pub(crate) async fn handle_message_stream(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<TaskStream> {
        self.prepare_task_stream(params).await
    }

    pub(crate) async fn handle_task_resubscribe(
        &self,
        params: TaskIdParams,
    ) -> AgentResult<TaskStream> {
        let auth_ctx = self.runtime.auth_service().get_auth_context();
        let stored_task = self
            .runtime
            .task_manager()
            .get_task(&auth_ctx, &params.id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: params.id.clone(),
            })?;

        let events = self
            .runtime
            .task_manager()
            .get_task_events(&auth_ctx, &params.id)
            .await?;

        let task = self.reconstruct_a2a_task(&auth_ctx, &stored_task).await?;

        let is_final = status_mapper::is_terminal_state(&task.status.state)
            || events.iter().any(|event| {
                matches!(
                    event,
                    TaskEvent::StatusUpdate(update) if update.is_final
                )
            });

        let receiver = if is_final {
            None
        } else {
            Some(self.runtime.event_bus().subscribe(&params.id))
        };

        Ok(TaskStream {
            task,
            initial_events: events,
            receiver,
        })
    }

    pub(crate) async fn prepare_task_stream(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<TaskStream> {
        let auth_context = self.runtime.auth_service().get_auth_context();
        match (
            params.message.context_id.clone(),
            params.message.task_id.clone(),
        ) {
            (Some(context_id), Some(task_id)) => {
                self.process_existing_task(&auth_context, context_id, task_id, params)
                    .await
            }
            (Some(context_id), None) => {
                self.process_existing_context(&auth_context, context_id, params)
                    .await
            }
            (None, _) => self.process_new_context(&auth_context, params).await,
        }
    }

    async fn process_existing_task(
        &self,
        auth_ctx: &AuthContext,
        context_id: String,
        task_id: String,
        params: MessageSendParams,
    ) -> AgentResult<TaskStream> {
        let existing_task = self
            .runtime
            .task_manager()
            .get_task(auth_ctx, &task_id)
            .await?;

        let task = existing_task.ok_or_else(|| AgentError::TaskNotFound {
            task_id: task_id.clone(),
        })?;

        if task.context_id != context_id {
            return Err(AgentError::InvalidInput(format!(
                "Task id {} does not belong to context id {}",
                task_id, context_id
            )));
        }

        let baseline_len = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task_id)
            .await?
            .len();

        let user_content = Content::from(params.message.clone());
        let user_message = utils::create_a2a_message(
            Some(&context_id),
            Some(&task_id),
            Role::User,
            user_content.clone(),
        );
        let message_event = TaskEvent::Message(user_message);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &message_event)
            .await?;
        self.runtime.event_bus().publish(&message_event);

        let updated_task = self.continue_task(auth_ctx, task, user_content).await?;

        let events: Vec<TaskEvent> = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task_id)
            .await?
            .into_iter()
            .skip(baseline_len)
            .collect();

        let is_final = status_mapper::is_terminal_state(&updated_task.status.state)
            || events
                .iter()
                .any(|event| matches!(event, TaskEvent::StatusUpdate(update) if update.is_final));

        let receiver = if is_final {
            None
        } else {
            Some(self.runtime.event_bus().subscribe(&task_id))
        };

        Ok(TaskStream {
            task: updated_task,
            initial_events: events,
            receiver,
        })
    }

    async fn process_existing_context(
        &self,
        auth_ctx: &AuthContext,
        context_id: String,
        params: MessageSendParams,
    ) -> AgentResult<TaskStream> {
        let related_tasks = self
            .runtime
            .task_manager()
            .list_task_ids(auth_ctx, Some(&context_id))
            .await?;

        let negotiation_messages = self
            .runtime
            .task_manager()
            .get_negotiating_messages(auth_ctx, &context_id)
            .await?;

        if related_tasks.is_empty() && negotiation_messages.is_empty() {
            return Err(AgentError::InvalidInput(format!(
                "Invalid context id {}",
                context_id
            )));
        }

        let content = Content::from(params.message);

        let user_message =
            utils::create_a2a_message(Some(context_id.as_str()), None, Role::User, content.clone());
        let event = TaskEvent::Message(user_message);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &event)
            .await?;
        self.runtime.event_bus().publish(&event);

        let decision = self
            .runtime
            .negotiator()
            .negotiate(
                auth_ctx,
                self.agent_def,
                &context_id,
                content.clone(),
                negotiation_messages,
            )
            .await?;

        let task = self
            .handle_negotiation_decision(auth_ctx, &context_id, content, decision)
            .await?;

        let events: Vec<TaskEvent> = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task.id)
            .await?;

        let is_final = status_mapper::is_terminal_state(&task.status.state)
            || events
                .iter()
                .any(|event| matches!(event, TaskEvent::StatusUpdate(update) if update.is_final));

        let receiver = if is_final {
            None
        } else {
            Some(self.runtime.event_bus().subscribe(&task.id))
        };

        Ok(TaskStream {
            task,
            initial_events: events,
            receiver,
        })
    }

    async fn process_new_context(
        &self,
        auth_ctx: &AuthContext,
        params: MessageSendParams,
    ) -> AgentResult<TaskStream> {
        if params.message.task_id.is_some() {
            return Err(AgentError::InvalidInput(
                "context_id is required when passing task_id".to_string(),
            ));
        }

        let content = Content::from(params.message);
        let context_id = Uuid::new_v4().to_string();

        let user_message =
            utils::create_a2a_message(Some(context_id.as_str()), None, Role::User, content.clone());
        let event = TaskEvent::Message(user_message);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &event)
            .await?;
        self.runtime.event_bus().publish(&event);

        let decision = self
            .runtime
            .negotiator()
            .negotiate(
                auth_ctx,
                self.agent_def,
                &context_id,
                content.clone(),
                Vec::new(),
            )
            .await?;

        let task = self
            .handle_negotiation_decision(auth_ctx, &context_id, content, decision)
            .await?;

        let events: Vec<TaskEvent> = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task.id)
            .await?;

        let is_final = status_mapper::is_terminal_state(&task.status.state)
            || events
                .iter()
                .any(|event| matches!(event, TaskEvent::StatusUpdate(update) if update.is_final));

        let receiver = if is_final {
            None
        } else {
            Some(self.runtime.event_bus().subscribe(&task.id))
        };

        Ok(TaskStream {
            task,
            initial_events: events,
            receiver,
        })
    }

    async fn handle_get_task(&self, params: TaskQueryParams) -> AgentResult<a2a_types::Task> {
        let auth_context = self.runtime.auth_service().get_auth_context();
        let task_id = params.id;

        let stored_task = self
            .runtime
            .task_manager()
            .get_task(&auth_context, &task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.clone(),
            })?;

        // Reconstruct history from events
        let events = self
            .runtime
            .task_manager()
            .get_task_events(&auth_context, &task_id)
            .await?;
        let history: Vec<a2a_types::Message> = events
            .into_iter()
            .filter_map(|event| match event {
                TaskEvent::Message(msg) => Some(msg),
                TaskEvent::StatusUpdate(update) => update.status.message,
                _ => None,
            })
            .collect();

        let a2a_task = a2a_types::Task {
            id: stored_task.id,
            context_id: stored_task.context_id,
            status: stored_task.status,
            artifacts: stored_task.artifacts,
            history,
            kind: "task".to_string(),
            metadata: None, // TODO: Load metadata if stored separately
        };

        Ok(a2a_task)
    }

    async fn handle_cancel_task(&self, params: TaskIdParams) -> AgentResult<a2a_types::Task> {
        Err(AgentError::NotImplemented {
            feature: format!("tasks/cancel for task_id {}", params.id),
        })
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Finds a skill handler by its ID in the agent definition.
    ///
    /// # Arguments
    /// * `skill_id` - The unique identifier of the skill to find
    ///
    /// # Returns
    /// Reference to the skill registration or SkillNotFound error
    fn find_skill_by_id(&self, skill_id: &str) -> AgentResult<&SkillRegistration> {
        self.agent_def
            .skills
            .iter()
            .find(|skill| skill.id() == skill_id)
            .ok_or_else(|| AgentError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })
    }

    /// Reconstructs a full A2A Task from the stored Task + events.
    ///
    /// This method retrieves the task's event history and extracts message
    /// events to populate the task's history field per A2A protocol requirements.
    ///
    /// # Arguments
    /// * `auth_ctx` - Authentication context for access control
    /// * `task` - The stored task to reconstruct
    ///
    /// # Returns
    /// A complete A2A Task with populated history
    async fn reconstruct_a2a_task(
        &self,
        auth_ctx: &AuthContext,
        task: &Task,
    ) -> AgentResult<a2a_types::Task> {
        let events = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task.id)
            .await?;

        let history: Vec<a2a_types::Message> = events
            .into_iter()
            .filter_map(|event| match event {
                TaskEvent::Message(msg) => Some(msg),
                TaskEvent::StatusUpdate(update) => update.status.message,
                _ => None,
            })
            .collect();

        Ok(a2a_types::Task {
            kind: a2a_types::TASK_KIND.to_string(),
            id: task.id.clone(),
            context_id: task.context_id.clone(),
            status: task.status.clone(),
            history,
            artifacts: task.artifacts.clone(),
            metadata: None,
        })
    }

    /// Emits an assistant message from an OnRequestResult.
    ///
    /// Extracts the message content from the result and stores it as an
    /// assistant message event in the task manager.
    ///
    /// # Arguments
    /// * `auth_ctx` - Authentication context
    /// * `task_id` - The task this message belongs to
    /// * `context_id` - The context this message belongs to
    /// * `result` - The skill handler result containing the message
    async fn emit_result_message(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        context_id: &str,
        result: &OnRequestResult,
    ) -> AgentResult<()> {
        let message_content = match result {
            OnRequestResult::InputRequired { message, .. } => Some(message.clone()),
            OnRequestResult::Completed { message, .. } => message.clone(),
            OnRequestResult::Failed { error } => Some(error.clone()),
            OnRequestResult::Rejected { reason } => Some(reason.clone()),
        };

        if let Some(content) = message_content {
            let assistant_message = utils::create_a2a_message(
                Some(context_id),
                Some(task_id),
                Role::Assistant,
                content,
            );
            let event = TaskEvent::Message(assistant_message);
            self.runtime
                .task_manager()
                .add_task_event(auth_ctx, &event)
                .await?;
            self.runtime.event_bus().publish(&event);
        }

        Ok(())
    }

    /// Emits an assistant message from an OnInputResult.
    ///
    /// Similar to emit_result_message but for input continuation results.
    ///
    /// Note: OnInputResult does not have a Rejected variant.
    ///
    /// # Arguments
    /// * `auth_ctx` - Authentication context
    /// * `task_id` - The task this message belongs to
    /// * `context_id` - The context this message belongs to
    /// * `result` - The skill handler result containing the message
    async fn emit_input_result_message(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        context_id: &str,
        result: &OnInputResult,
    ) -> AgentResult<()> {
        let message_content = match result {
            OnInputResult::InputRequired { message, .. } => Some(message.clone()),
            OnInputResult::Completed { message, .. } => message.clone(),
            OnInputResult::Failed { error } => Some(error.clone()),
        };

        if let Some(content) = message_content {
            let assistant_message = utils::create_a2a_message(
                Some(context_id),
                Some(task_id),
                Role::Assistant,
                content,
            );
            let event = TaskEvent::Message(assistant_message);
            self.runtime
                .task_manager()
                .add_task_event(auth_ctx, &event)
                .await?;
            self.runtime.event_bus().publish(&event);
        }

        Ok(())
    }

    // ========================================================================
    // Task Execution Methods
    // ========================================================================

    /// Creates and executes a new task with the specified skill.
    ///
    /// This method orchestrates the complete task creation and execution flow:
    /// 1. Creates a new task with Working status
    /// 2. Executes the skill's on_request handler
    /// 3. Maps the result to A2A TaskStatus
    /// 4. Emits appropriate events (status updates, messages, artifacts)
    /// 5. Persists task state and context
    ///
    /// # Arguments
    /// * `auth_ctx` - Authentication context for multi-tenancy
    /// * `context_id` - Context ID for grouping related tasks
    /// * `skill_id` - ID of the skill to execute
    /// * `initial_message` - The user's initial message content
    ///
    /// # Returns
    /// The created task with its current status and history
    async fn start_task(
        &self,
        auth_ctx: &AuthContext,
        context_id: &str,
        skill_id: &str,
        initial_message: Content,
    ) -> AgentResult<a2a_types::Task> {
        // 1. Generate task ID and find skill
        let task_id = Uuid::new_v4().to_string();
        let skill_reg = self.find_skill_by_id(skill_id)?;

        // 2. Create initial task with Working status
        let mut task = Task {
            id: task_id.clone(),
            context_id: context_id.to_string(),
            status: status_mapper::working_status(),
            artifacts: vec![],
        };

        // 3. Save initial task
        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

        // 4. Associate skill with task
        self.runtime
            .task_manager()
            .set_task_skill(auth_ctx, &task_id, skill_id)
            .await?;

        // 5. Record the user's message against the concrete task so history remains consistent.
        let user_message_event = {
            let message = utils::create_a2a_message(
                Some(context_id),
                Some(&task_id),
                Role::User,
                initial_message.clone(),
            );
            TaskEvent::Message(message)
        };
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &user_message_event)
            .await?;
        self.runtime.event_bus().publish(&user_message_event);

        // 6. Create execution contexts
        let mut task_context = TaskContext::for_task(
            auth_ctx.clone(),
            self.runtime.task_manager(),
            self.runtime.event_bus(),
            context_id.to_string(),
            task_id.clone(),
        );
        let context = Context::new(auth_ctx.clone());

        // 7. Execute skill
        let result = skill_reg
            .handler
            .on_request(
                &mut task_context,
                &context,
                self.runtime.as_ref(),
                initial_message,
            )
            .await?;

        // 8. Map result to A2A status
        let final_status = status_mapper::on_request_to_status(&result);
        task.status = final_status.clone();

        // 9. Update task artifacts if completed (convert to A2A artifacts)
        if let OnRequestResult::Completed { artifacts, .. } = &result {
            task.artifacts = utils::artifacts_to_a2a(artifacts.clone());
        }

        // 10. Store slot information if InputRequired; otherwise clear it
        if let OnRequestResult::InputRequired { slot, .. } = &result {
            task_context.set_pending_slot(slot.clone());
        } else {
            task_context.clear_pending_slot();
        }

        // 11. Save task context for potential continuation
        self.runtime
            .task_manager()
            .save_task_context(auth_ctx, &task_id, &task_context)
            .await?;

        // 12. Emit assistant message if present
        self.emit_result_message(auth_ctx, &task_id, context_id, &result)
            .await?;

        // 13. Emit status update event
        let is_final = status_mapper::is_terminal_state(&final_status.state)
            || final_status.state == a2a_types::TaskState::InputRequired;
        let status_event =
            status_mapper::create_status_update_event(&task_id, context_id, final_status, is_final);
        let task_event = TaskEvent::StatusUpdate(status_event);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &task_event)
            .await?;
        self.runtime.event_bus().publish(&task_event);

        // 14. Save updated task
        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

        // 15. Reconstruct full A2A task with history
        self.reconstruct_a2a_task(auth_ctx, &task).await
    }

    /// Continues an existing task by providing additional input.
    ///
    /// This method handles the task continuation flow:
    /// 1. Validates the task is in InputRequired state
    /// 2. Loads the skill handler and task context
    /// 3. Calls the skill's on_input_received handler
    /// 4. Updates task status and emits events
    ///
    /// # Arguments
    /// * `auth_ctx` - Authentication context
    /// * `task` - The existing task to continue
    /// * `user_message` - The user's input message
    ///
    /// # Returns
    /// The updated task with new status and history
    async fn continue_task(
        &self,
        auth_ctx: &AuthContext,
        mut task: Task,
        user_message: Content,
    ) -> AgentResult<a2a_types::Task> {
        // 1. Validate current state
        if !status_mapper::can_continue(&task.status.state) {
            return Err(AgentError::InvalidTaskStateTransition {
                from: format!("{:?}", task.status.state),
                to: "continuation".to_string(),
            });
        }

        // 2. Get associated skill
        let skill_id = self
            .runtime
            .task_manager()
            .get_task_skill(auth_ctx, &task.id)
            .await?
            .ok_or_else(|| AgentError::Internal {
                component: "task_manager".to_string(),
                reason: format!("No skill associated with task {}", task.id),
            })?;

        let skill_reg = self.find_skill_by_id(&skill_id)?;

        // 3. Load task context
        let mut task_context = self
            .runtime
            .task_manager()
            .load_task_context(auth_ctx, &task.id)
            .await?
            .unwrap_or_else(TaskContext::new);
        task_context.attach_handles(
            auth_ctx.clone(),
            self.runtime.task_manager(),
            self.runtime.event_bus(),
            task.context_id.clone(),
            task.id.clone(),
        );

        // 4. Update to Working state
        task.status = status_mapper::working_status();
        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

        // 5. Create execution context
        let context = Context::new(auth_ctx.clone());

        // 6. Execute skill continuation
        let result = skill_reg
            .handler
            .on_input_received(
                &mut task_context,
                &context,
                self.runtime.as_ref(),
                user_message,
            )
            .await?;

        // 7. Map result to A2A status
        let final_status = status_mapper::on_input_to_status(&result);
        task.status = final_status.clone();

        // 8. Update artifacts if completed (convert to A2A artifacts)
        if let OnInputResult::Completed { artifacts, .. } = &result {
            task.artifacts = utils::artifacts_to_a2a(artifacts.clone());
        }

        // 9. Store slot information if InputRequired (for additional turns)
        if let OnInputResult::InputRequired { slot, .. } = &result {
            task_context.set_pending_slot(slot.clone());
        } else {
            task_context.clear_pending_slot();
        }

        // 10. Save task context again
        self.runtime
            .task_manager()
            .save_task_context(auth_ctx, &task.id, &task_context)
            .await?;

        // 11. Emit status update event
        let is_final = status_mapper::is_terminal_state(&final_status.state)
            || final_status.state == a2a_types::TaskState::InputRequired;
        let status_event = status_mapper::create_status_update_event(
            &task.id,
            &task.context_id,
            final_status,
            is_final,
        );
        let task_event = TaskEvent::StatusUpdate(status_event);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &task_event)
            .await?;
        self.runtime.event_bus().publish(&task_event);

        // 12. Emit assistant message
        self.emit_input_result_message(auth_ctx, &task.id, &task.context_id, &result)
            .await?;

        // 13. Save updated task
        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

        // 14. Reconstruct full A2A task
        self.reconstruct_a2a_task(auth_ctx, &task).await
    }

    /// Helper method to handle negotiation decisions.
    ///
    /// Stores assistant messages and converts a NegotiationDecision into an A2A response.
    async fn handle_negotiation_decision(
        &self,
        auth_ctx: &crate::runtime::context::AuthContext,
        context_id: &str,
        content: Content,
        decision: NegotiationDecision,
    ) -> AgentResult<a2a_types::Task> {
        match decision {
            NegotiationDecision::StartTask {
                skill_id,
                reasoning,
            } => {
                // Store assistant message explaining the decision
                let reasoning_content = Content::from_text(&reasoning);
                let assistant_message = utils::create_a2a_message(
                    Some(context_id),
                    None, // No task_id yet
                    Role::Assistant,
                    reasoning_content,
                );
                let event = TaskEvent::Message(assistant_message);
                self.runtime
                    .task_manager()
                    .add_task_event(auth_ctx, &event)
                    .await?;
                self.runtime.event_bus().publish(&event);

                // Create and execute the task
                let task = self
                    .start_task(auth_ctx, context_id, &skill_id, content)
                    .await?;
                Ok(task)
            }
            NegotiationDecision::AskClarification { message } => {
                // Store assistant's clarification message
                let clarification_content = Content::from_text(&message);
                let assistant_message = utils::create_a2a_message(
                    Some(context_id),
                    None,
                    Role::Assistant,
                    clarification_content,
                );
                let event = TaskEvent::Message(assistant_message);
                self.runtime
                    .task_manager()
                    .add_task_event(auth_ctx, &event)
                    .await?;
                self.runtime.event_bus().publish(&event);

                // TODO: Return the clarification message as a proper Message response
                Err(AgentError::NotImplemented {
                    feature: format!("Return clarification message: {}", message),
                })
            }
            NegotiationDecision::Reject { reason } => {
                // Store assistant's rejection message
                let rejection_content = Content::from_text(&reason);
                let assistant_message = utils::create_a2a_message(
                    Some(context_id),
                    None,
                    Role::Assistant,
                    rejection_content,
                );
                let event = TaskEvent::Message(assistant_message);
                self.runtime
                    .task_manager()
                    .add_task_event(auth_ctx, &event)
                    .await?;
                self.runtime.event_bus().publish(&event);

                // TODO: Return the rejection message as a proper response
                Err(AgentError::NotImplemented {
                    feature: format!("Return rejection message: {}", reason),
                })
            }
        }
    }
}
