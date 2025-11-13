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
//! This approach reuses the existing `TaskContext` persistence mechanism rather than
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

use crate::agent::{
    builder::SkillRegistration, skill::SkillHandler, AgentDefinition, OnInputResult,
    OnRequestResult,
};
use crate::compat;
use crate::errors::{AgentError, AgentResult};
use crate::models::{utils, Content, Role};
use crate::runtime::context::{AuthContext, Context, TaskContext};
use crate::runtime::core::{negotiator::NegotiationDecision, status_mapper, TaskEventReceiver};
use crate::runtime::task_manager::{Task, TaskEvent};
use crate::runtime::{DefaultRuntime, Runtime};
use a2a_types::{MessageSendParams, SendMessageResult, TaskIdParams, TaskQueryParams};
use std::sync::Arc;
use tracing::error;
use uuid::Uuid;

pub struct RequestExecutor<'a> {
    runtime: Arc<dyn Runtime>,
    agent_def: &'a AgentDefinition,
}

/// Buffered task events plus optional event-bus subscription used to power
/// the `message/stream` and `tasks/resubscribe` SSE responses defined in the
/// A2A specification (§7.2 and §7.10 respectively).
#[derive(Debug)]
pub(crate) struct TaskStream {
    pub(crate) initial_events: Vec<TaskEvent>,
    pub(crate) receiver: Option<TaskEventReceiver>,
}

#[derive(Debug)]
pub(crate) enum PreparedSendMessage {
    Task(TaskStream),
    Message(a2a_types::Message),
}

#[derive(Copy, Clone, Debug)]
enum DeliveryMode {
    /// Blocking delivery corresponds to the `message/send` RPC defined in the
    /// A2A specification §7.1, where a single response is returned.
    Blocking,
    /// Streaming delivery corresponds to `message/stream` (§7.2) where the
    /// caller subscribes to Server-Sent Events.
    Streaming,
}

/// Internal routing outcome used to keep the synchronous (`message/send`) and
/// streaming (`message/stream`) flows aligned with the negotiation/task logic.
#[derive(Debug)]
enum DeliveryOutcome {
    Task(a2a_types::Task),
    Message(a2a_types::Message),
    Stream(TaskStream),
}

/// Indicates whether the inbound user payload is targeting an existing task,
/// an established context, or a brand-new context (per spec §2 and §7.1).
#[derive(Debug)]
enum MessageRoute {
    ExistingTask { context_id: String, task_id: String },
    ExistingContext { context_id: String },
    NewContext,
}

impl MessageRoute {
    /// Determines the message scope based on the presence of `context_id` /
    /// `task_id`, enforcing the A2A requirement that `task_id` cannot be sent
    /// without its parent `context_id`.
    fn from_params(params: &MessageSendParams) -> AgentResult<Self> {
        match (
            params.message.context_id.clone(),
            params.message.task_id.clone(),
        ) {
            (Some(context_id), Some(task_id)) => Ok(Self::ExistingTask {
                context_id,
                task_id,
            }),
            (Some(context_id), None) => Ok(Self::ExistingContext { context_id }),
            (None, Some(_)) => Err(AgentError::InvalidInput(
                "context_id is required when passing task_id".to_string(),
            )),
            (None, None) => Ok(Self::NewContext),
        }
    }
}

impl<'a> RequestExecutor<'a> {
    #[must_use]
    pub fn new(runtime: Arc<DefaultRuntime>, agent_def: &'a AgentDefinition) -> Self {
        Self { runtime, agent_def }
    }

    /// Handles a message send request and returns the result.
    ///
    /// # Errors
    ///
    /// Returns an error if message processing fails or the task cannot be created.
    pub async fn handle_send_message(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<SendMessageResult> {
        self.handle_send_message_internal(params).await
    }

    pub(crate) async fn handle_message_stream(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<PreparedSendMessage> {
        self.handle_message_stream_internal(params).await
    }

    async fn handle_send_message_internal(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<SendMessageResult> {
        match self
            .dispatch_message(params, DeliveryMode::Blocking)
            .await?
        {
            DeliveryOutcome::Task(task) => Ok(SendMessageResult::Task(task)),
            DeliveryOutcome::Message(message) => Ok(SendMessageResult::Message(message)),
            DeliveryOutcome::Stream(_) => Err(AgentError::Internal {
                component: "runtime".into(),
                reason: "streaming response produced for blocking message/send".into(),
            }),
        }
    }

    async fn handle_message_stream_internal(
        &self,
        params: MessageSendParams,
    ) -> AgentResult<PreparedSendMessage> {
        match self
            .dispatch_message(params, DeliveryMode::Streaming)
            .await?
        {
            DeliveryOutcome::Stream(stream) => Ok(PreparedSendMessage::Task(stream)),
            DeliveryOutcome::Message(message) => Ok(PreparedSendMessage::Message(message)),
            DeliveryOutcome::Task(_) => Err(AgentError::Internal {
                component: "runtime".into(),
                reason: "blocking task returned while handling message/stream".into(),
            }),
        }
    }

    /// Routes an inbound `message/send` or `message/stream` payload into the
    /// correct code path (existing task continuation, contextual negotiation,
    /// or creation of a brand-new context) while preserving the behavioral
    /// differences mandated by §7.1 (single response) and §7.2 (SSE stream).
    async fn dispatch_message(
        &self,
        params: MessageSendParams,
        mode: DeliveryMode,
    ) -> AgentResult<DeliveryOutcome> {
        let auth_context = self.runtime.auth_service().get_auth_context();
        match MessageRoute::from_params(&params)? {
            MessageRoute::ExistingTask {
                context_id,
                task_id,
            } => {
                self.handle_existing_task(&auth_context, context_id, task_id, params, mode)
                    .await
            }
            MessageRoute::ExistingContext { context_id } => {
                self.handle_existing_context(&auth_context, context_id, params, mode)
                    .await
            }
            MessageRoute::NewContext => self.handle_new_context(&auth_context, params, mode).await,
        }
    }

    /// Continues an in-flight task, either synchronously (returning a `Task`
    /// snapshot as required by `message/send`) or asynchronously (spawning
    /// background execution for `message/stream`). This enforces the Life of a
    /// Task rule that only `input-required` tasks can progress further.
    async fn handle_existing_task(
        &self,
        auth_ctx: &AuthContext,
        context_id: String,
        task_id: String,
        params: MessageSendParams,
        mode: DeliveryMode,
    ) -> AgentResult<DeliveryOutcome> {
        let stored_task = self
            .runtime
            .task_manager()
            .get_task(auth_ctx, &task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.clone(),
            })?;

        if stored_task.context_id != context_id {
            return Err(AgentError::InvalidInput(format!(
                "Task id {task_id} does not belong to context id {context_id}"
            )));
        }

        let baseline_len = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, &task_id)
            .await?
            .len();

        let content = Content::from(params.message);
        self.append_message_event(
            auth_ctx,
            &context_id,
            Some(&task_id),
            Role::User,
            content.clone(),
        )
        .await?;

        match mode {
            DeliveryMode::Blocking => {
                let updated_task = self
                    .continue_task_blocking(auth_ctx, stored_task, content)
                    .await?;
                Ok(DeliveryOutcome::Task(updated_task))
            }
            DeliveryMode::Streaming => {
                let stream = self
                    .continue_task_streaming(auth_ctx, stored_task, content, baseline_len)
                    .await?;
                Ok(DeliveryOutcome::Stream(stream))
            }
        }
    }

    /// Handles messages that reference an existing context but no specific
    /// task, invoking the negotiator to decide whether we should start a new
    /// task, ask for clarification, or reject the request (A2A §7.1/§7.2).
    async fn handle_existing_context(
        &self,
        auth_ctx: &AuthContext,
        context_id: String,
        params: MessageSendParams,
        mode: DeliveryMode,
    ) -> AgentResult<DeliveryOutcome> {
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
                "Invalid context id {context_id}"
            )));
        }

        let content = Content::from(params.message);
        self.append_message_event(auth_ctx, &context_id, None, Role::User, content.clone())
            .await?;

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

        self.deliver_negotiation_decision(auth_ctx, &context_id, content, decision, mode)
            .await
    }

    /// Handles the very first message in a conversation by minting a new
    /// `context_id`, persisting the user turn, and invoking the negotiator.
    async fn handle_new_context(
        &self,
        auth_ctx: &AuthContext,
        params: MessageSendParams,
        mode: DeliveryMode,
    ) -> AgentResult<DeliveryOutcome> {
        let content = Content::from(params.message);
        let context_id = Uuid::new_v4().to_string();

        self.append_message_event(auth_ctx, &context_id, None, Role::User, content.clone())
            .await?;

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

        self.deliver_negotiation_decision(auth_ctx, &context_id, content, decision, mode)
            .await
    }

    pub(crate) async fn handle_task_resubscribe(
        &self,
        params: TaskIdParams,
    ) -> AgentResult<TaskStream> {
        self.handle_task_resubscribe_internal(params).await
    }

    /// Implements `tasks/resubscribe` (§7.10) by replaying the persisted event
    /// log and, if the task is still running, wiring up a fresh subscription
    /// to future `TaskEvent`s.
    async fn handle_task_resubscribe_internal(
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

        let is_final = status_mapper::is_terminal_state(&stored_task.status.state)
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
            initial_events: events,
            receiver,
        })
    }

    /// Retrieves a task by its ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is not found or cannot be retrieved.
    pub async fn handle_get_task(&self, params: TaskQueryParams) -> AgentResult<a2a_types::Task> {
        self.handle_get_task_internal(params).await
    }

    async fn handle_get_task_internal(
        &self,
        params: TaskQueryParams,
    ) -> AgentResult<a2a_types::Task> {
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

        self.reconstruct_a2a_task(&auth_context, &stored_task).await
    }

    /// Cancels a task by its ID.
    ///
    /// # Errors
    ///
    /// Returns an error as this feature is not yet implemented.
    #[allow(clippy::unused_async)] // Kept async for API consistency; will need async when implemented
    pub async fn handle_cancel_task(&self, params: TaskIdParams) -> AgentResult<a2a_types::Task> {
        Self::handle_cancel_task_internal(&params)
    }

    fn handle_cancel_task_internal(params: &TaskIdParams) -> AgentResult<a2a_types::Task> {
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
    /// Reference to the skill registration or `SkillNotFound` error
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
                TaskEvent::ArtifactUpdate(_) => None,
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

    // ========================================================================
    // Task Execution Methods
    // ========================================================================

    /// Creates and executes a new task with the specified skill while honoring
    /// the blocking semantics of `message/send`: we run the skill to completion
    /// (or to `input-required`) before responding with the latest task snapshot
    /// mandated by §7.1 and the "Life of a Task" guide.
    async fn start_task_blocking(
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
        let task = Task {
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
        let task_context = TaskContext::for_task(
            auth_ctx.clone(),
            self.runtime.task_manager(),
            self.runtime.event_bus(),
            context_id.to_string(),
            task_id.clone(),
        );
        let handler = skill_reg.handler_arc();
        let identifiers = TaskIdentifiers::new(context_id.to_string(), task_id.clone());
        let task = drive_on_request(
            Arc::clone(&self.runtime),
            handler,
            task_context,
            auth_ctx.clone(),
            identifiers,
            initial_message,
            task,
        )
        .await?;

        // 15. Reconstruct full A2A task with history
        self.reconstruct_a2a_task(auth_ctx, &task).await
    }

    /// Drives an existing task forward using `SkillHandler::on_input_received`
    /// and returns an updated A2A `Task`. Only tasks in `input-required`
    /// state are eligible, matching the state-machine expectations in the
    /// specification and the Life-of-a-Task document.
    async fn continue_task_blocking(
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

        let handler = skill_reg.handler_arc();
        let identifiers = TaskIdentifiers::new(task.context_id.clone(), task.id.clone());
        let task = drive_on_input(
            Arc::clone(&self.runtime),
            handler,
            task_context,
            auth_ctx.clone(),
            identifiers,
            user_message,
            task,
        )
        .await?;

        // Reconstruct full A2A task
        self.reconstruct_a2a_task(auth_ctx, &task).await
    }

    /// Persists the negotiator's assistant-facing response (reasoning,
    /// clarification, or rejection) and maps the `NegotiationDecision` into
    /// either a concrete task execution or a plain `Message`, depending on
    /// whether we're servicing `message/send` or `message/stream`.
    async fn deliver_negotiation_decision(
        &self,
        auth_ctx: &AuthContext,
        context_id: &str,
        content: Content,
        decision: NegotiationDecision,
        mode: DeliveryMode,
    ) -> AgentResult<DeliveryOutcome> {
        match decision {
            NegotiationDecision::StartTask {
                skill_id,
                reasoning,
            } => {
                self.append_message_event(
                    auth_ctx,
                    context_id,
                    None,
                    Role::Assistant,
                    Content::from_text(&reasoning),
                )
                .await?;

                match mode {
                    DeliveryMode::Blocking => {
                        let task = self
                            .start_task_blocking(auth_ctx, context_id, &skill_id, content)
                            .await?;
                        Ok(DeliveryOutcome::Task(task))
                    }
                    DeliveryMode::Streaming => {
                        let stream = self
                            .start_task_streaming(auth_ctx, context_id, &skill_id, content)
                            .await?;
                        Ok(DeliveryOutcome::Stream(stream))
                    }
                }
            }
            NegotiationDecision::AskClarification { message } => {
                let assistant_message = self
                    .append_message_event(
                        auth_ctx,
                        context_id,
                        None,
                        Role::Assistant,
                        Content::from_text(&message),
                    )
                    .await?;
                Ok(DeliveryOutcome::Message(assistant_message))
            }
            NegotiationDecision::Reject { reason } => {
                let assistant_message = self
                    .append_message_event(
                        auth_ctx,
                        context_id,
                        None,
                        Role::Assistant,
                        Content::from_text(&reason),
                    )
                    .await?;
                Ok(DeliveryOutcome::Message(assistant_message))
            }
        }
    }

    /// Starts a task for `message/stream`, immediately returning the buffered
    /// events (e.g., the user message) plus an event-bus subscription so the
    /// SSE layer can emit `SendStreamingMessageResponse` objects as work
    /// progresses.
    async fn start_task_streaming(
        &self,
        auth_ctx: &AuthContext,
        context_id: &str,
        skill_id: &str,
        initial_message: Content,
    ) -> AgentResult<TaskStream> {
        let task_id = Uuid::new_v4().to_string();
        let skill_reg = self.find_skill_by_id(skill_id)?;

        let task = Task {
            id: task_id.clone(),
            context_id: context_id.to_string(),
            status: status_mapper::working_status(),
            artifacts: vec![],
        };

        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

        self.runtime
            .task_manager()
            .set_task_skill(auth_ctx, &task_id, skill_id)
            .await?;

        let user_message = utils::create_a2a_message(
            Some(context_id),
            Some(&task_id),
            Role::User,
            initial_message.clone(),
        );
        let message_event = TaskEvent::Message(user_message);
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &message_event)
            .await?;
        self.runtime.event_bus().publish(&message_event);

        let initial_events = self.collect_new_events(auth_ctx, &task_id, 0).await?;

        let task_context = TaskContext::for_task(
            auth_ctx.clone(),
            self.runtime.task_manager(),
            self.runtime.event_bus(),
            context_id.to_string(),
            task_id.clone(),
        );

        let handler = skill_reg.handler_arc();
        let identifiers = TaskIdentifiers::new(context_id.to_string(), task_id.clone());
        let runtime = Arc::clone(&self.runtime);
        let auth_clone = auth_ctx.clone();
        let message_clone = initial_message.clone();
        let receiver = Some(self.runtime.event_bus().subscribe(&task_id));

        compat::spawn({
            let log_task_id = identifiers.task_id.clone();
            async move {
                if let Err(err) = drive_on_request(
                    runtime,
                    handler,
                    task_context,
                    auth_clone,
                    identifiers,
                    message_clone,
                    task,
                )
                .await
                {
                    error!(
                        task_id = %log_task_id,
                        error = %err,
                        "failed to execute streaming task"
                    );
                }
            }
        });

        Ok(TaskStream {
            initial_events,
            receiver,
        })
    }

    /// Continues a task in streaming mode by emitting the buffered user turn
    /// immediately and spawning background execution using `compat::spawn`,
    /// ensuring portability across native and WASI targets.
    async fn continue_task_streaming(
        &self,
        auth_ctx: &AuthContext,
        mut task: Task,
        user_message: Content,
        baseline_len: usize,
    ) -> AgentResult<TaskStream> {
        if !status_mapper::can_continue(&task.status.state) {
            return Err(AgentError::InvalidTaskStateTransition {
                from: format!("{:?}", task.status.state),
                to: "continuation".to_string(),
            });
        }

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

        let initial_events = self
            .collect_new_events(auth_ctx, &task.id, baseline_len)
            .await?;

        task.status = status_mapper::working_status();
        self.runtime
            .task_manager()
            .save_task(auth_ctx, &task)
            .await?;

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

        let handler = skill_reg.handler_arc();
        let identifiers = TaskIdentifiers::new(task.context_id.clone(), task.id.clone());
        let runtime = Arc::clone(&self.runtime);
        let auth_clone = auth_ctx.clone();
        let user_content_clone = user_message.clone();
        let receiver = Some(self.runtime.event_bus().subscribe(&task.id));
        let task_for_execution = task;

        compat::spawn({
            let task_id_for_exec = identifiers.task_id.clone();
            async move {
                if let Err(err) = drive_on_input(
                    runtime,
                    handler,
                    task_context,
                    auth_clone,
                    identifiers,
                    user_content_clone,
                    task_for_execution,
                )
                .await
                {
                    error!(
                        task_id = %task_id_for_exec,
                        error = %err,
                        "failed to execute streaming continuation"
                    );
                }
            }
        });

        Ok(TaskStream {
            initial_events,
            receiver,
        })
    }

    /// Collects task events recorded after a given baseline index so we can
    /// replay them to SSE clients before live streaming begins.
    async fn collect_new_events(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        baseline_len: usize,
    ) -> AgentResult<Vec<TaskEvent>> {
        let events = self
            .runtime
            .task_manager()
            .get_task_events(auth_ctx, task_id)
            .await?;
        Ok(events.into_iter().skip(baseline_len).collect())
    }

    /// Persists a user/assistant message turn as a `TaskEvent::Message` and
    /// returns the generated A2A `Message` for downstream use.
    async fn append_message_event(
        &self,
        auth_ctx: &AuthContext,
        context_id: &str,
        task_id: Option<&str>,
        role: Role,
        content: Content,
    ) -> AgentResult<a2a_types::Message> {
        let message = utils::create_a2a_message(Some(context_id), task_id, role, content);
        let event = TaskEvent::Message(message.clone());
        self.runtime
            .task_manager()
            .add_task_event(auth_ctx, &event)
            .await?;
        self.runtime.event_bus().publish(&event);
        Ok(message)
    }
}

#[derive(Clone)]
struct TaskIdentifiers {
    context_id: String,
    task_id: String,
}

impl TaskIdentifiers {
    #[allow(clippy::missing_const_for_fn)] // Takes owned `String`s, so it cannot be const.
    fn new(context_id: String, task_id: String) -> Self {
        Self {
            context_id,
            task_id,
        }
    }
}

async fn drive_on_request(
    runtime: Arc<dyn Runtime>,
    handler: Arc<dyn SkillHandler>,
    mut task_context: TaskContext,
    auth_ctx: AuthContext,
    identifiers: TaskIdentifiers,
    initial_message: Content,
    mut task: Task,
) -> AgentResult<Task> {
    let TaskIdentifiers {
        context_id,
        task_id,
    } = identifiers;
    let context = Context::new(auth_ctx.clone());
    let result = handler
        .on_request(
            &mut task_context,
            &context,
            runtime.as_ref(),
            initial_message,
        )
        .await?;

    let final_status = status_mapper::on_request_to_status(&result);
    task.status = final_status.clone();

    if let OnRequestResult::Completed { artifacts, .. } = &result {
        task.artifacts = utils::artifacts_to_a2a(artifacts);

        for artifact in artifacts {
            let event = a2a_types::TaskArtifactUpdateEvent {
                kind: a2a_types::ARTIFACT_UPDATE_KIND.to_string(),
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: utils::artifact_to_a2a(artifact),
                append: None,
                last_chunk: Some(true),
                metadata: None,
            };
            let task_event = TaskEvent::ArtifactUpdate(event);
            runtime
                .task_manager()
                .add_task_event(&auth_ctx, &task_event)
                .await?;
            runtime.event_bus().publish(&task_event);
        }
    }

    if let OnRequestResult::InputRequired { slot, .. } = &result {
        task_context.set_pending_slot(slot.clone());
    } else {
        task_context.clear_pending_slot();
    }

    runtime
        .task_manager()
        .save_task_context(&auth_ctx, &task_id, &task_context)
        .await?;

    emit_on_request_message(
        Arc::clone(&runtime),
        &auth_ctx,
        &task_id,
        &context_id,
        &result,
    )
    .await?;

    let is_final = status_mapper::is_terminal_state(&final_status.state)
        || final_status.state == a2a_types::TaskState::InputRequired;
    let status_event =
        status_mapper::create_status_update_event(&task_id, &context_id, final_status, is_final);
    let task_event = TaskEvent::StatusUpdate(status_event);
    runtime
        .task_manager()
        .add_task_event(&auth_ctx, &task_event)
        .await?;
    runtime.event_bus().publish(&task_event);

    runtime.task_manager().save_task(&auth_ctx, &task).await?;

    Ok(task)
}

async fn drive_on_input(
    runtime: Arc<dyn Runtime>,
    handler: Arc<dyn SkillHandler>,
    mut task_context: TaskContext,
    auth_ctx: AuthContext,
    identifiers: TaskIdentifiers,
    user_message: Content,
    mut task: Task,
) -> AgentResult<Task> {
    let TaskIdentifiers {
        context_id,
        task_id,
    } = identifiers;
    let context = Context::new(auth_ctx.clone());
    let result = handler
        .on_input_received(&mut task_context, &context, runtime.as_ref(), user_message)
        .await?;

    let final_status = status_mapper::on_input_to_status(&result);
    task.status = final_status.clone();

    if let OnInputResult::Completed { artifacts, .. } = &result {
        task.artifacts = utils::artifacts_to_a2a(artifacts);

        for artifact in artifacts {
            let event = a2a_types::TaskArtifactUpdateEvent {
                kind: a2a_types::ARTIFACT_UPDATE_KIND.to_string(),
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: utils::artifact_to_a2a(artifact),
                append: None,
                last_chunk: Some(true),
                metadata: None,
            };
            let task_event = TaskEvent::ArtifactUpdate(event);
            runtime
                .task_manager()
                .add_task_event(&auth_ctx, &task_event)
                .await?;
            runtime.event_bus().publish(&task_event);
        }
    }

    runtime
        .task_manager()
        .save_task_context(&auth_ctx, &task_id, &task_context)
        .await?;

    emit_on_input_message(
        Arc::clone(&runtime),
        &auth_ctx,
        &task_id,
        &context_id,
        &result,
    )
    .await?;

    let is_final = status_mapper::is_terminal_state(&final_status.state)
        || final_status.state == a2a_types::TaskState::InputRequired;
    let status_event =
        status_mapper::create_status_update_event(&task_id, &context_id, final_status, is_final);
    let task_event = TaskEvent::StatusUpdate(status_event);
    runtime
        .task_manager()
        .add_task_event(&auth_ctx, &task_event)
        .await?;
    runtime.event_bus().publish(&task_event);

    runtime.task_manager().save_task(&auth_ctx, &task).await?;

    Ok(task)
}

async fn emit_on_request_message(
    runtime: Arc<dyn Runtime>,
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
        let assistant_message =
            utils::create_a2a_message(Some(context_id), Some(task_id), Role::Assistant, content);
        let event = TaskEvent::Message(assistant_message);
        runtime
            .task_manager()
            .add_task_event(auth_ctx, &event)
            .await?;
        runtime.event_bus().publish(&event);
    }

    Ok(())
}

async fn emit_on_input_message(
    runtime: Arc<dyn Runtime>,
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
        let assistant_message =
            utils::create_a2a_message(Some(context_id), Some(task_id), Role::Assistant, content);
        let event = TaskEvent::Message(assistant_message);
        runtime
            .task_manager()
            .add_task_event(auth_ctx, &event)
            .await?;
        runtime.event_bus().publish(&event);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, RegisteredSkill, SkillHandler, SkillMetadata, SkillSlot};
    use crate::models::{Content, LlmResponse};
    use crate::test_support::FakeLlm;
    use a2a_types::{Message, MessageRole, MessageSendParams, Part};

    fn negotiation_response(skill_id: &str, reasoning: &str) -> AgentResult<LlmResponse> {
        let decision = serde_json::json!({
            "type": "start_task",
            "skill_id": skill_id,
            "reasoning": reasoning,
        });
        FakeLlm::text_response(serde_json::to_string(&decision).expect("valid JSON"))
    }

    fn clarification_response(message: &str) -> AgentResult<LlmResponse> {
        let decision = serde_json::json!({
            "type": "ask_clarification",
            "message": message,
        });
        FakeLlm::text_response(serde_json::to_string(&decision).expect("valid JSON"))
    }

    fn rejection_response(reason: &str) -> AgentResult<LlmResponse> {
        let decision = serde_json::json!({
            "type": "reject",
            "reason": reason,
        });
        FakeLlm::text_response(serde_json::to_string(&decision).expect("valid JSON"))
    }

    fn make_message_params(
        text: &str,
        context_id: Option<&str>,
        task_id: Option<&str>,
    ) -> MessageSendParams {
        MessageSendParams {
            message: Message {
                kind: "message".into(),
                message_id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: text.to_string(),
                    metadata: None,
                }],
                context_id: context_id.map(|id| id.to_string()),
                task_id: task_id.map(|id| id.to_string()),
                reference_task_ids: Vec::new(),
                extensions: Vec::new(),
                metadata: None,
            },
            configuration: None,
            metadata: None,
        }
    }

    struct SimpleSkill;

    static SIMPLE_METADATA: SkillMetadata = SkillMetadata::new(
        "simple-skill",
        "Simple Skill",
        "Completes immediately",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for SimpleSkill {
        async fn on_request(
            &self,
            _task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn Runtime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Ok(OnRequestResult::Completed {
                message: Some(Content::from_text("done")),
                artifacts: Vec::new(),
            })
        }
    }

    impl RegisteredSkill for SimpleSkill {
        fn metadata() -> &'static SkillMetadata {
            &SIMPLE_METADATA
        }
    }

    struct MultiTurnSkill;

    static MULTI_METADATA: SkillMetadata = SkillMetadata::new(
        "multi-skill",
        "Multi-turn Skill",
        "Requests additional input",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for MultiTurnSkill {
        async fn on_request(
            &self,
            task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn Runtime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            task_context.save_data("asked", &true)?;
            Ok(OnRequestResult::InputRequired {
                message: Content::from_text("Need more info"),
                slot: SkillSlot::new("details"),
            })
        }

        async fn on_input_received(
            &self,
            task_context: &mut TaskContext,
            _context: &Context,
            _runtime: &dyn Runtime,
            content: Content,
        ) -> Result<OnInputResult, AgentError> {
            task_context.clear_pending_slot();
            let response = format!("Received: {}", content.joined_texts().unwrap_or_default());
            Ok(OnInputResult::Completed {
                message: Some(Content::from_text(response)),
                artifacts: Vec::new(),
            })
        }
    }

    impl RegisteredSkill for MultiTurnSkill {
        fn metadata() -> &'static SkillMetadata {
            &MULTI_METADATA
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn new_context_creates_task_and_records_events() {
        let llm = FakeLlm::with_responses(
            "negotiator",
            [negotiation_response(
                SIMPLE_METADATA.id,
                "Using simple skill",
            )],
        );
        let agent = Agent::builder()
            .with_name("Agent")
            .with_skill(SimpleSkill)
            .build();
        let runtime = Arc::new(DefaultRuntime::new(llm).agents(vec![agent]));
        let agent_ref = runtime.agents.get(0).expect("agent");
        let executor = RequestExecutor::new(Arc::clone(&runtime), agent_ref);

        let params = make_message_params("Do the task", None, None);
        let result = executor
            .handle_send_message(params)
            .await
            .expect("send message result");

        let task = match result {
            SendMessageResult::Task(task) => task,
            _ => panic!("expected task result"),
        };

        assert_eq!(task.status.state, a2a_types::TaskState::Completed);
        let auth = runtime.auth_service().get_auth_context();
        let stored = runtime
            .task_manager()
            .get_task(&auth, &task.id)
            .await
            .expect("get task")
            .expect("task exists");
        assert_eq!(stored.status.state, a2a_types::TaskState::Completed);

        let events = runtime
            .task_manager()
            .get_task_events(&auth, &task.id)
            .await
            .expect("events");
        assert!(events.iter().any(
            |event| matches!(event, TaskEvent::Message(msg) if msg.role == MessageRole::Agent)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn continuation_flow_updates_task() {
        let llm = FakeLlm::with_responses(
            "negotiator",
            [negotiation_response(MULTI_METADATA.id, "Need more info")],
        );
        let agent = Agent::builder()
            .with_name("Agent")
            .with_skill(MultiTurnSkill)
            .build();
        let runtime = Arc::new(DefaultRuntime::new(llm).agents(vec![agent]));
        let agent_ref = runtime.agents.get(0).expect("agent");
        let executor = RequestExecutor::new(Arc::clone(&runtime), agent_ref);

        let first_result = executor
            .handle_send_message(make_message_params("Start", None, None))
            .await
            .expect("first result");
        let task = match first_result {
            SendMessageResult::Task(task) => task,
            _ => panic!("expected task"),
        };
        assert_eq!(task.status.state, a2a_types::TaskState::InputRequired);

        let auth = runtime.auth_service().get_auth_context();

        let continue_params =
            make_message_params("Provide details", Some(&task.context_id), Some(&task.id));
        let continued = executor
            .handle_send_message(continue_params)
            .await
            .expect("continuation");
        let updated = match continued {
            SendMessageResult::Task(task) => task,
            _ => panic!("expected task"),
        };
        assert_eq!(updated.status.state, a2a_types::TaskState::Completed);

        let context = runtime
            .task_manager()
            .load_task_context(&auth, &updated.id)
            .await
            .expect("load context")
            .expect("context present");
        let slot: Option<String> = context.load_slot().expect("slot load");
        assert!(slot.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn negotiation_clarification_yields_message() {
        let llm =
            FakeLlm::with_responses("negotiator", [clarification_response("Need more details")]);
        let agent = Agent::builder()
            .with_name("Agent")
            .with_skill(SimpleSkill)
            .build();
        let runtime = Arc::new(DefaultRuntime::new(llm).agents(vec![agent]));
        let agent_ref = runtime.agents.get(0).expect("agent");
        let executor = RequestExecutor::new(Arc::clone(&runtime), agent_ref);

        let result = executor
            .handle_send_message(make_message_params("Clarify", None, None))
            .await
            .expect("clarification");

        match result {
            SendMessageResult::Message(msg) => {
                assert_eq!(msg.role, MessageRole::Agent);
                assert!(msg.context_id.is_some());
            }
            other => panic!("expected message result, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn negotiation_reject_returns_message() {
        let llm = FakeLlm::with_responses("negotiator", [rejection_response("Out of scope")]);
        let agent = Agent::builder()
            .with_name("Agent")
            .with_skill(SimpleSkill)
            .build();
        let runtime = Arc::new(DefaultRuntime::new(llm).agents(vec![agent]));
        let agent_ref = runtime.agents.get(0).expect("agent");
        let executor = RequestExecutor::new(Arc::clone(&runtime), agent_ref);

        let result = executor
            .handle_send_message(make_message_params("Reject", None, None))
            .await
            .expect("rejection");

        match result {
            SendMessageResult::Message(msg) => {
                assert_eq!(msg.role, MessageRole::Agent);
                assert!(msg.context_id.is_some());
            }
            other => panic!("expected message result, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancel_task_returns_not_implemented() {
        let llm = FakeLlm::with_responses("negotiator", [clarification_response("Need more info")]);
        let agent = Agent::builder()
            .with_name("Agent")
            .with_skill(SimpleSkill)
            .build();
        let runtime = Arc::new(DefaultRuntime::new(llm).agents(vec![agent]));
        let agent_ref = runtime.agents.get(0).expect("agent");
        let executor = RequestExecutor::new(Arc::clone(&runtime), agent_ref);

        let err = executor
            .handle_cancel_task(TaskIdParams {
                id: "task-1".into(),
                metadata: None,
            })
            .await
            .expect_err("expected not implemented");
        match err {
            AgentError::NotImplemented { feature } => {
                assert!(feature.contains("tasks/cancel"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
