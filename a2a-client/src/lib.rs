//! # A2A Protocol Client
//!
//! This crate provides a client for calling remote A2A (Agent-to-Agent) protocol compliant agents.
//! It supports both streaming and non-streaming interactions over HTTP/HTTPS.
//!
//! ## Features
//!
//! - Full A2A protocol support (JSON-RPC 2.0)
//! - Non-streaming and streaming message support
//! - Task retrieval and listing
//! - Agent discovery via agent cards
//! - Authentication support (Bearer tokens)
//! - Error handling with detailed error types
//!
//! ## Example
//!
//! ```rust,no_run
//! use a2a_client::A2AClient;
//! use a2a_types::{Message, MessageRole, MessageSendParams, Part};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create client from agent card URL
//! let client = A2AClient::from_card_url("https://agent.example.com")
//!     .await?
//!     .with_auth_token("your_api_key");
//!
//! // Create message
//! let message = Message {
//!     kind: "message".to_string(),
//!     message_id: "msg_123".to_string(),
//!     role: MessageRole::User,
//!     parts: vec![Part::Text {
//!         text: "Hello!".to_string(),
//!         metadata: None,
//!     }],
//!     context_id: None,
//!     task_id: None,
//!     reference_task_ids: Vec::new(),
//!     extensions: Vec::new(),
//!     metadata: None,
//! };
//!
//! // Send message
//! let params = MessageSendParams {
//!     message,
//!     configuration: None,
//!     metadata: None,
//! };
//!
//! let result = client.send_message(params).await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod constants;
pub mod error;

pub use client::A2AClient;
pub use error::{A2AError, A2AResult};
