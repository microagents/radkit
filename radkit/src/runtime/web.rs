#![cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]

//! Web server handlers for the `DefaultRuntime`.

use crate::agent::AgentDefinition;
use crate::errors::AgentError;
use crate::runtime::core::error_mapper;
use crate::runtime::core::executor::{PreparedSendMessage, RequestExecutor, TaskStream};
use crate::runtime::DefaultRuntime;
use a2a_types::{
    A2ARequestPayload, AgentCard, AgentSkill, CancelTaskResponse, CancelTaskSuccessResponse,
    GetTaskResponse, GetTaskSuccessResponse, JSONRPCErrorResponse, JSONRPCId, MessageSendParams,
    SendMessageResponse, SendMessageResult, SendMessageSuccessResponse,
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

/// Infers a base URL from a bind address.
///
/// This handles common bind address patterns:
/// - `0.0.0.0:PORT` → `http://localhost:PORT`
/// - `127.0.0.1:PORT` → `http://localhost:PORT`
/// - `localhost:PORT` → `http://localhost:PORT`
/// - `HOST:PORT` → `http://HOST:PORT`
/// - `PORT` → `http://localhost:PORT`
fn infer_base_url(bind_address: &str) -> String {
    // Extract port from address
    let port = bind_address
        .split(':')
        .next_back()
        .and_then(|p| p.parse::<u16>().ok());

    match (bind_address, port) {
        // 0.0.0.0:PORT → localhost:PORT
        (addr, Some(port)) if addr.starts_with("0.0.0.0:") => {
            format!("http://localhost:{port}")
        }
        // 127.0.0.1:PORT → localhost:PORT
        (addr, Some(port)) if addr.starts_with("127.0.0.1:") => {
            format!("http://localhost:{port}")
        }
        // localhost:PORT → http://localhost:PORT
        (addr, Some(port)) if addr.starts_with("localhost:") => {
            format!("http://localhost:{port}")
        }
        // Just a port number → localhost:PORT
        (addr, Some(port)) if addr == port.to_string() => {
            format!("http://localhost:{port}")
        }
        // HOST:PORT → http://HOST:PORT
        (_, Some(port)) => {
            let host = bind_address
                .rsplit_once(':')
                .map_or("localhost", |(h, _)| h);
            format!("http://{host}:{port}")
        }
        // No port found → just localhost
        _ => "http://localhost".to_string(),
    }
}

pub(crate) fn build_agent_card(runtime: &DefaultRuntime, agent: &AgentDefinition) -> AgentCard {
    let base_url = runtime.base_url.clone().unwrap_or_else(|| {
        runtime.bind_address.as_ref().map_or_else(
            || "http://localhost".to_string(),
            |addr| infer_base_url(addr),
        )
    });

    let normalized_base = base_url.trim_end_matches('/');
    let version = agent.version();
    let agent_id = agent.id();

    let mut card = AgentCard::new(
        agent.name(),
        agent.description().unwrap_or_default(),
        version,
        format!("{normalized_base}/{agent_id}/{version}/rpc"),
    );

    card.capabilities.streaming = Some(true);
    card = card.add_interface(
        TransportProtocol::HttpJson,
        format!("{normalized_base}/{agent_id}/{version}"),
    );

    card.skills = agent
        .skills()
        .iter()
        .map(|skill| AgentSkill {
            id: skill.id().to_string(),
            name: skill.name().to_string(),
            description: skill.description().to_string(),
            tags: skill
                .metadata()
                .tags
                .iter()
                .map(|tag| (*tag).to_string())
                .collect(),
            examples: skill
                .metadata()
                .examples
                .iter()
                .map(|example| (*example).to_string())
                .collect(),
            input_modes: skill
                .metadata()
                .input_modes
                .iter()
                .map(|mode| (*mode).to_string())
                .collect(),
            output_modes: skill
                .metadata()
                .output_modes
                .iter()
                .map(|mode| (*mode).to_string())
                .collect(),
            security: Vec::new(),
        })
        .collect();

    card
}

pub mod dev_ui;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_localhost_variants() {
        assert_eq!(infer_base_url("0.0.0.0:8080"), "http://localhost:8080");
        assert_eq!(infer_base_url("127.0.0.1:3000"), "http://localhost:3000");
        assert_eq!(infer_base_url("localhost:9000"), "http://localhost:9000");
        assert_eq!(infer_base_url("8080"), "http://localhost:8080");
    }

    #[test]
    fn keeps_host_when_present() {
        assert_eq!(
            infer_base_url("example.com:7000"),
            "http://example.com:7000"
        );
        assert_eq!(
            infer_base_url("api.internal:443"),
            "http://api.internal:443"
        );
    }

    #[cfg(feature = "test-support")]
    mod handler_tests {
        use super::*;
        use crate::agent::{
            Agent, OnInputResult, OnRequestResult, RegisteredSkill, SkillHandler, SkillMetadata,
        };
        use crate::errors::{AgentError, AgentResult};
        use crate::models::{Content, ContentPart, LlmResponse};
        use crate::runtime::context::{Context, TaskContext};
        use crate::runtime::{DefaultRuntime, Runtime};
        use crate::test_support::FakeLlm;
        use crate::tools::ToolCall;
        use a2a_types::{
            A2ARequest, A2ARequestPayload, JSONRPCId, Message, MessageRole, MessageSendParams,
            Part, SendMessageResponse, SendStreamingMessageResponse, SendStreamingMessageResult,
            TaskState,
        };
        use axum::extract::State;
        use axum::http::StatusCode;
        use axum::response::Response;
        use axum::Json;
        use serde_json::json;
        use std::sync::Arc;

        async fn response_bytes(response: Response) -> Vec<u8> {
            use http_body_util::BodyExt;

            let (_, body) = response.into_parts();
            let collected = body.collect().await.expect("collect body");
            collected.to_bytes().to_vec()
        }

        fn negotiation_response(skill_id: &str) -> AgentResult<LlmResponse> {
            let tool_call = ToolCall::new(
                "call-1",
                "radkit_structured_output",
                json!({
                    "type": "start_task",
                    "skill_id": skill_id,
                    "reasoning": "selected in test"
                }),
            );
            FakeLlm::content_response(Content::from_parts(vec![ContentPart::ToolCall(tool_call)]))
        }

        fn create_message(
            text: &str,
            context_id: Option<String>,
            task_id: Option<String>,
        ) -> Message {
            Message {
                kind: "message".to_string(),
                message_id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: text.to_string(),
                    metadata: None,
                }],
                context_id,
                task_id,
                reference_task_ids: Vec::new(),
                extensions: Vec::new(),
                metadata: None,
            }
        }

        struct ImmediateSkill;

        static IMMEDIATE_METADATA: SkillMetadata = SkillMetadata::new(
            "immediate-skill",
            "Immediate Skill",
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
        impl SkillHandler for ImmediateSkill {
            async fn on_request(
                &self,
                _task_context: &mut TaskContext,
                _context: &Context,
                _runtime: &dyn Runtime,
                _content: Content,
            ) -> Result<OnRequestResult, AgentError> {
                Ok(OnRequestResult::Completed {
                    message: Some(Content::from_text("Done")),
                    artifacts: Vec::new(),
                })
            }

            async fn on_input_received(
                &self,
                _task_context: &mut TaskContext,
                _context: &Context,
                _runtime: &dyn Runtime,
                _input: Content,
            ) -> Result<OnInputResult, AgentError> {
                unreachable!("immediate skill never continues");
            }
        }

        impl RegisteredSkill for ImmediateSkill {
            fn metadata() -> &'static SkillMetadata {
                &IMMEDIATE_METADATA
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn agent_card_handler_returns_agent_card() {
            let llm =
                FakeLlm::with_responses("fake-llm", [negotiation_response(IMMEDIATE_METADATA.id)]);
            let agent = Agent::builder()
                .with_id("agent-1")
                .with_version("1.0.0")
                .with_name("Test Agent")
                .with_skill(ImmediateSkill)
                .build();
            let runtime = Arc::new(
                DefaultRuntime::new(llm)
                    .base_url("http://localhost:3000")
                    .agents(vec![agent]),
            );

            let response = agent_card_handler(
                State(Arc::clone(&runtime)),
                axum::extract::Path("agent-1".to_string()),
            )
            .await;

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response_bytes(response).await;
            let card: a2a_types::AgentCard = serde_json::from_slice(&bytes).expect("agent card");
            assert_eq!(card.capabilities.streaming, Some(true));
            assert_eq!(card.skills.len(), 1);
            assert!(card
                .additional_interfaces
                .iter()
                .any(|iface| iface.url.contains("/agent-1/1.0.0")));
        }

        #[tokio::test(flavor = "current_thread")]
        async fn json_rpc_handler_send_message_returns_task() {
            let llm =
                FakeLlm::with_responses("fake-llm", [negotiation_response(IMMEDIATE_METADATA.id)]);
            let agent = Agent::builder()
                .with_id("agent-1")
                .with_version("1.0.0")
                .with_name("Test Agent")
                .with_skill(ImmediateSkill)
                .build();
            let runtime = Arc::new(
                DefaultRuntime::new(llm)
                    .base_url("http://localhost:3000")
                    .agents(vec![agent]),
            );

            let params = MessageSendParams {
                message: create_message("hello", None, None),
                configuration: None,
                metadata: None,
            };

            let payload = A2ARequest {
                jsonrpc: "2.0".to_string(),
                id: Some(JSONRPCId::String("req-1".into())),
                payload: A2ARequestPayload::SendMessage { params },
            };

            let response = json_rpc_handler(
                State(Arc::clone(&runtime)),
                axum::extract::Path(("agent-1".to_string(), "1.0.0".to_string())),
                Json(payload),
            )
            .await;

            assert_eq!(response.status(), StatusCode::OK);
            let body = response_bytes(response).await;
            let parsed: SendMessageResponse =
                serde_json::from_slice(&body).expect("JSON-RPC send response");

            match parsed {
                SendMessageResponse::Success(success) => {
                    assert_eq!(success.id, Some(JSONRPCId::String("req-1".into())));
                    match &success.result {
                        a2a_types::SendMessageResult::Task(task) => {
                            assert_eq!(task.status.state, TaskState::Completed);
                            assert!(!task.history.is_empty());
                        }
                        other => panic!("expected task, got {:?}", other),
                    }
                }
                SendMessageResponse::Error(err) => {
                    panic!("unexpected error response: {:?}", err);
                }
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn message_stream_handler_emits_terminal_task_event() {
            let llm =
                FakeLlm::with_responses("fake-llm", [negotiation_response(IMMEDIATE_METADATA.id)]);
            let agent = Agent::builder()
                .with_id("agent-1")
                .with_version("1.0.0")
                .with_name("Test Agent")
                .with_skill(ImmediateSkill)
                .build();
            let runtime = Arc::new(
                DefaultRuntime::new(llm)
                    .base_url("http://localhost:3000")
                    .agents(vec![agent]),
            );

            let params = MessageSendParams {
                message: create_message("hello", None, None),
                configuration: None,
                metadata: None,
            };

            let response = message_stream_handler(
                State(Arc::clone(&runtime)),
                axum::extract::Path(("agent-1".to_string(), "1.0.0".to_string())),
                Json(params),
            )
            .await;

            assert_eq!(response.status(), StatusCode::OK);
            let body = response_bytes(response).await;
            let body_str = String::from_utf8(body).expect("utf8");
            let mut events: Vec<SendStreamingMessageResponse> = Vec::new();
            for chunk in body_str
                .split("\n\n")
                .filter(|chunk| !chunk.trim().is_empty())
            {
                let data = chunk
                    .trim()
                    .strip_prefix("data:")
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(json) = data {
                    events.push(
                        serde_json::from_str::<SendStreamingMessageResponse>(json).expect("event"),
                    );
                }
            }
            assert!(
                !events.is_empty(),
                "expected at least one SSE event, got none"
            );
            let last = events.last().expect("final event");
            match last {
                SendStreamingMessageResponse::Success(success) => match &success.result {
                    SendStreamingMessageResult::Task(task) => {
                        assert_eq!(task.status.state, TaskState::Completed);
                    }
                    other => panic!("expected final task event, got {:?}", other),
                },
                other => panic!("unexpected error event: {:?}", other),
            }
        }

        #[tokio::test(flavor = "current_thread")]
        async fn task_resubscribe_handler_requires_suffix() {
            let llm =
                FakeLlm::with_responses("fake-llm", [negotiation_response(IMMEDIATE_METADATA.id)]);
            let agent = Agent::builder()
                .with_id("agent-1")
                .with_version("1.0.0")
                .with_name("Test Agent")
                .with_skill(ImmediateSkill)
                .build();
            let runtime = Arc::new(
                DefaultRuntime::new(llm)
                    .base_url("http://localhost:3000")
                    .agents(vec![agent]),
            );

            let response = task_resubscribe_handler(
                State(Arc::clone(&runtime)),
                axum::extract::Path((
                    "agent-1".to_string(),
                    "1.0.0".to_string(),
                    "task-123".to_string(),
                )),
            )
            .await;

            let status = response.status();
            assert_eq!(status, StatusCode::BAD_REQUEST);
            let body = response_bytes(response).await;
            let parsed: serde_json::Value =
                serde_json::from_slice(&body).expect("json error response");
            let message = parsed["error"].as_str().expect("error message string");
            assert!(message.contains("suffix"), "message: {message}");
        }

        #[tokio::test(flavor = "current_thread")]
        async fn build_streaming_sse_emits_final_event() {
            use crate::runtime::core::executor::TaskStream;
            use crate::runtime::task_manager::TaskEvent;
            use axum::response::IntoResponse;

            let task = a2a_types::Task {
                kind: a2a_types::TASK_KIND.to_string(),
                id: "task-1".into(),
                context_id: "ctx-1".into(),
                status: a2a_types::TaskStatus {
                    state: TaskState::Completed,
                    timestamp: Some(chrono::Utc::now().to_rfc3339()),
                    message: None,
                },
                history: Vec::new(),
                artifacts: Vec::new(),
                metadata: None,
            };

            let status_event = a2a_types::TaskStatusUpdateEvent {
                kind: a2a_types::STATUS_UPDATE_KIND.to_string(),
                task_id: task.id.clone(),
                context_id: task.context_id.clone(),
                status: task.status.clone(),
                is_final: true,
                metadata: None,
            };

            let stream = TaskStream {
                task: task.clone(),
                initial_events: vec![TaskEvent::StatusUpdate(status_event)],
                receiver: None,
            };

            let response = build_streaming_sse(None, stream).into_response();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response_bytes(response).await;
            let body_str = String::from_utf8(body).expect("utf8");
            let events: Vec<SendStreamingMessageResponse> = body_str
                .split("\n\n")
                .filter_map(|chunk| {
                    chunk
                        .trim()
                        .strip_prefix("data:")
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                })
                .map(|json| {
                    serde_json::from_str::<SendStreamingMessageResponse>(json).expect("event json")
                })
                .collect();

            assert!(events.iter().any(|event| matches!(
                event,
                SendStreamingMessageResponse::Success(success)
                    if matches!(success.result, SendStreamingMessageResult::Task(_))
            )));
        }
    }
}

/// Axum handler for serving an agent's `AgentCard`.
///
/// This function will handle `GET /:agent_id/.well-known/agent-card.json` requests.
pub async fn agent_card_handler(
    State(runtime): State<Arc<DefaultRuntime>>,
    Path(agent_id): Path<String>,
) -> Response {
    let agent_def = runtime.agents.iter().find(|a| a.id() == agent_id);

    if let Some(agent) = agent_def {
        let card = build_agent_card(&runtime, agent);
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
            format!("Agent version {version} not found for {agent_id}"),
        )
            .into_response();
    }

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent_def);

    let request_id = payload.id.clone();

    match payload.payload {
        A2ARequestPayload::SendStreamingMessage { params } => {
            match executor.handle_message_stream(params).await {
                Ok(PreparedSendMessage::Task(stream)) => {
                    build_streaming_sse(request_id.clone(), stream).into_response()
                }
                Ok(PreparedSendMessage::Message(message)) => {
                    build_message_sse(request_id.clone(), message).into_response()
                }
                Err(error) => build_streaming_error_response(request_id.clone(), error),
            }
        }
        A2ARequestPayload::TaskResubscription { params } => {
            match executor.handle_task_resubscribe(params).await {
                Ok(stream) => build_streaming_sse(request_id.clone(), stream).into_response(),
                Err(error) => build_streaming_error_response(request_id.clone(), error),
            }
        }
        A2ARequestPayload::SendMessage { params } => {
            match executor.handle_send_message(params).await {
                Ok(result) => build_send_message_success_response(request_id.clone(), result),
                Err(error) => build_send_message_error_response(request_id.clone(), error),
            }
        }
        A2ARequestPayload::GetTask { params } => match executor.handle_get_task(params).await {
            Ok(task) => build_get_task_success_response(request_id.clone(), task),
            Err(error) => build_get_task_error_response(request_id.clone(), error),
        },
        A2ARequestPayload::CancelTask { params } => {
            match executor.handle_cancel_task(params).await {
                Ok(task) => build_cancel_task_success_response(request_id.clone(), task),
                Err(error) => build_cancel_task_error_response(request_id.clone(), error),
            }
        }
        _ => AgentError::NotImplemented {
            feature: "This RPC method".to_string(),
        }
        .into_response(),
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
            format!("Agent version {version} not found for {agent_id}"),
        )
            .into_response();
    }

    let executor = RequestExecutor::new(Arc::clone(&runtime), agent_def);

    match executor.handle_message_stream(params).await {
        Ok(PreparedSendMessage::Task(stream)) => build_streaming_sse(None, stream).into_response(),
        Ok(PreparedSendMessage::Message(message)) => {
            build_message_sse(None, message).into_response()
        }
        Err(error) => build_streaming_error_response(None, error),
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
            format!("Agent version {version} not found for {agent_id}"),
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
        Err(error) => build_streaming_error_response(None, error),
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

        for event in initial_events.by_ref() {
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

fn build_message_sse(
    request_id: Option<JSONRPCId>,
    message: a2a_types::Message,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        let response = SendStreamingMessageResponse::Success(Box::new(
            SendStreamingMessageSuccessResponse {
                jsonrpc: "2.0".to_string(),
                result: SendStreamingMessageResult::Message(message),
                id: request_id.clone(),
            },
        ));

        if let Ok(data) = serde_json::to_string(&response) {
            yield Ok(Event::default().data(data));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn build_send_message_success_response(
    request_id: Option<JSONRPCId>,
    result: SendMessageResult,
) -> Response {
    let response = SendMessageResponse::Success(Box::new(SendMessageSuccessResponse {
        jsonrpc: "2.0".to_string(),
        result,
        id: request_id,
    }));

    Json(response).into_response()
}

fn build_send_message_error_response(request_id: Option<JSONRPCId>, error: AgentError) -> Response {
    let response = SendMessageResponse::Error(JSONRPCErrorResponse {
        jsonrpc: "2.0".to_string(),
        error: error_mapper::to_jsonrpc_error(error),
        id: request_id,
    });

    Json(response).into_response()
}

fn build_get_task_success_response(
    request_id: Option<JSONRPCId>,
    task: a2a_types::Task,
) -> Response {
    let response = GetTaskResponse::Success(Box::new(GetTaskSuccessResponse {
        jsonrpc: "2.0".to_string(),
        result: task,
        id: request_id,
    }));

    Json(response).into_response()
}

fn build_get_task_error_response(request_id: Option<JSONRPCId>, error: AgentError) -> Response {
    let response = GetTaskResponse::Error(JSONRPCErrorResponse {
        jsonrpc: "2.0".to_string(),
        error: error_mapper::to_jsonrpc_error(error),
        id: request_id,
    });

    Json(response).into_response()
}

fn build_cancel_task_success_response(
    request_id: Option<JSONRPCId>,
    task: a2a_types::Task,
) -> Response {
    let response = CancelTaskResponse::Success(Box::new(CancelTaskSuccessResponse {
        jsonrpc: "2.0".to_string(),
        result: task,
        id: request_id,
    }));

    Json(response).into_response()
}

fn build_cancel_task_error_response(request_id: Option<JSONRPCId>, error: AgentError) -> Response {
    let response = CancelTaskResponse::Error(JSONRPCErrorResponse {
        jsonrpc: "2.0".to_string(),
        error: error_mapper::to_jsonrpc_error(error),
        id: request_id,
    });

    Json(response).into_response()
}

fn build_streaming_error_response(request_id: Option<JSONRPCId>, error: AgentError) -> Response {
    let response = SendStreamingMessageResponse::Error(JSONRPCErrorResponse {
        jsonrpc: "2.0".to_string(),
        error: error_mapper::to_jsonrpc_error(error),
        id: request_id,
    });

    Json(response).into_response()
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
