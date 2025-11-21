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

/// Trait for types that can be deserialized from LLM outputs with fuzzy matching.
///
/// This is a trait alias for `tryparse::deserializer::LlmDeserialize`, providing:
/// - Fuzzy field name matching (`camelCase` ↔ `snake_case` ↔ `kebab-case`)
/// - Fuzzy enum variant matching (case-insensitive with edit-distance scoring)
/// - Type coercion (string → number, single → array, etc.)
///
/// Use the `#[derive(LLMOutput)]` macro from `radkit::macros` to implement this trait.
///
/// # Important Usage Notes
///
/// ## Required Derives
///
/// Types must derive **both** `serde::Deserialize` and `LLMOutput`:
///
/// ```ignore
/// use radkit::macros::LLMOutput;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, LLMOutput)]
/// struct MyOutput {
///     field: String,
/// }
/// ```
///
/// ## Supported Types
///
/// All standard integer and float types are supported:
///
/// ```ignore
/// #[derive(Deserialize, LLMOutput)]
/// struct Example {
///     count: i32,     // ✅ All integer types supported
///     size: usize,    // ✅ usize/isize supported
///     score: f64,     // ✅ f32/f64 supported
///     id: u64,        // ✅ Unsigned types supported
/// }
/// ```
///
/// ## Trait vs Macro Import
///
/// - Use `radkit::macros::LLMOutput` for the **derive macro**
/// - Use `radkit::models::LLMOutputTrait` for **trait bounds**
///
/// ```ignore
/// // For deriving on types:
/// use radkit::macros::LLMOutput;
///
/// #[derive(Deserialize, LLMOutput, JsonSchema)]
/// struct MyOutput { /* ... */ }
///
/// // For trait bounds in functions/impl blocks:
/// use radkit::models::LLMOutputTrait;
///
/// fn process<T: LLMOutputTrait>(input: T) { /* ... */ }
/// ```
///
/// # Example
///
/// ```ignore
/// use radkit::macros::LLMOutput;
/// use radkit::models::LLMOutputTrait;
/// use schemars::JsonSchema;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, LLMOutput, JsonSchema)]
/// struct UserData {
///     user_name: String,  // Matches userName, UserName, user-name, etc.
///     age: u32,           // All integer types supported
/// }
///
/// fn parse_user<T: LLMOutputTrait>(text: &str) -> Result<T, tryparse::TryParseError> {
///     tryparse::parse_llm(text)
/// }
/// ```
pub trait LLMOutputTrait: tryparse::deserializer::LlmDeserialize {}

// Blanket implementation: any type implementing LlmDeserialize also implements LLMOutputTrait
impl<T: tryparse::deserializer::LlmDeserialize> LLMOutputTrait for T {}
