use super::{BaseLlm, LlmRequest, LlmResponse, ProviderCapabilities, StreamingInfo};
use crate::a2a::MessageRole;
use crate::errors::{AgentError, AgentResult};
use crate::models::content::{Content, ContentPart};
use crate::tools::BaseToolset;
use async_trait::async_trait;
use futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;
use tracing::error;
use uuid::Uuid;

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAILlm {
    model_name: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAILlm {
    pub fn new(model_name: String, api_key: String) -> Self {
        Self {
            model_name,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

// Tool-related types for OpenAI API
#[derive(Debug, Serialize)]
struct ToolDefinition {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionDefinition,
}

#[derive(Debug, Serialize)]
struct FunctionDefinition {
    name: String,
    description: String,
    parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ToolChoice {
    Auto,
    None,
    Required,
    Specific {
        #[serde(rename = "type")]
        choice_type: String,
        function: ToolChoiceFunction,
    },
}

#[derive(Debug, Serialize)]
struct ToolChoiceFunction {
    name: String,
}

// Message types for OpenAI API
#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OpenAIContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

// OpenAI content can be string or array
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum OpenAIContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String, // OpenAI returns this as a JSON string
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: i64,
    model: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    index: i32,
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
}

#[derive(Debug, Deserialize)]
struct OpenAIError {
    error: OpenAIErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenAIErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    code: Option<String>,
}

// Streaming response types
#[derive(Debug, Deserialize)]
struct StreamResponse {
    id: String,
    object: String,
    created: i64,
    model: String,
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    index: i32,
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    tool_type: Option<String>,
    function: Option<FunctionCallDelta>,
}

#[derive(Debug, Deserialize)]
struct FunctionCallDelta {
    name: Option<String>,
    arguments: Option<String>,
}

// A2A-native conversion functions
impl OpenAILlm {
    /// Convert toolset to OpenAI tools format
    async fn convert_toolset_to_openai_tools(
        &self,
        toolset: &dyn BaseToolset,
    ) -> AgentResult<Vec<ToolDefinition>> {
        let available_tools = toolset.get_tools().await;

        let mut openai_tools = Vec::new();
        for tool in available_tools {
            if let Some(declaration) = tool.get_declaration() {
                openai_tools.push(ToolDefinition {
                    tool_type: "function".to_string(),
                    function: FunctionDefinition {
                        name: declaration.name,
                        description: declaration.description,
                        parameters: declaration.parameters,
                        strict: Some(true), // Enable structured outputs for better reliability
                    },
                });
            }
        }

        Ok(openai_tools)
    }

    /// Convert OpenAI tool calls to ContentParts
    fn convert_tool_calls_to_content_parts(tool_calls: &[ToolCall]) -> Vec<ContentPart> {
        tool_calls
            .iter()
            .map(|tool_call| {
                // Parse the arguments JSON string
                let arguments = serde_json::from_str::<Value>(&tool_call.function.arguments)
                    .unwrap_or_else(|_| json!({}));

                ContentPart::FunctionCall {
                    name: tool_call.function.name.clone(),
                    arguments,
                    tool_use_id: Some(tool_call.id.clone()),
                    metadata: None,
                }
            })
            .collect()
    }

    /// Convert Content messages to OpenAI message format
    fn content_messages_to_openai_messages(
        messages: &[Content],
        current_task_id: &str,
    ) -> Vec<OpenAIMessage> {
        let mut openai_messages = Vec::new();

        for content_msg in messages {
            let role = match content_msg.role {
                MessageRole::User => "user",
                MessageRole::Agent => "assistant",
            };

            // Process each content part
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();
            let mut tool_responses = Vec::new();

            for part in &content_msg.parts {
                match part {
                    ContentPart::Text { text, .. } => {
                        if !text.is_empty() {
                            let content = if content_msg.task_id != current_task_id {
                                format!("[Task: {}] {}", content_msg.task_id, text)
                            } else {
                                text.clone()
                            };
                            text_parts.push(content);
                        }
                    }
                    ContentPart::FunctionCall {
                        name,
                        arguments,
                        tool_use_id,
                        ..
                    } => {
                        // Convert to OpenAI tool call format
                        let tool_call = ToolCall {
                            id: tool_use_id.clone().unwrap_or_else(|| {
                                format!("call_{}", Uuid::new_v4().to_string().replace("-", ""))
                            }),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: name.clone(),
                                arguments: serde_json::to_string(arguments)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            },
                        };
                        tool_calls.push(tool_call);
                    }
                    ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        error_message,
                        tool_use_id,
                        ..
                    } => {
                        // OpenAI expects tool responses as separate messages
                        let content = if *success {
                            serde_json::to_string(result).unwrap_or_else(|_| result.to_string())
                        } else {
                            error_message
                                .clone()
                                .unwrap_or_else(|| "Unknown error".to_string())
                        };

                        tool_responses.push(OpenAIMessage {
                            role: "tool".to_string(),
                            content: Some(OpenAIContent::Text(content)),
                            name: Some(name.clone()),
                            tool_calls: None,
                            tool_call_id: tool_use_id.clone(),
                        });
                    }
                    _ => {
                        // Handle other content types if needed
                    }
                }
            }

            // Add the main message if it has content
            if !text_parts.is_empty() || !tool_calls.is_empty() {
                let content = if !text_parts.is_empty() {
                    Some(OpenAIContent::Text(text_parts.join("\n")))
                } else {
                    None
                };

                let tool_calls_option = if !tool_calls.is_empty() {
                    Some(tool_calls)
                } else {
                    None
                };

                openai_messages.push(OpenAIMessage {
                    role: role.to_string(),
                    content,
                    name: None,
                    tool_calls: tool_calls_option,
                    tool_call_id: None,
                });
            }

            // Add tool response messages
            openai_messages.extend(tool_responses);
        }

        openai_messages
    }

    /// Process a streaming chunk from OpenAI and convert it to LlmResponse
    fn process_stream_chunk(
        chunk: StreamResponse,
        task_id: &str,
        context_id: &str,
    ) -> AgentResult<LlmResponse> {
        let mut content = Content::new(
            task_id.to_string(),
            context_id.to_string(),
            uuid::Uuid::new_v4().to_string(),
            MessageRole::Agent,
        );

        if chunk.choices.is_empty() {
            return Ok(LlmResponse {
                message: content,
                streaming_info: StreamingInfo {
                    partial: true,
                    turn_complete: false,
                    sequence_number: Some(0),
                },
                usage_metadata: None,
                error_info: None,
                provider_metadata: std::collections::HashMap::new(),
            });
        }

        let choice = &chunk.choices[0];
        let delta = &choice.delta;

        // Handle text content
        if let Some(text) = &delta.content {
            if !text.is_empty() {
                content.add_text(text.clone(), None);
            }
        }

        // Handle tool calls
        if let Some(tool_calls) = &delta.tool_calls {
            for (index, tool_call_delta) in tool_calls.iter().enumerate() {
                // For streaming, we need to accumulate tool call data
                // This is a simplified approach - in production, you might want to
                // track tool call state across multiple chunks
                if let Some(function) = &tool_call_delta.function {
                    if let (Some(name), Some(arguments)) = (&function.name, &function.arguments) {
                        if !name.is_empty() || !arguments.is_empty() {
                            // Only add function call if we have meaningful data
                            let tool_use_id = tool_call_delta
                                .id
                                .clone()
                                .unwrap_or_else(|| format!("call_{index}"));

                            let parsed_arguments = match serde_json::from_str(arguments) {
                                Ok(args) => args,
                                Err(_) => json!({ "raw_arguments": arguments }),
                            };

                            content.add_function_call(
                                name.clone(),
                                parsed_arguments,
                                Some(tool_use_id),
                                None,
                            );
                        }
                    }
                }
            }
        }

        let is_final = choice.finish_reason.is_some();

        Ok(LlmResponse {
            message: content,
            streaming_info: StreamingInfo {
                partial: !is_final,
                turn_complete: is_final,
                sequence_number: Some(0), // OpenAI doesn't provide sequence numbers
            },
            usage_metadata: None, // Usage info typically comes at the end
            error_info: None,
            provider_metadata: std::collections::HashMap::new(),
        })
    }

    /// Add system message if provided
    fn add_system_message(messages: &mut Vec<OpenAIMessage>, system_instruction: Option<String>) {
        if let Some(instruction) = system_instruction {
            // Insert system message at the beginning
            messages.insert(
                0,
                OpenAIMessage {
                    role: "system".to_string(),
                    content: Some(OpenAIContent::Text(instruction)),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            );
        }
    }
}

#[async_trait]
impl BaseLlm for OpenAILlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn generate_content(&self, request: LlmRequest) -> AgentResult<LlmResponse> {
        // Convert Content messages to OpenAI messages
        let mut openai_messages =
            Self::content_messages_to_openai_messages(&request.messages, &request.current_task_id);

        // Add system message if present
        Self::add_system_message(&mut openai_messages, request.system_instruction);

        // Get tools from LlmRequest toolset
        let openai_tools = if let Some(toolset) = &request.toolset {
            let tools = self
                .convert_toolset_to_openai_tools(toolset.as_ref())
                .await?;
            if !tools.is_empty() { Some(tools) } else { None }
        } else {
            None
        };

        // Set tool_choice based on whether tools are available
        let tool_choice = if openai_tools.is_some() {
            Some("auto".to_string())
        } else {
            None
        };

        let openai_request = OpenAIRequest {
            model: self.model_name.clone(),
            messages: openai_messages,
            temperature: request.config.temperature,
            max_tokens: request.config.max_tokens.map(|t| t as i32),
            tools: openai_tools,
            tool_choice,
            response_format: None, // Can be configured for JSON mode if needed
            stream: None,          // Non-streaming request
        };

        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::Network {
                operation: "openai_api_request".to_string(),
                reason: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("OpenAI API error: {}", error_text);

            // Try to parse as structured error
            if let Ok(error) = serde_json::from_str::<OpenAIError>(&error_text) {
                return Err(AgentError::LlmProvider {
                    provider: "openai".to_string(),
                    message: error.error.message,
                });
            } else {
                return Err(AgentError::LlmProvider {
                    provider: "openai".to_string(),
                    message: error_text,
                });
            }
        }

        let openai_response: OpenAIResponse =
            response
                .json()
                .await
                .map_err(|e| AgentError::Serialization {
                    format: "json".to_string(),
                    reason: e.to_string(),
                })?;

        // Extract the first choice
        let choice =
            openai_response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| AgentError::LlmProvider {
                    provider: "openai".to_string(),
                    message: "No choices in response".to_string(),
                })?;

        // Convert response to Content format
        let mut content_parts = Vec::new();

        // Add text content if present
        if let Some(content) = choice.message.content {
            match content {
                OpenAIContent::Text(text) => {
                    if !text.is_empty() {
                        content_parts.push(ContentPart::Text {
                            text,
                            metadata: None,
                        });
                    }
                }
                OpenAIContent::Parts(parts) => {
                    content_parts.extend(parts);
                }
            }
        }

        // Add tool calls if present
        if let Some(tool_calls) = choice.message.tool_calls {
            content_parts.extend(Self::convert_tool_calls_to_content_parts(&tool_calls));
        }

        // Create Content with all parts
        let response_content = Content {
            task_id: request.current_task_id.clone(),
            context_id: request.context_id.clone(),
            message_id: Uuid::new_v4().to_string(),
            role: MessageRole::Agent,
            parts: content_parts,
            metadata: None,
        };

        Ok(LlmResponse::success(response_content))
    }

    async fn generate_content_stream(
        &self,
        request: LlmRequest,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<LlmResponse>> + Send>>> {
        // Convert Content messages to OpenAI messages
        let mut openai_messages =
            Self::content_messages_to_openai_messages(&request.messages, &request.current_task_id);

        // Add system message if present
        Self::add_system_message(&mut openai_messages, request.system_instruction);

        // Get tools from LlmRequest toolset
        let openai_tools = if let Some(toolset) = &request.toolset {
            Some(
                self.convert_toolset_to_openai_tools(toolset.as_ref())
                    .await?,
            )
        } else {
            None
        };

        let tool_choice = if openai_tools.is_some() {
            Some("auto".to_string())
        } else {
            None
        };

        let openai_request = OpenAIRequest {
            model: self.model_name.clone(),
            messages: openai_messages,
            temperature: request.config.temperature,
            max_tokens: request.config.max_tokens.map(|t| t as i32),
            tools: openai_tools,
            tool_choice,
            response_format: None, // Can be configured for JSON mode if needed
            stream: Some(true),    // Enable streaming
        };

        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::LlmProvider {
                provider: "OpenAI".to_string(),
                message: format!("Request failed: {e}"),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();

            // Try to parse as OpenAI error
            if let Ok(openai_error) = serde_json::from_str::<OpenAIError>(&error_body) {
                return Err(AgentError::LlmProvider {
                    provider: "OpenAI".to_string(),
                    message: openai_error.error.message,
                });
            }

            return Err(AgentError::LlmProvider {
                provider: "OpenAI".to_string(),
                message: format!("HTTP {status}: {error_body}"),
            });
        }

        // Clone task_id and context_id for the closure
        let task_id = request.current_task_id.clone();
        let context_id = request.context_id.clone();

        // Convert byte stream to line-based stream using tokio-util
        let byte_stream = response.bytes_stream().map_err(|e| {
            std::io::Error::other(format!("Request error: {e}"))
        });

        // Use tokio-util to handle proper line buffering
        let stream_reader = tokio_util::io::StreamReader::new(byte_stream);
        let lines =
            tokio_util::codec::FramedRead::new(stream_reader, tokio_util::codec::LinesCodec::new());

        let processed_stream = lines
            .map_err(|e| AgentError::Network {
                operation: "stream_read".to_string(),
                reason: format!("Stream decode error: {e}"),
            })
            .filter_map(move |line_result| {
                let task_id = task_id.clone();
                let context_id = context_id.clone();
                async move {
                    match line_result {
                        Ok(line) => {
                            // Skip empty lines
                            if line.trim().is_empty() {
                                return None;
                            }

                            // Handle SSE format - lines start with "data: "
                            if let Some(data) = line.strip_prefix("data: ") {
                                // Check for termination signal
                                if data.trim() == "[DONE]" {
                                    // Send a final response to indicate completion
                                    let final_content = Content::new(
                                        task_id.to_string(),
                                        context_id.to_string(),
                                        uuid::Uuid::new_v4().to_string(),
                                        MessageRole::Agent,
                                    );

                                    return Some(Ok(LlmResponse {
                                        message: final_content,
                                        streaming_info: StreamingInfo {
                                            partial: false,
                                            turn_complete: true,
                                            sequence_number: Some(999), // Final sequence number
                                        },
                                        usage_metadata: None,
                                        error_info: None,
                                        provider_metadata: std::collections::HashMap::new(),
                                    }));
                                }

                                // Skip empty data
                                if data.trim().is_empty() {
                                    return None;
                                }

                                // Parse the JSON chunk
                                match serde_json::from_str::<StreamResponse>(data.trim()) {
                                    Ok(stream_chunk) => {
                                        return Some(Self::process_stream_chunk(
                                            stream_chunk,
                                            &task_id,
                                            &context_id,
                                        ));
                                    }
                                    Err(e) => {
                                        // Log the problematic data for debugging
                                        tracing::warn!(
                                            "Failed to parse SSE data: '{}', error: {}",
                                            data,
                                            e
                                        );
                                        // Don't fail the entire stream on a single parse error, just skip it
                                        return None;
                                    }
                                }
                            }
                            // Skip non-data lines (e.g., "event: " lines)
                            None
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
            });

        Ok(Box::pin(processed_stream))
    }

    fn supports_streaming(&self) -> bool {
        true // OpenAI supports streaming
    }

    fn supports_function_calling(&self) -> bool {
        true // OpenAI supports function calling
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        // Capabilities vary by model, these are for GPT-4 models
        let (context_length, max_output) = match self.model_name.as_str() {
            "gpt-4o" | "gpt-4o-2024-08-06" => (128000, 16384),
            "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => (128000, 16384),
            "gpt-4-turbo" | "gpt-4-turbo-preview" => (128000, 4096),
            "gpt-4" => (8192, 4096),
            "gpt-3.5-turbo" => (16385, 4096),
            _ => (128000, 4096), // Default to newer model limits
        };

        ProviderCapabilities {
            max_context_length: Some(context_length),
            supported_file_types: vec![
                "text/plain".to_string(),
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "image/webp".to_string(),
            ],
            supports_system_instructions: true,
            supports_json_schema: true, // OpenAI supports structured outputs
            max_output_tokens: Some(max_output),
        }
    }
}
