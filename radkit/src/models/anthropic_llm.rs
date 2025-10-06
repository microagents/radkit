use super::{BaseLlm, LlmRequest, LlmResponse, ProviderCapabilities};
use crate::errors::{AgentError, AgentResult};
use crate::models::content::{Content, ContentPart};
use crate::observability::utils as obs_utils;
use crate::tools::BaseToolset;
use a2a_types::MessageRole;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;
use tracing::error;
use uuid::Uuid;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicLlm {
    model_name: String,
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicLlm {
    pub fn new(model_name: String, api_key: String) -> Self {
        Self {
            model_name,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

// Tool-related types for Anthropic API
#[derive(Debug, Serialize)]
struct ToolParam {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ToolChoice {
    Auto,
    #[allow(dead_code)]
    Any,
    #[allow(dead_code)]
    Tool {
        name: String,
    },
}

// Content block types for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: i32,
    temperature: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicMessageContent,
}

// Anthropic messages can have either string content or array of content blocks
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicMessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    role: String,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: String,
}

// A2A-native conversion functions
impl AnthropicLlm {
    /// Clean architecture: Convert toolset directly to Anthrapic tools, no message parsing needed
    async fn convert_toolset_to_anthropic_tools(
        &self,
        toolset: &dyn BaseToolset,
    ) -> AgentResult<Vec<ToolParam>> {
        // Get all tools without context filtering for schema generation
        let available_tools = toolset.get_tools().await;

        let mut anthropic_tools = Vec::new();
        for tool in available_tools {
            if let Some(declaration) = tool.get_declaration() {
                anthropic_tools.push(ToolParam {
                    name: declaration.name,
                    description: declaration.description,
                    input_schema: declaration.parameters,
                });
            }
        }

        Ok(anthropic_tools)
    }

    /// Convert Anthropic content blocks to ContentParts
    fn convert_content_block_to_content_part(block: &ContentBlock) -> ContentPart {
        match block {
            ContentBlock::Text { text } => ContentPart::Text {
                text: text.clone(),
                metadata: None,
            },
            ContentBlock::ToolUse { id, name, input } => ContentPart::FunctionCall {
                name: name.clone(),
                arguments: input.clone(),
                tool_use_id: Some(id.clone()),
                metadata: None,
            },
            ContentBlock::ToolResult {
                tool_use_id,
                content,
            } => {
                // Parse content back to JSON if possible
                let response =
                    serde_json::from_str::<Value>(content).unwrap_or_else(|_| json!(content));

                ContentPart::FunctionResponse {
                    name: "unknown".to_string(), // Anthropic doesn't provide function name in result
                    success: true, // Anthropic returns tool results, so assume success
                    result: response,
                    error_message: None,
                    tool_use_id: Some(tool_use_id.clone()),
                    duration_ms: None,
                    metadata: None,
                }
            }
        }
    }

    /// Convert Content messages to Anthropic message format
    fn content_messages_to_anthropic_messages(
        messages: &[Content],
        current_task_id: &str,
    ) -> Vec<AnthropicMessage> {
        let mut anthropic_messages = Vec::new();

        for content_msg in messages {
            let role = match content_msg.role {
                MessageRole::User => "user",
                MessageRole::Agent => "assistant",
            };

            let mut anthropic_content = Vec::new();

            // Process each content part
            for part in &content_msg.parts {
                match part {
                    ContentPart::Text { text, .. } => {
                        if !text.is_empty() {
                            let content = if content_msg.task_id != current_task_id {
                                format!("[Task: {}] {}", content_msg.task_id, text)
                            } else {
                                text.clone()
                            };

                            anthropic_content.push(ContentBlock::Text { text: content });
                        }
                    }
                    ContentPart::FunctionCall {
                        name,
                        arguments,
                        tool_use_id,
                        ..
                    } => {
                        anthropic_content.push(ContentBlock::ToolUse {
                            id: tool_use_id.clone().unwrap_or_else(|| {
                                format!("toolu_{}", Uuid::new_v4().to_string().replace("-", ""))
                            }),
                            name: name.clone(),
                            input: arguments.clone(),
                        });
                    }
                    ContentPart::FunctionResponse {
                        success,
                        result,
                        error_message,
                        tool_use_id,
                        ..
                    } => {
                        let content = if *success {
                            serde_json::to_string(result).unwrap_or_else(|_| result.to_string())
                        } else {
                            error_message
                                .clone()
                                .unwrap_or_else(|| "Unknown error".to_string())
                        };

                        anthropic_content.push(ContentBlock::ToolResult {
                            tool_use_id: tool_use_id
                                .clone()
                                .unwrap_or_else(|| Uuid::new_v4().to_string()),
                            content,
                        });
                    }
                    _ => {
                        // Handle other content types (File, Data) if needed
                    }
                }
            }

            if !anthropic_content.is_empty() {
                anthropic_messages.push(AnthropicMessage {
                    role: role.to_string(),
                    content: AnthropicMessageContent::Blocks(anthropic_content),
                });
            }
        }

        anthropic_messages
    }
}

#[async_trait]
impl BaseLlm for AnthropicLlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    #[tracing::instrument(
        name = "radkit.llm.generate_content",
        skip(self, request),
        fields(
            llm.provider = "anthropic",
            llm.model = %self.model_name,
            llm.request.messages_count = request.messages.len(),
            llm.response.prompt_tokens = tracing::field::Empty,
            llm.response.completion_tokens = tracing::field::Empty,
            llm.response.total_tokens = tracing::field::Empty,
            llm.cost_usd = tracing::field::Empty,
            otel.kind = "client",
        )
    )]
    async fn generate_content(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        // Convert Content messages to Anthropic messages
        let anthropic_messages = Self::content_messages_to_anthropic_messages(
            &request.messages,
            &request.current_task_id,
        );

        // Get tools from LlmRequest toolset
        let anthropic_tools = if let Some(toolset) = &request.toolset {
            self.convert_toolset_to_anthropic_tools(toolset.as_ref())
                .await?
        } else {
            Vec::new()
        };

        // Set tool_choice based on whether tools are available
        let tool_choice = if !anthropic_tools.is_empty() {
            Some(ToolChoice::Auto)
        } else {
            None
        };

        // **COMPLETE CONVERSATION HISTORY**: Use all converted messages (no reconstruction needed!)
        // LlmRequest already contains the full ordered conversation history

        let anthropic_request = AnthropicRequest {
            model: self.model_name.clone(),
            messages: anthropic_messages,
            system: request.system_instruction,
            max_tokens: request.config.max_tokens.unwrap_or(4096) as i32,
            temperature: request.config.temperature.unwrap_or(0.7),
            tools: anthropic_tools,
            tool_choice,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| AgentError::Network {
                operation: "anthropic_api_request".to_string(),
                reason: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Anthropic API error: {}", error_text);

            // Try to parse as structured error
            let err = if let Ok(error) = serde_json::from_str::<AnthropicError>(&error_text) {
                AgentError::LlmProvider {
                    provider: "anthropic".to_string(),
                    message: error.error.message,
                }
            } else {
                AgentError::LlmProvider {
                    provider: "anthropic".to_string(),
                    message: error_text,
                }
            };

            obs_utils::record_error(&err);
            return Err(err);
        }

        let anthropic_response: AnthropicResponse =
            response
                .json()
                .await
                .map_err(|e| AgentError::Serialization {
                    format: "json".to_string(),
                    reason: e.to_string(),
                })?;

        // Convert back to Content format
        let content_parts: Vec<ContentPart> = anthropic_response
            .content
            .iter()
            .map(Self::convert_content_block_to_content_part)
            .collect();

        // Create Content with all parts (including function calls/responses)
        let response_content = Content {
            task_id: request.current_task_id.clone(),
            context_id: request.context_id.clone(),
            message_id: Uuid::new_v4().to_string(),
            role: MessageRole::Agent,
            parts: content_parts,
            metadata: None,
        };

        // Record token usage and cost
        let prompt_tokens = anthropic_response.usage.input_tokens;
        let completion_tokens = anthropic_response.usage.output_tokens;
        let total_tokens = prompt_tokens + completion_tokens;

        let span = tracing::Span::current();
        span.record("llm.response.prompt_tokens", prompt_tokens);
        span.record("llm.response.completion_tokens", completion_tokens);
        span.record("llm.response.total_tokens", total_tokens);

        let cost = obs_utils::calculate_llm_cost(&self.model_name, prompt_tokens, completion_tokens);
        span.record("llm.cost_usd", cost);

        obs_utils::record_llm_tokens_metric(&self.model_name, prompt_tokens, completion_tokens);
        obs_utils::record_success();

        Ok(LlmResponse::success(response_content))
    }

    async fn generate_content_stream(
        &self,
        _request: LlmRequest,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<LlmResponse>> + Send>>> {
        // For now, just return an error - we'll implement streaming later
        Err(AgentError::NotImplemented {
            feature: "Streaming for AnthropicLlm".to_string(),
        })
    }

    fn supports_function_calling(&self) -> bool {
        true // Anthropic supports tool use via Messages API
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: Some(200000), // Claude-3 context length
            supported_file_types: vec![
                "text/plain".to_string(),
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "image/webp".to_string(),
            ],
            supports_system_instructions: true,
            supports_json_schema: false,
            max_output_tokens: Some(4096),
        }
    }
}
