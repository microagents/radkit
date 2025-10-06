//! OpenTelemetry semantic conventions for RadKit
//!
//! This module defines standard span names and attributes for observability.
//! Following GenAI semantic conventions where applicable.

/// GenAI semantic conventions
pub mod genai {
    /// Standard span names
    pub mod span {
        /// Agent message send operation
        pub const AGENT_SEND_MESSAGE: &str = "radkit.agent.send_message";

        /// Agent streaming message send operation
        pub const AGENT_SEND_STREAMING_MESSAGE: &str = "radkit.agent.send_streaming_message";

        /// Core conversation execution loop
        pub const CONVERSATION_EXECUTE: &str = "radkit.conversation.execute_core";

        /// LLM content generation
        pub const LLM_GENERATE: &str = "radkit.llm.generate_content";

        /// Tool execution
        pub const TOOL_EXECUTE: &str = "radkit.tool.execute_call";

        /// Tool batch processing
        pub const TOOLS_PROCESS: &str = "radkit.tools.process_calls";
    }

    /// Standard attribute names
    pub mod attr {
        // Agent attributes
        pub const AGENT_NAME: &str = "agent.name";
        pub const APP_NAME: &str = "app.name";
        pub const USER_ID: &str = "user.id";

        // LLM attributes
        pub const LLM_PROVIDER: &str = "llm.provider";
        pub const LLM_MODEL: &str = "llm.model";
        pub const LLM_REQUEST_MESSAGES_COUNT: &str = "llm.request.messages_count";
        pub const LLM_RESPONSE_PROMPT_TOKENS: &str = "llm.response.prompt_tokens";
        pub const LLM_RESPONSE_COMPLETION_TOKENS: &str = "llm.response.completion_tokens";
        pub const LLM_RESPONSE_TOTAL_TOKENS: &str = "llm.response.total_tokens";
        pub const LLM_COST_USD: &str = "llm.cost_usd";

        // Tool attributes
        pub const TOOL_NAME: &str = "tool.name";
        pub const TOOL_DURATION_MS: &str = "tool.duration_ms";
        pub const TOOL_SUCCESS: &str = "tool.success";
        pub const TOOLS_COUNT: &str = "tools.count";
        pub const TOOLS_TOTAL_DURATION_MS: &str = "tools.total_duration_ms";

        // Task/iteration attributes
        pub const TASK_ID: &str = "task.id";
        pub const ITERATION_COUNT: &str = "iteration.count";

        // OpenTelemetry standard attributes
        pub const OTEL_KIND: &str = "otel.kind";
        pub const OTEL_STATUS_CODE: &str = "otel.status_code";

        // Error attributes
        pub const ERROR_MESSAGE: &str = "error.message";

        // Streaming attribute
        pub const STREAMING: &str = "streaming";
    }

    /// Standard span kinds
    pub mod kind {
        pub const SERVER: &str = "server";
        pub const CLIENT: &str = "client";
        pub const INTERNAL: &str = "internal";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_names() {
        assert_eq!(genai::span::AGENT_SEND_MESSAGE, "radkit.agent.send_message");
        assert_eq!(genai::span::LLM_GENERATE, "radkit.llm.generate_content");
        assert_eq!(genai::span::TOOL_EXECUTE, "radkit.tool.execute_call");
    }

    #[test]
    fn test_attribute_names() {
        assert_eq!(genai::attr::AGENT_NAME, "agent.name");
        assert_eq!(genai::attr::LLM_PROVIDER, "llm.provider");
        assert_eq!(genai::attr::TOOL_NAME, "tool.name");
    }

    #[test]
    fn test_kind_values() {
        assert_eq!(genai::kind::SERVER, "server");
        assert_eq!(genai::kind::CLIENT, "client");
        assert_eq!(genai::kind::INTERNAL, "internal");
    }
}
