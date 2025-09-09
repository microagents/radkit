use crate::errors::AgentResult;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub mod anthropic_llm;
pub mod content;
pub mod gemini_llm;
pub mod llm_request;
pub mod llm_response;
pub mod mock_llm;
pub mod openai_llm;

pub use anthropic_llm::AnthropicLlm;
pub use gemini_llm::GeminiLlm;
pub use llm_request::{GenerateContentConfig, LlmRequest};
pub use llm_response::{ErrorInfo, ErrorType, LlmResponse, StreamingInfo, UsageMetadata};
pub use mock_llm::MockLlm;
pub use openai_llm::OpenAILlm;

/// Base trait for all LLM providers
///
/// This trait defines the interface between agents and LLM providers,
/// using LlmRequest/LlmResponse for clean architecture inspired by Python ADK
#[async_trait]
pub trait BaseLlm: Send + Sync {
    /// Get the model name
    fn model_name(&self) -> &str;

    /// Generate a response from an LLM request
    ///
    /// This is the main interface - agents convert A2A MessageSendParams to LlmRequest,
    /// providers convert LlmResponse back to A2A AgentResponse
    async fn generate_content(&self, request: LlmRequest) -> AgentResult<LlmResponse>;

    /// Generate a streaming response from an LLM request
    ///
    /// Returns a stream of partial LlmResponse objects for real-time interaction
    async fn generate_content_stream(
        &self,
        request: LlmRequest,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<LlmResponse>> + Send>>>;

    /// Check if this provider supports streaming
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Check if this provider supports function calling
    fn supports_function_calling(&self) -> bool {
        false
    }

    /// Get provider-specific capabilities
    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }
}

/// Provider capability information
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    /// Maximum context length in tokens
    pub max_context_length: Option<u32>,

    /// Supported file types for multimodal input
    pub supported_file_types: Vec<String>,

    /// Whether provider supports system instructions
    pub supports_system_instructions: bool,

    /// Whether provider supports JSON schema responses
    pub supports_json_schema: bool,

    /// Maximum tokens per request
    pub max_output_tokens: Option<u32>,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            max_context_length: Some(8192),
            supported_file_types: vec!["text/plain".to_string()],
            supports_system_instructions: true,
            supports_json_schema: false,
            max_output_tokens: Some(4096),
        }
    }
}
