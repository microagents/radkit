use axum::{
    extract::{Extension, State},
    http::HeaderMap,
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use radkit::agents::Agent;
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::{
    auth::AuthContext,
    error::{Error, Result},
};

use a2a_types::{JSONRPCErrorResponse, JSONRPCId, JSONRPCRequest, JSONRPCSuccessResponse};

/// JSON-RPC 2.0 Response (can be either success or error)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum JsonRpcResponse {
    Success(JSONRPCSuccessResponse),
    Error(JSONRPCErrorResponse),
}

impl JsonRpcResponse {
    /// Create a success response
    fn success(id: Option<JSONRPCId>, result: serde_json::Value) -> Self {
        JsonRpcResponse::Success(JSONRPCSuccessResponse {
            jsonrpc: "2.0".to_string(),
            result,
            id,
        })
    }
}

/// Validate JSON-RPC request
fn validate_request(req: &JSONRPCRequest) -> Result<()> {
    if req.jsonrpc != "2.0" {
        return Err(Error::InvalidRequest(
            "Invalid JSON-RPC version".to_string(),
        ));
    }
    Ok(())
}

/// State shared across all routes
#[derive(Clone)]
pub struct ServerState {
    pub agent: Arc<Agent>,
}

/// Create all A2A protocol routes
pub fn create_routes(state: ServerState) -> Router {
    Router::new()
        // Standard A2A: Single endpoint with JSON-RPC method-based routing
        .route("/", post(json_rpc_handler))
        // Agent Card endpoints
        .route("/.well-known/agent-card.json", get(agent_card))
        .with_state(state)
}

/// Main JSON-RPC handler that routes based on method field
async fn json_rpc_handler(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
    Json(request): Json<JSONRPCRequest>,
) -> Response {
    // Validate JSON-RPC request
    if let Err(e) = validate_request(&request) {
        return e.into_response();
    }

    // Check if client wants streaming based on Accept header
    let wants_streaming = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/event-stream"))
        .unwrap_or(false);

    // Route based on method
    match request.method.as_str() {
        "message/send" => {
            if wants_streaming {
                // Handle as streaming
                match handle_message_stream(state, auth, request).await {
                    Ok(sse) => sse.into_response(),
                    Err(e) => e.into_response(),
                }
            } else {
                // Handle as non-streaming
                match handle_message_send(state, auth, request).await {
                    Ok(json) => json.into_response(),
                    Err(e) => e.into_response(),
                }
            }
        }
        "message/stream" => {
            // Always streaming
            match handle_message_stream(state, auth, request).await {
                Ok(sse) => sse.into_response(),
                Err(e) => e.into_response(),
            }
        }
        "tasks/get" => match handle_tasks_get(state, auth, request).await {
            Ok(json) => json.into_response(),
            Err(e) => e.into_response(),
        },
        "tasks/cancel" => match handle_tasks_cancel(request).await {
            Ok(json) => json.into_response(),
            Err(e) => e.into_response(),
        },
        "tasks/resubscribe" => match handle_tasks_resubscribe(request).await {
            Ok(sse) => sse.into_response(),
            Err(e) => e.into_response(),
        },
        "tasks/list" => match handle_tasks_list(state, auth, request).await {
            Ok(json) => json.into_response(),
            Err(e) => e.into_response(),
        },
        "agent/getAuthenticatedExtendedCard" => {
            match handle_authenticated_agent_card(state, request).await {
                Ok(json) => json.into_response(),
                Err(e) => e.into_response(),
            }
        }
        _ => {
            Error::MethodNotFound(format!("Method '{}' not found", request.method)).into_response()
        }
    }
}

/// Handle message/send
async fn handle_message_send(
    state: ServerState,
    auth: AuthContext,
    request: JSONRPCRequest,
) -> Result<Json<JsonRpcResponse>> {
    // Parse params
    let params: a2a_types::MessageSendParams = if let Some(p) = request.params {
        serde_json::from_value(p).map_err(|e| Error::InvalidParams(e.to_string()))?
    } else {
        return Err(Error::InvalidParams("Missing params".to_string()));
    };

    // Call agent
    let result = state
        .agent
        .send_message(auth.app_name, auth.user_id, params)
        .await
        .map_err(Error::Agent)?;

    // Convert to A2A SendMessageResult (just the result, not the events)
    let a2a_result = result.result;

    // Return JSON-RPC response
    Ok(Json(JsonRpcResponse::success(
        request.id,
        serde_json::to_value(a2a_result)?,
    )))
}

/// Handle message/stream (SSE)
async fn handle_message_stream(
    state: ServerState,
    auth: AuthContext,
    request: JSONRPCRequest,
) -> Result<Sse<impl Stream<Item = std::result::Result<axum::response::sse::Event, Infallible>>>> {
    // Parse params
    let params: a2a_types::MessageSendParams = if let Some(p) = request.params {
        serde_json::from_value(p).map_err(|e| Error::InvalidParams(e.to_string()))?
    } else {
        return Err(Error::InvalidParams("Missing params".to_string()));
    };

    // Get streaming response from agent
    let stream_result = state
        .agent
        .send_streaming_message(auth.app_name, auth.user_id, params)
        .await
        .map_err(Error::Agent)?;

    // Convert to SSE events
    let request_id = request.id.clone();
    let sse_stream = stream_result.a2a_stream.map(move |msg| {
        let response = JsonRpcResponse::success(
            request_id.clone(),
            serde_json::to_value(msg).unwrap_or(serde_json::Value::Null),
        );

        Ok::<_, Infallible>(
            axum::response::sse::Event::default()
                .data(serde_json::to_string(&response).unwrap_or_default()),
        )
    });

    Ok(Sse::new(sse_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    ))
}

/// Handle tasks/get
async fn handle_tasks_get(
    state: ServerState,
    auth: AuthContext,
    request: JSONRPCRequest,
) -> Result<Json<JsonRpcResponse>> {
    // Parse params
    let params: a2a_types::TaskQueryParams = if let Some(p) = request.params {
        serde_json::from_value(p).map_err(|e| Error::InvalidParams(e.to_string()))?
    } else {
        return Err(Error::InvalidParams("Missing params".to_string()));
    };

    // Call agent
    let task = state
        .agent
        .get_task(&auth.app_name, &auth.user_id, params)
        .await
        .map_err(Error::Agent)?;

    // Return JSON-RPC response
    Ok(Json(JsonRpcResponse::success(
        request.id,
        serde_json::to_value(task)?,
    )))
}

/// Handle tasks/list (non-standard but commonly implemented)
async fn handle_tasks_list(
    state: ServerState,
    auth: AuthContext,
    request: JSONRPCRequest,
) -> Result<Json<JsonRpcResponse>> {
    // Call agent (context_id filtering not yet implemented in radkit)
    let tasks = state
        .agent
        .list_tasks(&auth.app_name, &auth.user_id)
        .await
        .map_err(Error::Agent)?;

    // Return JSON-RPC response
    Ok(Json(JsonRpcResponse::success(
        request.id,
        serde_json::to_value(tasks)?,
    )))
}

/// Handle tasks/cancel
async fn handle_tasks_cancel(_request: JSONRPCRequest) -> Result<Json<JsonRpcResponse>> {
    // Task cancellation not yet implemented in radkit
    Err(Error::MethodNotFound(
        "tasks/cancel not yet implemented".to_string(),
    ))
}

/// Handle tasks/resubscribe (SSE)
async fn handle_tasks_resubscribe(
    _request: JSONRPCRequest,
) -> Result<Sse<impl Stream<Item = std::result::Result<axum::response::sse::Event, Infallible>>>> {
    // For now, return an error stream
    use futures::stream;

    let error_stream = stream::once(async move {
        Ok::<_, Infallible>(
            axum::response::sse::Event::default().data("Resubscribe not yet implemented"),
        )
    });

    Ok(Sse::new(error_stream))
}

/// Handle authenticated agent card
async fn handle_authenticated_agent_card(
    state: ServerState,
    request: JSONRPCRequest,
) -> Result<Json<JsonRpcResponse>> {
    // For now, return the same card
    // In production, might return additional skills or configuration
    Ok(Json(JsonRpcResponse::success(
        request.id,
        serde_json::to_value(&state.agent.agent_card)?,
    )))
}

/// Handler for agent card (public)
async fn agent_card(State(state): State<ServerState>) -> Json<a2a_types::AgentCard> {
    Json(state.agent.agent_card.clone())
}
