# Task Management Guide

Tasks in Radkit represent units of work that agents perform. They follow the A2A protocol specification and provide comprehensive lifecycle management, artifact generation, and event streaming. This guide covers everything you need to know about working with tasks.

## Understanding Tasks

A **Task** is the fundamental unit of work in the A2A protocol:
- Contains a complete conversation history
- Tracks status through a defined lifecycle
- Stores generated artifacts (files, analysis, etc.)
- Emits A2A-compliant events for real-time monitoring
- Provides thread-safe atomic operations

## Task Lifecycle

Tasks progress through well-defined states:

```
┌──────────┐    ┌─────────┐    ┌──────────────┐    ┌───────────┐
│Submitted │ -> │Working  │ -> │InputRequired │ -> │Completed  │
└──────────┘    └─────────┘    └──────────────┘    └───────────┘
                     │              │                    ↑
                     │              ↓                    │
                     │         ┌──────────────┐         │
                     │         │AuthRequired  │ --------┘
                     │         └──────────────┘
                     │
                     ↓
             ┌─────────────────┐
             │Failed/Rejected  │
             │   /Canceled     │
             └─────────────────┘
```

### Task States

- **Submitted**: Initial state when task is created
- **Working**: Agent is actively processing the task
- **InputRequired**: Agent needs additional input from user
- **AuthRequired**: Agent needs authentication or permissions
- **Completed**: Task finished successfully
- **Failed**: Task failed due to error
- **Rejected**: Task was rejected (e.g., inappropriate request)
- **Canceled**: Task was canceled by user or system

## Creating and Managing Tasks

### Automatic Task Creation

Tasks are created automatically when you send a message without a `task_id`:

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::Agent;

let params = MessageSendParams {
    message: Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Analyze this data and create a report".to_string(),
            metadata: None,
        }],
        context_id: None,  // New session will be created
        task_id: None,     // New task will be created
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    },
    configuration: None,
    metadata: None,
};

let response = agent.send_message(
    "analytics_app".to_string(),
    "analyst_001".to_string(),
    params,
).await?;

if let SendMessageResult::Task(task) = response.result {
    println!("New task created: {}", task.id);
    println!("Initial status: {:?}", task.status.state);
}
```

### Continuing Existing Tasks

To add messages to an existing task:

```rust
// Continue working on an existing task
let mut params = MessageSendParams {
    message: Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please add more details to the analysis".to_string(),
            metadata: None,
        }],
        context_id: Some("existing_context_id".to_string()),
        task_id: Some("existing_task_id".to_string()),  // ← Continue this task
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    },
    configuration: None,
    metadata: None,
};

let execution = agent.send_message(app, user, params).await?;
```

### Direct Task Access

You can also work with tasks directly through the TaskManager:

```rust
// Access the task manager
let task_manager = agent.task_manager();

// Get a specific task
let task = task_manager.get_task(
    "my_app",
    "user123", 
    "task_id_here"
).await?;

if let Some(task) = task {
    println!("Task status: {:?}", task.status.state);
    println!("Messages in history: {}", task.history.len());
    println!("Artifacts: {}", task.artifacts.len());
}

// List all tasks for a user
let all_tasks = task_manager.list_tasks(
    "my_app",
    "user123",
    None  // No context filter
).await?;

println!("User has {} total tasks", all_tasks.len());

// List tasks for a specific session
let session_tasks = task_manager.list_tasks(
    "my_app",
    "user123",
    Some("session_id")  // Filter by context
).await?;
```

## Built-in Task Management Tools

Radkit provides built-in tools that agents can use to manage their own task lifecycle:

### Enabling Built-in Tools

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
.with_builtin_task_tools();  // Enables update_status and save_artifact
```

### The update_status Tool

This tool allows agents to control their task status:

```rust
// Agent can use this tool in their LLM calls
// The tool generates A2A TaskStatusUpdate events automatically

// Example tool usage by agent:
// {
//   "function_call": {
//     "name": "update_status",
//     "args": {
//       "status": "working",
//       "message": "Starting data analysis..."
//     }
//   }
// }
```

Available status values:
- `"submitted"`, `"working"`, `"input_required"`, `"auth_required"`
- `"completed"`, `"failed"`, `"rejected"`, `"canceled"`

### The save_artifact Tool

This tool allows agents to save important outputs:

```rust
// Agent can save artifacts during execution
// The tool generates A2A TaskArtifactUpdate events automatically

// Example tool usage by agent:
// {
//   "function_call": {
//     "name": "save_artifact",
//     "args": {
//       "name": "Data Analysis Report",
//       "content": "...",
//       "content_type": "text/markdown",
//       "description": "Comprehensive analysis of the provided dataset"
//     }
//   }
// }
```

## Working with Task Artifacts

Artifacts represent outputs generated by agents:

```rust
use radkit::a2a::{Artifact, Part, TaskQueryParams};

// Access artifacts from a completed task
let retrieved_task = agent.get_task(
    "app",
    "user", 
    TaskQueryParams {
        id: "task_id".to_string(),
        history_length: None,
        metadata: None,
    }
).await?;

for artifact in &retrieved_task.artifacts {
    println!("Artifact: {}", artifact.artifact_id);
    
    if let Some(name) = &artifact.name {
        println!("  Name: {}", name);
    }
    
    if let Some(desc) = &artifact.description {
        println!("  Description: {}", desc);
    }
    
    // Access content
    for part in &artifact.parts {
        match part {
            Part::Text { text, .. } => {
                println!("  Text content: {} chars", text.len());
            }
            Part::File { file, .. } => {
                println!("  File content: {:?}", file);
            }
            Part::Data { data, .. } => {
                println!("  Data content: {}", data);
            }
        }
    }
}
```

## Task Events and Monitoring

Tasks automatically generate A2A-compliant events:

### TaskStatusUpdate Events

Generated when task status changes:

```rust
// Monitor status updates in streaming mode
while let Some(event) = execution.stream.next().await {
    if let SendStreamingMessageResult::TaskStatusUpdate(update) = event {
        println!("Task {} status: {:?}", update.task_id, update.status.state);
        
        if update.is_final {
            println!("Task reached final state");
        }
        
        // Check for status message
        if let Some(msg) = &update.status.message {
            for part in &msg.parts {
                if let Part::Text { text, .. } = part {
                    println!("Status message: {}", text);
                }
            }
        }
    }
}
```

### TaskArtifactUpdate Events

Generated when artifacts are added:

```rust
// Monitor artifact updates
while let Some(event) = execution.stream.next().await {
    if let SendStreamingMessageResult::TaskArtifactUpdate(update) = event {
        println!("New artifact: {}", update.artifact.artifact_id);
        
        if let Some(name) = &update.artifact.name {
            println!("  Name: {}", name);
        }
        
        // Check if this is the final chunk
        if update.last_chunk == Some(true) {
            println!("  Artifact complete");
        }
    }
}
```

## Error Handling and Recovery

### Handling Failed Tasks

```rust
use ak_rust::errors::AgentError;

async fn handle_task_failure(
    agent: &Agent,
    app_name: &str,
    user_id: &str,
    failed_task_id: &str,
) -> Result<Task, Box<dyn std::error::Error>> {
    
    // Get the failed task
    let failed_task = agent.task_manager()
        .get_task(app_name, user_id, failed_task_id)
        .await?
        .ok_or("Task not found")?;
    
    // Analyze failure
    let failure_reason = failed_task.status.message.as_ref()
        .and_then(|msg| msg.parts.first())
        .and_then(|part| match part {
            Part::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Unknown failure".to_string());
    
    println!("Task failed: {}", failure_reason);
    
    // Create recovery task
    let recovery_prompt = format!(
        "The previous task failed with error: {}\n\
         Please analyze the failure and create a corrected version.\n\
         Original request was: {}",
        failure_reason,
        extract_original_request(&failed_task)
    );
    
    let params = create_message_params(&recovery_prompt);
    let execution = agent.send_message(
        app_name.to_string(),
        user_id.to_string(),
        params,
    ).await?;
    
    if let SendMessageResult::Task(recovery_task) = execution.result {
        Ok(recovery_task)
    } else {
        Err("Recovery task creation failed".into())
    }
}

fn extract_original_request(task: &Task) -> String {
    task.history.iter()
        .find(|msg| msg.role == MessageRole::User)
        .and_then(|msg| msg.parts.first())
        .and_then(|part| match part {
            Part::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "No original request found".to_string())
}
```

## Best Practices

### 1. Handle All Task States

```rust
match task.status.state {
    TaskState::Submitted => println!("Task queued"),
    TaskState::Working => println!("Task in progress"),
    TaskState::InputRequired => {
        // Handle input requirement
        println!("Waiting for user input");
    }
    TaskState::AuthRequired => {
        // Handle authentication
        println!("Authentication needed");
    }
    TaskState::Completed => {
        // Process successful completion
        println!("Task completed successfully");
    }
    TaskState::Failed => {
        // Handle failure
        println!("Task failed - check error details");
    }
    TaskState::Rejected => {
        // Handle rejection
        println!("Task was rejected");
    }
    TaskState::Canceled => {
        // Handle cancellation
        println!("Task was canceled");
    }
}
```

### 2. Monitor Task Progress

```rust
use std::time::Instant;

async fn monitor_task_with_timeout(
    agent: &Agent,
    app_name: String,
    user_id: String,
    params: MessageSendParams,
    timeout_seconds: u64,
) -> Result<Task, Box<dyn std::error::Error>> {
    
    let start = Instant::now();
    let mut execution = agent.send_streaming_message(app_name, user_id, params).await?;
    
    while let Some(event) = execution.stream.next().await {
        // Check timeout
        if start.elapsed().as_secs() > timeout_seconds {
            return Err("Task timeout".into());
        }
        
        match event {
            SendStreamingMessageResult::Task(task) => {
                return Ok(task);
            }
            SendStreamingMessageResult::TaskStatusUpdate(update) => {
                if matches!(update.status.state, TaskState::Failed | TaskState::Rejected) {
                    return Err(format!("Task failed: {:?}", update.status.state).into());
                }
            }
            _ => {}
        }
    }
    
    Err("Task stream ended unexpectedly".into())
}
```

## Next Steps
- [Sessions Guide](./sessions.md) - Understand session-task relationships