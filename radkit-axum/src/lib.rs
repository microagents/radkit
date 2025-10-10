pub mod auth;
pub mod error;
pub mod routes;
pub mod server;

pub use auth::{AuthContext, AuthExtractor};
pub use error::{Error, Result};
pub use server::{A2AServer, A2AServerBuilder};

// Re-export only what's needed for trait implementations
pub use axum::async_trait;
