use super::{BaseLlm, LlmRequest, LlmResponse, ProviderCapabilities};
use crate::errors::{AgentError, AgentResult};
use crate::models::content::{Content, ContentPart};
use crate::observability::utils as obs_utils;
use crate::tools::BaseToolset;
use a2a_types::MessageRole;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use uuid::Uuid;

const GEMINI_API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiLlm {
    model_name: String,
    api_key: String,
    client: reqwest::Client,
}

impl GeminiLlm {
    pub fn new(model_name: String, api_key: String) -> Self {
        Self {
            model_name,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    generation_config: GenerationConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    temperature: f32,
    max_output_tokens: i32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: u64,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: u64,
    #[serde(rename = "totalTokenCount")]
    #[allow(dead_code)]
    total_token_count: u64,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: GeminiContent,
}

// A2A-native conversion functions
impl GeminiLlm {
    /// Clean architecture: Convert toolset directly to Gemini tools, no message parsing needed
    async fn convert_toolset_to_gemini_tools(
        &self,
        toolset: &dyn BaseToolset,
    ) -> AgentResult<Vec<GeminiTool>> {
        // Get all tools without context filtering for schema generation
        let available_tools = toolset.get_tools().await;

        let mut function_declarations = Vec::new();
        for tool in available_tools {
            if let Some(declaration) = tool.get_declaration() {
                function_declarations.push(GeminiFunctionDeclaration {
                    name: declaration.name,
                    description: declaration.description,
                    parameters: Some(declaration.parameters),
                });
            }
        }

        if function_declarations.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![GeminiTool {
                function_declarations,
            }])
        }
    }

    /// Convert Gemini parts to ContentParts
    fn convert_gemini_part_to_content_part(part: GeminiPart) -> ContentPart {
        match part {
            GeminiPart::Text { text } => ContentPart::Text {
                text,
                metadata: None,
            },
            GeminiPart::FunctionCall { function_call } => {
                ContentPart::FunctionCall {
                    name: function_call.name.clone(),
                    arguments: function_call.args.clone(),
                    tool_use_id: None, // Gemini doesn't use tool_use_id like Anthropic
                    metadata: None,
                }
            }
            GeminiPart::FunctionResponse { function_response } => {
                // Store function name in metadata for proper round-trip conversion
                let mut metadata = std::collections::HashMap::new();
                metadata.insert(
                    "function_name".to_string(),
                    serde_json::Value::String(function_response.name.clone()),
                );

                ContentPart::FunctionResponse {
                    name: function_response.name.clone(),
                    success: true, // Gemini returns responses, assume success
                    result: function_response.response.clone(),
                    error_message: None,
                    tool_use_id: None,
                    duration_ms: None,
                    metadata: Some(metadata),
                }
            }
        }
    }

    /// Convert Content messages to Gemini content format  
    fn content_messages_to_gemini_contents(
        messages: &[Content],
        current_task_id: &str,
    ) -> Vec<GeminiContent> {
        let mut contents = Vec::new();

        for content_msg in messages {
            let role = match content_msg.role {
                MessageRole::User => "user",
                MessageRole::Agent => "model",
            };

            let mut gemini_parts = Vec::new();

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

                            gemini_parts.push(GeminiPart::Text { text: content });
                        }
                    }
                    ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        gemini_parts.push(GeminiPart::FunctionCall {
                            function_call: GeminiFunctionCall {
                                name: name.clone(),
                                args: arguments.clone(),
                            },
                        });
                    }
                    ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        error_message,
                        ..
                    } => {
                        let response = if *success {
                            result.clone()
                        } else {
                            json!({"error": error_message.clone().unwrap_or_else(|| "Unknown error".to_string())})
                        };

                        gemini_parts.push(GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: name.clone(),
                                response,
                            },
                        });
                    }
                    _ => {
                        // Handle other content types (File, Data) if needed
                    }
                }
            }

            // Add the message if it has content
            if !gemini_parts.is_empty() {
                // Ensure each message has at least some text content for Gemini
                let has_text = gemini_parts
                    .iter()
                    .any(|part| matches!(part, GeminiPart::Text { .. }));
                let has_function_content = gemini_parts.iter().any(|part| {
                    matches!(
                        part,
                        GeminiPart::FunctionCall { .. } | GeminiPart::FunctionResponse { .. }
                    )
                });

                // If we only have function content but no text, add a minimal text part
                let final_parts = if has_function_content && !has_text {
                    let mut parts_with_text = vec![GeminiPart::Text {
                        text: " ".to_string(),
                    }];
                    parts_with_text.extend(gemini_parts);
                    parts_with_text
                } else {
                    gemini_parts
                };

                contents.push(GeminiContent {
                    role: role.to_string(),
                    parts: final_parts,
                });
            }
        }

        contents
    }
}

#[async_trait]
impl BaseLlm for GeminiLlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    #[tracing::instrument(
        name = "radkit.llm.generate_content",
        skip(self, request),
        fields(
            llm.provider = "gemini",
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
        // Convert Content messages to Gemini contents
        let gemini_contents =
            Self::content_messages_to_gemini_contents(&request.messages, &request.current_task_id);

        // Extract system instruction from LlmRequest
        let system_instruction =
            request
                .system_instruction
                .map(|instruction| GeminiSystemInstruction {
                    parts: vec![GeminiPart::Text { text: instruction }],
                });

        // Get tools from LlmRequest toolset
        let tools = if let Some(toolset) = &request.toolset {
            self.convert_toolset_to_gemini_tools(toolset.as_ref())
                .await?
        } else {
            Vec::new()
        };
        let tools_option = if tools.is_empty() { None } else { Some(tools) };

        let gemini_request = GeminiRequest {
            contents: gemini_contents,
            system_instruction,
            generation_config: GenerationConfig {
                temperature: request.config.temperature.unwrap_or(0.7),
                max_output_tokens: request.config.max_tokens.unwrap_or(2048) as i32,
            },
            tools: tools_option,
        };

        let url = format!("{}/{}:generateContent", GEMINI_API_URL, self.model_name);

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(&gemini_request)
            .send()
            .await
            .map_err(|e| AgentError::Network {
                operation: "gemini_api_request".to_string(),
                reason: e.to_string(),
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(AgentError::LlmProvider {
                provider: "gemini".to_string(),
                message: error_text,
            });
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError::Serialization {
                format: "text".to_string(),
                reason: e.to_string(),
            })?;

        let gemini_response: GeminiResponse =
            serde_json::from_str(&response_text).map_err(|e| AgentError::Serialization {
                format: "json".to_string(),
                reason: e.to_string(),
            })?;

        let candidate = gemini_response
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::LlmProvider {
                provider: "gemini".to_string(),
                message: "No candidates in response".to_string(),
            })?;

        // Convert back to Content format
        let content_parts: Vec<ContentPart> = candidate
            .content
            .parts
            .into_iter()
            .map(Self::convert_gemini_part_to_content_part)
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
        let (prompt_tokens, completion_tokens) = if let Some(usage) = gemini_response.usage_metadata {
            (usage.prompt_token_count, usage.candidates_token_count)
        } else {
            (0, 0)
        };
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
            feature: "Streaming for GeminiLlm".to_string(),
        })
    }

    fn supports_function_calling(&self) -> bool {
        true // Gemini supports function calling
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: Some(1048576), // Gemini 2.0 Flash context length
            supported_file_types: vec![
                "text/plain".to_string(),
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "image/webp".to_string(),
            ],
            supports_system_instructions: true,
            supports_json_schema: false,
            max_output_tokens: Some(8192),
        }
    }
}
