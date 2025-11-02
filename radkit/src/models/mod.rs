//! Data models for the radkit agent SDK.
//!
//! This module provides the core types for representing LLM interactions,
//! including threads, events, content, and the base LLM trait.

pub mod base_llm;
pub mod content;
pub mod content_part;
pub mod event;
pub mod llm_response;
pub mod providers;
pub mod thread;
pub mod utils;

// Re-export primary types for convenient access
pub use self::base_llm::{BaseLlm, BaseLlmExt};
pub use self::content::Content;
pub use self::content_part::{ContentPart, Data, DataSource};
pub use self::event::{Event, Role};
pub use self::llm_response::{LlmResponse, TokenUsage};
pub use self::thread::Thread;
