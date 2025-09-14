use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use radkit::agents::Agent;
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::{
    auth::AuthContext,
    error::{Error, Result},
    json_rpc::{JsonRpcRequest, JsonRpcResponse},
};

/// State shared across all routes
#[derive(Clone)]
pub struct ServerState {
    pub agent: Arc<Agent>,
}

/// Create all A2A protocol routes
pub fn create_routes(state: ServerState) -> Router {
    Router::new()
        // Core A2A protocol endpoints (JSON-RPC)
        .route("/message/send", post(message_send))
        .route("/message/stream", post(message_stream))
        .route("/tasks/get", post(tasks_get))
        .route("/tasks/cancel", post(tasks_cancel))
        .route("/tasks/resubscribe", post(tasks_resubscribe))
        // Agent Card endpoints
        .route("/.well-known/agent-card.json", get(agent_card))
        .route(
            "/agent/getAuthenticatedExtendedCard",
            post(authenticated_agent_card),
        )
        .with_state(state)
}

/// Handler for message/send
async fn message_send(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>> {
    // Validate JSON-RPC request
    crate::json_rpc::validate_request(&request)?;

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

/// Handler for message/stream (SSE)
async fn message_stream(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Sse<impl Stream<Item = std::result::Result<axum::response::sse::Event, Infallible>>>> {
    // Validate JSON-RPC request
    crate::json_rpc::validate_request(&request)?;

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

/// Handler for tasks/get
async fn tasks_get(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>> {
    // Validate JSON-RPC request
    crate::json_rpc::validate_request(&request)?;

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

/// Handler for tasks/cancel
async fn tasks_cancel(
    State(_state): State<ServerState>,
    Extension(_auth): Extension<AuthContext>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>> {
    // Validate JSON-RPC request
    crate::json_rpc::validate_request(&request)?;

    // Task cancellation not yet implemented in radkit
    Err(Error::MethodNotFound(
        "tasks/cancel not yet implemented".to_string(),
    ))
}

/// Handler for tasks/resubscribe (SSE)
async fn tasks_resubscribe(
    State(_state): State<ServerState>,
    Extension(_auth): Extension<AuthContext>,
    Json(_request): Json<JsonRpcRequest>,
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

/// Handler for agent card (public)
async fn agent_card(State(state): State<ServerState>) -> Json<a2a_types::AgentCard> {
    Json(state.agent.agent_card.clone())
}

/// Handler for authenticated agent card
async fn authenticated_agent_card(
    State(state): State<ServerState>,
    Extension(_auth): Extension<AuthContext>,
) -> Json<a2a_types::AgentCard> {
    // For now, return the same card
    // In production, might return additional skills or configuration
    Json(state.agent.agent_card.clone())
}
