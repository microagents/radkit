//! Google Gemini LLM provider implementation.
//!
//! API Documentation: <https://ai.google.dev/api/generate-content>
//! Model Names: <https://ai.google.dev/gemini-api/docs/models/gemini>
//! Pricing: <https://ai.google.dev/pricing>

use std::sync::Arc;

use serde_json::{json, Value};

use crate::errors::{AgentError, AgentResult};
use crate::models::{BaseLlm, Content, ContentPart, LlmResponse, Role, Thread, TokenUsage};
use crate::tools::{BaseToolset, ToolCall};

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/";

/// Google Gemini LLM implementation.
///
/// Provides access to Gemini models through the Google AI API.
/// Supports text generation, multi-modal inputs (images, documents), and tool use.
///
/// # Authentication
///
/// The API key can be provided explicitly or loaded from the `GEMINI_API_KEY`
/// environment variable via [`from_env`](GeminiLlm::from_env).
///
/// # Model Selection
///
/// Common model names:
/// - `gemini-2.0-flash-exp` - Gemini 2.0 Flash Experimental
/// - `gemini-1.5-pro` - Gemini 1.5 Pro
/// - `gemini-1.5-flash` - Gemini 1.5 Flash
///
/// # Examples
///
/// ```ignore
/// use radkit::models::providers::GeminiLlm;
/// use radkit::models::{BaseLlm, Thread};
///
/// // From environment variable
/// let llm = GeminiLlm::from_env("gemini-2.0-flash-exp")?;
///
/// // With explicit API key
/// let llm = GeminiLlm::new("gemini-2.0-flash-exp", "api-key");
///
/// // Generate content
/// let thread = Thread::from_user("Explain quantum computing");
/// let response = llm.generate_content(thread, None).await?;
/// println!("{}", response.content().first_text().unwrap_or("No response"));
/// ```
pub struct GeminiLlm {
    model_name: String,
    api_key: String,
    base_url: String,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

impl GeminiLlm {
    /// Environment variable name for the Gemini API key.
    pub const API_KEY_ENV: &str = "GEMINI_API_KEY";

    /// Creates a new Gemini LLM instance with explicit API key.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The model to use (e.g., "gemini-2.0-flash-exp")
    /// * `api_key` - Google AI API key
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let llm = GeminiLlm::new("gemini-2.0-flash-exp", "api-key");
    /// ```
    pub fn new(model_name: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            api_key: api_key.into(),
            base_url: GEMINI_BASE_URL.to_string(),
            max_tokens: None,
            temperature: None,
        }
    }

    /// Creates a new Gemini LLM instance loading API key from environment.
    ///
    /// Reads the API key from the `GEMINI_API_KEY` environment variable.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The model to use (e.g., "gemini-2.0-flash-exp")
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is not set or is empty.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let llm = GeminiLlm::from_env("gemini-2.0-flash-exp")?;
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
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Sets the maximum number of tokens to generate.
    #[must_use] pub const fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Sets the temperature for generation (0.0 to 2.0).
    ///
    /// Higher values produce more random outputs.
    #[must_use] pub const fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Converts a Thread into Gemini API request format.
    async fn build_request_payload(
        &self,
        thread: Thread,
        toolset: Option<Arc<dyn BaseToolset>>,
    ) -> AgentResult<Value> {
        let (system_prompt, events) = thread.into_parts();

        // Build contents array (Gemini's message format)
        let mut contents = Vec::new();
        let mut system_parts = Vec::new();

        // Add system prompt if present
        if let Some(system) = system_prompt {
            system_parts.push(system);
        }

        for event in events {
            let role = event.role().clone();
            let content = event.into_content();

            match role {
                Role::System => {
                    // System messages go into systemInstruction
                    if let Some(text) = content.joined_texts() {
                        system_parts.push(text);
                    }
                }
                Role::User => {
                    // User role maps to "user" in Gemini
                    let mut parts = Vec::new();

                    for part in content {
                        match part {
                            ContentPart::Text(text) => {
                                parts.push(json!({"text": text}));
                            }
                            ContentPart::Data(data) => {
                                let part = match data.source {
                                    crate::models::DataSource::Base64(b64) => {
                                        json!({
                                            "inline_data": {
                                                "mime_type": data.content_type,
                                                "data": b64
                                            }
                                        })
                                    }
                                    crate::models::DataSource::Uri(uri) => {
                                        json!({
                                            "fileData": {
                                                "mime_type": data.content_type,
                                                "fileUri": uri
                                            }
                                        })
                                    }
                                };
                                parts.push(part);
                            }
                            ContentPart::ToolCall(tool_call) => {
                                // Tool calls in user messages (for echoing back)
                                parts.push(json!({
                                    "functionCall": {
                                        "name": tool_call.name(),
                                        "args": tool_call.arguments()
                                    }
                                }));
                            }
                            ContentPart::ToolResponse(tool_response) => {
                                // Tool responses go as functionResponse
                                let result = tool_response.result();
                                let response_content = if result.is_success() {
                                    result.data().clone()
                                } else {
                                    json!({
                                        "error": result.error_message().unwrap_or("Unknown error")
                                    })
                                };

                                parts.push(json!({
                                    "functionResponse": {
                                        "name": tool_response.tool_call_id(),
                                        "response": {
                                            "name": tool_response.tool_call_id(),
                                            "content": response_content
                                        }
                                    }
                                }));
                            }
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(json!({
                            "role": "user",
                            "parts": parts
                        }));
                    }
                }
                Role::Assistant => {
                    // Assistant role maps to "model" in Gemini
                    let mut parts = Vec::new();

                    for part in content {
                        match part {
                            ContentPart::Text(text) => {
                                parts.push(json!({"text": text}));
                            }
                            ContentPart::ToolCall(tool_call) => {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": tool_call.name(),
                                        "args": tool_call.arguments()
                                    }
                                }));
                            }
                            _ => {} // Gemini doesn't support other types in assistant messages
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(json!({
                            "role": "model",
                            "parts": parts
                        }));
                    }
                }
                Role::Tool => {
                    // Tool role messages go as user messages with functionResponse
                    let mut parts = Vec::new();

                    for part in content {
                        match part {
                            ContentPart::ToolResponse(tool_response) => {
                                let result = tool_response.result();
                                let response_content = if result.is_success() {
                                    result.data().clone()
                                } else {
                                    json!({
                                        "error": result.error_message().unwrap_or("Unknown error")
                                    })
                                };

                                parts.push(json!({
                                    "functionResponse": {
                                        "name": tool_response.tool_call_id(),
                                        "response": {
                                            "name": tool_response.tool_call_id(),
                                            "content": response_content
                                        }
                                    }
                                }));
                            }
                            ContentPart::ToolCall(tool_call) => {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": tool_call.name(),
                                        "args": tool_call.arguments()
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(json!({
                            "role": "user",
                            "parts": parts
                        }));
                    }
                }
            }
        }

        // Build the base payload
        let mut payload = json!({
            "contents": contents
        });

        // Add systemInstruction if we have system messages
        if !system_parts.is_empty() {
            let system_text = system_parts.join("\n");
            payload["systemInstruction"] = json!({
                "parts": [{"text": system_text}]
            });
        }

        // Add generationConfig
        let mut generation_config = json!({});

        if let Some(temperature) = self.temperature {
            generation_config["temperature"] = json!(temperature);
        }

        if let Some(max_tokens) = self.max_tokens {
            generation_config["maxOutputTokens"] = json!(max_tokens);
        }

        if !generation_config.as_object().unwrap().is_empty() {
            payload["generationConfig"] = generation_config;
        }

        // Add tools if provided
        if let Some(toolset) = toolset {
            let tools_list = toolset.get_tools().await;
            if !tools_list.is_empty() {
                let function_declarations: Vec<Value> = tools_list
                    .iter()
                    .map(|tool| {
                        let decl = tool.declaration();
                        json!({
                            "name": decl.name(),
                            "description": decl.description(),
                            "parameters": decl.parameters()
                        })
                    })
                    .collect();

                payload["tools"] = json!([{
                    "function_declarations": function_declarations
                }]);
            }
        }

        Ok(payload)
    }

    /// Parses Gemini API response into Content.
    fn parse_response(&self, response_body: &Value) -> AgentResult<Content> {
        let mut content = Content::default();

        // Extract first candidate
        let candidates = response_body
            .get("candidates")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::LlmProvider {
                provider: "Gemini".to_string(),
                message: "Missing or invalid 'candidates' field in response".to_string(),
            })?;

        let first_candidate = candidates.first().ok_or_else(|| AgentError::LlmProvider {
            provider: "Gemini".to_string(),
            message: "Empty candidates array in response".to_string(),
        })?;

        // Extract parts from the candidate
        let parts = first_candidate
            .get("content")
            .and_then(|v| v.get("parts"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::LlmProvider {
                provider: "Gemini".to_string(),
                message: "Missing or invalid 'content.parts' in candidate".to_string(),
            })?;

        for part in parts {
            // Check for text content
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    content.push(ContentPart::Text(text.to_string()));
                }
            }

            // Check for function call
            if let Some(function_call) = part.get("functionCall") {
                let name = function_call
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgentError::LlmProvider {
                        provider: "Gemini".to_string(),
                        message: "Missing 'name' in functionCall".to_string(),
                    })?;

                let args = function_call.get("args").cloned().unwrap_or(Value::Null);

                // Gemini doesn't provide call_id, so use name as id
                content.push(ContentPart::ToolCall(ToolCall::new(name, name, args)));
            }
        }

        Ok(content)
    }

    /// Parses token usage from Gemini API response.
    ///
    /// **Important**: Gemini's `candidatesTokenCount` does NOT include `thoughtsTokenCount`.
    /// We must add them together to get total completion tokens (OpenAI-style normalization).
    fn parse_usage(&self, response_body: &Value) -> TokenUsage {
        let usage_obj = match response_body.get("usageMetadata") {
            Some(obj) => obj,
            None => return TokenUsage::empty(),
        };

        let prompt_tokens = usage_obj
            .get("promptTokenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);

        // Extract candidates tokens and thoughts tokens separately
        let candidates_tokens = usage_obj
            .get("candidatesTokenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);

        let thoughts_tokens = usage_obj
            .get("thoughtsTokenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);

        // IMPORTANT: For Gemini, thoughtsTokenCount is NOT included in candidatesTokenCount
        // We must add them together to normalize to OpenAI-style completion_tokens
        let completion_tokens = match (candidates_tokens, thoughts_tokens) {
            (Some(c), Some(t)) => Some(c + t),
            (Some(c), None) => Some(c),
            (None, Some(t)) => Some(t),
            (None, None) => None,
        };

        let total_tokens = usage_obj
            .get("totalTokenCount")
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32);

        TokenUsage::partial(prompt_tokens, completion_tokens, total_tokens)
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseLlm for GeminiLlm {
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

        // Build URL with model name
        let url = format!(
            "{}models/{}:generateContent",
            self.base_url, self.model_name
        );

        // Create HTTP client
        let client = reqwest::Client::new();

        // Make request - Gemini uses x-goog-api-key header
        let response = client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("Content-Type", "application/json")
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
                401 | 403 => AgentError::LlmAuthentication {
                    provider: "Gemini".to_string(),
                },
                429 => AgentError::LlmRateLimit {
                    provider: "Gemini".to_string(),
                },
                _ => AgentError::LlmProvider {
                    provider: "Gemini".to_string(),
                    message: format!("HTTP {status}: {error_body}"),
                },
            });
        }

        // Parse response
        let response_body: Value = response.json().await?;

        // Check for error in response body
        if let Some(error) = response_body.get("error") {
            return Err(AgentError::LlmProvider {
                provider: "Gemini".to_string(),
                message: format!("API error: {error}"),
            });
        }

        // Parse content and usage
        let content = self.parse_response(&response_body)?;
        let usage = self.parse_usage(&response_body);

        Ok(LlmResponse::new(content, usage))
    }
}
