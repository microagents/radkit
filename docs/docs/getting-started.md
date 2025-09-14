# Getting Started with Radkit

This guide will help you get up and running with Radkit, the A2A-native agent development kit for Rust.

## Prerequisites

- **Rust**: Version 1.70 or higher
- **Cargo**: Rust's package manager
- **API Keys**: For LLM providers (Anthropic Claude, Google Gemini)

## Installation

Add these dependencies to your `Cargo.toml`:

```toml
[dependencies]
radkit = "0.0.1"
futures = "0.3.31"
tokio = "1.47.1"
uuid = "1.18.0"
dotenvy = "0.15.7"
```

## Setting Up Your First Agent

### Step 1: Environment Configuration

Create a `.env` file in your project root:

```bash
# For Anthropic Claude
ANTHROPIC_API_KEY=sk-ant-your-key-here

# For Google Gemini
GEMINI_API_KEY=your-gemini-key-here
```

### Step 2: Basic Agent Creation

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::Agent;
use radkit::models::AnthropicLlm;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenvy::dotenv().ok();
    
    // Create an LLM provider
    let llm = Arc::new(AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY")?,
    ));
    
    // Create an agent (services are created automatically)
    let agent = Agent::builder(
        "You are a knowledgeable and friendly assistant.".to_string(),
        anthropic_llm,
    )
    .with_card(|c| c.with_name("MyFirstAgent".to_string()).with_description("A helpful AI assistant".to_string()))
    .build();
    
    println!("✅ Agent created successfully!");
    
    // Your agent is ready to use!
    Ok(())
}
```

### Step 3: Sending Your First Message

```rust
// Helper function to create a user message
fn create_user_message(text: &str) -> Message {
    Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: text.to_string(),
            metadata: None,
        }],
        context_id: None,  // Will create a new session
        task_id: None,     // Will create a new task
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    }
}

// Create message parameters
let params = MessageSendParams {
    message: create_user_message("Hello! What can you help me with today?"),
    configuration: None,
    metadata: None,
};

// Send the message (non-streaming)
let response = agent.send_message(
    "my_app".to_string(),      // Application name
    "user123".to_string(),     // User ID
    params,
).await?;

// Process the response
match response.result {
    SendMessageResult::Task(task) => {
        println!("✅ Task created: {}", task.id);
        println!("Context ID: {}", task.context_id);
        println!("Status: {:?}", task.status.state);
        
        // Get the agent's response
        for message in &task.history {
            if message.role == MessageRole::Agent {
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        println!("Agent: {}", text);
                    }
                }
            }
        }
    }
    SendMessageResult::Message(msg) => {
        println!("Received direct message response");
    }
}
```

## Understanding Core Concepts

### 1. Multi-Tenancy

Radkit provides built-in multi-tenant isolation:

```rust
// Each request requires app_name and user_id
agent.send_message(
    "app_name".to_string(),   // Isolates different applications
    "user_id".to_string(),    // Isolates different users
    params,
).await?;
```

This ensures complete data isolation between different applications and users.

### 2. Tasks and Sessions

Every agent interaction creates or continues:
- **Task**: A unit of work with history and artifacts
- **Session**: A conversation context (maps to A2A contextId)

```rust
// Continue an existing conversation in the same session
let mut follow_up_message = create_user_message("Do you remember what we were talking about?");
follow_up_message.context_id = Some("existing_context_id".to_string());
// task_id remains None to create a new task in the same session

let params = MessageSendParams {
    message: follow_up_message,
    configuration: None,
    metadata: None,
};

let response = agent.send_message("my_app".to_string(), "user123".to_string(), params).await?;
```

#### How Conversations are Built from Session Events

**Key Architecture**: Radkit builds conversations dynamically from `session.events` rather than storing static message history. This enables:

- **Cross-Task Memory**: Agents remember conversations across multiple tasks in the same session
- **Event-Driven Conversation**: All interactions (messages, tool calls, state changes) are stored as events
- **A2A Context Mapping**: Session ID directly maps to A2A `contextId` for protocol compliance

```rust
// Example: How the agent reconstructs conversation context
async fn create_llm_request(&self, context: &ExecutionContext) -> LlmRequest {
    // Get conversation from EventProcessor which reconstructs from session events
    let content_messages = context.get_llm_conversation().await?;

    // The LLM sees the ENTIRE conversation history reconstructed from events
    LlmRequest {
        messages: content_messages,     // All content from session events
        current_task_id: context.task_id.clone(),
        context_id: context.context_id.clone(),   // Maps to A2A contextId
        system_instruction: Some(self.agent.instruction().to_string()),
        config: GenerateContentConfig::default(),
        toolset: self.agent.toolset().cloned(),
        metadata: context.current_params.metadata.clone().unwrap_or_default(),
    }
}

// SessionEvent to Content conversion preserves:
// - User messages (SessionEventType::UserMessage)
// - Agent responses (SessionEventType::AgentMessage)  
// - Function calls and responses (ContentPart within messages)
// - All metadata and context information
```

This means when you continue a conversation:
1. **Same `context_id`** = Agent remembers everything from previous tasks
2. **New `context_id`** = Fresh conversation with no memory
3. **Events are the source of truth** = No separate message storage needed

### 3. Built-in Tools

Enable task management capabilities:

```rust
use radkit::agents::AgentConfig;

let config = AgentConfig::default().with_max_iterations(10);

let agent = Agent::builder(
        "You can update task status and save artifacts using built-in tools.".to_string(),
        anthropic_llm,
    )
    .with_card(|c| c.with_name("builtin_agent".to_string()).with_description("Agent with built-in tools".to_string()))
    .build()
.with_config(config)
.with_builtin_task_tools();  // Adds update_status and save_artifact tools

// The agent can now use:
// - update_status: Update task status (submitted, working, completed, failed, etc.)
// - save_artifact: Save analysis results, files, or any generated content
// These tools automatically emit A2A-compliant events
```

### 4. Custom Function Tools

Create your own tools using `FunctionTool`:

```rust
use radkit::tools::{FunctionTool, ToolResult, ToolContext};
use serde_json::{json, Value};
use std::collections::HashMap;

// Create a weather tool
fn create_weather_tool() -> FunctionTool {
    FunctionTool::new(
        "get_weather".to_string(),
        "Get the current weather for a location".to_string(),
        |args: HashMap<String, Value>, _context| Box::pin(async move {
            let location = args
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let weather_data = json!({
                "location": location,
                "temperature": "72°F",
                "conditions": "Partly cloudy",
                "humidity": "45%"
            });

            ToolResult {
                success: true,
                data: weather_data,
                error_message: None,
            }
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "description": "The city and state, e.g. San Francisco, CA"
            }
        },
        "required": ["location"]
    }))
}

// Example tool that uses ToolContext for state management
fn create_preference_tool() -> FunctionTool {
    FunctionTool::new(
        "set_preference".to_string(),
        "Set user preferences".to_string(),
        |args: HashMap<String, Value>, context| Box::pin(async move {
            let key = args
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let value = args
                .get("value")
                .cloned()
                .unwrap_or(json!(null));

            // Use the new ToolContext API for state management
            match context.set_user_state(key.to_string(), value.clone()).await {
                Ok(()) => ToolResult::success(json!({
                    "message": format!("Set preference '{}' to: {}", key, value)
                })),
                Err(e) => ToolResult::error(format!("Failed to set preference: {}", e))
            }
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "key": {
                "type": "string",
                "description": "Preference key"
            },
            "value": {
                "description": "Preference value (any JSON type)"
            }
        },
        "required": ["key", "value"]
    }))
}

// Create a calculator tool
fn create_calculator_tool() -> FunctionTool {
    FunctionTool::new(
        "calculate".to_string(),
        "Perform basic mathematical calculations".to_string(),
        |args: HashMap<String, Value>, _context| Box::pin(async move {
            let expression = args
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let result = match expression {
                "2+2" => 4,
                "10*5" => 50,
                "100/4" => 25,
                "15-3" => 12,
                _ => {
                    return ToolResult {
                        success: false,
                        data: json!(null),
                        error_message: Some(format!("Cannot calculate: {}", expression)),
                    };
                }
            };

            ToolResult {
                success: true,
                data: json!({"result": result, "expression": expression}),
                error_message: None,
            }
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "expression": {
                "type": "string",
                "description": "Mathematical expression to evaluate (e.g., '2+2', '10*5')"
            }
        },
        "required": ["expression"]
    }))
}

// Add tools to your agent
let tools: Vec<Arc<dyn radkit::tools::BaseTool>> = vec![
    Arc::new(create_weather_tool()),
    Arc::new(create_calculator_tool()),
];

let agent = Agent::builder(
        "You are a helpful assistant. Use the available tools when requested by the user.".to_string(),
        anthropic_llm,
    )
    .with_card(|c| c.with_name("tool_agent".to_string()).with_description("Agent with custom tools".to_string()))
    .build()
.with_config(config)
.with_tools(tools);
```

### 5. Monitoring Tool Calls and Responses

Tool calls and responses are captured in session events and can be monitored in real-time via streaming:

```rust
use futures::StreamExt;
use radkit::a2a::SendStreamingMessageResult;

// Send a message that will trigger tool usage
let message = create_user_message(
    "What's the weather like in San Francisco? Please use the get_weather tool."
);

let params = MessageSendParams {
    message,
    configuration: None,
    metadata: None,
};

// Use streaming to monitor tool calls in real-time
let mut execution = agent.send_streaming_message(
    "test_app".to_string(),
    "test_user".to_string(),
    params
).await?;

let mut final_task = None;

// Process A2A events and monitor tool responses
while let Some(event) = execution.a2a_stream.next().await {
    match event {
        SendStreamingMessageResult::Message(msg) => {
            // Agent's response includes tool results
            println!("💬 Agent (role={:?}):", msg.role);
            for part in &msg.parts {
                if let Part::Text { text, .. } = part {
                    println!("  {}", text);
                }
            }
        }
        SendStreamingMessageResult::Task(task) => {
            final_task = Some(task);
            break;
        }
        _ => {}
    }
}

// Tool calls are persisted in session events with detailed information
if let Some(task) = final_task {
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await?
        .expect("Session should exist");

    println!("\n📋 Session Events Summary:");
    println!("  Total events: {}", session.events.len());
    
    // Count different types of content in session events
    let mut tool_calls = 0;
    let mut tool_responses = 0;
    
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        tool_calls += 1;
                        println!("  🔧 Tool Call: {}", name);
                    }
                    radkit::models::content::ContentPart::FunctionResponse { name, success, .. } => {
                        tool_responses += 1;
                        println!("  ⚙️ Tool Response: {} (success: {})", name, success);
                    }
                    _ => {}
                }
            }
        }
    }
    
    println!("  Tool calls: {}, Tool responses: {}", tool_calls, tool_responses);
}
```

### 6. Tool Error Handling

Handle tool failures gracefully:

```rust
// Create a tool that sometimes fails for demonstration
let error_tool = FunctionTool::new(
    "risky_operation".to_string(),
    "An operation that might fail".to_string(),
    |args: HashMap<String, Value>, _context| Box::pin(async move {
        let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("");

        if input == "fail" {
            ToolResult {
                success: false,
                data: json!(null),
                error_message: Some("Operation failed as requested".to_string()),
            }
        } else {
            ToolResult::success(json!({ "result": "Operation succeeded" }))
        }
    }),
).with_parameters_schema(json!({
    "type": "object",
    "properties": {
        "input": {"type": "string", "description": "Test input"}
    },
    "required": ["input"]
}));

// Monitor tool failures through session events after completion
let response = agent.send_message(app, user, params).await?;

if let SendMessageResult::Task(task) = response.result {
    // Check session events for tool failures
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await?
        .expect("Session should exist");

    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionResponse { 
                    name, success, error_message, .. 
                } = part {
                    if !success {
                        println!("❌ Tool '{}' failed: {}", name, 
                            error_message.as_deref().unwrap_or("Unknown error"));
                    }
                }
            }
        }
    }
}
```

## Streaming vs Non-Streaming

### Non-Streaming (Complete Response)

Best for: Simple queries, batch processing, testing

```rust
let execution = agent.send_message(app, user, params).await?;

// Access the complete response
if let SendMessageResult::Task(task) = execution.result {
    println!("Task completed: {}", task.id);
}

// Access ALL events including function calls/responses
println!("Total events captured: {}", execution.all_events.len());

for event in &execution.all_events {
    match &event.event_type {
        radkit::sessions::SessionEventType::UserMessage { content } |
        radkit::sessions::SessionEventType::AgentMessage { content } => {
            // Check for function calls and responses
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, arguments, .. } => {
                        println!("🔧 Function called: {} with args: {:?}", name, arguments);
                    }
                    radkit::models::content::ContentPart::FunctionResponse { 
                        name, success, result, error_message, duration_ms, .. 
                    } => {
                        println!("⚙️ Function {} returned (success: {})", name, success);
                        if *success {
                            println!("   Result: {:?}", result);
                        } else if let Some(err) = error_message {
                            println!("   Error: {}", err);
                        }
                        if let Some(ms) = duration_ms {
                            println!("   Duration: {}ms", ms);
                        }
                    }
                    radkit::models::content::ContentPart::Text { text, .. } => {
                        if content.role == MessageRole::Agent {
                            println!("💬 Agent: {}", text);
                        }
                    }
                    _ => {}
                }
            }
        }
        radkit::sessions::SessionEventType::TaskStatusChanged { new_state, .. } => {
            println!("📊 Task status: {:?}", new_state);
        }
        radkit::sessions::SessionEventType::ArtifactSaved { artifact } => {
            println!("💾 Artifact saved: {:?}", artifact.name);
        }
        _ => {}
    }
}

// Access A2A protocol events only (filtered subset)
println!("A2A events: {}", execution.a2a_events.len());
```

### Streaming (Real-Time Response)

Best for: Interactive chat, progress updates, long-running tasks

```rust
use futures::StreamExt;
use radkit::a2a::{SendStreamingMessageResult, TaskState};

// Create a message that will trigger tool usage
let message = create_user_message(
    "Please update the task status to 'working' and save a config file as an artifact."
);

let params = MessageSendParams {
    message,
    configuration: None,
    metadata: None,
};

// Use streaming to capture events in real-time
let mut execution = agent.send_streaming_message(
    "my_app".to_string(),
    "user123".to_string(),
    params
).await?;

// Option 1: Monitor A2A protocol events (for UI updates)
let mut status_updates = 0;
let mut artifacts = 0;
let mut final_task = None;

// Process A2A events as they arrive
while let Some(event) = execution.a2a_stream.next().await {
    match event {
        SendStreamingMessageResult::Message(msg) => {
            // Real-time message content
            for part in &msg.parts {
                if let Part::Text { text, .. } = part {
                    print!("{}", text);
                }
            }
        }
        SendStreamingMessageResult::TaskStatusUpdate(update) => {
            status_updates += 1;
            println!("✅ Status update: {:?} (final: {})", 
                update.status.state, update.is_final);
        }
        SendStreamingMessageResult::TaskArtifactUpdate(update) => {
            artifacts += 1;
            println!("✅ Artifact saved: {:?}", update.artifact.name);
        }
        SendStreamingMessageResult::Task(task) => {
            // Final complete task
            final_task = Some(task);
            break;
        }
    }
}

println!("Streaming completed: {} status updates, {} artifacts", status_updates, artifacts);

// Option 2: Also monitor ALL events stream for detailed debugging
// You can spawn a task to monitor all_events_stream in parallel
tokio::spawn(async move {
    let mut all_events_stream = execution.all_events_stream;
    
    while let Some(event) = all_events_stream.next().await {
        match &event.event_type {
            radkit::sessions::SessionEventType::UserMessage { content } |
            radkit::sessions::SessionEventType::AgentMessage { content } => {
                // Monitor function calls and responses in real-time
                for part in &content.parts {
                    match part {
                        radkit::models::content::ContentPart::FunctionCall { name, arguments, .. } => {
                            println!("🔧 [DEBUG] Function called: {} with {:?}", name, arguments);
                        }
                        radkit::models::content::ContentPart::FunctionResponse { 
                            name, success, result, duration_ms, .. 
                        } => {
                            println!("⚙️ [DEBUG] Function {} returned in {:?}ms (success: {})", 
                                name, duration_ms, success);
                            if *success {
                                println!("   [DEBUG] Result: {:?}", result);
                            }
                        }
                        _ => {}
                    }
                }
            }
            radkit::sessions::SessionEventType::TaskCreated { .. } => {
                println!("📋 [DEBUG] Task created");
            }
            radkit::sessions::SessionEventType::StateChanged { key, new_value, .. } => {
                println!("📝 [DEBUG] State changed: {} = {}", key, new_value);
            }
            _ => {}
        }
    }
});

// Access session events for debugging after completion
if let Some(task) = &final_task {
    let session_service = agent.session_service();
    let session = session_service
        .get_session("my_app", "user123", &task.context_id)
        .await?
        .expect("Session should exist");
    println!("Session has {} total events", session.events.len());
}
```

## Event Monitoring

Both streaming and non-streaming modes capture events in the session:

```rust
use radkit::models::content::ContentPart;

// Access session events after completion
let session_service = agent.session_service();
let session = session_service
    .get_session("test_app", "test_user", &task.context_id)
    .await?
    .expect("Session should exist");

// Monitor all activity in the session
println!("📋 Session Event Summary:");
println!("  Total events: {}", session.events.len());

for event in &session.events {
    match &event.event_type {
        radkit::sessions::SessionEventType::UserMessage { content } => {
            println!("👤 User message: {}", 
                content.parts.iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text, .. } => Some(text.chars().take(50).collect::<String>()),
                        _ => None
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        radkit::sessions::SessionEventType::AgentMessage { content } => {
            println!("🤖 Agent response");
            
            // Monitor tool calls and responses
            for part in &content.parts {
                match part {
                    ContentPart::FunctionCall { name, arguments, .. } => {
                        println!("  🔧 Function Call: {} with args: {:?}", name, arguments);
                    }
                    ContentPart::FunctionResponse { name, success, result, error_message, duration_ms, .. } => {
                        println!("  ⚙️ Function Response: {} (success: {})", name, success);
                        if let Some(error) = error_message {
                            println!("     Error: {}", error);
                        }
                        if let Some(duration) = duration_ms {
                            println!("     Duration: {}ms", duration);
                        }
                    }
                    ContentPart::Text { text, .. } => {
                        println!("  💬 Text: {}", text.chars().take(50).collect::<String>());
                    }
                    _ => {}
                }
            }
        }
        radkit::sessions::SessionEventType::TaskStatusChanged { new_state, .. } => {
            println!("📊 Task status changed to: {:?}", new_state);
        }
        radkit::sessions::SessionEventType::ArtifactSaved { artifact } => {
            println!("💾 Artifact saved: {:?}", artifact.name);
        }
        radkit::sessions::SessionEventType::StateChanged { key, new_value, scope, .. } => {
            println!("📝 State changed: {} = {} (scope: {:?})", key, new_value, scope);
        }
        _ => {}
    }
}
```

## Complete Example: Math Tutor Agent

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::AnthropicLlm;
use radkit::sessions::SessionEventType;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    
    // Create a specialized math tutor agent
    let llm = Arc::new(AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY")?,
    ));
    
    let agent = Agent::builder(
        r#"You are a patient math tutor. When solving problems:
        1. Break down the problem into steps
        2. Explain each step clearly
        3. Use the save_artifact tool to save the solution
        4. Update your status as you work through the problem"#.to_string(),
        anthropic_llm,
    )
    .with_card(|c| c.with_name("MathTutor".to_string()).with_description("An AI math tutor that explains concepts step-by-step".to_string()))
    .build()
    .with_config(AgentConfig::default().with_max_iterations(10))
    .with_builtin_task_tools();
    
    // Student asks a question
    let question = "A train travels 120 miles in 2 hours. What is its average speed?";
    
    let params = MessageSendParams {
        message: Message {
            kind: "message".to_string(),
            message_id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: question.to_string(),
                metadata: None,
            }],
            context_id: None,
            task_id: None,
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        },
        configuration: None,
        metadata: None,
    };
    
    println!("📚 Math Tutor Agent");
    println!("Student: {}\n", question);
    
    // Get the solution
    let response = agent.send_message(
        "math_tutor_app".to_string(),
        "student_001".to_string(),
        params,
    ).await?;
    
    // Display the response
    if let SendMessageResult::Task(task) = response.result {
        // Find the tutor's explanation
        for message in &task.history {
            if message.role == MessageRole::Agent {
                println!("Tutor:");
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        println!("{}", text);
                    }
                }
            }
        }
        
        // Check for saved solutions
        if !task.artifacts.is_empty() {
            println!("\n📝 Saved Solutions:");
            for artifact in &task.artifacts {
                if let Some(name) = &artifact.name {
                    println!("- {}", name);
                }
            }
        }
        
        println!("\n✅ Task Status: {:?}", task.status.state);
    }
    
    // Analyze session events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("math_tutor_app", "student_001", &task.context_id)
        .await?
        .expect("Session should exist");
    
    println!("📊 Session Analysis:");
    println!("  Total events: {}", session.events.len());
    
    let mut message_events = 0;
    let mut status_changes = 0;
    let mut artifacts = 0;
    
    for event in &session.events {
        match &event.event_type {
            radkit::sessions::SessionEventType::UserMessage { .. } | 
            radkit::sessions::SessionEventType::AgentMessage { .. } => {
                message_events += 1;
            }
            radkit::sessions::SessionEventType::TaskStatusChanged { .. } => {
                status_changes += 1;
            }
            radkit::sessions::SessionEventType::ArtifactSaved { .. } => {
                artifacts += 1;
            }
            _ => {}
        }
    }
    
    println!("  Messages: {}, Status changes: {}, Artifacts: {}", 
        message_events, status_changes, artifacts);
    
    Ok(())
}
```

## Error Handling

Always handle potential errors gracefully:

```rust
use radkit::errors::AgentError;

match agent.send_message(app, user, params).await {
    Ok(response) => {
        // Process successful execution
    }
    Err(AgentError::LlmRateLimit { provider }) => {
        println!("Rate limited by {}. Retrying in 60 seconds...", provider);
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Retry logic
    }
    Err(AgentError::SessionNotFound { session_id, .. }) => {
        println!("Session {} not found. Creating new session...", session_id);
        // Create new session
    }
    Err(e) => {
        eprintln!("Error: {}", e);
        // Generic error handling
    }
}
```

## Best Practices

1. **Always Use Multi-Tenancy**: Provide app_name and user_id for proper isolation
2. **Enable Built-in Tools**: Use `with_builtin_task_tools()` for task management
3. **Monitor Events**: Use captured events for debugging and observability
4. **Handle Errors**: Implement proper error handling and retry logic
5. **Use Streaming for Interactive Apps**: Better user experience with real-time responses
6. **Set Appropriate Timeouts**: Configure max_iterations to prevent infinite loops
7. **Secure API Keys**: Never hardcode API keys; use environment variables

## Next Steps

- [Tasks Guide](./tasks.md) - Understand task lifecycle
- [Sessions Guide](./sessions.md) - Learn about state management