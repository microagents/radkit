//! Common test utilities and mocks for radkit tests.

use radkit::compat::Mutex;
use radkit::errors::AgentResult;
use radkit::models::{BaseLlm, Content, LlmResponse, Thread, TokenUsage};
use radkit::tools::{BaseTool, BaseToolset};
use radkit::{MaybeSend, MaybeSync};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Mock LLM implementation for testing that returns predefined responses.
pub struct MockLlm {
    pub model_name: String,
    pub responses: Arc<Mutex<Vec<Content>>>,
    pub call_count: Arc<Mutex<usize>>,
}

impl MockLlm {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            responses: Arc::new(Mutex::new(Vec::new())),
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn with_response(self, content: impl Into<Content>) -> Self {
        // Store responses in a Vec that we'll access during generate_content
        // We can't use blocking_lock in test context, so we build the responses upfront
        let mut responses = Vec::new();

        // Get existing responses
        #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
        {
            // Try to get lock without blocking
            if let Ok(existing) = self.responses.try_lock() {
                responses.extend(existing.iter().cloned());
            }
        }

        responses.push(content.into());

        Self {
            model_name: self.model_name,
            responses: Arc::new(Mutex::new(responses)),
            call_count: self.call_count,
        }
    }

    pub async fn call_count(&self) -> usize {
        *self.call_count.lock().await
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseLlm for MockLlm {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    async fn generate_content(
        &self,
        _thread: Thread,
        _toolset: Option<Arc<dyn BaseToolset>>,
    ) -> AgentResult<LlmResponse> {
        let mut count = self.call_count.lock().await;
        *count += 1;

        let mut responses = self.responses.lock().await;

        let content = if responses.is_empty() {
            Content::from_text("Mock response")
        } else {
            responses.remove(0)
        };

        let usage = TokenUsage::new(10, 20, 30);
        Ok(LlmResponse::new(content, usage))
    }
}

/// Mock tool implementation for testing.
pub struct MockTool {
    pub name: String,
    pub description: String,
    pub call_count: Arc<Mutex<usize>>,
    pub response: Value,
}

impl MockTool {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            call_count: Arc::new(Mutex::new(0)),
            response: json!({"success": true}),
        }
    }

    pub fn with_response(mut self, response: Value) -> Self {
        self.response = response;
        self
    }

    pub async fn call_count(&self) -> usize {
        *self.call_count.lock().await
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for MockTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> radkit::tools::FunctionDeclaration {
        radkit::tools::FunctionDeclaration::new(
            self.name.clone(),
            self.description.clone(),
            json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                }
            }),
        )
    }

    async fn run_async(
        &self,
        _args: HashMap<String, Value>,
        _context: &radkit::tools::ToolContext<'_>,
    ) -> radkit::tools::ToolResult {
        let mut count = self.call_count.lock().await;
        *count += 1;
        radkit::tools::ToolResult::success(self.response.clone())
    }
}
