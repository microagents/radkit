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
    let agent = Agent::new(
        "MyFirstAgent".to_string(),
        "A helpful AI assistant".to_string(),
        "You are a knowledgeable and friendly assistant.".to_string(),
        llm,
    );
    
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
async fn create_llm_request(&self, context: &ProjectedExecutionContext) -> LlmRequest {
    // Get the full session with ALL events across ALL tasks
    let session = self.agent.session_service()
        .get_session(&context.app_name, &context.user_id, &context.context_id)
        .await?;

    // Convert session events to Content messages for LLM
    let content_messages = self
        .convert_events_to_content_messages(&session.events, &context.task_id)
        .await?;

    // The LLM sees the ENTIRE conversation history reconstructed from events
    LlmRequest::with_messages(
        content_messages,               // All content from session events
        context.task_id.clone(),        // Current task for highlighting  
        context.context_id.clone(),     // Maps to A2A contextId
    )
}

// Events to Content conversion preserves:
// - User messages (MessageReceived with User role)
// - Agent responses (MessageReceived with Agent role)  
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

let agent = Agent::new(
    "builtin_agent".to_string(),
    "Agent with built-in tools".to_string(),
    "You can update task status and save artifacts using built-in tools.".to_string(),
    llm,
)
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
use radkit::tools::{FunctionTool, ToolResult};
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

let agent = Agent::new(
    "tool_agent".to_string(),
    "Agent with custom tools".to_string(),
    "You are a helpful assistant. Use the available tools when requested by the user.".to_string(),
    llm,
)
.with_config(config)
.with_tools(tools);
```

### 5. Monitoring Tool Calls and Responses

Tool calls and responses are captured as internal events and can be monitored in real-time:

```rust
use radkit::events::InternalEvent;
use radkit::models::content::ContentPart;
use futures::StreamExt;

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

// Monitor tool execution via internal events
let mut internal_events = execution.internal_events;
tokio::spawn(async move {
    while let Some(event) = internal_events.recv().await {
        match event {
            InternalEvent::MessageReceived { content, metadata, .. } => {
                // Check for function calls and responses in the content
                for part in &content.parts {
                    match part {
                        ContentPart::FunctionCall { name, arguments, .. } => {
                            println!("🔧 Tool Call: {}", name);
                            println!("   Arguments: {}", serde_json::to_string_pretty(arguments).unwrap_or_default());
                        }
                        ContentPart::FunctionResponse { name, success, result, error_message, duration_ms, .. } => {
                            let status = if *success { "✓" } else { "✗" };
                            println!("⚙️  Tool Response: {} {}", name, status);
                            if *success {
                                println!("   Result: {}", serde_json::to_string_pretty(result).unwrap_or_default());
                            } else if let Some(error) = error_message {
                                println!("   Error: {}", error);
                            }
                            if let Some(duration) = duration_ms {
                                println!("   Duration: {}ms", duration);
                            }
                        }
                        ContentPart::Text { text, .. } => {
                            if content.role == MessageRole::Agent {
                                println!("💬 Agent: {}", text);
                            }
                        }
                        _ => {}
                    }
                }
            }
            InternalEvent::StateChange { key, new_value, scope, .. } => {
                println!("📝 State changed: {} = {} (scope: {:?})", key, new_value, scope);
            }
        }
    }
});

// Process A2A events normally
let mut final_task = None;
while let Some(event) = execution.stream.next().await {
    match event {
        SendStreamingMessageResult::Message(msg) => {
            // Agent's response with tool results integrated
            for part in &msg.parts {
                if let Part::Text { text, .. } = part {
                    print!("{}", text);
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

// Tool calls are also persisted in session events
if let Some(task) = final_task {
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await?
        .expect("Session should exist");

    println!("\n📋 Persisted Tool Calls in Session:");
    for event in &session.events {
        if let InternalEvent::MessageReceived { content, .. } = event {
            for part in &content.parts {
                match part {
                    ContentPart::FunctionCall { name, arguments, .. } => {
                        println!("  🔧 Stored Tool Call: {} with {:?}", name, arguments);
                    }
                    ContentPart::FunctionResponse { name, success, result, .. } => {
                        println!("  ⚙️ Stored Tool Response: {} (success: {})", name, success);
                        if *success {
                            println!("     Result: {:?}", result);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
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

// Monitor tool failures
while let Some(event) = internal_events.recv().await {
    if let InternalEvent::MessageReceived { content, .. } = event {
        for part in &content.parts {
            if let ContentPart::FunctionResponse { name, success, error_message, .. } = part {
                if !success {
                    println!("❌ Tool '{}' failed: {}", name, 
                        error_message.as_deref().unwrap_or("Unknown error"));
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
// Get complete response with all events captured
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

// Use streaming to capture A2A events in real-time
let mut execution = agent.send_streaming_message(
    "my_app".to_string(),
    "user123".to_string(),
    params
).await?;

let mut status_updates = 0;
let mut artifacts = 0;
let mut final_task = None;

// Process events as they arrive
while let Some(event) = execution.stream.next().await {
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

// Process internal events for debugging
let mut internal_events = Vec::new();
while let Ok(event) = execution.internal_events.try_recv() {
    internal_events.push(event);
}
println!("Captured {} internal events", internal_events.len());
```

## Event Monitoring

Both streaming and non-streaming modes capture events:

```rust
// Access internal events for debugging
for event in &response.internal_events {
    use radkit::events::InternalEvent;
    use radkit::models::content::ContentPart;
    
    match event {
        InternalEvent::MessageReceived { content, metadata, .. } => {
            // Monitor tool calls and responses
            for part in &content.parts {
                match part {
                    ContentPart::FunctionCall { name, arguments, .. } => {
                        println!("🔧 Function Call: {} with args: {:?}", name, arguments);
                    }
                    ContentPart::FunctionResponse { name, success, result, error_message, duration_ms, .. } => {
                        println!("⚙️ Function Response: {} (success: {})", name, success);
                        if let Some(error) = error_message {
                            println!("   Error: {}", error);
                        }
                        if let Some(duration) = duration_ms {
                            println!("   Duration: {}ms", duration);
                        }
                    }
                    ContentPart::Text { text, .. } => {
                        if content.role == MessageRole::Agent {
                            println!("💬 Agent response: {}", text.chars().take(50).collect::<String>());
                        }
                    }
                    _ => {}
                }
            }
            
            // Check if this message has performance metadata
            if let Some(perf) = &metadata.performance {
                println!("⏱️  Performance: {}ms", perf.duration_ms);
            }
            
            // Check if this message has model metadata
            if let Some(model_info) = &metadata.model_info {
                println!("🤖 Model: {} (tokens: {}→{})", 
                    model_info.model_name,
                    model_info.prompt_tokens.unwrap_or(0),
                    model_info.response_tokens.unwrap_or(0)
                );
            }
        }
        InternalEvent::StateChange { key, new_value, scope, .. } => {
            println!("📝 State changed: {} = {} (scope: {:?})", key, new_value, scope);
        }
    }
}

// Access A2A protocol events
println!("A2A events: {}", response.a2a_events.len());
```

## Complete Example: Math Tutor Agent

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::AnthropicLlm;
use radkit::events::InternalEvent;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    
    // Create a specialized math tutor agent
    let llm = Arc::new(AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY")?,
    ));
    
    let agent = Agent::new(
        "MathTutor".to_string(),
        "An AI math tutor that explains concepts step-by-step".to_string(),
        r#"You are a patient math tutor. When solving problems:
        1. Break down the problem into steps
        2. Explain each step clearly
        3. Use the save_artifact tool to save the solution
        4. Update your status as you work through the problem"#.to_string(),
        llm,
    )
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
    
    // Analyze performance from captured events
    let mut total_time = 0u64;
    let mut model_calls = 0;
    
    for event in &response.internal_events {
        match event {
            InternalEvent::MessageReceived { metadata, .. } => {
                if let Some(perf) = &metadata.performance {
                    total_time += perf.duration_ms;
                }
                if metadata.model_info.is_some() {
                    model_calls += 1;
                }
            }
            _ => {}
        }
    }
    println!("⏱️ Total processing time: {}ms across {} model calls", total_time, model_calls);
    
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