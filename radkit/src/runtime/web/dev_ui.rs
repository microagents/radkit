use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use a2a_types::{
    Artifact, FileContent, JSONRPCId, Message, MessageRole, Part, SendStreamingMessageResponse,
    SendStreamingMessageResult, SendStreamingMessageSuccessResponse, TaskArtifactUpdateEvent,
    TaskState, TaskStatus, TaskStatusUpdateEvent,
};
use async_stream::stream;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde::Deserialize;
use std::convert::Infallible;
use tokio::time::Duration;
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::agent::{builder::SkillRegistration, AgentDefinition};
use crate::errors::{AgentError, AgentResult};
use crate::runtime::core::executor::{PreparedSendMessage, RequestExecutor, TaskStream};
use crate::runtime::task_manager::{ListTasksFilter, Task, TaskEvent};
use crate::runtime::{DefaultRuntime, Runtime};

const STATIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dev_ui/static");

/// Builds the router that exposes the developer UI.
pub fn router() -> Router<Arc<DefaultRuntime>> {
    let static_dir = ServeDir::new(STATIC_DIR);

    Router::new()
        .route("/", get(index))
        .route("/agents/:agent_id", get(agent_detail))
        .route("/agents/:agent_id/console/contexts", get(agent_contexts))
        .route("/agents/:agent_id/console/send", post(agent_console_send))
        .route("/agents/:agent_id/console/watch", get(agent_console_watch))
        .nest_service("/static", static_dir)
}

async fn index(State(runtime): State<Arc<DefaultRuntime>>) -> Html<String> {
    let base_url = resolve_base_url(&runtime);

    let markup = render_layout(
        "radkit Dev UI",
        runtime.agents.len(),
        Some(base_url.as_str()),
        html! {
            (agents_listing(&runtime))
        },
    );

    Html(markup.into_string())
}

async fn agent_detail(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let base_url = resolve_base_url(&runtime);
    let agent = runtime
        .agents
        .iter()
        .find(|agent| agent.id() == agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let card_path = format!("/{}/.well-known/agent-card.json", agent.id());
    let card_url = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        card_path.trim_start_matches('/')
    );
    let console_state = gather_console_state(&runtime, agent)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let console_panel = agent_console_panel(agent, &base_url, &console_state);

    let markup = render_layout(
        agent.name(),
        runtime.agents.len(),
        Some(base_url.as_str()),
        html! {
            (detail_header(agent, &card_url))
            div class="detail-layout" {
                div class="detail-layout__main" {
                    (endpoint_overview(agent, &base_url))
                    (skill_overview(agent))
                }
                div class="detail-layout__sidebar" {
                    (console_panel)
                }
            }
        },
    );

    Ok(Html(markup.into_string()))
}

async fn agent_contexts(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
    Query(query): Query<ConsoleContextQuery>,
) -> Result<Html<String>, StatusCode> {
    let agent = runtime
        .agents
        .iter()
        .find(|agent| agent.id() == agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let state = gather_console_state(&runtime, agent)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let markup = render_console_contexts(&state, query.highlight.as_deref());
    Ok(Html(markup.into_string()))
}

async fn agent_console_send(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
    Form(form): Form<ConsoleSendForm>,
) -> Result<Html<String>, StatusCode> {
    let agent = runtime
        .agents
        .iter()
        .find(|agent| agent.id() == agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if agent.version() != form.version {
        return Err(StatusCode::BAD_REQUEST);
    }

    let message_body = form.message.trim();
    if message_body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let context_id = form.context_id.filter(|s| !s.trim().is_empty());
    let task_id = form.task_id.filter(|s| !s.trim().is_empty());

    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: message_body.to_string(),
            metadata: None,
        }],
        context_id: context_id.clone(),
        task_id: task_id.clone(),
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params = a2a_types::MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent);
    let version = agent.version();

    match executor.handle_message_stream(params).await {
        Ok(PreparedSendMessage::Task(stream)) => {
            let markup = render_console_stream_response(agent.id(), version, stream);
            Ok(Html(markup.into_string()))
        }
        Ok(PreparedSendMessage::Message(message)) => {
            let markup = render_console_message_response(&message);
            Ok(Html(markup.into_string()))
        }
        Err(error) => {
            let markup = render_console_error(&error);
            Ok(Html(markup.into_string()))
        }
    }
}

async fn agent_console_watch(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
    Query(query): Query<ConsoleWatchQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let agent = runtime
        .agents
        .iter()
        .find(|agent| agent.id() == agent_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    if agent.version() != query.version {
        return Err(StatusCode::BAD_REQUEST);
    }

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent);
    let params = a2a_types::TaskIdParams {
        id: query.task_id.clone(),
        metadata: None,
    };

    match executor.handle_task_resubscribe(params).await {
        Ok(stream) => Ok(build_console_stream_sse(stream)),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

#[derive(Default)]
struct ConsoleState {
    contexts: Vec<ContextState>,
    context_ids: Vec<String>,
}

struct ContextState {
    id: String,
    tasks: Vec<TaskView>,
    negotiation: Vec<Message>,
}

impl ContextState {
    fn new(id: String) -> Self {
        Self {
            id,
            tasks: Vec::new(),
            negotiation: Vec::new(),
        }
    }
}

struct TaskView {
    id: String,
    status: TaskStatus,
    artifacts: Vec<Artifact>,
    messages: Vec<Message>,
    status_events: Vec<TaskStatusUpdateEvent>,
    artifact_events: Vec<TaskArtifactUpdateEvent>,
    last_update_display: Option<String>,
    last_update_order: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Default)]
struct ConsoleContextQuery {
    #[serde(default)]
    highlight: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConsoleSendForm {
    version: String,
    message: String,
    #[serde(default)]
    context_id: Option<String>,
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConsoleWatchQuery {
    version: String,
    task_id: String,
}

async fn gather_console_state(
    runtime: &DefaultRuntime,
    _agent: &AgentDefinition,
) -> AgentResult<ConsoleState> {
    let auth_ctx = runtime.auth_service().get_auth_context();
    let task_manager = runtime.task_manager();
    let tasks = task_manager
        .list_tasks(&auth_ctx, &ListTasksFilter::default())
        .await?;

    let mut contexts: BTreeMap<String, ContextState> = BTreeMap::new();
    let mut context_ids: HashSet<String> = HashSet::new();

    for task in tasks.items {
        context_ids.insert(task.context_id.clone());
        let events = task_manager
            .get_task_events(&auth_ctx, &task.id)
            .await
            .unwrap_or_default();

        let view = build_task_view(task.clone(), events);
        contexts
            .entry(task.context_id.clone())
            .or_insert_with(|| ContextState::new(task.context_id.clone()))
            .tasks
            .push(view);
    }

    for context_id in context_ids.iter() {
        let negotiation = task_manager
            .get_negotiating_messages(&auth_ctx, context_id)
            .await
            .unwrap_or_default();
        if !negotiation.is_empty() {
            contexts
                .entry(context_id.clone())
                .or_insert_with(|| ContextState::new(context_id.clone()))
                .negotiation = negotiation;
        }
    }

    let mut context_vec: Vec<ContextState> = contexts
        .into_iter()
        .map(|(_, mut ctx)| {
            ctx.tasks
                .sort_by(|a, b| b.last_update_order.cmp(&a.last_update_order));
            ctx
        })
        .collect();
    context_vec.sort_by(|a, b| a.id.cmp(&b.id));

    let mut context_ids_vec: Vec<String> = context_ids.into_iter().collect();
    context_ids_vec.sort();

    Ok(ConsoleState {
        contexts: context_vec,
        context_ids: context_ids_vec,
    })
}

fn build_task_view(task: Task, events: Vec<TaskEvent>) -> TaskView {
    let mut messages = Vec::new();
    let mut status_events = Vec::new();
    let mut artifact_events = Vec::new();
    let mut last_timestamp: Option<DateTime<Utc>> =
        parse_timestamp(task.status.timestamp.as_deref());
    let mut last_display = task.status.timestamp.clone();

    for event in events {
        match event {
            TaskEvent::Message(msg) => messages.push(msg),
            TaskEvent::StatusUpdate(update) => {
                if let Some(ts) = parse_timestamp(update.status.timestamp.as_deref()) {
                    if last_timestamp.map(|current| ts > current).unwrap_or(true) {
                        last_timestamp = Some(ts);
                        last_display = update.status.timestamp.clone();
                    }
                }
                status_events.push(update);
            }
            TaskEvent::ArtifactUpdate(update) => artifact_events.push(update),
        }
    }

    TaskView {
        id: task.id,
        status: task.status,
        artifacts: task.artifacts,
        messages,
        status_events,
        artifact_events,
        last_update_display: last_display,
        last_update_order: last_timestamp,
    }
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn agent_console_panel(agent: &AgentDefinition, base_url: &str, state: &ConsoleState) -> Markup {
    let version = agent.version();
    let send_url = format!("/agents/{}/console/send", agent.id());
    let contexts_url = format!("/agents/{}/console/contexts", agent.id());
    let watch_base = format!("/agents/{}/console/watch", agent.id());
    let stream_placeholder = html! {
        section class="panel panel--inset console-stream" id="console-stream" data-watch-base=(watch_base) {
            div class="stack stack--tight" {
                span class="body" { "Stream output" }
                span class="body-sm muted" {
                    "Submit a request to start streaming task updates. "
                    "This panel shows status transitions, assistant messages, and artifact updates in real time."
                }
            }
        }
    };

    let context_selector = if state.context_ids.is_empty() {
        html! {
            select name="context_id" class="field__control" {
                option value="" selected { "Start new context" }
            }
        }
    } else {
        html! {
            select name="context_id" class="field__control" {
                option value="" { "Start new context" }
                optgroup label="Existing contexts" {
                    @for ctx in &state.context_ids {
                        option value=(ctx) { (ctx) }
                    }
                }
            }
        }
    };

    html! {
        section class="panel stack stack--tight agent-console" {
            div class="cluster cluster--between cluster--align-center" {
                div class="stack stack--tight" {
                    h2 class="section-label" { "agent console" }
                    span class="body-sm muted" {
                        "Dispatch test inputs to "
                        span class="mono" { (agent.id()) }
                        " and observe how tasks progress across contexts."
                    }
                }
                div class="cluster cluster--gap-xs" {
                    a class="button button--ghost body-sm" href=(format!("{}/{}", base_url.trim_end_matches('/'), agent.id())) target="_blank" {
                        "Open RPC guide"
                    }
                    button
                        type="button"
                        class="button button--ghost body-sm"
                        data-console-refresh
                        data-console-refresh-url=(contexts_url.clone())
                    {
                        "Refresh contexts"
                    }
                }
            }

            form
                id="console-send-form"
                class="panel panel--inset stack stack--tight console-form"
                hx-post=(send_url.clone())
                hx-target="#console-stream"
                hx-swap="outerHTML"
            {
                input type="hidden" name="version" value=(version);
                fieldset class="stack stack--tight" {
                    legend class="body-sm muted" { "Send test input" }
                    label class="field" {
                        span class="field__label body-sm muted" { "Context" }
                        (context_selector)
                        span class="body-xs muted" {
                            "Leave blank to start a fresh negotiation. "
                            "Selecting an existing context allows follow-up conversations."
                        }
                    }
                    label class="field" {
                        span class="field__label body-sm muted" { "Existing task (optional)" }
                        input type="text" name="task_id" class="field__control" placeholder="task identifier for continuation";
                        span class="body-xs muted" {
                            "Provide when continuing a task that is waiting for input."
                        }
                    }
                    label class="field" {
                        span class="field__label body-sm muted" { "Message" }
                        textarea
                            name="message"
                            class="field__control field__control--multiline"
                            rows="4"
                            required
                            placeholder="Describe the request you want the agent to handle." {}
                    }
                }
                div class="cluster cluster--gap-sm cluster--wrap" {
                    button type="submit" class="button button--primary" { "Send message" }
                    span class="body-xs muted" {
                        "The response stream appears below. "
                        "Task activity also updates the context timeline."
                    }
                }
            }

            (stream_placeholder)

            div
                class="panel panel--inset stack stack--tight console-contexts"
                id="console-contexts"
                data-contexts-url=(contexts_url)
            {
                (render_console_contexts(state, None))
            }
        }
    }
}

fn render_console_contexts(state: &ConsoleState, highlight: Option<&str>) -> Markup {
    if state.contexts.is_empty() {
        return html! {
            div class="empty-state" {
                span class="body" { "No tasks recorded yet" }
                span class="body-sm muted" {
                    "As you exercise the agent, negotiation history, task lifecycles, and artifact events will appear here."
                }
            }
        };
    }

    html! {
        @for context in &state.contexts {
            (render_console_context(context, highlight))
        }
    }
}

fn render_console_context(context: &ContextState, highlight: Option<&str>) -> Markup {
    let highlight_class = if highlight == Some(context.id.as_str()) {
        "console-context console-context--highlight"
    } else {
        "console-context"
    };

    let negotiation_section = if context.negotiation.is_empty() {
        html! {}
    } else {
        html! {
            details class="console-context__negotiation" open {
                summary class="body-sm muted" { "Negotiation history" }
                ul class="console-context__messages" {
                    @for message in &context.negotiation {
                        (render_message_list_item(message))
                    }
                }
            }
        }
    };

    html! {
        section class=(highlight_class) data-context-id=(context.id.clone()) {
            header class="console-context__header cluster cluster--between cluster--align-center" {
                div class="stack stack--tight" {
                    span class="body-sm muted" { "Context" }
                    span class="mono" { (context.id) }
                }
                span class="badge" { (format!("{} task{}", context.tasks.len(), if context.tasks.len() == 1 { "" } else { "s" })) }
            }
            (negotiation_section)
            @for task in &context.tasks {
                (render_context_task(task))
            }
        }
    }
}

fn render_context_task(task: &TaskView) -> Markup {
    let state_label = format_task_state(&task.status.state);
    let status_class = match task.status.state {
        TaskState::Completed => "status-pill status-pill--complete",
        TaskState::Failed => "status-pill status-pill--failed",
        TaskState::Rejected => "status-pill status-pill--rejected",
        TaskState::InputRequired => "status-pill status-pill--input",
        TaskState::Working => "status-pill status-pill--working",
        TaskState::Submitted => "status-pill status-pill--submitted",
        _ => "status-pill",
    };

    let last_updated = task
        .last_update_display
        .clone()
        .unwrap_or_else(|| "recent activity".to_string());

    html! {
        article class="console-task" data-task-id=(task.id.clone()) {
            header class="console-task__header cluster cluster--between cluster--align-center" {
                div class="stack stack--tight" {
                    span class="body-sm muted" { "Task" }
                    span class="mono" { (task.id) }
                }
                span class=(status_class) { (state_label) }
            }
            div class="console-task__meta body-xs muted" {
                span { "Last update: " (last_updated) }
            }

            @if !task.messages.is_empty() {
                div class="console-task__section" {
                    span class="body-sm muted" { "Conversation" }
                    ul class="console-context__messages" {
                        @for message in &task.messages {
                            (render_message_list_item(message))
                        }
                    }
                }
            }

            @if !task.status_events.is_empty() {
                div class="console-task__section" {
                    span class="body-sm muted" { "Status timeline" }
                    ul class="timeline" {
                        @for event in &task.status_events {
                            (render_status_timeline_item(event))
                        }
                    }
                }
            }

            @if !task.artifact_events.is_empty() || !task.artifacts.is_empty() {
                div class="console-task__section" {
                    span class="body-sm muted" { "Artifacts" }
                    ul class="console-task__artifacts" {
                        @for artifact in &task.artifacts {
                            li { (artifact_entry(artifact)) }
                        }
                        @for event in &task.artifact_events {
                            li { (artifact_event_entry(event)) }
                        }
                    }
                }
            }
        }
    }
}

fn render_message_list_item(message: &Message) -> Markup {
    let role = match message.role {
        MessageRole::User => "User",
        MessageRole::Agent => "Agent",
    };
    let summary = extract_message_text(message).unwrap_or_else(|| "<non-text payload>".to_string());

    html! {
        li class="console-message" {
            span class="console-message__role" { (role) }
            span class="console-message__text" { (summary) }
        }
    }
}

fn render_status_timeline_item(event: &TaskStatusUpdateEvent) -> Markup {
    let label = format_task_state(&event.status.state);
    let timestamp = event
        .status
        .timestamp
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let mut badge = label.to_string();
    if event.is_final {
        badge.push_str(" · final");
    }

    let message = event.status.message.as_ref().and_then(extract_message_text);

    html! {
        li class="timeline__item" {
            div class="timeline__marker" {}
            div class="timeline__content" {
                div class="timeline__meta" {
                    span class="timeline__label" { (badge) }
                    span class="timeline__timestamp body-xs muted" { (timestamp) }
                }
                @if let Some(text) = message {
                    p class="timeline__body body-sm" { (text) }
                }
            }
        }
    }
}

fn artifact_entry(artifact: &Artifact) -> Markup {
    let name = artifact
        .name
        .clone()
        .unwrap_or_else(|| artifact.artifact_id.clone());
    html! {
        div class="artifact-card" {
            span class="body-sm" { (name) }
            @if !artifact.parts.is_empty() {
                span class="body-xs muted" { (format!("{} part{}", artifact.parts.len(), if artifact.parts.len() == 1 { "" } else { "s" })) }
            }
        }
    }
}

fn artifact_event_entry(event: &TaskArtifactUpdateEvent) -> Markup {
    let name = event
        .artifact
        .name
        .clone()
        .unwrap_or_else(|| event.artifact.artifact_id.clone());
    let mut details = format!("update for {}", name);
    if event.last_chunk.unwrap_or(false) {
        details.push_str(" (final)");
    }
    html! {
        div class="artifact-card artifact-card--update" {
            span class="body-sm" { (event.artifact.artifact_id.clone()) }
            span class="body-xs muted" { (details) }
        }
    }
}

fn render_console_stream_response(agent_id: &str, version: &str, stream: TaskStream) -> Markup {
    let TaskStream {
        task,
        initial_events,
        ..
    } = stream;
    let context_id = task.context_id.clone();
    let watch_url = format!(
        "/agents/{}/console/watch?version={}&task_id={}",
        agent_id, version, task.id
    );

    let summary = render_task_summary(&task);

    html! {
        section
            class="panel panel--inset console-stream"
            id="console-stream"
            data-stream-url=(watch_url)
            data-task-id=(task.id.clone())
            data-context-id=(context_id.clone())
        {
            (summary)
            @if !initial_events.is_empty() {
                div class="console-stream__initial" {
                    @for event in initial_events.iter() {
                        (render_stream_event(event))
                    }
                }
            }
            div class="console-stream__output" data-stream-output {
                span class="body-xs muted" { "Listening for live updates…" }
            }
        }
        div data-trigger-context-refresh data-highlight=(context_id) {}
    }
}

fn render_console_message_response(message: &Message) -> Markup {
    let role = match message.role {
        MessageRole::User => "Agent clarification",
        MessageRole::Agent => "Assistant reply",
    };
    let summary = extract_message_text(message).unwrap_or_else(|| "<non-text payload>".to_string());
    let highlight = message.context_id.clone();

    html! {
        section class="panel panel--inset console-stream console-stream--message" id="console-stream" {
            div class="stack stack--tight" {
                span class="body-sm muted" { (role) }
                p class="body" { (summary) }
                span class="body-xs muted" {
                    "No task was created. Continue negotiation in the same context or start a new request."
                }
            }
        }
        @if let Some(ctx) = highlight {
            div data-trigger-context-refresh data-highlight=(ctx) {}
        }
    }
}

fn render_console_error(error: &AgentError) -> Markup {
    html! {
        section class="panel panel--inset console-stream console-stream--error" id="console-stream" {
            span class="body-sm muted" { "Request failed" }
            pre class="console-stream__error" { (format!("{error:?}")) }
        }
    }
}

fn render_task_summary(task: &a2a_types::Task) -> Markup {
    let status_label = format_task_state(&task.status.state);
    html! {
        header class="console-stream__header cluster cluster--between cluster--align-center" {
            div class="stack stack--tight" {
                span class="body-sm muted" { "Task" }
                span class="mono" { (task.id.clone()) }
                span class="body-xs muted" {
                    "Context "
                    span class="mono" { (task.context_id.clone()) }
                }
            }
            span class="status-pill" { (status_label) }
        }
    }
}

fn render_stream_event(event: &TaskEvent) -> Markup {
    match event {
        TaskEvent::StatusUpdate(update) => {
            let label = format_task_state(&update.status.state);
            let timestamp = update
                .status
                .timestamp
                .clone()
                .unwrap_or_else(|| "—".to_string());
            let mut badge = label.to_string();
            if update.is_final {
                badge.push_str(" · final");
            }
            let message = update
                .status
                .message
                .as_ref()
                .and_then(extract_message_text);

            html! {
                div class="stream-entry stream-entry--status" {
                    div class="stream-entry__meta" {
                        span class="stream-entry__label" { (badge) }
                        span class="stream-entry__timestamp body-xs muted" { (timestamp) }
                    }
                    @if let Some(text) = message {
                        p class="stream-entry__body body-sm" { (text) }
                    }
                }
            }
        }
        TaskEvent::ArtifactUpdate(update) => {
            let name = update
                .artifact
                .name
                .clone()
                .unwrap_or_else(|| update.artifact.artifact_id.clone());
            let mut details = String::new();
            if update.last_chunk.unwrap_or(false) {
                details.push_str("final chunk · ");
            }
            if let Some(append) = update.append {
                if append {
                    details.push_str("append");
                }
            }

            html! {
                div class="stream-entry stream-entry--artifact" {
                    div class="stream-entry__meta" {
                        span class="stream-entry__label" { "Artifact update" }
                        span class="stream-entry__timestamp body-xs muted" { (name) }
                    }
                    @if !details.is_empty() {
                        span class="stream-entry__body body-xs muted" { (details) }
                    }
                }
            }
        }
        TaskEvent::Message(message) => {
            let role = match message.role {
                MessageRole::User => "User",
                MessageRole::Agent => "Agent",
            };
            let summary =
                extract_message_text(message).unwrap_or_else(|| "<non-text payload>".to_string());

            html! {
                div class="stream-entry stream-entry--message" {
                    div class="stream-entry__meta" {
                        span class="stream-entry__label" { (role) }
                        span class="stream-entry__timestamp body-xs muted" {
                            (message.context_id.clone().unwrap_or_else(|| "-".to_string()))
                        }
                    }
                    p class="stream-entry__body body-sm" { (summary) }
                }
            }
        }
    }
}

fn extract_message_text(message: &Message) -> Option<String> {
    message.parts.iter().find_map(|part| match part {
        Part::Text { text, .. } => Some(text.clone()),
        Part::Data { data, .. } => Some(data.to_string()),
        Part::File { file, .. } => match file {
            FileContent::WithBytes(inner) => inner
                .name
                .clone()
                .or_else(|| inner.mime_type.clone())
                .or(Some("binary content".to_string())),
            FileContent::WithUri(inner) => inner.name.clone().or(Some(inner.uri.clone())),
        },
    })
}

fn format_task_state(state: &TaskState) -> &'static str {
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::InputRequired => "input required",
        TaskState::Completed => "completed",
        TaskState::Canceled => "canceled",
        TaskState::Failed => "failed",
        TaskState::Rejected => "rejected",
        TaskState::AuthRequired => "auth required",
        TaskState::Unknown => "unknown",
    }
}

fn build_console_stream_sse(
    stream_state: TaskStream,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut initial_events = stream_state.initial_events.into_iter();
    let task = stream_state.task.clone();
    let receiver = stream_state.receiver;

    let stream = stream! {
        for event in initial_events.by_ref() {
            if let Some(evt) = console_task_event_to_event(&event) {
                let is_final = matches!(
                    &event,
                    TaskEvent::StatusUpdate(update) if update.is_final
                );
                yield Ok(evt);
                if is_final {
                    if let Some(task_evt) = console_task_to_event(&task) {
                        yield Ok(task_evt);
                    }
                    return;
                }
            }
        }

        if let Some(mut rx) = receiver {
            while let Some(event) = rx.recv().await {
                if let Some(evt) = console_task_event_to_event(&event) {
                    let is_final = matches!(
                        &event,
                        TaskEvent::StatusUpdate(update) if update.is_final
                    );
                    yield Ok(evt);
                    if is_final {
                        if let Some(task_evt) = console_task_to_event(&task) {
                            yield Ok(task_evt);
                        }
                        break;
                    }
                }
            }
        } else if let Some(task_evt) = console_task_to_event(&task) {
            yield Ok(task_evt);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn console_task_event_to_event(event: &TaskEvent) -> Option<Event> {
    let result = match event {
        TaskEvent::StatusUpdate(update) => {
            SendStreamingMessageResult::TaskStatusUpdate(update.clone())
        }
        TaskEvent::ArtifactUpdate(update) => {
            SendStreamingMessageResult::TaskArtifactUpdate(update.clone())
        }
        TaskEvent::Message(message) => SendStreamingMessageResult::Message(message.clone()),
    };

    let response =
        SendStreamingMessageResponse::Success(Box::new(SendStreamingMessageSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result,
            id: Option::<JSONRPCId>::None,
        }));

    serde_json::to_string(&response)
        .ok()
        .map(|data| Event::default().data(data))
}

fn console_task_to_event(task: &a2a_types::Task) -> Option<Event> {
    let response =
        SendStreamingMessageResponse::Success(Box::new(SendStreamingMessageSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result: SendStreamingMessageResult::Task(task.clone()),
            id: Option::<JSONRPCId>::None,
        }));

    serde_json::to_string(&response)
        .ok()
        .map(|data| Event::default().data(data))
}

fn render_layout(
    title: &str,
    agent_count: usize,
    base_url: Option<&str>,
    content: Markup,
) -> Markup {
    let boot_script = PreEscaped(
        r#"(function () {
            var root = document.documentElement;
            var key = 'radkit:theme';
            try {
                var stored = window.localStorage.getItem(key);
                if (stored === 'light' || stored === 'dark') {
                    root.dataset.theme = stored;
                    return;
                }
            } catch (_) {}
            root.removeAttribute('data-theme');
        })();
        "#,
    );

    let toggle_script = PreEscaped(
        r#"document.addEventListener('DOMContentLoaded', function () {
            var key = 'radkit:theme';
            var root = document.documentElement;
            var control = document.querySelector('[data-theme-toggle]');
            if (!control) {
                return;
            }
            var label = control.querySelector('[data-theme-toggle-label]');
            var ensure = function (value) {
                if (value === 'dark' || value === 'light') {
                    return value;
                }
                return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
            };
            var stored = null;
            try {
                stored = window.localStorage.getItem(key);
            } catch (_) {}
            var apply = function (value) {
                var theme = ensure(value);
                root.dataset.theme = theme;
                if (label) {
                    label.textContent = theme === 'dark' ? 'Dark mode' : 'Light mode';
                }
                control.setAttribute(
                    'aria-label',
                    theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'
                );
                try {
                    window.localStorage.setItem(key, theme);
                } catch (_) {}
            };
            apply(ensure(root.dataset.theme || stored));
            control.addEventListener('click', function () {
                apply(root.dataset.theme === 'dark' ? 'light' : 'dark');
            });
        });
        "#,
    );

    let console_script = PreEscaped(
        r#"document.addEventListener('DOMContentLoaded', function () {
            var streams = new Map();

            function closeStream(el) {
                var existing = streams.get(el);
                if (existing) {
                    existing.close();
                    streams.delete(el);
                }
            }

            function extractText(message) {
                if (!message || !message.parts) {
                    return '';
                }
                for (var i = 0; i < message.parts.length; i++) {
                    var part = message.parts[i];
                    if (part.kind === 'text' && part.text) {
                        return part.text;
                    }
                }
                return '';
            }

            function describeEvent(payload) {
                var result = payload && payload.result ? payload.result : payload;
                if (!result) {
                    return { type: 'raw', title: 'event', body: '' };
                }

                if (result.kind === 'status-update' || result.status) {
                    var title = 'Status: ' + (result.status && result.status.state ? result.status.state : 'update');
                    var body = '';
                    if (result.status && result.status.message) {
                        body = extractText(result.status.message);
                    }
                    if (result.final) {
                        title += ' (final)';
                    }
                    return { type: 'status', title: title, body: body };
                }

                if (result.kind === 'artifact-update' || result.artifact) {
                    var artifactId = result.artifact && result.artifact.artifactId ? result.artifact.artifactId : 'artifact';
                    return { type: 'artifact', title: 'Artifact update: ' + artifactId, body: '' };
                }

                if (result.role) {
                    var label = result.role === 'agent' ? 'Agent message' : 'User message';
                    return { type: 'message', title: label, body: extractText(result) };
                }

                if (result.contextId && result.id) {
                    return { type: 'status', title: 'Task ' + result.id, body: '' };
                }

                return { type: 'raw', title: 'event', body: '' };
            }

            function appendEntry(container, info) {
                if (!container) {
                    return;
                }
                var entry = document.createElement('div');
                entry.className = 'stream-entry stream-entry--' + info.type;

                var meta = document.createElement('div');
                meta.className = 'stream-entry__meta';
                var label = document.createElement('span');
                label.className = 'stream-entry__label';
                label.textContent = info.title;
                meta.appendChild(label);
                entry.appendChild(meta);

                if (info.body) {
                    var body = document.createElement('div');
                    body.className = 'stream-entry__body body-sm';
                    body.textContent = info.body;
                    entry.appendChild(body);
                }

                container.appendChild(entry);
                container.scrollTop = container.scrollHeight;
            }

            function attachStream(el) {
                var url = el.dataset.streamUrl;
                if (!url) {
                    return;
                }

                closeStream(el);
                var output = el.querySelector('[data-stream-output]');
                if (output) {
                    output.textContent = '';
                }

                try {
                    var source = new EventSource(url);
                    el.dataset.streamStatus = 'open';

                    source.onmessage = function (evt) {
                        try {
                            var payload = JSON.parse(evt.data);
                            appendEntry(output, describeEvent(payload));
                        } catch (err) {
                            appendEntry(output, { type: 'raw', title: 'event', body: evt.data });
                        }
                    };

                    source.onerror = function () {
                        el.dataset.streamStatus = 'closed';
                        source.close();
                        streams.delete(el);
                    };

                    streams.set(el, source);
                } catch (_) {
                    // Ignore connection errors for development purposes.
                }
            }

            function refreshContexts(highlight) {
                var container = document.getElementById('console-contexts');
                if (!container) {
                    return;
                }
                var url = container.dataset.contextsUrl;
                if (!url || !window.htmx) {
                    return;
                }
                var requestUrl = url;
                if (highlight) {
                    requestUrl += (url.indexOf('?') === -1 ? '?' : '&') + 'highlight=' + encodeURIComponent(highlight);
                }
                window.htmx.ajax('GET', requestUrl, { target: '#console-contexts', swap: 'innerHTML' });
            }

            document.body.addEventListener('click', function (evt) {
                var trigger = evt.target.closest('[data-console-refresh]');
                if (trigger && window.htmx) {
                    var manualUrl = trigger.getAttribute('data-console-refresh-url');
                    if (manualUrl) {
                        window.htmx.ajax('GET', manualUrl, { target: '#console-contexts', swap: 'innerHTML' });
                    } else {
                        refreshContexts();
                    }
                }
            });

            document.body.addEventListener('htmx:beforeSwap', function (evt) {
                evt.target.querySelectorAll('[data-stream-url]').forEach(closeStream);
            });

            document.body.addEventListener('htmx:afterSwap', function (evt) {
                evt.target.querySelectorAll('[data-stream-url]').forEach(attachStream);
                var marker = evt.target.querySelector('[data-trigger-context-refresh]');
                if (marker) {
                    refreshContexts(marker.dataset.highlight || null);
                    marker.remove();
                }
            });

            document.querySelectorAll('[data-stream-url]').forEach(attachStream);

            window.addEventListener('beforeunload', function () {
                streams.forEach(function (source) { source.close(); });
                streams.clear();
            });
        });
        "#,
    );

    let _base_display = base_url.unwrap_or("http://localhost");

    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (format!("{title} · radkit Dev UI")) }
                script { (boot_script) }
                link rel="stylesheet" href="/static/dev-ui.css?v=20251103";
                script defer src="https://unpkg.com/htmx.org@1.9.10" {};
            }
            body class="app" {
                header class="app__header" {
                    div class="brand" {
                        span class="brand__title" { "radkit dashboard" }
                        span class="brand__badge" { (format!("{agent_count} agents")) }
                    }
                    button
                        type="button"
                        class="theme-toggle"
                        data-theme-toggle
                        aria-live="polite"
                        aria-label="Switch to light mode"
                    {
                        span class="theme-toggle__icon theme-toggle__icon--light" aria-hidden="true" {
                            (PreEscaped(
                                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"></circle><line x1="12" y1="2" x2="12" y2="4"></line><line x1="12" y1="20" x2="12" y2="22"></line><line x1="4.93" y1="4.93" x2="6.34" y2="6.34"></line><line x1="17.66" y1="17.66" x2="19.07" y2="19.07"></line><line x1="2" y1="12" x2="4" y2="12"></line><line x1="20" y1="12" x2="22" y2="12"></line><line x1="4.93" y1="19.07" x2="6.34" y2="17.66"></line><line x1="17.66" y1="6.34" x2="19.07" y2="4.93"></line></svg>"#,
                            ))
                        }
                        span class="theme-toggle__icon theme-toggle__icon--dark" aria-hidden="true" {
                            (PreEscaped(
                                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.8A9 9 0 0 1 11.2 3 7 7 0 1 0 21 12.8Z"></path></svg>"#,
                            ))
                        }
                        span class="theme-toggle__label" data-theme-toggle-label { "Dark mode" }
                    }
                }
                main class="app__main stack stack--loose" {
                    (content)
                }
                footer class="app__footer" { "radkit runtime dev tools" }
                script { (toggle_script) }
                script { (console_script) }
            }
        }
    }
}

fn agents_listing(runtime: &DefaultRuntime) -> Markup {
    if runtime.agents.is_empty() {
        return html! {
            section class="panel panel--inset empty-state" {
                div class="stack stack--tight" {
                    span class="body" { "No agents registered" }
                    span class="body-sm muted" {
                        "Use "
                        span class="mono" { ".agents(vec![...])" }
                        " to register agents before starting the runtime."
                    }
                }
            }
        };
    }

    html! {
        section class="stack stack--tight" {
            div class="cluster cluster--wrap" {
                span class="section-label" { "agents" }
            }
            div class="card-list card-list--agents" {
                @for agent in runtime.agents.iter() {
                    (agent_card(agent))
                }
            }
        }
    }
}

fn agent_card(agent: &AgentDefinition) -> Markup {
    html! {
        a class="panel panel--dense agent-card" href={(format!("/agents/{}", agent.id()))} {
            div class="agent-card__meta" {
                span { "agent" }
                span { (agent.version()) }
            }
            h3 class="body" { (agent.name()) }
            p class="body-sm muted" { (agent.description().unwrap_or("No description provided.")) }
            span class="body-sm muted" { (format!("{} skill(s)", agent.skills().len())) }
        }
    }
}

fn detail_header(agent: &AgentDefinition, card_url: &str) -> Markup {
    html! {
        section class="stack stack--loose" {
            a class="button button--ghost" href="/" { "← All agents" }
            div class="panel stack stack--tight" {
                div class="cluster cluster--between cluster--wrap" {
                    div class="stack stack--tight" {
                        span class="section-label" { "agent" }
                        h1 class="headline" { (agent.name()) }
                        p class="body muted" { (agent.description().unwrap_or("No description provided.")) }
                    }
                    div class="cluster cluster--wrap cluster--gap-sm" {
                        a class="button button--primary" href=(card_url) target="_blank" rel="noreferrer" {
                            "Agent card"
                        }
                    }
                }
                div class="stat-group" {
                    div class="stat" {
                        span class="stat__label" { "identifier" }
                        span class="mono body-sm" { (agent.id()) }
                    }
                    div class="stat" {
                        span class="stat__label" { "version" }
                        span class="mono body-sm" { (agent.version()) }
                    }
                }
            }
        }
    }
}

fn endpoint_overview(agent: &AgentDefinition, base_url: &str) -> Markup {
    let base_trimmed = base_url.trim_end_matches('/');
    let agent_base = format!("{base_trimmed}/{}", agent.id());
    let versioned = format!("{agent_base}/{}", agent.version());

    html! {
        section class="panel stack stack--tight" {
            h2 class="section-label" { "endpoints" }
            ul class="endpoint-list" {
                (endpoint_item("Agent root", &agent_base, "Shared prefix for agent-specific routes."))
                (endpoint_item("JSON-RPC", &format!("{versioned}/rpc"), "Invoke transport requests using JSON-RPC."))
                (endpoint_item(
                    "Message stream",
                    &format!("{versioned}/message:stream"),
                    "Listen for streaming responses via SSE."
                ))
                (endpoint_item(
                    "Task resubscribe",
                    &format!("{versioned}/tasks/:id:subscribe"),
                    "Resume SSE for an existing task."
                ))
            }
        }
    }
}

fn endpoint_item(title: &str, url: &str, description: &str) -> Markup {
    html! {
        li class="endpoint" {
            span class="endpoint__title" { (title) }
            span class="body-sm muted" { (description) }
            span class="endpoint__url" { (url) }
        }
    }
}

fn skill_overview(agent: &AgentDefinition) -> Markup {
    if agent.skills().is_empty() {
        return html! {
            section class="panel panel--inset empty-state" {
                "No skills registered for this agent."
            }
        };
    }

    html! {
        section class="stack stack--tight" {
            h2 class="section-label" { "skills" }
            div class="card-list card-list--skills" {
                @for skill in agent.skills().iter() {
                    (skill_card(skill))
                }
            }
        }
    }
}

fn skill_card(skill: &SkillRegistration) -> Markup {
    let metadata = skill.metadata();
    let tags = metadata.tags;
    let inputs = metadata.input_modes;
    let outputs = metadata.output_modes;
    let examples = metadata.examples;

    html! {
        div class="panel panel--dense stack stack--tight" {
            h3 class="body" { (metadata.name) }
            p class="body-sm muted" { (metadata.description) }
            span class="mono body-sm muted" { (metadata.id) }
            @if !tags.is_empty() {
                div class="chip-group" {
                    @for tag in tags {
                        span class="chip" { (tag) }
                    }
                }
            }
            @if !inputs.is_empty() || !outputs.is_empty() {
                div class="chip-group" {
                    @for input in inputs {
                        span class="chip" { (format!("in {input}")) }
                    }
                    @for output in outputs {
                        span class="chip" { (format!("out {output}")) }
                    }
                }
            }
            @if !examples.is_empty() {
                details class="accordion" {
                    summary class="accordion__summary" { "usage examples" }
                    ul class="accordion__list" {
                        @for sample in examples {
                            li { (sample) }
                        }
                    }
                }
            }
        }
    }
}

fn resolve_base_url(runtime: &DefaultRuntime) -> String {
    runtime.base_url.clone().unwrap_or_else(|| {
        runtime.bind_address.as_ref().map_or_else(
            || "http://localhost".to_string(),
            |addr| super::infer_base_url(addr),
        )
    })
}
