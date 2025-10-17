use super::{BaseLlm, LlmRequest, LlmResponse, ProviderCapabilities};
use crate::errors::{AgentError, AgentResult};
use crate::models::content::{Content, ContentPart};
use crate::observability::utils as obs_utils;
use a2a_types::MessageRole;
use async_trait::async_trait;
use futures::Stream;
use serde_json::json;
use std::pin::Pin;
use uuid::Uuid;

pub struct MockLlm {
    model_name: String,
}

impl MockLlm {
    pub fn new(model_name: String) -> Self {
        Self { model_name }
    }
}

#[async_trait]
impl BaseLlm for MockLlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn to_model_config(&self) -> crate::config::ModelConfig {
        // MockLlm returns OpenAI config as a placeholder
        crate::config::ModelConfig::OpenAI {
            name: self.model_name.clone(),
            api_key_env: "MOCK_API_KEY".to_string(),
        }
    }

    #[tracing::instrument(
        name = "radkit.llm.generate_content",
        skip(self, request, _env_resolver),
        fields(
            llm.provider = "mock",
            llm.model = %self.model_name,
            llm.request.messages_count = request.messages.len(),
            llm.response.prompt_tokens = 10,
            llm.response.completion_tokens = 20,
            llm.response.total_tokens = 30,
            otel.kind = "client",
        )
    )]
    async fn generate_content(
        &self,
        request: LlmRequest,
        _env_resolver: Option<crate::config::EnvResolverFn>,
    ) -> AgentResult<LlmResponse> {
        // **PROPER CONVERSATION HISTORY**: Access full message history from LlmRequest
        let conversation_context = format!(
            " [Conversation history: {} messages]",
            request.messages.len()
        );

        // Extract the latest user input from messages
        let latest_message_text = request
            .messages
            .iter()
            .rev()
            .find_map(|content| {
                if matches!(content.role, MessageRole::User) {
                    content.parts.iter().find_map(|part| match part {
                        ContentPart::Text { text, .. } => Some(text.clone()),
                        _ => None,
                    })
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "No user message found".to_string());

        // Check if tools are available and user is asking for a function call
        let should_simulate_function_call = request.toolset.is_some()
            && (latest_message_text.contains("call")
                || latest_message_text.contains("tool")
                || latest_message_text.contains("function")
                || latest_message_text.contains("use")
                || latest_message_text.contains("update")
                || latest_message_text.contains("save"));

        // Simulate a function call if tools are available and requested
        if should_simulate_function_call {
            // Get available tools and pick the first one, or look for a specific tool mentioned
            let tool_name = if let Some(toolset) = &request.toolset {
                let tools = toolset.get_tools().await;

                // Try to find a tool name mentioned in the message
                let mut found_tool = None;
                for tool in &tools {
                    let tool_name = tool.name();
                    if latest_message_text
                        .to_lowercase()
                        .contains(&tool_name.to_lowercase())
                    {
                        found_tool = Some(tool_name.to_string());
                        break;
                    }
                }

                // If no specific tool mentioned, use the first available tool
                // or "mock_function" if that exists
                found_tool.unwrap_or_else(|| {
                    tools
                        .iter()
                        .find(|t| t.name() == "mock_function")
                        .map(|t| t.name().to_string())
                        .unwrap_or_else(|| {
                            tools
                                .first()
                                .map(|t| t.name().to_string())
                                .unwrap_or_else(|| "mock_function".to_string())
                        })
                })
            } else {
                "mock_function".to_string()
            };

            // Create appropriate args based on the tool name
            let args = match tool_name.as_str() {
                "update_status" => json!({"status": "working", "message": "Processing"}),
                "save_artifact" => json!({"name": "test_artifact", "data": "test data"}),
                "failing_tool" => json!({"should_fail": true}),
                _ => json!({"test": "mock_arg"}),
            };

            // Create Content with function call
            let response_content = Content {
                task_id: request.current_task_id.clone(),
                context_id: request.context_id.clone(),
                message_id: Uuid::new_v4().to_string(),
                role: MessageRole::Agent,
                parts: vec![ContentPart::FunctionCall {
                    name: tool_name,
                    arguments: args,
                    tool_use_id: Some(format!("mock_{}", Uuid::new_v4())),
                    metadata: None,
                }],
                metadata: None,
            };

            obs_utils::record_success();
            return Ok(LlmResponse::success(response_content));
        }

        let response_text = format!(
            "Mock LLM ({}) received: {}.{} Here's a mock response.",
            self.model_name, latest_message_text, conversation_context
        );

        let response_content = Content {
            task_id: request.current_task_id.clone(),
            context_id: request.context_id.clone(),
            message_id: Uuid::new_v4().to_string(),
            role: MessageRole::Agent,
            parts: vec![ContentPart::Text {
                text: response_text,
                metadata: None,
            }],
            metadata: None,
        };

        obs_utils::record_success();
        Ok(LlmResponse::success(response_content))
    }

    async fn generate_content_stream(
        &self,
        _request: LlmRequest,
        _env_resolver: Option<crate::config::EnvResolverFn>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<LlmResponse>> + Send>>> {
        // For now, just return an error - we'll implement streaming later
        Err(AgentError::NotImplemented {
            feature: "Streaming for MockLlm".to_string(),
        })
    }

    fn supports_function_calling(&self) -> bool {
        true // MockLlm can simulate function calls
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: Some(32768),
            supported_file_types: vec!["text/plain".to_string()],
            supports_system_instructions: true,
            supports_json_schema: false,
            max_output_tokens: Some(4096),
        }
    }
}
