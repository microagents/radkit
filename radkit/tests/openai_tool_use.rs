//! OpenAI GPT Tool Use Tests
//!
//! Tests function calling capabilities with OpenAI GPT models using the Agent for full round-trip validation.
//! These tests require OPENAI_API_KEY environment variable to be set.
//! Tests comprehensive A2A event validation, internal event monitoring, and tool execution tracking.

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult, SendStreamingMessageResult,
};
use radkit::agents::Agent;
use radkit::models::OpenAILlm;
use radkit::sessions::InMemorySessionService;
use radkit::tools::{FunctionTool, ToolResult};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

mod common;
use common::{get_openai_key, init_test_env};

/// Helper function to create Agent with tools if API key is available
fn create_test_agent_with_tools(tools: Vec<FunctionTool>) -> Option<Agent> {
    init_test_env();
    get_openai_key().map(|api_key| {
        let openai_llm = OpenAILlm::new("gpt-4o-mini".to_string(), api_key);
        let session_service = InMemorySessionService::new();
        Agent::builder(
            "You are a helpful assistant. Use the available tools when requested by the user. Always call tools when they can help answer the user's question.",
            openai_llm
        )
        .with_card(|c| c
            .with_name("test_agent")
            .with_description("Test agent for function calling")
        )
        .with_session_service(session_service)
        .with_tools(tools)
        .build()
    })
}

/// Helper function to create a simple user message
fn create_user_message(text: &str) -> Message {
    Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: text.to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    }
}

/// Create a weather tool for testing
fn create_weather_tool() -> FunctionTool {
    FunctionTool::new(
        "get_weather".to_string(),
        "Get the current weather for a location".to_string(),
        |args: HashMap<String, Value>, _context| {
            Box::pin(async move {
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
            })
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "description": "The city and state, e.g. San Francisco, CA"
            }
        },
        "required": ["location"],
        "additionalProperties": false
    }))
}

/// Create a calculation tool for testing
fn create_calculation_tool() -> FunctionTool {
    FunctionTool::new(
        "calculate".to_string(),
        "Perform a mathematical calculation".to_string(),
        |args: HashMap<String, Value>, _context| {
            Box::pin(async move {
                let operation = args
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("add");

                let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);

                let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);

                let result = match operation {
                    "add" => a + b,
                    "subtract" => a - b,
                    "multiply" => a * b,
                    "divide" => {
                        if b != 0.0 {
                            a / b
                        } else {
                            return ToolResult {
                                success: false,
                                data: json!(null),
                                error_message: Some("Division by zero".to_string()),
                            };
                        }
                    }
                    _ => {
                        return ToolResult {
                            success: false,
                            data: json!(null),
                            error_message: Some(format!("Unknown operation: {}", operation)),
                        };
                    }
                };

                ToolResult {
                    success: true,
                    data: json!({
                        "operation": operation,
                        "a": a,
                        "b": b,
                        "result": result
                    }),
                    error_message: None,
                }
            })
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["add", "subtract", "multiply", "divide"],
                "description": "The mathematical operation to perform"
            },
            "a": {
                "type": "number",
                "description": "The first number"
            },
            "b": {
                "type": "number",
                "description": "The second number"
            }
        },
        "required": ["operation", "a", "b"],
        "additionalProperties": false
    }))
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_single_tool_use() {
    let weather_tool = create_weather_tool();
    let Some(agent) = create_test_agent_with_tools(vec![weather_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI single tool use with comprehensive validation...");

    let message = create_user_message("What's the weather like in San Francisco, CA?");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let send_result = result.unwrap();

    // ✅ 1. Validate Task Result Structure
    let task = match send_result.result {
        SendMessageResult::Task(task) => {
            println!("✅ Received task with {} messages", task.history.len());
            task
        }
        _ => panic!("Expected Task result"),
    };

    // ✅ 2. Validate Tool Execution via Session Events (following Anthropic pattern)
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut session_function_calls = 0;
    let mut session_function_responses = 0;
    let mut session_weather_called = false;
    let mut session_weather_succeeded = false;

    println!("✅ Validating tool execution in session events:");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall {
                        name, arguments, ..
                    } => {
                        session_function_calls += 1;
                        println!("  🔧 Function Call: {}", name);
                        if name.as_str() == "get_weather" {
                            session_weather_called = true;
                            if let Some(location) = arguments.get("location") {
                                println!("    📍 Location: {}", location);
                                assert!(
                                    location.as_str().unwrap_or("").contains("San Francisco"),
                                    "Should call weather for San Francisco"
                                );
                            }
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        session_function_responses += 1;
                        println!("  ⚙️ Function Response: {} (success: {})", name, success);
                        if name.as_str() == "get_weather" && *success {
                            session_weather_succeeded = true;
                            if let Some(temp) = result.get("temperature") {
                                println!("    🌡️ Temperature: {}", temp);
                                assert!(
                                    temp.as_str().unwrap_or("").contains("72"),
                                    "Should return temperature from mock"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ✅ 3. Validate Tool Execution Occurred
    assert!(
        session_function_calls >= 1,
        "Should have made at least 1 function call"
    );
    assert!(
        session_function_responses >= 1,
        "Should have received at least 1 function response"
    );
    assert!(
        session_weather_called,
        "get_weather tool should have been called"
    );
    assert!(
        session_weather_succeeded,
        "get_weather tool should have succeeded"
    );

    // ✅ 4. Validate A2A Task History (should contain conversation but NOT function calls)
    println!("✅ Validating A2A task history:");
    assert!(
        task.history.len() >= 2,
        "Should have at least user question + agent response"
    );

    let user_messages = task
        .history
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .count();
    let agent_messages = task
        .history
        .iter()
        .filter(|m| m.role == MessageRole::Agent)
        .count();
    assert!(user_messages >= 1, "Should have at least 1 user message");
    assert!(agent_messages >= 1, "Should have at least 1 agent message");

    // ✅ 5. Validate Response Content includes Tool Results
    let mut response_text = String::new();
    for message in &task.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    response_text.push_str(text);
                }
            }
        }
    }

    let response_lower = response_text.to_lowercase();
    assert!(
        response_lower.contains("72") || response_lower.contains("temperature"),
        "Response should include temperature from weather tool: {}",
        response_text
    );

    println!("✅ Single tool use test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_multiple_tool_use() {
    let weather_tool = create_weather_tool();
    let calc_tool = create_calculation_tool();

    let Some(agent) = create_test_agent_with_tools(vec![weather_tool, calc_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI multiple tool use with comprehensive validation...");

    let message =
        create_user_message("What's 15 multiplied by 3? Also, what's the weather in New York, NY?");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let send_result = result.unwrap();
    let task = match send_result.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // ✅ Validate Multiple Tool Calls via Session Events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut calc_called = false;
    let mut weather_called = false;
    let mut calc_succeeded = false;
    let mut weather_succeeded = false;

    println!("✅ Validating multiple tool execution in session events:");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        println!("  🔧 Function Call: {}", name);
                        match name.as_str() {
                            "calculate" => calc_called = true,
                            "get_weather" => weather_called = true,
                            _ => {}
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        ..
                    } => {
                        println!("  ⚙️ Function Response: {} (success: {})", name, success);
                        if *success {
                            match name.as_str() {
                                "calculate" => calc_succeeded = true,
                                "get_weather" => weather_succeeded = true,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ✅ Validate Both Tools Were Called
    assert!(calc_called, "calculate tool should have been called");
    assert!(weather_called, "get_weather tool should have been called");
    assert!(calc_succeeded, "calculate tool should have succeeded");
    assert!(weather_succeeded, "get_weather tool should have succeeded");

    // ✅ Validate Final Response Contains Both Results
    let mut response_text = String::new();
    for message in &task.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    response_text.push_str(text);
                }
            }
        }
    }

    let response_lower = response_text.to_lowercase();

    // Check for calculation result (15 * 3 = 45)
    assert!(
        response_lower.contains("45"),
        "Response should include calculation result (45): {}",
        response_text
    );

    // Check for weather information
    assert!(
        response_lower.contains("72")
            || response_lower.contains("weather")
            || response_lower.contains("cloudy"),
        "Response should include weather information: {}",
        response_text
    );

    println!("✅ Multiple tool use test completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_streaming_with_tools() {
    let weather_tool = create_weather_tool();
    let Some(agent) = create_test_agent_with_tools(vec![weather_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI streaming with tool use...");

    let message = create_user_message("What's the weather like in Tokyo, Japan?");

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Use streaming message API
    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let mut execution = result.unwrap();
    let mut final_task: Option<radkit::a2a::Task> = None;

    // ✅ Process streaming events
    println!("✅ Processing streaming events:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                println!(
                    "  📨 Streaming Message: role={:?}, parts={}",
                    message.role,
                    message.parts.len()
                );
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ Received final task with {} messages",
                    task.history.len()
                );
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    assert!(final_task.is_some(), "Should receive final task");

    // ✅ Validate tool interactions via real-time internal events
    let mut realtime_weather_called = false;
    let mut realtime_weather_succeeded = false;

    println!("✅ Processing real-time internal events:");
    // Give internal events time to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut event_count = 0;
    while let Some(internal_event) = execution.all_events_stream.next().await {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            &internal_event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        println!("  🔧 Real-time Function Call: {}", name);
                        if name.as_str() == "get_weather" {
                            realtime_weather_called = true;
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        ..
                    } => {
                        println!(
                            "  ⚙️ Real-time Function Response: {} (success: {})",
                            name, success
                        );
                        if name.as_str() == "get_weather" && *success {
                            realtime_weather_succeeded = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        event_count += 1;
        if event_count >= 10 {
            break;
        } // Process only first 10 events
    }

    // ✅ Also validate via session persistence (as fallback)
    let task = final_task.unwrap();
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut session_weather_called = false;
    let mut session_weather_succeeded = false;

    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, .. } => {
                        if name.as_str() == "get_weather" {
                            session_weather_called = true;
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        ..
                    } => {
                        if name.as_str() == "get_weather" && *success {
                            session_weather_succeeded = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // At least one source should capture tool execution
    let weather_called = realtime_weather_called || session_weather_called;
    let weather_succeeded = realtime_weather_succeeded || session_weather_succeeded;

    assert!(
        weather_called,
        "Should observe weather tool call in real-time events or session"
    );
    assert!(
        weather_succeeded,
        "Should observe weather tool success in real-time events or session"
    );

    println!("✅ Streaming with tools test completed successfully");
}

/// Test tool error handling
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_tool_error_handling() {
    let calc_tool = create_calculation_tool();

    let Some(agent) = create_test_agent_with_tools(vec![calc_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI tool error handling...");

    // Request a division by zero to trigger an error - be explicit about using the tool
    let message = create_user_message(
        "Please use the calculate tool to compute 10 divided by 0. I need you to call the function to get the result.",
    );

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(
        result.is_ok(),
        "Agent execution should succeed even with tool error: {:?}",
        result.err()
    );

    let send_result = result.unwrap();
    let task = match send_result.result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // ✅ Validate error handling via session events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &task.context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut found_error = false;
    let mut found_recovery = false;

    println!("✅ Validating tool error handling in session events:");
    for event in &session.events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } = &event.event_type
        {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionResponse {
                        success,
                        error_message,
                        name,
                        ..
                    } => {
                        if name.as_str() == "calculate" && !success {
                            found_error = true;
                            println!(
                                "  ❌ Tool Error: {}",
                                error_message
                                    .as_ref()
                                    .unwrap_or(&"No error message".to_string())
                            );
                            if let Some(err_msg) = error_message {
                                assert!(
                                    err_msg.contains("Division by zero"),
                                    "Error message should mention division by zero, got: {}",
                                    err_msg
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Check if model acknowledged the error in response
    for message in &task.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    let text_lower = text.to_lowercase();
                    if text_lower.contains("divide") && text_lower.contains("zero")
                        || text_lower.contains("undefined")
                        || text_lower.contains("error")
                        || text_lower.contains("cannot")
                    {
                        found_recovery = true;
                        println!("  ✅ Model acknowledged the error appropriately");
                    }
                }
            }
        }
    }

    assert!(
        found_error,
        "Should have encountered and captured the tool error"
    );
    assert!(
        found_recovery,
        "Model should acknowledge the division by zero error"
    );

    println!("✅ Tool error handling test completed successfully");
}
