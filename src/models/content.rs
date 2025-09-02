use crate::a2a::{FileContent, Message, MessageRole, Part};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Content represents a message with extended parts that can include function calls/responses
/// This is the internal representation that bridges A2A protocol and LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Task ID this content belongs to
    pub task_id: String,
    /// Context/Session ID
    pub context_id: String,
    /// Message ID for tracking
    pub message_id: String,
    /// Role of the message sender
    pub role: MessageRole,
    /// Content parts including function calls/responses
    pub parts: Vec<ContentPart>,
    /// Optional metadata
    pub metadata: Option<HashMap<String, Value>>,
}

/// ContentPart extends A2A Part with function call/response capabilities
/// This enum represents all possible content types in a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, Value>>,
    },
    /// File content
    File {
        file: FileContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, Value>>,
    },
    /// Arbitrary data content
    Data {
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, Value>>,
    },
    /// Function/tool call request
    FunctionCall {
        /// Name of the function to call
        name: String,
        /// Arguments to pass to the function
        arguments: Value,
        /// Unique identifier for this tool use (for correlation with response)
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
        /// Additional metadata about the call
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, Value>>,
    },
    /// Function/tool execution result
    FunctionResponse {
        /// Name of the function that was called
        name: String,
        /// Whether the function executed successfully
        success: bool,
        /// The result data from the function
        result: Value,
        /// Error message if the function failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        /// Tool use ID this response corresponds to
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
        /// Execution duration in milliseconds
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        /// Additional metadata about the execution
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, Value>>,
    },
}

/// Parameters for adding a function response
pub struct FunctionResponseParams {
    pub name: String,
    pub success: bool,
    pub result: Value,
    pub error_message: Option<String>,
    pub tool_use_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub metadata: Option<HashMap<String, Value>>,
}

impl Content {
    /// Create a new Content instance
    pub fn new(task_id: String, context_id: String, message_id: String, role: MessageRole) -> Self {
        Self {
            task_id,
            context_id,
            message_id,
            role,
            parts: Vec::new(),
            metadata: None,
        }
    }

    /// Create Content from an A2A Message
    pub fn from_message(message: Message, task_id: String, context_id: String) -> Self {
        let parts = message
            .parts
            .iter()
            .map(|part| ContentPart::from_a2a_part(part.clone()))
            .collect();

        Self {
            task_id,
            context_id,
            message_id: message.message_id,
            role: message.role,
            parts,
            metadata: message.metadata,
        }
    }

    /// Create Content from an LLM response
    pub fn from_llm_response(
        response: crate::models::LlmResponse,
        task_id: String,
        context_id: String,
    ) -> Self {
        // LlmResponse already contains a Content message, so just update the IDs
        let mut content = response.message;
        content.task_id = task_id;
        content.context_id = context_id;
        content
    }

    /// Convert to A2A Message (filters out function call/response parts)
    pub fn to_a2a_message(&self) -> Message {
        let a2a_parts: Vec<Part> = self
            .parts
            .iter()
            .filter_map(|part| part.to_a2a_part())
            .collect();

        Message {
            kind: "message".to_string(),
            message_id: self.message_id.clone(),
            role: self.role.clone(),
            parts: a2a_parts,
            context_id: Some(self.context_id.clone()),
            task_id: Some(self.task_id.clone()),
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: self.metadata.clone(),
        }
    }

    /// Add a text part
    pub fn add_text(&mut self, text: String, metadata: Option<HashMap<String, Value>>) {
        self.parts.push(ContentPart::Text { text, metadata });
    }

    /// Add a function call
    pub fn add_function_call(
        &mut self,
        name: String,
        arguments: Value,
        tool_use_id: Option<String>,
        metadata: Option<HashMap<String, Value>>,
    ) {
        self.parts.push(ContentPart::FunctionCall {
            name,
            arguments,
            tool_use_id,
            metadata,
        });
    }

    /// Add a function response
    pub fn add_function_response(&mut self, params: FunctionResponseParams) {
        self.parts.push(ContentPart::FunctionResponse {
            name: params.name,
            success: params.success,
            result: params.result,
            error_message: params.error_message,
            tool_use_id: params.tool_use_id,
            duration_ms: params.duration_ms,
            metadata: params.metadata,
        });
    }

    /// Check if this content has function calls
    pub fn has_function_calls(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, ContentPart::FunctionCall { .. }))
    }

    /// Check if this content has function responses
    pub fn has_function_responses(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, ContentPart::FunctionResponse { .. }))
    }

    /// Get all function calls
    pub fn get_function_calls(&self) -> Vec<&ContentPart> {
        self.parts
            .iter()
            .filter(|part| matches!(part, ContentPart::FunctionCall { .. }))
            .collect()
    }

    /// Get all function responses
    pub fn get_function_responses(&self) -> Vec<&ContentPart> {
        self.parts
            .iter()
            .filter(|part| matches!(part, ContentPart::FunctionResponse { .. }))
            .collect()
    }

    /// Check if content has any visible parts (non-function parts)
    pub fn has_visible_content(&self) -> bool {
        self.parts.iter().any(|part| match part {
            ContentPart::Text { text, .. } => !text.is_empty(),
            ContentPart::File { .. } => true,
            ContentPart::Data { .. } => true,
            ContentPart::FunctionCall { .. } | ContentPart::FunctionResponse { .. } => false,
        })
    }
}

impl ContentPart {
    /// Convert from A2A Part
    pub fn from_a2a_part(part: Part) -> Self {
        match part {
            Part::Text { text, metadata } => ContentPart::Text { text, metadata },
            Part::File { file, metadata } => ContentPart::File { file, metadata },
            Part::Data { data, metadata } => ContentPart::Data { data, metadata },
        }
    }

    /// Convert to A2A Part (returns None for function-related parts)
    pub fn to_a2a_part(&self) -> Option<Part> {
        match self {
            ContentPart::Text { text, metadata } => {
                // Only include non-empty text parts
                if !text.is_empty() {
                    Some(Part::Text {
                        text: text.clone(),
                        metadata: metadata.clone(),
                    })
                } else {
                    None
                }
            }
            ContentPart::File { file, metadata } => Some(Part::File {
                file: file.clone(),
                metadata: metadata.clone(),
            }),
            ContentPart::Data { data, metadata } => Some(Part::Data {
                data: data.clone(),
                metadata: metadata.clone(),
            }),
            ContentPart::FunctionCall { .. } | ContentPart::FunctionResponse { .. } => {
                // Function-related parts are not included in A2A messages
                None
            }
        }
    }

    /// Check if this is a function-related part
    pub fn is_function_part(&self) -> bool {
        matches!(
            self,
            ContentPart::FunctionCall { .. } | ContentPart::FunctionResponse { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_content_creation() {
        let content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        assert_eq!(content.task_id, "task1");
        assert_eq!(content.context_id, "ctx1");
        assert_eq!(content.message_id, "msg1");
        assert_eq!(content.role, MessageRole::User);
        assert!(content.parts.is_empty());
    }

    #[test]
    fn test_content_from_message() {
        let message = Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: MessageRole::Agent,
            parts: vec![
                Part::Text {
                    text: "Hello".to_string(),
                    metadata: None,
                },
                Part::Data {
                    data: json!({"key": "value"}),
                    metadata: None,
                },
            ],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        };

        let content = Content::from_message(message, "task1".to_string(), "ctx1".to_string());

        assert_eq!(content.parts.len(), 2);
        assert!(matches!(&content.parts[0], ContentPart::Text { text, .. } if text == "Hello"));
        assert!(matches!(&content.parts[1], ContentPart::Data { .. }));
    }

    #[test]
    fn test_content_to_a2a_message() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::Agent,
        );

        // Add visible content
        content.add_text("Response text".to_string(), None);

        // Add function call (should be filtered out)
        content.add_function_call(
            "test_tool".to_string(),
            json!({"param": "value"}),
            Some("tool_123".to_string()),
            None,
        );

        // Add function response (should be filtered out)
        content.add_function_response(FunctionResponseParams {
            name: "test_tool".to_string(),
            success: true,
            result: json!({"result": "success"}),
            error_message: None,
            tool_use_id: Some("tool_123".to_string()),
            duration_ms: Some(150),
            metadata: None,
        });

        let message = content.to_a2a_message();

        // Only text part should be in A2A message
        assert_eq!(message.parts.len(), 1);
        assert!(matches!(&message.parts[0], Part::Text { text, .. } if text == "Response text"));
    }

    #[test]
    fn test_function_call_handling() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::Agent,
        );

        content.add_text("I'll help you with that".to_string(), None);
        content.add_function_call(
            "weather_tool".to_string(),
            json!({"location": "San Francisco"}),
            Some("tool_456".to_string()),
            None,
        );

        assert!(content.has_function_calls());
        assert!(!content.has_function_responses());

        let function_calls = content.get_function_calls();
        assert_eq!(function_calls.len(), 1);

        if let ContentPart::FunctionCall {
            name,
            arguments,
            tool_use_id,
            ..
        } = function_calls[0]
        {
            assert_eq!(name, "weather_tool");
            assert_eq!(arguments, &json!({"location": "San Francisco"}));
            assert_eq!(tool_use_id, &Some("tool_456".to_string()));
        } else {
            panic!("Expected FunctionCall");
        }
    }

    #[test]
    fn test_function_response_handling() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        content.add_function_response(FunctionResponseParams {
            name: "weather_tool".to_string(),
            success: true,
            result: json!({"temperature": "72°F", "conditions": "sunny"}),
            error_message: None,
            tool_use_id: Some("tool_456".to_string()),
            duration_ms: Some(250),
            metadata: None,
        });

        assert!(!content.has_function_calls());
        assert!(content.has_function_responses());

        let responses = content.get_function_responses();
        assert_eq!(responses.len(), 1);

        if let ContentPart::FunctionResponse {
            name,
            success,
            result,
            duration_ms,
            ..
        } = responses[0]
        {
            assert_eq!(name, "weather_tool");
            assert_eq!(*success, true);
            assert_eq!(result["temperature"], "72°F");
            assert_eq!(*duration_ms, Some(250));
        } else {
            panic!("Expected FunctionResponse");
        }
    }

    #[test]
    fn test_error_function_response() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        content.add_function_response(FunctionResponseParams {
            name: "failing_tool".to_string(),
            success: false,
            result: json!({}),
            error_message: Some("Tool execution failed: Network error".to_string()),
            tool_use_id: Some("tool_789".to_string()),
            duration_ms: Some(50),
            metadata: None,
        });

        let responses = content.get_function_responses();
        if let ContentPart::FunctionResponse {
            success,
            error_message,
            ..
        } = responses[0]
        {
            assert_eq!(*success, false);
            assert_eq!(
                error_message.as_ref().unwrap(),
                "Tool execution failed: Network error"
            );
        } else {
            panic!("Expected FunctionResponse");
        }
    }

    #[test]
    fn test_visible_content_check() {
        let mut content = Content::new(
            "task1".to_string(),
            "ctx1".to_string(),
            "msg1".to_string(),
            MessageRole::Agent,
        );

        // No visible content initially
        assert!(!content.has_visible_content());

        // Add empty text (not visible)
        content.add_text("".to_string(), None);
        assert!(!content.has_visible_content());

        // Add function call (not visible)
        content.add_function_call("tool".to_string(), json!({}), None, None);
        assert!(!content.has_visible_content());

        // Add non-empty text (visible)
        content.add_text("Hello".to_string(), None);
        assert!(content.has_visible_content());
    }

    #[test]
    fn test_content_part_conversions() {
        // Test A2A Part to ContentPart
        let text_part = Part::Text {
            text: "Hello".to_string(),
            metadata: None,
        };
        let content_part = ContentPart::from_a2a_part(text_part.clone());
        assert!(matches!(&content_part, ContentPart::Text { text, .. } if text == "Hello"));

        // Test ContentPart to A2A Part (text)
        let a2a_part = content_part.to_a2a_part();
        assert!(a2a_part.is_some());

        // Test function parts don't convert to A2A
        let func_call = ContentPart::FunctionCall {
            name: "test".to_string(),
            arguments: json!({}),
            tool_use_id: None,
            metadata: None,
        };
        assert!(func_call.to_a2a_part().is_none());
        assert!(func_call.is_function_part());

        let func_resp = ContentPart::FunctionResponse {
            name: "test".to_string(),
            success: true,
            result: json!({}),
            error_message: None,
            tool_use_id: None,
            duration_ms: None,
            metadata: None,
        };
        assert!(func_resp.to_a2a_part().is_none());
        assert!(func_resp.is_function_part());
    }
}
