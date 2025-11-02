//! Procedural macros for the radkit agent framework.
//!
//! This crate provides the `#[skill]` attribute macro for defining A2A-compliant skills.

#![deny(unsafe_code, unreachable_patterns, unused_must_use)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)] // Common pattern in proc macro crates

mod skill;
mod validation;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Attribute macro for defining A2A skills with metadata.
///
/// This macro generates the `SkillMetadata` and implements the `RegisteredSkill` trait
/// for the annotated struct, making it usable with the radkit agent builder.
///
/// # Required Parameters
///
/// - `id`: A unique identifier for the skill (String)
/// - `name`: A human-readable name for the skill (String)
/// - `description`: A detailed description of what the skill does (String)
///
/// # Optional Parameters
///
/// - `tags`: Array of keywords describing the skill's capabilities (default: [])
/// - `examples`: Array of example prompts or scenarios (default: [])
/// - `input_modes`: Array of supported input MIME types (default: [])
/// - `output_modes`: Array of supported output MIME types (default: [])
///
/// # MIME Type Validation
///
/// The macro validates `input_modes` and `output_modes` against a list of common MIME types.
/// If an invalid type is provided, a compile error will be generated with suggestions.
///
/// # Example
///
/// ```ignore
/// use radkit::prelude::*;
///
/// #[skill(
///     id = "summarize_text",
///     name = "Text Summarizer",
///     description = "Summarizes long text documents into concise summaries",
///     tags = ["text", "summarization", "nlp"],
///     examples = [
///         "Summarize this article",
///         "Give me a brief summary of this document"
///     ],
///     input_modes = ["text/plain", "text/markdown"],
///     output_modes = ["text/plain", "application/json"]
/// )]
/// pub struct SummarizeTextSkill;
///
/// #[async_trait]
/// impl SkillHandler for SummarizeTextSkill {
///     async fn on_request(
///         &self,
///         task_context: &mut TaskContext,
///         context: &Context,
///         runtime: &dyn Runtime,
///         content: Content,
///     ) -> Result<OnRequestResult, AgentError> {
///         // Implementation here
///         Ok(OnRequestResult::Completed {
///             message: Some(Content::text("Summary here")),
///             artifacts: vec![],
///         })
///     }
/// }
/// ```
///
/// # Generated Code
///
/// The macro generates:
/// 1. A static `SkillMetadata` constant named `{STRUCT_NAME}_METADATA`
/// 2. An implementation of `RegisteredSkill` trait for the struct
///
/// This allows the skill to be registered with an agent using `.with_skill()`:
///
/// ```ignore
/// let agent = AgentBuilder::new()
///     .with_skill(SummarizeTextSkill)
///     .build(runtime)?;
/// ```
#[proc_macro_attribute]
pub fn skill(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as skill::SkillArgs);
    let item = proc_macro2::TokenStream::from(item);

    skill::generate_skill_impl(args, item).into()
}
