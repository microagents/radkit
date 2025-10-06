//! Error types for A2A client operations

use thiserror::Error;

/// Main error type for A2A client operations
#[derive(Debug, Error)]
pub enum A2AError {
    /// Network communication error
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// JSON serialization/deserialization error
    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    /// Remote agent returned an error
    #[error("Remote agent error: {message}")]
    RemoteAgentError { message: String, code: Option<i32> },

    /// Invalid configuration or parameters
    #[error("Invalid parameter: {message}")]
    InvalidParameter { message: String },
}

/// Convenience type alias for Results with A2AError
pub type A2AResult<T> = std::result::Result<T, A2AError>;

// Conversion from reqwest::Error
impl From<reqwest::Error> for A2AError {
    fn from(error: reqwest::Error) -> Self {
        A2AError::NetworkError {
            message: error.to_string(),
        }
    }
}

// Conversion from serde_json::Error
impl From<serde_json::Error> for A2AError {
    fn from(error: serde_json::Error) -> Self {
        A2AError::SerializationError {
            message: error.to_string(),
        }
    }
}
