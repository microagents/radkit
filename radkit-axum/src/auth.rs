use axum::{
    async_trait,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use std::fmt::Debug;

/// Authentication context extracted from HTTP requests
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The application/tenant name
    pub app_name: String,
    /// The user identifier
    pub user_id: String,
}

/// Trait for extracting authentication from HTTP requests
///
/// Implementers should extract app_name and user_id from the request,
/// typically from headers like Authorization, API keys, or cookies.
#[async_trait]
pub trait AuthExtractor: Send + Sync + 'static {
    /// Extract authentication context from request parts
    async fn extract(&self, parts: &mut Parts) -> Result<AuthContext, AuthError>;
}

/// Authentication error that can be converted to HTTP response
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Missing authentication credentials")]
    MissingCredentials,

    #[error("Invalid authentication token")]
    InvalidToken,

    #[error("Authentication failed: {0}")]
    Failed(String),

    #[error("Insufficient permissions")]
    Forbidden,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingCredentials => (StatusCode::UNAUTHORIZED, "Missing credentials"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::Failed(ref msg) => (StatusCode::UNAUTHORIZED, msg.as_str()),
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "Insufficient permissions"),
        };

        let body = serde_json::json!({
            "error": message,
            "code": status.as_u16(),
        });

        (status, axum::Json(body)).into_response()
    }
}
