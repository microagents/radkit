# Tools Guide

Tools in Radkit provide agents with the ability to perform actions beyond text generation. They enable function calling, state management, artifact creation, and interaction with external systems while maintaining security boundaries through the ToolContext system.

## Understanding the Tool System

### Core Concepts

- **BaseTool**: The fundamental trait that all tools must implement
- **ToolContext**: A secure, limited execution context that tools receive
- **FunctionTool**: A convenient wrapper for creating tools from async functions
- **Toolset**: Collections of tools that can be added to agents
- **Built-in Tools**: Pre-built tools for common agent operations

### Tool Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│     Agent       │───▶│  ExecutionContext│───▶│  EventProcessor │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                       │
         ▼                        ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│    Toolset      │    │   ToolContext    │    │ SessionService  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │
         ▼                        ▼
┌─────────────────┐    ┌──────────────────┐
│   BaseTool      │    │ Capability Traits│
└─────────────────┘    └──────────────────┘
```

## Creating Custom Tools

### Using FunctionTool (Recommended)

The `FunctionTool` wrapper is the easiest way to create tools:

```rust
use radkit::tools::{FunctionTool, ToolResult};
use serde_json::json;

fn create_weather_tool() -> FunctionTool {
    FunctionTool::new(
        "get_weather".to_string(),
        "Get current weather for a location".to_string(),
        |args, context| Box::pin(async move {
            // Extract arguments
            let location = args
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            
            // Check user preferences for temperature unit
            let temp_unit = context
                .get_user_state("temperature_unit")
                .await
                .ok()
                .flatten()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "fahrenheit".to_string());
            
            // Simulate weather API call
            let temperature = if temp_unit == "celsius" { "22°C" } else { "72°F" };
            
            let weather_data = json!({
                "location": location,
                "temperature": temperature,
                "conditions": "Partly cloudy",
                "humidity": "45%",
                "unit": temp_unit
            });
            
            ToolResult::success(weather_data)
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
```

### State-Aware Tools

Tools can read and modify user and session state:

```rust
fn create_settings_tool() -> FunctionTool {
    FunctionTool::new(
        "manage_settings".to_string(),
        "Get or set user preferences".to_string(),
        |args, context| Box::pin(async move {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("get");
            
            match action {
                "get" => {
                    // Read current settings
                    let theme = context.get_user_state("theme").await?;
                    let language = context.get_user_state("language").await?;
                    
                    ToolResult::success(json!({
                        "theme": theme,
                        "language": language
                    }))
                }
                "set" => {
                    // Update settings
                    if let Some(theme) = args.get("theme") {
                        context.set_user_state("theme".to_string(), theme.clone()).await?;
                    }
                    if let Some(language) = args.get("language") {
                        context.set_user_state("language".to_string(), language.clone()).await?;
                    }
                    
                    ToolResult::success(json!({"message": "Settings updated"}))
                }
                _ => ToolResult::error("Invalid action".to_string())
            }
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["get", "set"],
                "description": "Whether to get current settings or set new ones"
            },
            "theme": {
                "type": "string",
                "description": "Theme preference (when action=set)"
            },
            "language": {
                "type": "string", 
                "description": "Language preference (when action=set)"
            }
        },
        "required": ["action"]
    }))
}
```

### Interactive Tools

Tools can inject user input for interactive workflows:

```rust
fn create_confirmation_tool() -> FunctionTool {
    FunctionTool::new(
        "ask_confirmation".to_string(),
        "Ask user for confirmation before proceeding".to_string(),
        |args, context| Box::pin(async move {
            let question = args
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("Are you sure?");
            
            // Create user input content for confirmation
            let mut confirmation_content = radkit::models::content::Content::new(
                context.task_id().to_string(),
                context.context_id().to_string(),
                uuid::Uuid::new_v4().to_string(),
                MessageRole::User,
            );
            
            confirmation_content.add_text(
                question.to_string(),
                Some({
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("interaction_type".to_string(), json!("confirmation"));
                    metadata.insert("expects_response".to_string(), json!(true));
                    metadata
                }),
            );
            
            context.add_user_input(confirmation_content).await?;
            
            ToolResult::success(json!({
                "message": "Confirmation request sent to user",
                "awaiting_response": true
            }))
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "question": {
                "type": "string",
                "description": "The confirmation question to ask"
            }
        },
        "required": ["question"]
    }))
}
```

### Long-Running Tools

For tools that perform time-intensive operations:

```rust
fn create_data_processor() -> FunctionTool {
    FunctionTool::new(
        "process_large_dataset".to_string(),
        "Process a large dataset with progress updates".to_string(),
        |args, context| Box::pin(async move {
            let dataset = args
                .get("dataset")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![]);
            
            // Update task status to working
            context.update_task_status(
                TaskState::Working,
                Some("Starting data processing...".to_string())
            ).await?;
            
            let total = dataset.len();
            let mut processed = 0;
            
            for (i, item) in dataset.iter().enumerate() {
                // Simulate processing time
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                processed += 1;
                
                // Update progress every 10 items
                if i % 10 == 0 {
                    context.update_task_status(
                        TaskState::Working,
                        Some(format!("Processed {}/{} items", processed, total))
                    ).await?;
                }
            }
            
            // Save results as artifact
            let results = json!({
                "processed_count": processed,
                "summary": "Dataset processing complete",
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            
            let artifact = Artifact {
                artifact_id: format!("processing_results_{}", uuid::Uuid::new_v4()),
                parts: vec![Part::Text {
                    text: results.to_string(),
                    metadata: None,
                }],
                name: Some("Processing Results".to_string()),
                description: Some("Results from dataset processing".to_string()),
                extensions: vec![],
                metadata: None,
            };
            
            context.save_artifact(artifact).await?;
            
            // Mark task as completed
            context.update_task_status(
                TaskState::Completed,
                Some("Data processing finished successfully".to_string())
            ).await?;
            
            ToolResult::success(json!({
                "processed": processed,
                "total": total,
                "status": "completed"
            }))
        }),
    )
    .with_long_running(true)  // Mark as long-running tool
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "dataset": {
                "type": "array",
                "description": "Array of data items to process"
            }
        },
        "required": ["dataset"]
    }))
}
```

## Implementing BaseTool Directly

For advanced use cases, you can implement `BaseTool` directly:

```rust
use radkit::tools::{BaseTool, FunctionDeclaration, ToolResult, ToolContext};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct CustomDatabaseTool {
    connection_string: String,
}

impl CustomDatabaseTool {
    pub fn new(connection_string: String) -> Self {
        Self { connection_string }
    }
}

#[async_trait]
impl BaseTool for CustomDatabaseTool {
    fn name(&self) -> &str {
        "query_database"
    }
    
    fn description(&self) -> &str {
        "Execute SQL queries against the database"
    }
    
    fn is_long_running(&self) -> bool {
        true
    }
    
    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration {
            name: "query_database".to_string(),
            description: "Execute SQL queries against the database".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "SQL query to execute"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of rows to return",
                        "default": 100
                    }
                },
                "required": ["query"]
            })
        })
    }
    
    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Query is required".to_string()),
        };
        
        let limit = args.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);
        
        // Update status
        context.update_task_status(
            TaskState::Working,
            Some("Executing database query...".to_string())
        ).await.ok();
        
        // Simulate database query
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        // Mock results
        let results = json!({
            "query": query,
            "rows": [
                {"id": 1, "name": "John", "email": "john@example.com"},
                {"id": 2, "name": "Jane", "email": "jane@example.com"}
            ],
            "count": 2,
            "limit": limit
        });
        
        // Save query results as artifact
        let artifact = Artifact {
            artifact_id: format!("query_results_{}", uuid::Uuid::new_v4()),
            parts: vec![Part::Text {
                text: results.to_string(),
                metadata: None,
            }],
            name: Some("Query Results".to_string()),
            description: Some(format!("Results for query: {}", query)),
            extensions: vec![],
            metadata: None,
        };
        
        context.save_artifact(artifact).await.ok();
        
        ToolResult::success(results)
    }
}
```

## Built-in Tools

Radkit provides two essential built-in tools for task management:

### update_status Tool

Allows agents to control their task lifecycle:

```rust
// Enable built-in tools on your agent
let agent = Agent::builder(
        "You can use update_status to communicate your progress.",
        anthropic_llm,
    )
    .with_card(|c| c.with_name("task_agent").with_description("Agent with task management"))
    .with_builtin_task_tools()
    .build();

// Agent can then use the tool:
// {"function_call": {"name": "update_status", "args": {"status": "working", "message": "Processing data..."}}}
```

Available status values:
- `"submitted"` - Initial state
- `"working"` - Agent is processing  
- `"input_required"` - Need user input
- `"auth_required"` - Need authentication
- `"completed"` - Task finished successfully
- `"failed"` - Task failed due to error
- `"rejected"` - Task was rejected
- `"canceled"` - Task was canceled

### save_artifact Tool

Allows agents to persist important outputs:

```rust
// Agent can use this tool:
// {
//   "function_call": {
//     "name": "save_artifact",
//     "args": {
//       "name": "Analysis Report",
//       "content": "...",
//       "type": "document", 
//       "description": "Complete analysis results"
//     }
//   }
// }
```

Supported artifact types:
- `"file"` - File content
- `"data"` - Structured data
- `"result"` - Computation results
- `"log"` - Log entries
- `"image"` - Image data
- `"document"` - Text documents

## Toolsets

Organize tools into reusable collections:

### SimpleToolset

```rust
use radkit::tools::SimpleToolset;

let weather_tool = create_weather_tool();
let calculator_tool = create_calculator_tool();

let toolset = SimpleToolset::new()
    .add_tool(weather_tool)
    .add_tool(calculator_tool);

// Add to agent
let agent = Agent::builder(
        "You have access to weather and calculator tools.",
        anthropic_llm,
    )
    .with_card(|c| c.with_name("multi_tool_agent").with_description("Agent with multiple tools"))
    .with_toolset(toolset)
    .build();
```

### CombinedToolset

Merge multiple toolsets:

```rust
use radkit::tools::CombinedToolset;

let base_toolset = create_base_toolset();
let specialized_toolset = create_domain_specific_toolset();

let combined = CombinedToolset::new(base_toolset)
    .with_additional_toolset(specialized_toolset);

let agent = Agent::builder(
        "You have access to multiple toolsets.",
        anthropic_llm,
    )
    .with_toolset(combined)
    .build();
```

## Error Handling in Tools

### Graceful Error Handling

```rust
fn create_robust_tool() -> FunctionTool {
    FunctionTool::new(
        "robust_operation".to_string()).with_description("An operation that handles errors gracefully".to_string()))
    .build().await {
                Ok(result) => {
                    context.update_task_status(
                        TaskState::Completed,
                        Some("Operation completed successfully".to_string())
                    ).await.ok();
                    
                    ToolResult::success(result)
                }
                Err(e) => {
                    // Update task status to indicate failure
                    context.update_task_status(
                        TaskState::Working, // Continue working, don't fail task
                        Some(format!("Operation failed: {}", e))
                    ).await.ok();
                    
                    // Return error to agent
                    ToolResult::error(format!("Failed to complete operation: {}", e))
                }
            }
        }),
    )
}

async fn perform_risky_operation(args: &HashMap<String, Value>) -> Result<Value, String> {
    // Simulate operation that might fail
    if args.get("should_fail").and_then(|v| v.as_bool()).unwrap_or(false) {
        Err("Simulated failure".to_string())
    } else {
        Ok(json!({"success": true}))
    }
}
```

### Retry Logic

```rust
fn create_retry_tool() -> FunctionTool {
    FunctionTool::new(
        "retry_operation".to_string(),
        "Operation with automatic retry logic".to_string(),
        |args, context| Box::pin(async move {
            let max_retries = 3;
            
            for attempt in 1..=max_retries {
                context.update_task_status(
                    TaskState::Working,
                    Some(format!("Attempt {} of {}", attempt, max_retries))
                ).await.ok();
                
                match perform_unreliable_operation(&args).await {
                    Ok(result) => {
                        return ToolResult::success(result);
                    }
                    Err(e) if attempt == max_retries => {
                        return ToolResult::error(format!(
                            "Failed after {} attempts: {}", max_retries, e
                        ));
                    }
                    Err(_) => {
                        // Wait before retry
                        tokio::time::sleep(
                            tokio::time::Duration::from_millis(1000 * attempt)
                        ).await;
                    }
                }
            }
            
            unreachable!()
        }),
    )
}

async fn perform_unreliable_operation(args: &HashMap<String, Value>) -> Result<Value, String> {
    // Simulate unreliable operation
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    if rng.gen_bool(0.7) { // 70% chance of failure
        Err("Network timeout".to_string())
    } else {
        Ok(json!({"data": "success"}))
    }
}
```

## Testing Tools

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use radkit::tools::ToolContext;
    use radkit::events::{ExecutionContext, EventProcessor, InMemoryEventBus};
    use radkit::sessions::{InMemorySessionService, QueryService};
    use std::sync::Arc;
    
    async fn create_test_context() -> ExecutionContext {
        let session_service = Arc::new(InMemorySessionService::new());
        let query_service = Arc::new(QueryService::new(session_service.clone()));
        let event_bus = Arc::new(InMemoryEventBus::new());
        let event_processor = Arc::new(EventProcessor::new(session_service, event_bus));
        
        // Create execution context for testing
        ExecutionContext::new(
            "test_ctx".to_string(),
            "test_task".to_string(),
            "test_app".to_string(),
            "test_user".to_string(),
            params, // MessageSendParams
            event_processor,
            query_service,
        )
    }
    
    #[tokio::test]
    async fn test_weather_tool() {
        let tool = create_weather_tool();
        let exec_ctx = create_test_context().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);
        
        let mut args = HashMap::new();
        args.insert("location".to_string(), json!("San Francisco, CA"));
        
        let result = tool.run_async(args, &tool_ctx).await;
        
        assert!(result.success);
        assert!(result.data.get("location").is_some());
        assert!(result.data.get("temperature").is_some());
    }
    
    #[tokio::test]
    async fn test_settings_tool_state_management() {
        let tool = create_settings_tool();
        let exec_ctx = create_test_context().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);
        
        // Set a preference
        let mut args = HashMap::new();
        args.insert("action".to_string(), json!("set"));
        args.insert("theme".to_string(), json!("dark"));
        
        let result = tool.run_async(args, &tool_ctx).await;
        assert!(result.success);
        
        // Get preferences
        let mut args = HashMap::new();
        args.insert("action".to_string(), json!("get"));
        
        let result = tool.run_async(args, &tool_ctx).await;
        assert!(result.success);
        assert_eq!(result.data["theme"], json!("dark"));
    }
}
```

## Best Practices

### 1. Use Descriptive Names and Documentation

```rust
fn create_well_documented_tool() -> FunctionTool {
    FunctionTool::new(
        "analyze_sentiment".to_string(),
        "Analyze the emotional sentiment of text using natural language processing. Returns sentiment score (-1.0 to 1.0) and confidence level.".to_string(),
        |args, context| {
            // Implementation
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "text": {
                "type": "string",
                "description": "The text to analyze for sentiment",
                "minLength": 1,
                "maxLength": 5000
            },
            "model": {
                "type": "string", 
                "enum": ["basic", "advanced"],
                "default": "basic",
                "description": "Sentiment analysis model to use"
            }
        },
        "required": ["text"]
    }))
}
```

### 2. Handle Context Information

```rust
fn create_context_aware_tool() -> FunctionTool {
    FunctionTool::new(
        "context_aware_operation".to_string(),
        "Operation that uses context information".to_string(),
        |args, context| Box::pin(async move {
            // Access context information
            let task_id = context.task_id();
            let user_id = context.user_id();
            let app_name = context.app_name();
            
            // Use context in operation
            let result = json!({
                "result": "operation completed",
                "context": {
                    "task_id": task_id,
                    "user_id": user_id,
                    "app_name": app_name
                }
            });
            
            ToolResult::success(result)
        }),
    )
}
```

### 3. Implement Proper Validation

```rust
fn create_validated_tool() -> FunctionTool {
    FunctionTool::new(
        "send_email".to_string(),
        "Send an email to specified recipients".to_string(),
        |args, context| Box::pin(async move {
            // Validate required parameters
            let to = match args.get("to").and_then(|v| v.as_str()) {
                Some(email) if is_valid_email(email) => email,
                Some(invalid) => return ToolResult::error(
                    format!("Invalid email address: {}", invalid)
                ),
                None => return ToolResult::error("to field is required".to_string()),
            };
            
            let subject = args.get("subject")
                .and_then(|v| v.as_str())
                .unwrap_or("(No Subject)");
                
            let body = match args.get("body").and_then(|v| v.as_str()) {
                Some(b) if !b.trim().is_empty() => b,
                _ => return ToolResult::error("body cannot be empty".to_string()),
            };
            
            // Perform operation
            send_email_impl(to, subject, body).await
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "to": {
                "type": "string",
                "format": "email",
                "description": "Recipient email address"
            },
            "subject": {
                "type": "string",
                "maxLength": 200,
                "description": "Email subject line"
            },
            "body": {
                "type": "string",
                "minLength": 1,
                "maxLength": 10000,
                "description": "Email message body"
            }
        },
        "required": ["to", "body"]
    }))
}

fn is_valid_email(email: &str) -> bool {
    // Basic email validation
    email.contains('@') && email.contains('.')
}

async fn send_email_impl(to: &str, subject: &str, body: &str) -> ToolResult {
    // Implement email sending
    ToolResult::success(json!({"sent": true, "to": to}))
}
```

### 4. Provide Progress Updates

```rust
fn create_progress_tool() -> FunctionTool {
    FunctionTool::new(
        "batch_process".to_string(),
        "Process multiple items with progress updates".to_string(),
        |args, context| Box::pin(async move {
            let items = match args.get("items").and_then(|v| v.as_array()) {
                Some(items) => items,
                None => return ToolResult::error("items array is required".to_string()),
            };
            
            let total = items.len();
            let mut processed = Vec::new();
            
            for (index, item) in items.iter().enumerate() {
                // Update progress
                let progress = ((index as f64 / total as f64) * 100.0) as u32;
                context.update_task_status(
                    TaskState::Working,
                    Some(format!("Processing item {} of {} ({}%)", 
                        index + 1, total, progress))
                ).await.ok();
                
                // Process item
                let result = process_item(item).await?;
                processed.push(result);
                
                // Small delay to show progress
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            
            ToolResult::success(json!({
                "processed": processed,
                "total": total
            }))
        }),
    )
    .with_long_running(true)
}
```

## MCP (Model Context Protocol) Tools

Radkit includes built-in support for MCP servers, which provide external tools and data sources for agents. MCP tools are automatically discovered and integrated into your agent's toolset.

### Setting Up MCP Weather Agent

Here's a complete example of creating an agent that uses an MCP weather server:

```rust
use radkit::agents::Agent;
use radkit::models::AnthropicLlm;
use radkit::sessions::InMemorySessionService;
use radkit::tools::{MCPConnectionParams, MCPToolset};
use radkit::a2a::{Message, MessageSendParams, Part};
use std::collections::HashMap;
use std::time::Duration;
use tokio_stream::StreamExt;

async fn create_weather_agent() -> Agent {
    // Create LLM (Anthropic Claude in this example)
    let llm = AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required")
    );

    // Create session service
    let session_service = InMemorySessionService::new();

    // Configure MCP weather server connection
    let mcp_connection = MCPConnectionParams::Stdio {
        command: "uvx".to_string(),
        args: vec![
            "--from".to_string(),
            "git+https://github.com/microagents/mcp-servers.git#subdirectory=mcp-weather-free".to_string(),
            "mcp-weather-free".to_string(),
        ],
        env: HashMap::new(),
        timeout: Duration::from_secs(30),
    };

    // Create MCP toolset
    let mcp_toolset = MCPToolset::new(mcp_connection);

    // Build weather agent with MCP tools
    Agent::builder(
        "You are a helpful weather assistant. Use the available weather tools to provide accurate weather information for any location requested by the user. Always call the weather tools when users ask about weather conditions, forecasts, or climate information.",
        llm,
    )
    .with_card(|c| c.with_name("weather_agent").with_description("Weather Assistant"))
    .with_session_service(session_service)
    .with_toolset(mcp_toolset)
    .with_builtin_task_tools() // Include update_status and save_artifact tools
    .build()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the weather agent
    let agent = create_weather_agent().await;
    
    // Create user message asking for weather
    let user_message = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: radkit::a2a::MessageRole::User,
        parts: vec![Part::Text {
            text: "What's the current weather in San Francisco, California?".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };
    
    // Send message and get streaming response
    let params = MessageSendParams {
        message: user_message,
        configuration: None,
        metadata: None,
    };
    
    let mut execution = agent
        .send_streaming_message(
            "weather_app".to_string(),
            "user_123".to_string(),
            params,
        )
        .await?;
    
    // Process streaming response
    println!("🌤️ Weather Assistant Response:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            radkit::a2a::SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        print!("{}", text);
                    }
                }
            }
            radkit::a2a::SendStreamingMessageResult::Task(task) => {
                println!("\n✅ Task completed: {:?}", task.status.state);
                break;
            }
            _ => {}
        }
    }
    
    Ok(())
}
```

### MCP Connection Types

The `MCPConnectionParams` enum supports different MCP server connection methods:

#### Stdio Connection (Local Process)

```rust
use radkit::tools::MCPConnectionParams;
use std::time::Duration;
use std::collections::HashMap;

let stdio_connection = MCPConnectionParams::Stdio {
    command: "python".to_string(),
    args: vec!["-m".to_string(), "my_mcp_server".to_string()],
    env: {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "your_api_key".to_string());
        env
    },
    timeout: Duration::from_secs(30),
};
```

#### HTTP Connection (Remote Server)

```rust
let http_connection = MCPConnectionParams::Http {
    url: "https://api.example.com/mcp".to_string(),
    headers: {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers
    },
    timeout: Duration::from_secs(10),
};
```

### Tool Filtering

You can control which MCP tools are available to your agent:

```rust
use radkit::tools::{MCPToolset, MCPToolFilter};

// Include all tools (default)
let all_tools = MCPToolset::new(mcp_connection)
    .with_tool_filter(MCPToolFilter::All);

// Include only specific tools
let specific_tools = MCPToolset::new(mcp_connection)
    .with_tool_filter(MCPToolFilter::Include(vec![
        "get_weather".to_string(),
        "get_forecast".to_string(),
    ]));

// Exclude certain tools
let filtered_tools = MCPToolset::new(mcp_connection)
    .with_tool_filter(MCPToolFilter::Exclude(vec![
        "dangerous_tool".to_string(),
    ]));
```

### Multiple MCP Servers

You can combine multiple MCP toolsets for different capabilities:

```rust
use radkit::tools::{CombinedToolset, SimpleToolset};

async fn create_multi_capability_agent() -> Agent {
    // Weather MCP server
    let weather_connection = MCPConnectionParams::Stdio {
        command: "uvx".to_string(),
        args: vec!["--from".to_string(), "mcp-weather".to_string(), "mcp-weather".to_string()],
        env: HashMap::new(),
        timeout: Duration::from_secs(30),
    };
    let weather_toolset = MCPToolset::new(weather_connection);

    // Database MCP server
    let db_connection = MCPConnectionParams::Http {
        url: "https://db-mcp.example.com".to_string(),
        headers: {
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), "Bearer db_token".to_string());
            headers
        },
        timeout: Duration::from_secs(15),
    };
    let db_toolset = MCPToolset::new(db_connection);

    // Custom tools
    let custom_toolset = SimpleToolset::new()
        .add_tool(create_calculator_tool());

    // Combine all toolsets
    let combined_toolset = CombinedToolset::new(weather_toolset)
        .with_additional_toolset(db_toolset)
        .with_additional_toolset(custom_toolset);

    Agent::builder(
        "You have access to weather data, database queries, and calculation tools.",
        llm,
    )
    .with_card(|c| c.with_name("multi_agent").with_description("Multi-Capability Assistant"))
    .with_toolset(combined_toolset)
    .build()
}
```

## Next Steps

- [Getting Started Guide](./getting-started.md) - Learn basic agent setup
- [Tasks Guide](./tasks.md) - Understand task lifecycle
- [Sessions Guide](./sessions.md) - Learn about state management

The tools system in Radkit provides a powerful foundation for extending agent capabilities while maintaining security and reliability. Use the ToolContext system to create safe, capable tools that integrate seamlessly with the A2A protocol.