#![cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]

//! Web server handlers for the `DefaultRuntime`.

use crate::errors::AgentError;
use crate::runtime::core::executor::{RequestExecutor, TaskStream};
use crate::runtime::DefaultRuntime;
use a2a_types::{
    A2ARequestPayload, AgentCard, AgentSkill, JSONRPCId, MessageSendParams,
    SendStreamingMessageResponse, SendStreamingMessageResult, SendStreamingMessageSuccessResponse,
    TaskIdParams, TransportProtocol,
};
use async_stream::stream;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::time::Duration;

/// Axum handler for serving an agent's AgentCard.
///
/// This function will handle `GET /:agent_id/.well-known/agent-card.json` requests.
pub async fn agent_card_handler(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
) -> Response {
    let agent_def = runtime.agents.iter().find(|a| a.id() == agent_id);

    if let Some(agent) = agent_def {
        let mut card = AgentCard::new(
            agent.name(),
            agent.description().unwrap_or_default(),
            agent.version(),
            format!("http://localhost/{}/{}/rpc", agent.id(), agent.version()),
        );

        card.capabilities.streaming = Some(true);
        card = card.add_interface(
            TransportProtocol::HttpJson,
            format!("http://localhost/{}/{}", agent.id(), agent.version()),
        );

        card.skills = agent
            .skills()
            .iter()
            .map(|s| AgentSkill {
                id: s.id().to_string(),
                name: s.name().to_string(),
                description: s.description().to_string(),
                tags: s.metadata().tags.iter().map(|t| (*t).to_string()).collect(),
                examples: s
                    .metadata()
                    .examples
                    .iter()
                    .map(|e| (*e).to_string())
                    .collect(),
                input_modes: s
                    .metadata()
                    .input_modes
                    .iter()
                    .map(|m| (*m).to_string())
                    .collect(),
                output_modes: s
                    .metadata()
                    .output_modes
                    .iter()
                    .map(|m| (*m).to_string())
                    .collect(),
                security: Vec::new(),
            })
            .collect();

        (StatusCode::OK, Json(card)).into_response()
    } else {
        (StatusCode::NOT_FOUND, "Agent not found").into_response()
    }
}

/// Axum handler for agent-specific JSON-RPC requests.
///
/// This function will handle `POST /:agent_id/rpc` requests.
pub async fn json_rpc_handler(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path((agent_id, version)): Path<(String, String)>,
    Json(payload): Json<a2a_types::A2ARequest>,
) -> Response {
    let agent_def = match runtime.agents.iter().find(|a| a.id() == agent_id) {
        Some(agent) => agent,
        None => return AgentError::AgentNotFound { agent_id }.into_response(),
    };

    if agent_def.version() != version {
        return (
            StatusCode::NOT_FOUND,
            format!("Agent version {} not found for {}", version, agent_id),
        )
            .into_response();
    }

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent_def);

    let request_id = payload.id.clone();

    match payload.payload {
        A2ARequestPayload::SendStreamingMessage { params } => {
            match executor.handle_message_stream(params).await {
                Ok(stream) => build_streaming_sse(request_id.clone(), stream).into_response(),
                Err(e) => e.into_response(),
            }
        }
        A2ARequestPayload::TaskResubscription { params } => {
            match executor.handle_task_resubscribe(params).await {
                Ok(stream) => build_streaming_sse(request_id.clone(), stream).into_response(),
                Err(e) => e.into_response(),
            }
        }
        other => match executor.execute(other).await {
            Ok(response) => response.into_response(),
            Err(e) => e.into_response(),
        },
    }
}

/// Axum handler for the `message/stream` endpoint.
pub async fn message_stream_handler(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path((agent_id, version)): Path<(String, String)>,
    Json(params): Json<MessageSendParams>,
) -> Response {
    let agent_def = match runtime.agents.iter().find(|a| a.id() == agent_id) {
        Some(agent) => agent,
        None => return AgentError::AgentNotFound { agent_id }.into_response(),
    };

    if agent_def.version() != version {
        return (
            StatusCode::NOT_FOUND,
            format!("Agent version {} not found for {}", version, agent_id),
        )
            .into_response();
    }

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent_def);

    match executor.handle_message_stream(params).await {
        Ok(stream) => build_streaming_sse(None, stream).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Axum handler for the `tasks/resubscribe` endpoint.
pub async fn task_resubscribe_handler(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path((agent_id, version, raw_task_id)): Path<(String, String, String)>,
) -> Response {
    let agent_def = match runtime.agents.iter().find(|a| a.id() == agent_id) {
        Some(agent) => agent,
        None => return AgentError::AgentNotFound { agent_id }.into_response(),
    };

    if agent_def.version() != version {
        return (
            StatusCode::NOT_FOUND,
            format!("Agent version {} not found for {}", version, agent_id),
        )
            .into_response();
    }

    let task_id = match raw_task_id.strip_suffix(":subscribe") {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            return AgentError::InvalidInput(
                "tasks/:id:subscribe route requires suffix ':subscribe'".to_string(),
            )
            .into_response()
        }
    };

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent_def);
    let params = TaskIdParams {
        id: task_id,
        metadata: None,
    };

    match executor.handle_task_resubscribe(params).await {
        Ok(stream) => build_streaming_sse(None, stream).into_response(),
        Err(e) => e.into_response(),
    }
}
fn build_streaming_sse(
    request_id: Option<JSONRPCId>,
    stream_state: TaskStream,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut initial_events = stream_state.initial_events.into_iter();
    let task = stream_state.task.clone();
    let id_clone = request_id.clone();
    let receiver = stream_state.receiver;
    let stream = stream! {
        let mut final_seen = false;

        while let Some(event) = initial_events.next() {
            if let Some((evt, is_final)) = task_event_to_event(&request_id, event) {
                yield Ok(evt);
                if is_final {
                    final_seen = true;
                    if let Some(task_evt) = task_to_event(&request_id, &task) {
                        yield Ok(task_evt);
                    }
                    break;
                }
            }
        }

        if !final_seen {
            if let Some(mut rx) = receiver {
                while let Some(event) = rx.recv().await {
                    if let Some((evt, is_final)) = task_event_to_event(&id_clone, event) {
                        yield Ok(evt);
                        if is_final {
                            if let Some(task_evt) = task_to_event(&id_clone, &task) {
                                yield Ok(task_evt);
                            }
                            break;
                        }
                    }
                }
            } else if let Some(task_evt) = task_to_event(&id_clone, &task) {
                yield Ok(task_evt);
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn task_event_to_event(
    request_id: &Option<JSONRPCId>,
    event: crate::runtime::task_manager::TaskEvent,
) -> Option<(Event, bool)> {
    let (result, is_final) = match event {
        crate::runtime::task_manager::TaskEvent::StatusUpdate(update) => {
            let is_final = update.is_final;
            (
                SendStreamingMessageResult::TaskStatusUpdate(update),
                is_final,
            )
        }
        crate::runtime::task_manager::TaskEvent::ArtifactUpdate(update) => (
            SendStreamingMessageResult::TaskArtifactUpdate(update),
            false,
        ),
        crate::runtime::task_manager::TaskEvent::Message(message) => {
            (SendStreamingMessageResult::Message(message), false)
        }
    };

    let response =
        SendStreamingMessageResponse::Success(Box::new(SendStreamingMessageSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result,
            id: request_id.clone(),
        }));

    serde_json::to_string(&response)
        .ok()
        .map(|data| (Event::default().data(data), is_final))
}

fn task_to_event(request_id: &Option<JSONRPCId>, task: &a2a_types::Task) -> Option<Event> {
    let response =
        SendStreamingMessageResponse::Success(Box::new(SendStreamingMessageSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result: SendStreamingMessageResult::Task(task.clone()),
            id: request_id.clone(),
        }));

    serde_json::to_string(&response)
        .ok()
        .map(|data| Event::default().data(data))
}
