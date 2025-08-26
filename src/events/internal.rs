use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Import Content type for rich event data
use crate::models::content::Content;

/// Internal events for debugging, observability, and logging.
/// These events are NOT sent to A2A clients - they exist purely for internal tracking.
/// Now uses Content type for rich message representation including function calls/responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InternalEvent {
    /// Message event - unified representation of all conversation interactions
    /// Captures user inputs, model responses, function calls, and function results
    MessageReceived {
        /// The full content including text, function calls, and function responses
        content: Content,
        /// Additional metadata for the event
        metadata: EventMetadata,
        /// When this event occurred
        timestamp: DateTime<Utc>,
    },

    /// State was modified (app/user/session level)
    StateChange {
        context_id: Option<String>, // None for app-level changes
        app_name: String,
        user_id: Option<String>, // None for app-level changes
        scope: StateScope,       // App, User, or Session level
        key: String,
        old_value: Option<Value>,
        new_value: Value,
        change_reason: Option<String>, // Why the state changed
        timestamp: DateTime<Utc>,
    },
}

/// Metadata for conversation content events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Application name for multi-tenancy
    pub app_name: String,
    /// User ID
    pub user_id: String,
    /// Model-specific metadata (for model responses)
    pub model_info: Option<ModelInfo>,
    /// Performance metrics
    pub performance: Option<PerformanceMetrics>,
}

/// Model information for model-generated content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_name: String,
    pub prompt_tokens: Option<usize>,
    pub response_tokens: Option<usize>,
    pub cost_estimate: Option<f64>,
}

/// Performance metrics for any operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub duration_ms: u64,
    pub cache_hit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateScope {
    App,
    User,
    Session,
}

impl InternalEvent {
    /// Get the timestamp of the event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::MessageReceived { timestamp, .. } | Self::StateChange { timestamp, .. } => {
                *timestamp
            }
        }
    }

    /// Get the task_id if this event is associated with a task
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { content, .. } => Some(&content.task_id),
            Self::StateChange { .. } => None, // State changes may not be task-specific
        }
    }

    /// Get the context_id if this event is associated with a session
    pub fn context_id(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { content, .. } => Some(&content.context_id),
            Self::StateChange { context_id, .. } => context_id.as_deref(),
        }
    }

    /// Get the app_name for this event
    pub fn app_name(&self) -> &str {
        match self {
            Self::MessageReceived { metadata, .. } => &metadata.app_name,
            Self::StateChange { app_name, .. } => app_name,
        }
    }

    /// Get the user_id if this event is associated with a user
    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { metadata, .. } => Some(&metadata.user_id),
            Self::StateChange { user_id, .. } => user_id.as_deref(),
        }
    }

    /// Check if this is an error-related event
    pub fn is_error(&self) -> bool {
        match self {
            Self::MessageReceived { content, .. } => {
                // Check if content has function responses with errors
                content.get_function_responses().iter().any(|part| {
                    if let crate::models::content::ContentPart::FunctionResponse {
                        success, ..
                    } = part
                    {
                        !success
                    } else {
                        false
                    }
                })
            }
            _ => false,
        }
    }

    /// Check if this is a performance-related event (has timing data)
    pub fn is_performance_metric(&self) -> bool {
        match self {
            Self::MessageReceived { metadata, .. } => metadata.performance.is_some(),
            _ => false,
        }
    }

    /// Get performance timing data if available
    pub fn duration_ms(&self) -> Option<u64> {
        match self {
            Self::MessageReceived { metadata, .. } => {
                metadata.performance.as_ref().map(|p| p.duration_ms)
            }
            _ => None,
        }
    }

    /// Check if this is a tool-related event
    pub fn is_tool_event(&self) -> bool {
        match self {
            Self::MessageReceived { content, .. } => {
                content.has_function_calls() || content.has_function_responses()
            }
            _ => false,
        }
    }

    /// Get the function names for tool events
    pub fn function_names(&self) -> Vec<&str> {
        match self {
            Self::MessageReceived { content, .. } => {
                let mut names = Vec::new();
                for part in &content.parts {
                    match part {
                        crate::models::content::ContentPart::FunctionCall { name, .. }
                        | crate::models::content::ContentPart::FunctionResponse { name, .. } => {
                            names.push(name.as_str());
                        }
                        _ => {}
                    }
                }
                names
            }
            _ => Vec::new(),
        }
    }

    /// Check if this event represents a conversation turn
    pub fn is_conversation_turn(&self) -> bool {
        match self {
            Self::MessageReceived { content, .. } => {
                // A conversation turn has visible content (text, file, data)
                content.has_visible_content()
            }
            _ => false,
        }
    }

    // === Helper constructors ===

    /// Create a user input event from Content
    pub fn user_input(content: Content, app_name: String, user_id: String) -> Self {
        Self::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name,
                user_id,
                model_info: None,
                performance: None,
            },
            timestamp: Utc::now(),
        }
    }

    /// Create a model response event from Content
    pub fn model_response(
        content: Content,
        app_name: String,
        user_id: String,
        model_name: String,
        prompt_tokens: Option<usize>,
        response_tokens: Option<usize>,
        duration_ms: u64,
        cost_estimate: Option<f64>,
    ) -> Self {
        Self::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name,
                user_id,
                model_info: Some(ModelInfo {
                    model_name,
                    prompt_tokens,
                    response_tokens,
                    cost_estimate,
                }),
                performance: Some(PerformanceMetrics {
                    duration_ms,
                    cache_hit: None,
                }),
            },
            timestamp: Utc::now(),
        }
    }

    /// Create a tool execution event from Content
    pub fn tool_execution(
        content: Content,
        app_name: String,
        user_id: String,
        duration_ms: u64,
    ) -> Self {
        Self::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name,
                user_id,
                model_info: None,
                performance: Some(PerformanceMetrics {
                    duration_ms,
                    cache_hit: None,
                }),
            },
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::MessageRole;
    use crate::models::content::{Content, ContentPart};
    use serde_json::json;

    #[test]
    fn test_message_received_user_input() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::User,
        );
        content.add_text("Hello, how can you help me?".to_string(), None);

        let event =
            InternalEvent::user_input(content, "test_app".to_string(), "user_123".to_string());

        // Verify event properties
        assert_eq!(event.task_id(), Some("task_123"));
        assert_eq!(event.context_id(), Some("ctx_456"));
        assert_eq!(event.app_name(), "test_app");
        assert_eq!(event.user_id(), Some("user_123"));
        assert_eq!(event.is_conversation_turn(), true);
        assert_eq!(event.is_tool_event(), false);
        assert_eq!(event.is_error(), false);
        assert_eq!(event.is_performance_metric(), false);

        // Verify content is preserved
        if let InternalEvent::MessageReceived {
            content, metadata, ..
        } = event
        {
            assert_eq!(content.task_id, "task_123");
            assert_eq!(content.role, MessageRole::User);
            assert_eq!(content.parts.len(), 1);
            assert!(content.has_visible_content());

            // Verify metadata
            assert_eq!(metadata.app_name, "test_app");
            assert_eq!(metadata.user_id, "user_123");
            assert!(metadata.model_info.is_none());
            assert!(metadata.performance.is_none());
        } else {
            panic!("Expected MessageReceived event");
        }
    }

    #[test]
    fn test_message_received_model_response() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::Agent,
        );
        content.add_text("I can help you with that!".to_string(), None);

        let event = InternalEvent::model_response(
            content,
            "test_app".to_string(),
            "user_123".to_string(),
            "claude-3-5-sonnet".to_string(),
            Some(150),
            Some(75),
            1200,
            Some(0.045),
        );

        // Verify event properties
        assert_eq!(event.is_conversation_turn(), true);
        assert_eq!(event.is_performance_metric(), true);
        assert_eq!(event.duration_ms(), Some(1200));

        if let InternalEvent::MessageReceived {
            content, metadata, ..
        } = event
        {
            assert_eq!(content.role, MessageRole::Agent);
            assert!(content.has_visible_content());

            // Verify model metadata
            let model_info = metadata.model_info.expect("Should have model info");
            assert_eq!(model_info.model_name, "claude-3-5-sonnet");
            assert_eq!(model_info.prompt_tokens, Some(150));
            assert_eq!(model_info.response_tokens, Some(75));
            assert_eq!(model_info.cost_estimate, Some(0.045));

            // Verify performance metadata
            let perf = metadata.performance.expect("Should have performance data");
            assert_eq!(perf.duration_ms, 1200);
        } else {
            panic!("Expected MessageReceived event");
        }
    }

    #[test]
    fn test_message_received_with_function_calls() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::Agent,
        );

        content.add_text("I'll check the weather for you".to_string(), None);
        content.add_function_call(
            "get_weather".to_string(),
            json!({"location": "San Francisco", "units": "fahrenheit"}),
            Some("tool_001".to_string()),
            None,
        );

        let event =
            InternalEvent::user_input(content, "test_app".to_string(), "user_123".to_string());

        // Verify tool event detection
        assert_eq!(event.is_tool_event(), true);
        assert_eq!(event.is_conversation_turn(), true); // Has visible text

        let function_names = event.function_names();
        assert_eq!(function_names.len(), 1);
        assert_eq!(function_names[0], "get_weather");

        if let InternalEvent::MessageReceived { content, .. } = event {
            assert!(content.has_function_calls());
            assert!(!content.has_function_responses());

            let calls = content.get_function_calls();
            assert_eq!(calls.len(), 1);

            if let ContentPart::FunctionCall {
                name,
                arguments,
                tool_use_id,
                ..
            } = calls[0]
            {
                assert_eq!(name, "get_weather");
                assert_eq!(arguments["location"], "San Francisco");
                assert_eq!(tool_use_id, &Some("tool_001".to_string()));
            }
        }
    }

    #[test]
    fn test_message_received_with_function_responses() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::User, // Function results come as synthetic user messages
        );

        // Successful function response
        content.add_function_response(
            "get_weather".to_string(),
            true,
            json!({"temperature": "72°F", "conditions": "sunny", "humidity": "45%"}),
            None,
            Some("tool_001".to_string()),
            Some(350),
            None,
        );

        let event = InternalEvent::tool_execution(
            content,
            "test_app".to_string(),
            "user_123".to_string(),
            350,
        );

        // Verify tool and performance properties
        assert_eq!(event.is_tool_event(), true);
        assert_eq!(event.is_conversation_turn(), false); // No visible content
        assert_eq!(event.is_error(), false);
        assert_eq!(event.is_performance_metric(), true);
        assert_eq!(event.duration_ms(), Some(350));

        if let InternalEvent::MessageReceived {
            content, metadata, ..
        } = event
        {
            assert!(content.has_function_responses());
            assert!(!content.has_function_calls());

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
                assert_eq!(name, "get_weather");
                assert_eq!(*success, true);
                assert_eq!(result["temperature"], "72°F");
                assert_eq!(*duration_ms, Some(350));
            }

            // Verify performance metadata
            let perf = metadata.performance.expect("Should have performance data");
            assert_eq!(perf.duration_ms, 350);
        }
    }

    #[test]
    fn test_message_received_with_failed_function_response() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::User,
        );

        // Failed function response
        content.add_function_response(
            "get_weather".to_string(),
            false,
            json!({}),
            Some("API key invalid".to_string()),
            Some("tool_001".to_string()),
            Some(150),
            None,
        );

        let event = InternalEvent::tool_execution(
            content,
            "test_app".to_string(),
            "user_123".to_string(),
            150,
        );

        // Verify error detection
        assert_eq!(event.is_error(), true);
        assert_eq!(event.is_tool_event(), true);

        if let InternalEvent::MessageReceived { content, .. } = event {
            let responses = content.get_function_responses();
            if let ContentPart::FunctionResponse {
                success,
                error_message,
                ..
            } = responses[0]
            {
                assert_eq!(*success, false);
                assert_eq!(error_message.as_ref().unwrap(), "API key invalid");
            }
        }
    }

    #[test]
    fn test_state_change_event() {
        let event = InternalEvent::StateChange {
            context_id: Some("ctx_123".to_string()),
            app_name: "test_app".to_string(),
            user_id: Some("user_456".to_string()),
            scope: StateScope::Session,
            key: "preferred_language".to_string(),
            old_value: Some(json!("en")),
            new_value: json!("es"),
            change_reason: Some("User preference update".to_string()),
            timestamp: Utc::now(),
        };

        // State change events are not conversation or tool events
        assert_eq!(event.is_conversation_turn(), false);
        assert_eq!(event.is_tool_event(), false);
        assert_eq!(event.is_error(), false);
        assert_eq!(event.is_performance_metric(), false);

        // But have context info
        assert_eq!(event.context_id(), Some("ctx_123"));
        assert_eq!(event.app_name(), "test_app");
        assert_eq!(event.user_id(), Some("user_456"));
        assert_eq!(event.task_id(), None); // State changes don't have task IDs
    }

    #[test]
    fn test_mixed_content_event() {
        let mut content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::Agent,
        );

        // Mixed content: text + function call + function response
        content.add_text("I'll get the weather and analyze it".to_string(), None);
        content.add_function_call(
            "get_weather".to_string(),
            json!({"location": "NYC"}),
            Some("tool_001".to_string()),
            None,
        );
        content.add_function_response(
            "analyze_data".to_string(),
            true,
            json!({"trend": "warming"}),
            None,
            Some("tool_002".to_string()),
            Some(200),
            None,
        );

        let event = InternalEvent::model_response(
            content,
            "test_app".to_string(),
            "user_123".to_string(),
            "gpt-4".to_string(),
            Some(200),
            Some(100),
            1500,
            Some(0.08),
        );

        // Should detect all characteristics
        assert_eq!(event.is_conversation_turn(), true); // Has visible text
        assert_eq!(event.is_tool_event(), true); // Has function calls/responses
        assert_eq!(event.is_performance_metric(), true); // Has model metadata
        assert_eq!(event.is_error(), false); // No failed functions

        let function_names = event.function_names();
        assert_eq!(function_names.len(), 2);
        assert!(function_names.contains(&"get_weather"));
        assert!(function_names.contains(&"analyze_data"));
    }

    #[test]
    fn test_event_filtering_and_classification() {
        let events = vec![
            // User input event
            InternalEvent::user_input(
                {
                    let mut content = Content::new(
                        "task1".to_string(),
                        "ctx1".to_string(),
                        "msg1".to_string(),
                        MessageRole::User,
                    );
                    content.add_text("Hello".to_string(), None);
                    content
                },
                "app".to_string(),
                "user".to_string(),
            ),
            // Model response with performance
            InternalEvent::model_response(
                {
                    let mut content = Content::new(
                        "task1".to_string(),
                        "ctx1".to_string(),
                        "msg2".to_string(),
                        MessageRole::Agent,
                    );
                    content.add_text("Hi there!".to_string(), None);
                    content
                },
                "app".to_string(),
                "user".to_string(),
                "gpt-4".to_string(),
                Some(50),
                Some(25),
                800,
                Some(0.02),
            ),
            // Tool execution with error
            InternalEvent::tool_execution(
                {
                    let mut content = Content::new(
                        "task1".to_string(),
                        "ctx1".to_string(),
                        "msg3".to_string(),
                        MessageRole::User,
                    );
                    content.add_function_response(
                        "failing_tool".to_string(),
                        false,
                        json!("error"),
                        Some("Failed".to_string()),
                        None,
                        Some(100),
                        None,
                    );
                    content
                },
                "app".to_string(),
                "user".to_string(),
                100,
            ),
            // State change
            InternalEvent::StateChange {
                context_id: Some("ctx1".to_string()),
                app_name: "app".to_string(),
                user_id: Some("user".to_string()),
                scope: StateScope::Session,
                key: "test_key".to_string(),
                old_value: None,
                new_value: json!("value"),
                change_reason: None,
                timestamp: Utc::now(),
            },
        ];

        // Test filtering
        let conversation_events: Vec<_> =
            events.iter().filter(|e| e.is_conversation_turn()).collect();
        assert_eq!(conversation_events.len(), 2); // User input + model response

        let tool_events: Vec<_> = events.iter().filter(|e| e.is_tool_event()).collect();
        assert_eq!(tool_events.len(), 1); // Tool execution

        let error_events: Vec<_> = events.iter().filter(|e| e.is_error()).collect();
        assert_eq!(error_events.len(), 1); // Failed tool

        let perf_events: Vec<_> = events
            .iter()
            .filter(|e| e.is_performance_metric())
            .collect();
        assert_eq!(perf_events.len(), 2); // Model response + tool execution

        let task_events: Vec<_> = events.iter().filter(|e| e.task_id().is_some()).collect();
        assert_eq!(task_events.len(), 3); // All MessageReceived events have task IDs
    }
}
