//! Shared fixtures and helpers for radkit tests.
//!
//! These utilities are intentionally lightweight and WASI-friendly so they can be
//! reused across native and `wasip1` test targets without pulling in additional
//! dependencies. They are available when the `test-support` feature is enabled
//! or when running tests, to keep the public surface minimal while giving tests
//! convenient fakes.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::errors::{AgentError, AgentResult};
use crate::models::{BaseLlm, Content, LlmResponse, Thread, TokenUsage};
use crate::tools::{BaseTool, BaseToolset, FunctionDeclaration, ToolContext, ToolResult};
use serde::Serialize;
use serde_json::Value;

/// A simple LLM implementation that returns pre-seeded responses.
///
/// Tests can seed responses up-front and verify the inputs captured by inspecting
/// [`FakeLlm::calls`]. When responses are exhausted the fake surfaces an internal
/// error so missing expectations are obvious.
#[derive(Clone)]
pub struct FakeLlm {
    model_name: String,
    responses: Arc<Mutex<VecDeque<AgentResult<LlmResponse>>>>,
    calls: Arc<Mutex<Vec<Thread>>>,
}

impl FakeLlm {
    /// Creates a fake LLM that dequeues the provided responses.
    #[must_use]
    pub fn with_responses<I>(model_name: impl Into<String>, responses: I) -> Self
    where
        I: IntoIterator<Item = AgentResult<LlmResponse>>,
    {
        Self {
            model_name: model_name.into(),
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Pushes an additional response to the back of the queue.
    pub fn push_response(&self, response: AgentResult<LlmResponse>) {
        let mut guard = self
            .responses
            .lock()
            .expect("fake LLM responses mutex poisoned");
        guard.push_back(response);
    }

    /// Returns the threads the fake has been asked to process so far.
    #[must_use]
    pub fn calls(&self) -> Vec<Thread> {
        self.calls
            .lock()
            .expect("fake LLM calls mutex poisoned")
            .clone()
    }

    /// Creates a successful LLM response from plain text for convenience.
    #[must_use]
    pub fn text_response(text: impl Into<String>) -> AgentResult<LlmResponse> {
        Ok(LlmResponse::new(
            Content::from_text(text),
            TokenUsage::empty(),
        ))
    }

    /// Creates a successful response from the provided content.
    #[must_use]
    pub fn content_response(content: Content) -> AgentResult<LlmResponse> {
        Ok(LlmResponse::new(content, TokenUsage::empty()))
    }

    /// Returns the number of times the fake model has been invoked.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls
            .lock()
            .expect("fake LLM calls mutex poisoned")
            .len()
    }
}

impl Default for FakeLlm {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            responses: Arc::new(Mutex::new(VecDeque::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseLlm for FakeLlm {
    fn model_name(&self) -> &str {
        if self.model_name.is_empty() {
            "fake-llm"
        } else {
            &self.model_name
        }
    }

    async fn generate_content(
        &self,
        thread: Thread,
        _toolset: Option<Arc<dyn BaseToolset>>,
    ) -> AgentResult<LlmResponse> {
        self.calls
            .lock()
            .expect("fake LLM calls mutex poisoned")
            .push(thread);

        let mut guard = self
            .responses
            .lock()
            .expect("fake LLM responses mutex poisoned");

        guard.pop_front().unwrap_or_else(|| {
            Err(AgentError::Internal {
                component: "FakeLlm".to_string(),
                reason: "No more fake responses queued".to_string(),
            })
        })
    }
}

/// Convenience helper for building a [`Thread`] with a single user message.
#[must_use]
pub fn user_thread(text: impl Into<String>) -> Thread {
    Thread::from_user(text)
}

/// Convenience helper for building [`Content`] that wraps plain text.
#[must_use]
pub fn text_content(text: impl Into<String>) -> Content {
    Content::from_text(text)
}

/// Creates a structured output response for testing LlmWorker/LlmFunction.
///
/// This helper generates an [`LlmResponse`] with text-based JSON output wrapped in
/// markdown code blocks, matching the tryparse-based structured output pattern used
/// by `extract_structured_output()`.
///
/// # Arguments
///
/// * `value` - The value to serialize as JSON and wrap in the response
///
/// # Panics
///
/// Panics if serialization fails. This is acceptable in test code as it indicates
/// a test setup error that should be fixed immediately.
///
/// # Examples
///
/// ```ignore
/// use radkit::test_support::structured_response;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct Result { value: i32 }
///
/// let response = structured_response(&Result { value: 42 });
/// ```
#[must_use]
pub fn structured_response<T: Serialize>(value: &T) -> LlmResponse {
    // SAFETY: unwrap is acceptable in test helpers - serialization failures indicate
    // test setup bugs that should fail fast. All types in tests implement Serialize.
    let json_str = serde_json::to_string_pretty(value)
        .expect("Test value serialization failed - check Serialize implementation");

    LlmResponse::new(
        Content::from_text(format!("```json\n{}\n```", json_str)),
        TokenUsage::empty(),
    )
}

/// Simple tool implementation that records invocations for assertions.
///
/// This tool can be cloned cheaply because it uses `Arc` for interior state.
/// This allows passing one clone to a worker while keeping another to inspect calls.
#[derive(Clone)]
pub struct RecordingTool {
    name: String,
    description: String,
    results: Arc<Mutex<VecDeque<ToolResult>>>,
    calls: Arc<Mutex<Vec<HashMap<String, Value>>>>,
}

impl RecordingTool {
    /// Creates a recording tool with either a fixed result or a queue of results.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        results: impl Into<VecDeque<ToolResult>>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            results: Arc::new(Mutex::new(results.into())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the number of times the tool was invoked.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.calls
            .lock()
            .expect("recording tool calls mutex poisoned")
            .len()
    }

    /// Returns the captured argument list.
    #[must_use]
    pub fn calls(&self) -> Vec<HashMap<String, Value>> {
        self.calls
            .lock()
            .expect("recording tool calls mutex poisoned")
            .clone()
    }
}

impl Default for RecordingTool {
    fn default() -> Self {
        let mut results = VecDeque::new();
        results.push_back(ToolResult::success(Value::Null));
        Self::new("recording_tool", "Records invocations", results)
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for RecordingTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> FunctionDeclaration {
        FunctionDeclaration::new(
            self.name.clone(),
            self.description.clone(),
            serde_json::json!({"type": "object"}),
        )
    }

    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        _context: &ToolContext<'_>,
    ) -> ToolResult {
        self.calls
            .lock()
            .expect("recording tool calls mutex poisoned")
            .push(args);

        self.results
            .lock()
            .expect("recording tool results mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| ToolResult::success(Value::Null))
    }
}
