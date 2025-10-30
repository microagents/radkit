# Library Architecture: `radkit`

This document specifies the internal architecture of the open-source Rust library. The library's design is centered around providing a clean, scalable, and powerful developer experience.

## Error Handling

The library uses a unified `AgentError` enum defined in `radkit/src/errors.rs` for all error conditions. The type alias `AgentResult<T>` represents `Result<T, AgentError>` and is used throughout the codebase.

Key error categories include:
- **LLM errors**: `LlmProvider`, `LlmAuthentication`, `LlmRateLimit`, `ContentFiltered`, `ContextLengthExceeded`
- **Task/Session errors**: `TaskNotFound`, `SessionNotFound`, `InvalidTaskStateTransition`
- **Skill errors**: `SkillNotFound`, `InvalidInput`, `MissingInput`
- **Runtime errors**: `Internal`, `Network`, `Serialization`

All skill handlers and runtime services return `AgentResult<T>`, ensuring consistent error propagation and handling across the agent execution pipeline.

## 1. The `Runtime` Abstraction

The core of the library's architecture is the `Runtime` object. It is the container for all platform-level services and acts as the primary dependency injection mechanism. This approach avoids a bloated builder API and provides a clean, scalable way to manage the agent's environment.

### 1.1. The `Runtime` Trait

The library will define a `Runtime` trait that outlines the services an agent can rely on:

```rust
pub trait Runtime {
    fn task_manager(&self) -> Arc<dyn TaskManager>;
    fn memory_service(&self) -> Arc<dyn MemoryService>;
    fn logging_service(&self) -> Arc<dyn LoggingService>;
    // ... other services
}
```

### 1.2. Provided Runtimes

The library and its associated cloud platform will provide ways to satisfy this requirement:

1.  **`DefaultRuntime`**: An out-of-the-box implementation of the `Runtime` trait that works for both native and `wasm32-wasip1` targets. It uses conditional compilation to provide in-memory services for local development and testing, and delegates to the host environment when running as WASM.

2.  **`CustomRuntime` (User-Defined)**: Advanced users can implement the `Runtime` trait themselves to integrate with their own infrastructure (e.g., their own databases, logging systems, or vector stores).

3.  **`PaidRuntime` (Cloud-Native)**: Closed source, created when deployed via the CLI. A production-grade implementation provided by the cloud platform. It also automatically streams all logs, traces, and task updates to the cloud UI.

## 2. The `AgentDefinition`

The `Agent::builder()` does not create a running agent. It creates a declarative, serializable `AgentDefinition` struct. This struct is the blueprint for an agent, containing its ID, name, dispatcher prompt, and a list of its skills. This definition is the artifact that gets deployed.

## 3. The `SkillHandler` Trait

To provide the cleanest separation of logic, skills are defined by implementing the `SkillHandler` trait. The main "happy path" logic lives in the `on_request` handler, and more complex, multi-turn logic continues by overriding the `on_input_received` handler.

Skills are annotated with the `#[skill]` macro to provide A2A protocol metadata for automatic AgentCard generation and MIME type validation.

### 1. Handler Method Roles

-   `on_request()`: This is the **"Attempt" handler** and is the only required method. It is the primary entry point for a new task and should contain the full "happy path" logic. If it succeeds, it returns a `Completed` state. If it needs more information, it saves its partial work and returns an `InputRequired` state.

-   `on_input_received()`: This is the **"Continue" handler**. It has a default implementation and only needs to be overridden if the skill can enter an `InputRequired` state. Its job is to take the new user input, load the partial state saved by `on_request`, and continue the logic from where it left off.

-   **Other Lifecycle Hooks** (`on_completed()`, etc.): The trait can also provide optional, fire-and-forget hooks with empty default implementations for side effects like logging.

### 2. State Transition Enums

The handlers return explicit enums to control the interaction's outcome. This allows a skill to either respond with a stateless `Message` for simple interactions or initiate a stateful `Task` by returning a variant that maps directly to an A2A protocol task state. This provides compile-time safety and supports all agent types (message-only, task-generating, and hybrid).

```rust
// Return type for on_request
pub enum OnRequestResult {
    // NEW: Respond with a stateless A2A Message
    Message {
        message: Content,
    },

    // --- The following variants initiate a stateful A2A Task ---

    // Maps to A2A TaskState::InputRequired
    InputRequired {
        message: Content,
        slot: SkillSlot,
    },
    // Maps to A2A TaskState::Completed
    Completed {
        message: Option<Content>,
        artifacts: Vec<Artifact>,
    },
    // Maps to A2A TaskState::Failed
    Failed {
        error: String,
    },
    // Maps to A2A TaskState::Rejected
    Rejected {
        reason: String,
    },
}

// Return type for on_input_received
pub enum OnInputResult {
    // Can ask for input again if the user's response was invalid.
    InputRequired {
        message: Content,
        slot: SkillSlot,
    },
    // Completed after receiving input
    Completed {
        message: Option<Content>,
        artifacts: Vec<Artifact>,
    },
    // Failed to process input
    Failed {
        error: String,
    },
}
```

### 3. Intermediate Updates and Partial Artifacts

The `TaskContext` provides methods for sending intermediate updates during skill execution. These methods ensure automatic A2A compliance by restricting what developers can send and when.

#### 3.1. TaskContext Methods

```rust
pub struct TaskContext {
    // State storage for multi-turn conversations
    state_storage: HashMap<String, serde_json::Value>,
    // Internal channel for streaming updates (framework-managed)
    update_sender: Option<UpdateSender>,
}

impl TaskContext {
    /// Send an intermediate status update during execution.
    /// - Always uses TaskState::Working
    /// - Always sets final: false
    /// - Framework converts to A2A TaskStatusUpdateEvent
    pub async fn send_intermediate_update(&mut self, message: impl Into<Content>) -> AgentResult<()>;

    /// Send a partial artifact during execution.
    /// - Can be called multiple times
    /// - Artifacts are not marked as final
    /// - Framework converts to A2A TaskArtifactUpdateEvent
    pub async fn send_partial_artifact(&mut self, artifact: Artifact) -> AgentResult<()>;

    /// Save data for multi-turn conversations
    pub fn save_data<T: Serialize>(&mut self, key: &str, value: &T) -> AgentResult<()>;

    /// Load previously saved data
    pub fn load_data<T: DeserializeOwned>(&self, key: &str) -> AgentResult<Option<T>>;
}
```

#### 3.2. Return Types Control Final State

Only the return type of `on_request()` and `on_input_received()` can set terminal task states:

- **`InputRequired`** → A2A TaskState::InputRequired (terminal, final=true)
- **`Completed`** → A2A TaskState::Completed (terminal, final=true)
- **`Failed`** → A2A TaskState::Failed (terminal, final=true)
- **`Rejected`** → A2A TaskState::Rejected (terminal, final=true)

The `Completed` variant is the **only** way to send final artifacts. Artifacts in the return value are marked as final in the A2A protocol.

### 4. The `#[skill]` Macro

Skills use the `#[skill]` macro to declare A2A protocol metadata. This metadata is used to automatically generate the AgentCard and validate MIME types.

```rust
#[skill(
    id = "summarize_resume",
    name = "Resume Summarizer",
    description = "Extracts structured data from resumes and validates GitHub profiles",
    tags = ["hr", "resume", "data-extraction"],
    examples = [
        "Summarize this resume: John Doe, john@example.com, github.com/johndoe",
        "Extract information from the attached resume PDF"
    ],
    input_modes = ["text/plain", "application/pdf", "application/json"],
    output_modes = ["application/json"]
)]
pub struct SummarizeResumeSkill;
```

The macro generates:
- Implementation of `SkillMetadata` trait for AgentCard generation
- Automatic MIME type validation based on `input_modes`
- A2A protocol compliance checks

### 5. Example Skill Implementation

```rust
use radkit::prelude::*;

// The library provides the SkillHandler trait with default implementations.
#[async_trait]
pub trait SkillHandler: Send + Sync {
    // This method is required.
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,  // radkit::models::Content
    ) -> AgentResult<OnRequestResult>;

    // This method is optional.
    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnInputResult> {
        Ok(OnInputResult::Failed {
            error: "This skill does not support multi-turn input".into()
        })
    }
}

// Define data structures for the skill
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UserData {
    pub name: String,
    pub email: String,
    pub github_username: String,
}

// Define input slot enum for multi-turn conversations
#[derive(Serialize, Deserialize)]
pub enum ResumeInputSlot {
    CorrectedUsername,
}

// The developer implements the trait for their skill struct.
#[skill(
    id = "summarize_resume",
    name = "Resume Summarizer",
    description = "Extracts user data from resumes",
    tags = ["hr", "resume"],
    examples = ["Summarize this resume: John Doe..."],
    input_modes = ["text/plain"],
    output_modes = ["application/json"]
)]
pub struct SummarizeResumeSkill;

#[async_trait]
impl SkillHandler for SummarizeResumeSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        // Extract text from content
        let resume_text = content.first_text()
            .ok_or_else(|| AgentError::MissingInput("No text content in message".to_string()))?;

        // TODO: Get LLM instance (design pending - could be from skill init, config, etc.)
        let llm = todo!("Need to provide LLM to skill");

        // Use LLM function helper
        let user_data = extract_user_data(llm)
            .run(resume_text)
            .await?;

        // Check if we have all required data
        if user_data.github_username.is_empty() {
            // Save partial state and request input
            task_context.save_data("partial_user", &user_data)?;

            return Ok(OnRequestResult::InputRequired {
                message: Content::from_text("Please provide your GitHub username."),
                slot: SkillSlot::new(ResumeInputSlot::CorrectedUsername),
            });
        }

        // Create FINAL artifact (sent only in return type)
        let artifact = Artifact::from_json("user_profile.json", &user_data)?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Successfully extracted user profile")),
            artifacts: vec![artifact],  // FINAL artifacts only
        })
    }

    // This override is only needed for skills that handle user input.
    async fn on_input_received(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnInputResult> {
        // Load the partial state saved by on_request
        let mut user_data: UserData = task_context
            .load_data("partial_user")?
            .ok_or_else(|| AgentError::ContextError("No partial user data found".to_string()))?;

        // Get the new input
        let username = content.first_text()
            .ok_or_else(|| AgentError::MissingInput("No username provided".to_string()))?;

        user_data.github_username = username.to_string();

        // Create the final artifact
        let artifact = Artifact::from_json("user_profile.json", &user_data)?;

        Ok(OnInputResult::Completed {
            message: Some(Content::from_text("Profile completed!")),
            artifacts: vec![artifact],
        })
    }
}
```

### 6. Example: Skill with Intermediate Updates

This example demonstrates sending intermediate status updates and partial artifacts during execution:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AnalysisReport {
    pub summary: String,
    pub charts: Vec<Chart>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Analysis {
    pub data_points: Vec<DataPoint>,
    pub insights: Vec<String>,
}

#[skill(
    id = "generate_report",
    name = "Report Generator",
    description = "Generates comprehensive analysis reports with charts",
    tags = ["reports", "analysis"],
    examples = ["Generate Q4 financial report"],
    input_modes = ["text/plain", "application/json"],
    output_modes = ["application/json"]
)]
pub struct ReportGeneratorSkill;

#[async_trait]
impl SkillHandler for ReportGeneratorSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let llm = runtime.llm_provider().default_llm()?;

        // Send intermediate status update (TaskState::Working, final=false)
        task_context.send_intermediate_update("Analyzing data...").await?;

        // Step 1: Analyze data
        let analysis = analyze_data()
            .with_llm(llm.clone())
            .run(content.first_text().unwrap())
            .await?;

        // Send partial artifact (not final)
        let partial_artifact = Artifact::from_json("analysis.json", &analysis)?;
        task_context.send_partial_artifact(partial_artifact).await?;

        // Another intermediate status update
        task_context.send_intermediate_update("Generating charts...").await?;

        // Step 2: Generate charts
        let charts = generate_charts()
            .with_llm(llm.clone())
            .run(&analysis)
            .await?;

        // Send another partial artifact
        let charts_artifact = Artifact::from_json("charts.json", &charts)?;
        task_context.send_partial_artifact(charts_artifact).await?;

        // Final status update
        task_context.send_intermediate_update("Compiling final report...").await?;

        // Step 3: Compile final report
        let final_report = compile_report()
            .with_llm(llm)
            .run(&analysis, &charts)
            .await?;

        // Return FINAL state and FINAL artifacts
        let final_artifact = Artifact::from_json("final_report.json", &final_report)?;

        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Report generation complete!")),
            artifacts: vec![final_artifact],  // FINAL artifact
        })
    }
}
```

**Key points:**
- `send_intermediate_update()` - Always sends TaskState::Working with final=false
- `send_partial_artifact()` - Sends artifacts during execution (not final)
- Return type - Controls final state (Completed, Failed, InputRequired, Rejected)
- Artifacts in return - These are the FINAL artifacts

This design ensures A2A compliance at compile time - developers cannot send incorrect states or mark intermediate updates as final.

## 5. Callable Helpers

To simplify common patterns while adhering to Rust's principle of explicitness, the library provides generic "callable structs" instead of macros on empty functions. A developer defines a helper by writing a function that constructs and returns a configured instance of one of these structs.

**Key principle:** Skills can use multiple LLM functions and workers as needed, treating LLMs as composable library functions rather than monolithic entities.

These were explaind in [01_library_primitives.md](01_library_primitives.md)
### 5.1. `LlmFunction<Out>`

For abstracting LLM calls for structured extraction or generation without tool execution.

### 5.2. `LlmWorker<Out>`

For skills that require tool execution and multi-turn loops in addition to structured outputs.


### 5.3. Multiple LLM Calls Per Skill

A key feature of radkit is that skills can compose multiple LLM functions and workers. This allows developers to treat LLMs as library functions and build complex workflows:

```rust
#[async_trait]
impl SkillHandler for ComplexSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> AgentResult<OnRequestResult> {
        let llm = runtime.llm_provider().default_llm()?;

        // Step 1: Extract data (LlmFunction)
        let data = extract_data()
            .with_llm(llm.clone())
            .run(input)
            .await?;

        // Step 2: Validate (another LlmFunction)
        let is_valid = validate_data()
            .with_llm(llm.clone())
            .run(&data)
            .await?;

        // Step 3: Enrich with external data (LlmWorker with tools)
        let enriched = enrich_with_tools()
            .with_llm(llm.clone())
            .run(&data)
            .await?;

        // Step 4: Generate final output (another LlmWorker)
        let result = generate_output()
            .with_llm(llm)
            .run(&enriched)
            .await?;

        let artifact = Artifact::from_json("result.json", &result)?;
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Processing complete")),
            artifacts: vec![artifact],
        })
    }
}
```