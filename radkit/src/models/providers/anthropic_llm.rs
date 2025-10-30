//! Anthropic Claude LLM provider implementation.
//!
//! API Documentation: <https://docs.anthropic.com/en/api/messages>
//! Tool Documentation: <https://docs.anthropic.com/en/docs/build-with-claude/tool-use>
//! Model Names: <https://docs.anthropic.com/en/docs/models-overview>
//! Pricing: <https://www.anthropic.com/pricing#anthropic-api>

use std::sync::Arc;

use serde_json::{json, Value};

use crate::errors::{AgentError, AgentResult};
use crate::models::{BaseLlm, Content, ContentPart, LlmResponse, Role, Thread, TokenUsage};
use crate::tools::{BaseToolset, ToolCall};

const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1/messages";

// Default max_tokens values based on model names
const MAX_TOKENS_64K: u32 = 64000; // claude-sonnet-4, claude-3-7-sonnet
const MAX_TOKENS_32K: u32 = 32000; // claude-opus-4
const MAX_TOKENS_8K: u32 = 8192; // claude-3-5-sonnet, claude-3-5-haiku
const MAX_TOKENS_4K: u32 = 4096; // claude-3-opus, claude-3-haiku

/// Anthropic Claude LLM implementation.
///
/// Provides access to Claude models through the Anthropic Messages API.
/// Supports text generation, multi-modal inputs (images, documents), and tool use.
///
/// # Authentication
///
/// The API key can be provided explicitly or loaded from the `ANTHROPIC_API_KEY`
/// environment variable via [`from_env`](AnthropicLlm::from_env).
///
/// # Model Selection
///
/// Common model names:
/// - `claude-sonnet-4-5-20250929` - Latest Claude Sonnet 4.5
/// - `claude-opus-4-1-20250805` - Claude Opus 4
/// - `claude-3-5-haiku-latest` - Claude 3.5 Haiku
///
/// # Examples
///
/// ```ignore
/// use radkit::models::providers::AnthropicLlm;
/// use radkit::models::{BaseLlm, Thread};
///
/// // From environment variable
/// let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
///
/// // With explicit API key
/// let llm = AnthropicLlm::new("claude-sonnet-4-5-20250929", "sk-ant-...");
///
/// // Generate content
/// let thread = Thread::from_user("Explain quantum computing in simple terms");
/// let response = llm.generate_content(thread, None).await?;
/// println!("{}", response.first_text().unwrap_or("No response"));
/// ```
pub struct AnthropicLlm {
    model_name: String,
    api_key: String,
    base_url: String,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

impl AnthropicLlm {
    /// Environment variable name for the Anthropic API key.
    pub const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

    /// Creates a new Anthropic LLM instance with explicit API key.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The model to use (e.g., "claude-sonnet-4-5-20250929")
    /// * `api_key` - Anthropic API key
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let llm = AnthropicLlm::new("claude-sonnet-4-5-20250929", "sk-ant-...");
    /// ```
    pub fn new(model_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            api_key: api_key.into(),
            base_url: ANTHROPIC_BASE_URL.to_string(),
            max_tokens: None,
            temperature: None,
        }
    }

    /// Creates a new Anthropic LLM instance loading API key from environment.
    ///
    /// Reads the API key from the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The model to use (e.g., "claude-sonnet-4-5-20250929")
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is not set or is empty.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
    /// ```
    pub fn from_env(model_name: impl Into<String>) -> AgentResult<Self> {
        let api_key =
            std::env::var(Self::API_KEY_ENV).map_err(|_| AgentError::MissingConfiguration {
                field: Self::API_KEY_ENV.to_string(),
            })?;

        if api_key.is_empty() {
            return Err(AgentError::InvalidConfiguration {
                field: Self::API_KEY_ENV.to_string(),
                reason: "API key cannot be empty".to_string(),
            });
        }

        Ok(Self::new(model_name, api_key))
    }

    /// Sets a custom base URL for the API endpoint.
    ///
    /// Useful for testing or when using a proxy/gateway.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the maximum number of tokens to generate.
    ///
    /// If not set, a default value is chosen based on the model name.
    #[must_use] pub const fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets the temperature for generation (0.0 to 1.0).
    ///
    /// Higher values produce more random outputs.
    #[must_use] pub const fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Determines the appropriate `max_tokens` value based on model name.
    fn default_max_tokens(&self) -> u32 {
        let model = self.model_name.as_str();
        if model.contains("claude-sonnet") || model.contains("claude-3-7-sonnet") {
            MAX_TOKENS_64K
        } else if model.contains("claude-opus-4") {
            MAX_TOKENS_32K
        } else if model.contains("claude-3-5") {
            MAX_TOKENS_8K
        } else if model.contains("3-opus") || model.contains("3-haiku") {
            MAX_TOKENS_4K
        } else {
            MAX_TOKENS_64K
        }
    }

    /// Converts a Thread into Anthropic API request format.
    async fn build_request_payload(
        &self,
        thread: Thread,
        toolset: Option<Arc<dyn BaseToolset>>,
    ) -> AgentResult<Value> {
        let (system_prompt, events) = thread.into_parts();

        // Build messages array from events
        let mut messages = Vec::new();

        for event in events {
            let role_str = match event.role() {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "user", // Tool responses go as user messages in Anthropic
                Role::System => {
                    // System messages are not supported in messages array
                    continue;
                }
            };

            let content = event.into_content();

            // Determine if content can be represented as simple text
            if content.is_text_only() && !content.is_text_empty() {
                let text = content.joined_texts().unwrap_or_default();
                messages.push(json!({
                    "role": role_str,
                    "content": text
                }));
            } else {
                // Multi-part content
                let mut content_parts = Vec::new();

                for part in content {
                    match part {
                        ContentPart::Text(text) => {
                            content_parts.push(json!({
                                "type": "text",
                                "text": text
                            }));
                        }
                        ContentPart::Data(data) => {
                            let source_data = match &data.source {
                                crate::models::DataSource::Base64(b64) => b64.clone(),
                                crate::models::DataSource::Uri(_) => {
                                    return Err(AgentError::NotImplemented {
                                        feature: "Anthropic provider does not support image URIs. Please provide image data as base64.".to_string(),
                                    });
                                }
                            };

                            // Determine if it's an image or document based on content type
                            let is_image = data.content_type.starts_with("image/");

                            if is_image {
                                content_parts.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": data.content_type,
                                        "data": source_data
                                    }
                                }));
                            } else {
                                content_parts.push(json!({
                                    "type": "document",
                                    "source": {
                                        "type": "base64",
                                        "media_type": data.content_type,
                                        "data": source_data
                                    }
                                }));
                            }
                        }
                        ContentPart::ToolCall(tool_call) => {
                            content_parts.push(json!({
                                "type": "tool_use",
                                "id": tool_call.id(),
                                "name": tool_call.name(),
                                "input": tool_call.arguments()
                            }));
                        }
                        ContentPart::ToolResponse(tool_response) => {
                            let result = tool_response.result();
                            let content_value = if result.is_success() {
                                result.data().clone()
                            } else {
                                json!({
                                    "error": result.error_message().unwrap_or("Unknown error")
                                })
                            };

                            content_parts.push(json!({
                                "type": "tool_result",
                                "tool_use_id": tool_response.tool_call_id(),
                                "content": content_value.to_string()
                            }));
                        }
                    }
                }

                if !content_parts.is_empty() {
                    messages.push(json!({
                        "role": role_str,
                        "content": content_parts
                    }));
                }
            }
        }

        // Build the base payload
        let max_tokens = self.max_tokens.unwrap_or_else(|| self.default_max_tokens());

        let mut payload = json!({
            "model": self.model_name,
            "max_tokens": max_tokens,
            "messages": messages
        });

        // Add system prompt if present
        if let Some(system) = system_prompt {
            payload["system"] = json!(system);
        }

        // Add temperature if set
        if let Some(temperature) = self.temperature {
            payload["temperature"] = json!(temperature);
        }

        // Add tools if provided
        if let Some(toolset) = toolset {
            let tools_list = toolset.get_tools().await;
            if !tools_list.is_empty() {
                let tools: Vec<Value> = tools_list
                    .iter()
                    .map(|tool| {
                        let decl = tool.declaration();
                        json!({
                            "name": decl.name(),
                            "description": decl.description(),
                            "input_schema": decl.parameters()
                        })
                    })
                    .collect();

                payload["tools"] = json!(tools);
            }
        }

        Ok(payload)
    }

    /// Parses Anthropic API response into Content.
    fn parse_response(&self, response_body: &Value) -> AgentResult<Content> {
        let mut content = Content::default();

        // Extract content array from response
        let content_items = response_body
            .get("content")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::LlmProvider {
                provider: "Anthropic".to_string(),
                message: "Missing or invalid 'content' field in response".to_string(),
            })?;

        for item in content_items {
            let item_type = item.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
                AgentError::LlmProvider {
                    provider: "Anthropic".to_string(),
                    message: "Missing 'type' field in content item".to_string(),
                }
            })?;

            match item_type {
                "text" => {
                    let text = item.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
                        AgentError::LlmProvider {
                            provider: "Anthropic".to_string(),
                            message: "Missing 'text' field in text content item".to_string(),
                        }
                    })?;
                    content.push(ContentPart::Text(text.to_string()));
                }
                "tool_use" => {
                    let id = item.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
                        AgentError::LlmProvider {
                            provider: "Anthropic".to_string(),
                            message: "Missing 'id' field in tool_use content item".to_string(),
                        }
                    })?;
                    let name = item.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                        AgentError::LlmProvider {
                            provider: "Anthropic".to_string(),
                            message: "Missing 'name' field in tool_use content item".to_string(),
                        }
                    })?;
                    let input = item.get("input").cloned().unwrap_or(Value::Null);

                    let tool_call = ToolCall::new(id, name, input);
                    content.push(ContentPart::ToolCall(tool_call));
                }
                // Ignore other types like "thinking" for now
                _ => {}
            }
        }

        Ok(content)
    }

    /// Parses token usage from Anthropic API response.
    ///
    /// Anthropic returns usage with separate fields for `input_tokens`, cache tokens,
    /// and `output_tokens`. We normalize this by summing all input-related tokens.
    fn parse_usage(&self, response_body: &Value) -> TokenUsage {
        let usage_obj = match response_body.get("usage") {
            Some(obj) => obj,
            None => return TokenUsage::empty(),
        };

        // Extract individual token counts
        let input_tokens = usage_obj
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        let cache_creation_tokens = usage_obj
            .get("cache_creation_input_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        let cache_read_tokens = usage_obj
            .get("cache_read_input_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        let output_tokens = usage_obj
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        // Compute total input tokens (including cache operations)
        let total_input_tokens = input_tokens + cache_creation_tokens + cache_read_tokens;

        // Compute total
        let total_tokens = total_input_tokens + output_tokens;

        TokenUsage::new(total_input_tokens, output_tokens, total_tokens)
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseLlm for AnthropicLlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn generate_content(
        &self,
        thread: Thread,
        toolset: Option<Arc<dyn BaseToolset>>,
    ) -> AgentResult<LlmResponse> {
        // Build request payload
        let payload = self.build_request_payload(thread, toolset).await?;

        // Create HTTP client
        let client = reqwest::Client::new();

        // Make request
        let response = client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("content-type", "application/json")
            .json(&payload)
            .send()
            .await?;

        // Check for HTTP errors
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            return Err(match status.as_u16() {
                401 => AgentError::LlmAuthentication {
                    provider: "Anthropic".to_string(),
                },
                429 => AgentError::LlmRateLimit {
                    provider: "Anthropic".to_string(),
                },
                _ => AgentError::LlmProvider {
                    provider: "Anthropic".to_string(),
                    message: format!("HTTP {status}: {error_body}"),
                },
            });
        }

        // Parse response
        let response_body: Value = response.json().await?;

        // Check for stop_reason indicating content filtering
        if let Some(stop_reason) = response_body.get("stop_reason").and_then(|v| v.as_str()) {
            if stop_reason == "content_filter" {
                return Err(AgentError::ContentFiltered {
                    reason: "Content was filtered by Anthropic".to_string(),
                });
            }
        }

        // Parse content and usage
        let content = self.parse_response(&response_body)?;
        let usage = self.parse_usage(&response_body);

        Ok(LlmResponse::new(content, usage))
    }
}
