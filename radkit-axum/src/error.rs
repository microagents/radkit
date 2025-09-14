use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Agent error: {0}")]
    Agent(#[from] radkit::errors::AgentError),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Authentication error: {0}")]
    Auth(#[from] crate::auth::AuthError),

    #[error("Invalid JSON-RPC request: {0}")]
    InvalidRequest(String),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Invalid params: {0}")]
    InvalidParams(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Streaming not supported")]
    StreamingNotSupported,

    #[error("Push notifications not supported")]
    PushNotificationsNotSupported,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            Error::Agent(e) => (StatusCode::INTERNAL_SERVER_ERROR, -32603, e.to_string()),
            Error::Json(_) => (StatusCode::BAD_REQUEST, -32700, "Parse error".to_string()),
            Error::Auth(_) => (StatusCode::UNAUTHORIZED, -32603, self.to_string()),
            Error::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, -32600, msg.clone()),
            Error::MethodNotFound(method) => (
                StatusCode::NOT_FOUND,
                -32601,
                format!("Method not found: {}", method),
            ),
            Error::InvalidParams(msg) => (StatusCode::BAD_REQUEST, -32602, msg.clone()),
            Error::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, -32603, msg.clone()),
            Error::TaskNotFound(id) => (
                StatusCode::NOT_FOUND,
                -32001,
                format!("Task not found: {}", id),
            ),
            Error::StreamingNotSupported => (
                StatusCode::NOT_IMPLEMENTED,
                -32004,
                "Streaming not supported".to_string(),
            ),
            Error::PushNotificationsNotSupported => (
                StatusCode::NOT_IMPLEMENTED,
                -32003,
                "Push notifications not supported".to_string(),
            ),
        };

        let body = json!({
            "jsonrpc": "2.0",
            "error": {
                "code": error_code,
                "message": message,
            },
            "id": null
        });

        (status, Json(body)).into_response()
    }
}
