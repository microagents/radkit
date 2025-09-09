//! LLM Request Types
//!
//! Defines the internal request format for LLM providers, inspired by Python ADK
//! but optimized for A2A-native architecture with ExtendedMessage support.

use crate::models::content::Content;
use crate::tools::BaseToolset;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Internal LLM request format - conversion layer between A2A and LLM providers
#[derive(Clone)]
pub struct LlmRequest {
    /// Content messages representing the full conversation context
    /// Includes function calls/results alongside regular message parts
    pub messages: Vec<Content>,

    /// Current task ID to highlight in context
    pub current_task_id: String,

    /// Context ID for the session/conversation
    pub context_id: String,

    /// System instruction for the LLM
    pub system_instruction: Option<String>,

    /// Generation configuration
    pub config: GenerateContentConfig,

    /// Available tools (not embedded in messages)
    pub toolset: Option<Arc<dyn BaseToolset>>,

    /// Task context and metadata
    pub metadata: HashMap<String, Value>,
}

/// Configuration for content generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateContentConfig {
    /// Temperature for randomness (0.0 to 1.0)
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// Top-p sampling parameter
    pub top_p: Option<f32>,

    /// Top-k sampling parameter  
    pub top_k: Option<u32>,

    /// Stop sequences
    pub stop_sequences: Option<Vec<String>>,

    /// Response format schema
    pub response_schema: Option<Value>,

    /// Additional provider-specific config
    pub extra_config: HashMap<String, Value>,
}

impl Default for GenerateContentConfig {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: None,
            top_k: None,
            stop_sequences: None,
            response_schema: None,
            extra_config: HashMap::new(),
        }
    }
}

impl LlmRequest {
    /// Create request from content messages
    pub fn with_messages(
        messages: Vec<Content>,
        current_task_id: String,
        context_id: String,
    ) -> Self {
        Self {
            messages,
            current_task_id,
            context_id,
            system_instruction: None,
            config: GenerateContentConfig::default(),
            toolset: None,
            metadata: HashMap::new(),
        }
    }

    /// Add system instruction
    pub fn with_system_instruction(mut self, instruction: String) -> Self {
        self.system_instruction = Some(instruction);
        self
    }

    /// Add generation config
    pub fn with_config(mut self, config: GenerateContentConfig) -> Self {
        self.config = config;
        self
    }

    /// Add toolset
    pub fn with_toolset(mut self, toolset: Arc<dyn BaseToolset>) -> Self {
        self.toolset = Some(toolset);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if the request has function calls in messages
    pub fn has_function_calls(&self) -> bool {
        self.messages
            .iter()
            .any(|content| content.has_function_calls())
    }

    /// Check if the request has function responses in messages
    pub fn has_function_responses(&self) -> bool {
        self.messages
            .iter()
            .any(|content| content.has_function_responses())
    }

    /// Extract all function calls from the messages
    pub fn extract_function_calls(&self) -> Vec<(String, Value)> {
        let mut calls = Vec::new();

        for content in &self.messages {
            for call_part in content.get_function_calls() {
                if let crate::models::content::ContentPart::FunctionCall {
                    name, arguments, ..
                } = call_part
                {
                    calls.push((name.clone(), arguments.clone()));
                }
            }
        }

        calls
    }

    /// Get the last message if any
    pub fn last_message(&self) -> Option<&Content> {
        self.messages.last()
    }
}

impl std::fmt::Debug for LlmRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmRequest")
            .field("messages", &format!("{} messages", self.messages.len()))
            .field("current_task_id", &self.current_task_id)
            .field("context_id", &self.context_id)
            .field("system_instruction", &self.system_instruction)
            .field("config", &self.config)
            .field("toolset", &"Some(Arc<dyn BaseToolset>)".to_string())
            .field("metadata", &self.metadata)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::content::Content;
    use a2a_types::MessageRole;
    use serde_json::json;

    #[test]
    fn test_llm_request_creation() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );
        content.add_text("Hello".to_string(), None);

        let messages = vec![content];
        let request = LlmRequest::with_messages(messages, "task1".to_string(), "ctx1".to_string())
            .with_system_instruction("You are helpful".to_string());

        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.current_task_id, "task1");
        assert_eq!(request.context_id, "ctx1");
        assert_eq!(
            request.system_instruction,
            Some("You are helpful".to_string())
        );
    }

    #[test]
    fn test_function_call_detection() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::Agent,
        );

        content.add_text("Calling function".to_string(), None);
        content.add_function_call(
            "test_tool".to_string(),
            json!({"param": "value"}),
            Some("tool1".to_string()),
            None,
        );

        let messages = vec![content];
        let request = LlmRequest::with_messages(messages, "task1".to_string(), "ctx1".to_string());

        assert!(request.has_function_calls());
        let calls = request.extract_function_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "test_tool");
        assert_eq!(calls[0].1, json!({"param": "value"}));
    }

    #[test]
    fn test_last_message() {
        let mut content1 = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );
        content1.add_text("First".to_string(), None);

        let mut content2 = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg2".to_string(),
            MessageRole::Agent,
        );
        content2.add_text("Second".to_string(), None);

        let messages = vec![content1, content2];
        let request = LlmRequest::with_messages(messages, "task1".to_string(), "ctx1".to_string());

        let last = request.last_message().unwrap();
        assert_eq!(last.role, MessageRole::Agent);
    }
}
