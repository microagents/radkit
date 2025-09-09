//! LLM Response Types
//!
//! Defines the internal response format for LLM providers, inspired by Python ADK
//! but optimized for A2A-native architecture with ExtendedMessage support.

use crate::models::content::Content;
use a2a_types::{Message, MessageRole};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Internal LLM response format - conversion layer between LLM providers and A2A
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The generated message content with function call/result extensions
    pub message: Content,

    /// Streaming indicators
    pub streaming_info: StreamingInfo,

    /// Usage and cost metadata
    pub usage_metadata: Option<UsageMetadata>,

    /// Error information if generation failed
    pub error_info: Option<ErrorInfo>,

    /// Provider-specific metadata
    pub provider_metadata: HashMap<String, Value>,
}

/// Streaming response information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingInfo {
    /// Whether this is a partial response (for streaming)
    pub partial: bool,

    /// Whether this turn is complete
    pub turn_complete: bool,

    /// Sequence number for ordering partial responses
    pub sequence_number: Option<u32>,
}

impl Default for StreamingInfo {
    fn default() -> Self {
        Self {
            partial: false,
            turn_complete: true,
            sequence_number: None,
        }
    }
}

/// Usage and cost tracking metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    /// Input tokens consumed
    pub input_tokens: Option<u32>,

    /// Output tokens generated
    pub output_tokens: Option<u32>,

    /// Total tokens used
    pub total_tokens: Option<u32>,

    /// Cost in USD cents
    pub cost_cents: Option<u32>,

    /// Model name used
    pub model_name: Option<String>,

    /// Request timestamp
    pub timestamp: DateTime<Utc>,

    /// Additional usage metrics
    pub additional_metrics: HashMap<String, Value>,
}

/// Error information for failed generations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Error code from the provider
    pub error_code: String,

    /// Human-readable error message
    pub error_message: String,

    /// Error type/category
    pub error_type: ErrorType,

    /// Whether the error is retryable
    pub retryable: bool,

    /// Additional error context
    pub context: HashMap<String, Value>,
}

/// Categories of LLM errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    /// Authentication/authorization errors
    Authentication,

    /// Rate limiting errors
    RateLimit,

    /// Request validation errors
    InvalidRequest,

    /// Model/service unavailability
    ServiceUnavailable,

    /// Content filtering/safety errors
    ContentFiltered,

    /// Token/context length limits
    ContextLengthExceeded,

    /// Network/transport errors
    NetworkError,

    /// Internal provider errors
    InternalError,

    /// Unknown errors
    Unknown,
}

impl LlmResponse {
    /// Create a successful response from Content
    pub fn success(content: Content) -> Self {
        Self {
            message: content,
            streaming_info: StreamingInfo::default(),
            usage_metadata: None,
            error_info: None,
            provider_metadata: HashMap::new(),
        }
    }

    /// Create a successful response from a regular Message
    pub fn success_from_message(message: Message, task_id: String, context_id: String) -> Self {
        Self::success(Content::from_message(message, task_id, context_id))
    }

    /// Create an error response
    pub fn error(
        error_code: String,
        error_message: String,
        error_type: ErrorType,
        task_id: String,
        context_id: String,
    ) -> Self {
        let mut error_content = Content::new(
            task_id,
            context_id,
            uuid::Uuid::new_v4().to_string(),
            MessageRole::Agent,
        );
        error_content.add_text(format!("Error: {error_message}"), None);

        // Add error metadata
        let mut metadata = HashMap::new();
        metadata.insert("error".to_string(), serde_json::json!(true));
        metadata.insert(
            "error_code".to_string(),
            serde_json::json!(error_code.clone()),
        );
        error_content.metadata = Some(metadata);

        Self {
            message: error_content,
            streaming_info: StreamingInfo::default(),
            usage_metadata: None,
            error_info: Some(ErrorInfo {
                error_code,
                error_message,
                error_type,
                retryable: false,
                context: HashMap::new(),
            }),
            provider_metadata: HashMap::new(),
        }
    }

    /// Create a partial streaming response
    pub fn partial(message: Content, sequence_number: u32) -> Self {
        Self {
            message,
            streaming_info: StreamingInfo {
                partial: true,
                turn_complete: false,
                sequence_number: Some(sequence_number),
            },
            usage_metadata: None,
            error_info: None,
            provider_metadata: HashMap::new(),
        }
    }

    /// Mark this response as the final streaming response
    pub fn finalize_stream(mut self) -> Self {
        self.streaming_info.partial = false;
        self.streaming_info.turn_complete = true;
        self
    }

    /// Add usage metadata
    pub fn with_usage_metadata(mut self, usage: UsageMetadata) -> Self {
        self.usage_metadata = Some(usage);
        self
    }

    /// Add provider metadata
    pub fn with_provider_metadata(mut self, key: String, value: Value) -> Self {
        self.provider_metadata.insert(key, value);
        self
    }

    /// Check if this response contains function calls
    pub fn has_function_calls(&self) -> bool {
        self.message.has_function_calls()
    }

    /// Check if this response is successful (no errors)
    pub fn is_success(&self) -> bool {
        self.error_info.is_none()
    }

    /// Check if this response is a streaming response
    pub fn is_streaming(&self) -> bool {
        self.streaming_info.partial || !self.streaming_info.turn_complete
    }

    /// Get the text content of the response
    pub fn text_content(&self) -> String {
        self.message
            .parts
            .iter()
            .filter_map(|part| match part {
                crate::models::content::ContentPart::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Extract function calls from the response
    pub fn extract_function_calls(&self) -> Vec<(String, Value)> {
        self.message
            .get_function_calls()
            .iter()
            .filter_map(|part| {
                if let crate::models::content::ContentPart::FunctionCall {
                    name, arguments, ..
                } = part
                {
                    Some((name.clone(), arguments.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the underlying A2A message (without function extensions)
    pub fn to_a2a_message(&self) -> Message {
        self.message.to_a2a_message()
    }
}
