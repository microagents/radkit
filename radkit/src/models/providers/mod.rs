//! LLM provider implementations.
//!
//! This module contains concrete implementations of the [`BaseLlm`](crate::models::BaseLlm)
//! trait for various LLM providers including Anthropic, `OpenAI`, Gemini, and others.
//!
//! # Available Providers
//!
//! - [`AnthropicLlm`]: Claude models via Anthropic API
//! - [`OpenAILlm`]: GPT models via `OpenAI` API
//! - [`GeminiLlm`]: Gemini models via Google AI API
//! - [`GrokLlm`]: Grok models via XAI API (OpenAI-compatible)
//! - [`DeepSeekLlm`]: `DeepSeek` models via `DeepSeek` API (OpenAI-compatible)
//!
//! # Examples
//!
//! ```ignore
//! use radkit::models::providers::{AnthropicLlm, OpenAILlm, GeminiLlm};
//! use radkit::models::{BaseLlm, Thread};
//!
//! // Anthropic
//! let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
//!
//! // OpenAI
//! let llm = OpenAILlm::from_env("gpt-4o")?;
//!
//! // Gemini
//! let llm = GeminiLlm::from_env("gemini-2.0-flash-exp")?;
//!
//! let thread = Thread::from_user("Hello!");
//! let response = llm.generate_content(thread, None).await?;
//! ```

mod anthropic_llm;
mod deepseek_llm;
mod gemini_llm;
mod grok_llm;
mod openai_llm;

pub use anthropic_llm::AnthropicLlm;
pub use deepseek_llm::DeepSeekLlm;
pub use gemini_llm::GeminiLlm;
pub use grok_llm::GrokLlm;
pub use openai_llm::OpenAILlm;
