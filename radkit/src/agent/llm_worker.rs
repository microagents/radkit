//! LLM worker with toolset support and typed responses.
//!
//! This module provides [`LlmWorker`], a high-level abstraction for calling
//! LLMs with tool support. `LlmWorker` wraps an LLM client, manages toolsets,
//! handles tool execution loops, and deserializes responses into typed values.
//!
//! # Overview
//!
//! - [`LlmWorker<T>`]: Worker for LLM calls with tool execution and typed responses
//! - [`LlmWorkerBuilder<T>`]: Builder for constructing workers with multiple toolsets
//!

use std::collections::HashMap;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::structured::{
    build_structured_output_toolset, extract_structured_output, STRUCTURED_OUTPUT_TOOL_NAME,
};
use crate::errors::{AgentError, AgentResult};
use crate::models::{BaseLlm, ContentPart, Event, Thread};
use crate::tools::{
    BaseTool, BaseToolset, CombinedToolset, DefaultExecutionState, SimpleToolset, ToolContext,
    ToolResponse,
};
use crate::{compat::MaybeSend, compat::MaybeSync};

const DEFAULT_MAX_TOOL_ITERATIONS: usize = 20;

/// Worker for executing LLM calls with tool support and typed responses.
///
/// `LlmWorker<T>` manages the full lifecycle of LLM interactions including:
/// - Sending messages to the LLM
/// - Executing tool calls requested by the LLM
/// - Managing multi-turn tool execution loops
/// - Combining multiple toolsets
/// - Deserializing responses into typed values
///
/// # Type Parameters
///
/// * `T` - The type to deserialize the LLM response into. Must implement `DeserializeOwned`.
///        Use `Thread` if you want the raw conversation thread back without deserialization.
///
/// # Examples
///
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct MyResponse {
///     answer: String,
///     confidence: f64,
/// }
///
/// let worker = LlmWorker::<MyResponse>::builder(my_llm)
///     .with_tool(tool1)
///     .build();
///
/// let response: MyResponse = worker.run(thread).await?;
/// ```
///
/// Use [`LlmWorker::builder(model)`] to construct instances with the desired configuration.
pub struct LlmWorker<T> {
    model: Arc<dyn BaseLlm>,
    system_instructions: Option<String>,
    toolset: Option<Arc<dyn BaseToolset>>,
    max_iterations: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> LlmWorker<T>
where
    T: DeserializeOwned + JsonSchema + MaybeSend + MaybeSync + 'static,
{
    /// Creates a new builder for constructing an `LlmWorker<T>`.
    ///
    /// The model is required and must be provided upfront.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The response type to deserialize into
    ///
    /// # Arguments
    ///
    /// * `model` - The LLM model to use for this worker
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Response { answer: String }
    ///
    /// let worker = LlmWorker::<Response>::builder(my_llm)
    ///     .with_tool(tool1)
    ///     .build();
    /// ```
    pub fn builder(model: impl BaseLlm + 'static) -> LlmWorkerBuilder<T> {
        LlmWorkerBuilder::new(model)
    }

    /// Runs the worker on the given input thread.
    ///
    /// This method executes the LLM with the provided thread, handling any
    /// tool calls requested by the LLM. The worker will continue executing
    /// tools in a loop until the LLM produces a final response, which is then
    /// deserialized into type `T`.
    ///
    /// # Arguments
    ///
    /// * `input` - Thread or any type that can be converted into a Thread
    ///
    /// # Returns
    ///
    /// Returns the deserialized response of type `T`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The LLM call fails
    /// - Tool execution fails
    /// - Response deserialization fails
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::Thread;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct WeatherInfo {
    ///     temp: f64,
    ///     condition: String,
    /// }
    ///
    /// let thread = Thread::new().with_user("What's the weather?");
    /// let response: WeatherInfo = worker.run(thread).await?;
    /// println!("Temperature: {}°F", response.temp);
    /// ```
    pub async fn run<IT>(&self, input: IT) -> AgentResult<T>
    where
        IT: Into<Thread>,
    {
        let thread = self.apply_defaults(input.into());
        let outcome = self.execute(thread).await?;
        Ok(outcome.value)
    }

    /// Runs the worker and returns both the deserialized result and the thread for follow-up work.
    ///
    /// This method executes the LLM with the provided thread, handling tool calls
    /// and executing them in a loop. After completion, it returns both the deserialized
    /// response and the updated thread, allowing for multi-turn conversations.
    ///
    /// # Arguments
    ///
    /// * `input` - Thread or any type that can be converted into a Thread
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - The deserialized response of type `T`
    /// - The updated `Thread` with all tool calls and responses included
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The LLM call fails
    /// - Tool execution fails
    /// - Response deserialization fails
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::Thread;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Response { answer: String }
    ///
    /// let thread = Thread::new().with_user("What's the weather?");
    /// let (response, continued_thread) = worker.run_and_continue(thread).await?;
    /// println!("Answer: {}", response.answer);
    ///
    /// // Continue the conversation with the same thread
    /// let continued_thread = continued_thread
    ///     .with_user("What about tomorrow?");
    /// let (next_response, _) = worker.run_and_continue(continued_thread).await?;
    /// ```
    pub async fn run_and_continue<IT>(&self, input: IT) -> AgentResult<(T, Thread)>
    where
        IT: Into<Thread>,
    {
        let thread = self.apply_defaults(input.into());
        let outcome = self.execute(thread).await?;
        Ok((outcome.value, outcome.thread))
    }

    /// Checks if this worker has any toolsets configured.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if worker.has_tools() {
    ///     println!("Worker can execute tools");
    /// }
    /// ```
    #[must_use] pub fn has_tools(&self) -> bool {
        self.toolset.is_some()
    }

    /// Returns a reference to the configured toolset, if any.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if let Some(toolset) = worker.toolset() {
    ///     let tools = toolset.get_tools().await;
    ///     println!("Worker has {} tools", tools.len());
    /// }
    /// ```
    #[must_use] pub fn toolset(&self) -> Option<&Arc<dyn BaseToolset>> {
        self.toolset.as_ref()
    }

    /// Applies default system instructions to the thread if configured.
    fn apply_defaults(&self, thread: Thread) -> Thread {
        if let Some(default_system) = &self.system_instructions {
            thread.with_system(default_system.clone())
        } else {
            thread
        }
    }

    async fn execute(&self, thread: Thread) -> AgentResult<WorkerOutcome<T>> {
        let toolset = self.prepare_toolset()?;
        let tool_cache = load_tool_map(&toolset).await?;

        let execution_state = DefaultExecutionState::new();
        let tool_context = ToolContext::builder()
            .with_state(&execution_state)
            .build()
            .map_err(|err| AgentError::ToolSetupFailed {
                tool_name: "tool_context".to_string(),
                reason: err.to_string(),
            })?;

        let result = self
            .run_tool_loop(
                thread,
                Arc::clone(&toolset),
                &tool_cache,
                &tool_context,
                self.max_iterations,
            )
            .await;

        toolset.close().await;

        result
    }

    fn prepare_toolset(&self) -> AgentResult<Arc<dyn BaseToolset>> {
        let structured = build_structured_output_toolset::<T>()?;

        if let Some(user_toolset) = &self.toolset {
            let combined: Arc<dyn BaseToolset> =
                Arc::new(CombinedToolset::new(Arc::clone(user_toolset), structured));
            Ok(combined)
        } else {
            Ok(structured)
        }
    }

    async fn run_tool_loop(
        &self,
        mut thread: Thread,
        toolset: Arc<dyn BaseToolset>,
        tool_cache: &HashMap<String, Arc<dyn BaseTool>>,
        tool_context: &ToolContext<'_>,
        max_iterations: usize,
    ) -> AgentResult<WorkerOutcome<T>> {
        let mut iterations = 0usize;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(AgentError::Internal {
                    component: "llm_worker".to_string(),
                    reason: "Exceeded tool interaction iterations".to_string(),
                });
            }

            let response = self
                .model
                .generate_content(thread.clone(), Some(Arc::clone(&toolset)))
                .await?;

            let content = response.into_content();

            let tool_calls: Vec<_> = content
                .parts()
                .iter()
                .filter_map(|part| match part {
                    ContentPart::ToolCall(call) => Some(call.clone()),
                    _ => None,
                })
                .collect();

            let actionable_calls: Vec<_> = tool_calls
                .into_iter()
                .filter(|call| call.name() != STRUCTURED_OUTPUT_TOOL_NAME)
                .collect();

            if actionable_calls.is_empty() {
                let (value, assistant_content) = extract_structured_output::<T>(content)?;

                if let Some(content) = assistant_content {
                    thread = thread.add_event(Event::assistant(content));
                }

                return Ok(WorkerOutcome { value, thread });
            }

            thread = thread.add_event(Event::assistant(content));

            for call in actionable_calls {
                let tool = tool_cache
                    .get(call.name())
                    .ok_or_else(|| AgentError::ToolNotFound {
                        tool_name: call.name().to_string(),
                    })?
                    .clone();

                let args = value_to_arguments(call.name(), call.arguments())?;

                let result = tool.run_async(args, tool_context).await;
                let response = ToolResponse::new(call.id().to_string(), result);
                thread = thread.add_event(Event::from(response));
            }
        }
    }
}

struct WorkerOutcome<T> {
    value: T,
    thread: Thread,
}

async fn load_tool_map(
    toolset: &Arc<dyn BaseToolset>,
) -> AgentResult<HashMap<String, Arc<dyn BaseTool>>> {
    let tools = toolset.get_tools().await;
    let mut map = HashMap::with_capacity(tools.len());

    for tool in tools {
        map.insert(tool.name().to_string(), tool);
    }

    Ok(map)
}

fn value_to_arguments(tool_name: &str, value: &Value) -> AgentResult<HashMap<String, Value>> {
    match value {
        Value::Null => Ok(HashMap::new()),
        Value::Object(map) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        _ => Err(AgentError::ToolValidationError {
            tool_name: tool_name.to_string(),
            reason: "Tool arguments must be a JSON object".to_string(),
        }),
    }
}

/// Builder for constructing [`LlmWorker<T>`] instances.
///
/// The builder requires an LLM model upfront and allows configuring:
/// - System instructions
/// - Individual tools (via `with_tool()`)
/// - Multiple toolsets (automatically combined)
///
/// When building, individual tools are collected into a [`SimpleToolset`] and
/// combined with any provided toolsets.
///
/// # Type Parameters
///
/// * `T` - The response type to deserialize LLM responses into
///
/// # Examples
///
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct MyResponse { answer: String }
///
/// let worker = LlmWorker::<MyResponse>::builder(my_llm)
///     .with_system_instructions("You are a helpful assistant")
///     .with_tool(tool1)         // Add individual tools
///     .with_tool(tool2)
///     .with_toolset(toolset1)   // Add full toolsets
///     .with_toolset(toolset2)
///     .build();
/// ```
pub struct LlmWorkerBuilder<T> {
    model: Arc<dyn BaseLlm>,
    system_instructions: Option<String>,
    tools: Vec<Arc<dyn BaseTool>>,
    toolsets: Vec<Arc<dyn BaseToolset>>,
    max_iterations: usize,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> LlmWorkerBuilder<T>
where
    T: DeserializeOwned + JsonSchema + MaybeSend + MaybeSync + 'static,
{
    /// Creates a new builder with the required model.
    ///
    /// The model is required and must be provided upfront. Use
    /// [`LlmWorker::builder(model)`](LlmWorker::builder) instead of calling this directly.
    ///
    /// # Arguments
    ///
    /// * `model` - Any type implementing the `BaseLlm` trait
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = LlmWorker::<MyResponse>::builder(my_llm_client);
    /// ```
    pub fn new(model: impl BaseLlm + 'static) -> Self {
        Self {
            model: Arc::new(model) as Arc<dyn BaseLlm>,
            system_instructions: None,
            tools: Vec::new(),
            toolsets: Vec::new(),
            max_iterations: DEFAULT_MAX_TOOL_ITERATIONS,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Sets the default system instructions for this worker.
    ///
    /// These instructions will be prepended to all threads passed to the worker.
    ///
    /// # Arguments
    ///
    /// * `instructions` - System instructions text
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = LlmWorkerBuilder::new()
    ///     .with_system_instructions("You are a helpful assistant");
    /// ```
    pub fn with_system_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.system_instructions = Some(instructions.into());
        self
    }

    /// Adds an individual tool to the worker.
    ///
    /// This method can be called multiple times to add multiple tools. All individual
    /// tools will be collected into a [`SimpleToolset`] when the worker is built,
    /// and then combined with any toolsets added via [`with_toolset`](Self::with_toolset).
    ///
    /// # Arguments
    ///
    /// * `tool` - A tool to add to the worker
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use radkit::tools::{FunctionTool, BaseTool};
    ///
    /// let weather_tool = Arc::new(FunctionTool::new(
    ///     "get_weather",
    ///     "Get weather info",
    ///     |args, _| Box::pin(async { ToolResult::success(json!({"temp": 72})) })
    /// )) as Arc<dyn BaseTool>;
    ///
    /// let builder = LlmWorker::builder(my_llm)
    ///     .with_tool(weather_tool);
    /// ```
    pub fn with_tool(mut self, tool: Arc<dyn BaseTool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Adds multiple individual tools at once.
    ///
    /// This is a convenience method for adding multiple tools in a single call.
    /// The tools will be combined with any previously added tools.
    ///
    /// # Arguments
    ///
    /// * `tools` - Iterator of tools to add
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = LlmWorker::builder(my_llm)
    ///     .with_tools(vec![tool1, tool2, tool3]);
    /// ```
    pub fn with_tools<I>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn BaseTool>>,
    {
        self.tools.extend(tools);
        self
    }

    /// Adds a toolset to the worker.
    ///
    /// This method can be called multiple times. When multiple toolsets are added,
    /// they will be automatically combined using [`CombinedToolset`] when the
    /// worker is built.
    ///
    /// # Arguments
    ///
    /// * `toolset` - A toolset to add to the worker
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use radkit::tools::SimpleToolset;
    ///
    /// let toolset1 = Arc::new(SimpleToolset::new(vec![tool1]));
    /// let toolset2 = Arc::new(SimpleToolset::new(vec![tool2]));
    ///
    /// let builder = LlmWorker::builder(my_llm)
    ///     .with_toolset(toolset1)
    ///     .with_toolset(toolset2);  // Automatically combined
    /// ```
    pub fn with_toolset(mut self, toolset: Arc<dyn BaseToolset>) -> Self {
        self.toolsets.push(toolset);
        self
    }

    /// Overrides the maximum number of tool iterations the worker will execute before failing.
    #[must_use] pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations.max(1);
        self
    }

    /// Adds multiple toolsets at once.
    ///
    /// This is a convenience method for adding multiple toolsets in a single call.
    /// The toolsets will be combined with any previously added toolsets.
    ///
    /// # Arguments
    ///
    /// * `toolsets` - Iterator of toolsets to add
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = LlmWorker::builder(my_llm)
    ///     .with_toolsets(vec![toolset1, toolset2, toolset3]);
    /// ```
    pub fn with_toolsets<I>(mut self, toolsets: I) -> Self
    where
        I: IntoIterator<Item = Arc<dyn BaseToolset>>,
    {
        self.toolsets.extend(toolsets);
        self
    }

    /// Builds the [`LlmWorker<T>`] instance.
    ///
    /// This method performs the following:
    /// 1. If individual tools were added via [`with_tool`](Self::with_tool), creates
    ///    a [`SimpleToolset`] containing all those tools
    /// 2. Combines this `SimpleToolset` with any toolsets added via
    ///    [`with_toolset`](Self::with_toolset) using [`CombinedToolset`]
    /// 3. If multiple toolsets exist, chains them together with `CombinedToolset`
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Response { data: String }
    ///
    /// // Worker with individual tools only
    /// let worker = LlmWorker::<Response>::builder(my_llm)
    ///     .with_tool(tool1)
    ///     .with_tool(tool2)
    ///     .build();
    ///
    /// // Worker with mix of tools and toolsets
    /// let worker = LlmWorker::<Response>::builder(my_llm)
    ///     .with_tool(tool1)         // Individual tools
    ///     .with_toolset(toolset1)   // Full toolset
    ///     .with_tool(tool2)         // More individual tools
    ///     .build();
    /// ```
    #[must_use] pub fn build(self) -> LlmWorker<T> {
        // Start with all provided toolsets
        let mut all_toolsets = self.toolsets;

        // If we have individual tools, create a SimpleToolset and add it to the list
        if !self.tools.is_empty() {
            let simple_toolset = Arc::new(SimpleToolset::new(self.tools)) as Arc<dyn BaseToolset>;
            all_toolsets.push(simple_toolset);
        }

        // Combine all toolsets (including the SimpleToolset from individual tools)
        let combined_toolset = match all_toolsets.len() {
            0 => None,
            1 => Some(all_toolsets.into_iter().next().unwrap()),
            _ => {
                // Combine all toolsets using CombinedToolset
                let mut iter = all_toolsets.into_iter();
                let first = iter.next().unwrap();
                let combined = iter.fold(first, |acc, toolset| {
                    Arc::new(CombinedToolset::new(acc, toolset)) as Arc<dyn BaseToolset>
                });
                Some(combined)
            }
        };

        LlmWorker {
            model: self.model,
            system_instructions: self.system_instructions,
            toolset: combined_toolset,
            max_iterations: self.max_iterations,
            _phantom: std::marker::PhantomData,
        }
    }
}
